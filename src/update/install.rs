use super::*;
use std::{
    process::{Child, Command},
    thread,
    time::Instant,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct Receipt {
    pub success: bool,
    pub can_cleanup: bool,
    pub message: String,
}

struct AppliedFile {
    target: PathBuf,
    backup: Option<PathBuf>,
}

struct InstallFailure {
    message: String,
    restored: bool,
}

fn read_plan(path: &Path) -> Result<(PathBuf, InstallPlan)> {
    let stage = path
        .parent()
        .ok_or_else(|| message("升级计划路径无效"))?
        .canonicalize()?;
    let plan: InstallPlan = serde_json::from_slice(&fs::read(path)?)?;
    let app_dir = plan.app_dir.canonicalize()?;
    if stage.parent() != Some(app_dir.as_path())
        || !stage
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with(STAGE_PREFIX))
    {
        return Err(message("升级暂存目录必须位于程序目录内"));
    }
    let payload = stage.join("payload");
    if payload.canonicalize()?.parent() != Some(stage.as_path()) {
        return Err(message("升级内容路径无效"));
    }
    safe_executable_name(&plan.executable_name)?;
    if plan.files.len() != 1 || plan.files[0].path != MAIN_EXE || !valid_hash(&plan.files[0].sha256)
    {
        return Err(message("升级计划只能替换主程序"));
    }
    Ok((stage, plan))
}

fn rollback(applied: &mut Vec<AppliedFile>) -> std::result::Result<(), std::io::Error> {
    let mut first_error = None;
    for file in applied.drain(..).rev() {
        if let Err(error) = fs::remove_file(&file.target)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            first_error.get_or_insert(error);
        }
        if let Some(backup) = file.backup
            && let Err(error) = fs::rename(backup, &file.target)
        {
            first_error.get_or_insert(error);
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn install_transaction(
    stage: &Path,
    plan: &InstallPlan,
    verify: impl FnOnce(&Path) -> Result<()>,
) -> std::result::Result<(), InstallFailure> {
    let prepare = || -> Result<()> {
        verify_files(&stage.join("payload"), &plan.files)?;
        ensure_plain_path(&plan.app_dir, &plan.executable_name)?;
        fs::create_dir(stage.join("backup"))?;
        Ok(())
    };
    prepare().map_err(|error| InstallFailure {
        message: error.to_string(),
        restored: true,
    })?;
    let mut applied = Vec::new();
    let operation = (|| -> Result<()> {
        for file in &plan.files {
            let target = plan.app_dir.join(&plan.executable_name);
            let incoming = stage.join("payload").join(&file.path);
            let backup = stage.join("backup").join(&file.path);
            fs::create_dir_all(target.parent().unwrap())?;
            let previous = if target.exists() {
                fs::create_dir_all(backup.parent().unwrap())?;
                fs::rename(&target, &backup)?;
                Some(backup)
            } else {
                None
            };
            // Record before installing, so a failing rename restores the just-stashed file too.
            applied.push(AppliedFile {
                target: target.clone(),
                backup: previous,
            });
            fs::rename(incoming, target)?;
        }
        verify(&plan.app_dir.join(&plan.executable_name))?;
        Ok(())
    })();
    if let Err(error) = operation {
        return match rollback(&mut applied) {
            Ok(()) => Err(InstallFailure {
                message: format!("升级失败，已恢复旧版本：{error}"),
                restored: true,
            }),
            Err(rollback_error) => Err(InstallFailure {
                message: format!(
                    "升级失败：{error}；恢复旧文件失败：{rollback_error}。备份保留在 {}",
                    stage.display()
                ),
                restored: false,
            }),
        };
    }
    Ok(())
}

fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW, no console for the updater or verification.
    }
    command
}

fn wait_child(child: &mut Child, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(message("新版本启动校验失败"))
            };
        }
        thread::sleep(Duration::from_millis(50));
    }
    child.kill()?;
    let _ = child.wait();
    Err(message("新版本启动校验超时"))
}

#[cfg(windows)]
fn wait_process(pid: u32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError},
        System::Threading::{OpenProcess, WaitForSingleObject},
    };
    if pid == 0 {
        return Ok(());
    }
    // Only wait for the parent/helper. Never terminate a user's running instance.
    unsafe {
        let handle = OpenProcess(0x0010_0000, 0, pid); // SYNCHRONIZE
        if handle.is_null() {
            return if GetLastError() == 87 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error().into())
            };
        }
        let status = WaitForSingleObject(handle, 120_000);
        CloseHandle(handle);
        if status == 0 {
            Ok(())
        } else {
            Err(message("等待程序退出超时，尚未修改安装文件"))
        }
    }
}

#[cfg(not(windows))]
fn wait_process(pid: u32) -> Result<()> {
    if pid == 0 {
        Ok(())
    } else {
        Err(message("安装器运行在 Windows 上"))
    }
}

/// Preserve executable access when files are created on a Windows-mounted Samba share.
/// This copies only the existing program file's DACL, never its ownership or global share settings.
#[cfg(windows)]
pub fn copy_program_access(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetFileSecurityW, SetFileSecurityW,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut needed = 0_u32;
    unsafe {
        GetFileSecurityW(
            source.as_ptr(),
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
        if needed == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut descriptor = vec![0_u32; (needed as usize).div_ceil(4)];
        if GetFileSecurityW(
            source.as_ptr(),
            DACL_SECURITY_INFORMATION,
            descriptor.as_mut_ptr().cast(),
            needed,
            &mut needed,
        ) == 0
            || SetFileSecurityW(
                target.as_ptr(),
                DACL_SECURITY_INFORMATION,
                descriptor.as_mut_ptr().cast(),
            ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}

pub fn launch_installer(staged: StagedUpdate, parent_pid: u32, restart: bool) -> Result<Child> {
    let plan_path = staged.write_plan(parent_pid, restart)?;
    let source = staged.app_dir.join(&staged.executable_name);
    let runner = staged.directory.path().join("runner.exe");
    fs::copy(&source, &runner)?;
    #[cfg(windows)]
    {
        copy_program_access(&source, &runner)?;
        copy_program_access(
            &source,
            &staged.directory.path().join("payload").join(MAIN_EXE),
        )?;
    }
    let log = fs::File::create(staged.directory.path().join("installer.log"))?;
    let child = hidden(Command::new(&runner).arg("--apply-update").arg(&plan_path))
        .stdout(std::process::Stdio::from(log.try_clone()?))
        .stderr(std::process::Stdio::from(log))
        .spawn()?;
    // Ownership passes to the installer; the restarted application cleans this directory.
    let _ = staged.directory.keep();
    Ok(child)
}

pub fn apply_plan(path: &Path) -> Result<Receipt> {
    let (stage, plan) = read_plan(path)?;
    wait_process(plan.parent_pid)?;
    let outcome = install_transaction(&stage, &plan, |exe| {
        let mut child =
            hidden(Command::new(exe).arg("--verify-install").arg(&plan.version)).spawn()?;
        wait_child(&mut child, Duration::from_secs(20))
    });
    let receipt = match outcome {
        Ok(()) => Receipt {
            success: true,
            can_cleanup: true,
            message: format!("已升级至 v{}", plan.version),
        },
        Err(error) => Receipt {
            success: false,
            can_cleanup: error.restored,
            message: error.message,
        },
    };
    fs::write(
        stage.join("result.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    if plan.restart && receipt.can_cleanup {
        Command::new(plan.app_dir.join(&plan.executable_name))
            .arg("--finish-update")
            .arg(&stage)
            .arg(std::process::id().to_string())
            .current_dir(&plan.app_dir)
            .spawn()?;
    }
    Ok(receipt)
}

pub fn finish_update(stage: &Path, helper_pid: u32, app_dir: &Path) -> Result<String> {
    let (stage, plan) = read_plan(&stage.join("plan.json"))?;
    if plan.app_dir.canonicalize()? != app_dir.canonicalize()? {
        return Err(message("升级清理目录与当前程序不符"));
    }
    wait_process(helper_pid)?;
    let receipt: Receipt = serde_json::from_slice(&fs::read(stage.join("result.json"))?)?;
    if receipt.can_cleanup {
        fs::remove_dir_all(stage)?;
    }
    Ok(receipt.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, tempfile::TempDir, InstallPlan) {
        let app = tempfile::tempdir().unwrap();
        let stage = tempfile::Builder::new()
            .prefix(STAGE_PREFIX)
            .tempdir_in(app.path())
            .unwrap();
        fs::create_dir(stage.path().join("payload")).unwrap();
        fs::write(app.path().join("config.json"), b"personal settings").unwrap();
        fs::write(app.path().join("output.pdf"), b"user output").unwrap();
        let executable_name = "合票 工具.exe".to_owned();
        fs::write(app.path().join(&executable_name), b"old executable").unwrap();
        let incoming = stage.path().join("payload").join(MAIN_EXE);
        fs::write(&incoming, b"new executable").unwrap();
        let plan = InstallPlan {
            version: "7.3.0".into(),
            app_dir: app.path().canonicalize().unwrap(),
            executable_name,
            parent_pid: 0,
            restart: false,
            files: vec![PackageFile {
                path: MAIN_EXE.into(),
                sha256: hash_file(&incoming).unwrap(),
            }],
        };
        (app, stage, plan)
    }
    #[test]
    fn installation_replaces_only_the_running_executable() {
        let (app, stage, plan) = fixture();
        assert!(install_transaction(stage.path(), &plan, |_| Ok(())).is_ok());
        assert_eq!(
            fs::read(app.path().join(&plan.executable_name)).unwrap(),
            b"new executable"
        );
        assert_eq!(
            fs::read(app.path().join("config.json")).unwrap(),
            b"personal settings"
        );
        assert_eq!(
            fs::read(app.path().join("output.pdf")).unwrap(),
            b"user output"
        );
        assert!(!app.path().join(MAIN_EXE).exists());
    }
    #[test]
    fn failed_startup_restores_the_exact_previous_executable() {
        let (app, stage, plan) = fixture();
        let failure = install_transaction(stage.path(), &plan, |_| Err(message("invalid version")))
            .err()
            .unwrap();
        assert!(failure.restored);
        assert_eq!(
            fs::read(app.path().join(&plan.executable_name)).unwrap(),
            b"old executable"
        );
    }
    #[test]
    fn changed_payload_is_rejected_before_replacement() {
        let (app, stage, plan) = fixture();
        fs::write(stage.path().join("payload").join(MAIN_EXE), b"tampered").unwrap();
        assert!(install_transaction(stage.path(), &plan, |_| Ok(())).is_err());
        assert_eq!(
            fs::read(app.path().join(&plan.executable_name)).unwrap(),
            b"old executable"
        );
    }
    #[cfg(windows)]
    #[test]
    fn occupied_executable_is_left_intact() {
        use std::os::windows::fs::OpenOptionsExt;
        let (app, stage, plan) = fixture();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(app.path().join(&plan.executable_name))
            .unwrap();
        assert!(install_transaction(stage.path(), &plan, |_| Ok(())).is_err());
        drop(held);
        assert_eq!(
            fs::read(app.path().join(&plan.executable_name)).unwrap(),
            b"old executable"
        );
    }
}
