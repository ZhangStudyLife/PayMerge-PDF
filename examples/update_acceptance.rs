//! Single-file upgrade acceptance, with an isolated user-data root and no desktop automation.
#[cfg(windows)]
fn main() -> paymerge_pdf::Result<()> {
    use paymerge_pdf::{message, update};
    use std::{fs, io::Write, path::PathBuf};
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 4 {
        return Err(message(
            "usage: update_acceptance <single-exe> <isolated-root> <local|github>",
        ));
    }
    let source = PathBuf::from(&args[1]);
    let root = PathBuf::from(&args[2]);
    fs::create_dir_all(&root)?;
    let data = PathBuf::from(
        std::env::var_os("PAYMERGE_DATA_DIR")
            .ok_or_else(|| message("Set PAYMERGE_DATA_DIR inside the isolated root"))?,
    );
    fs::create_dir_all(&data)?;
    if !data.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(message("User data must be isolated inside the test root"));
    }
    fs::write(
        data.join("config.json"),
        b"{\"name\":\"synthetic-user\",\"group\":\"test\"}",
    )?;
    let output = root.join("synthetic-output.pdf");
    fs::write(&output, b"synthetic reimbursement output")?;
    let verified_download = root.join("verified-download.exe");
    for fail in [false, true] {
        let app = tempfile::Builder::new()
            .prefix("合票 升级-")
            .tempdir_in(&root)?;
        let installed = app.path().join("合票 工具.exe");
        fs::copy(&source, &installed)?;
        update::copy_program_access(&source, &installed)?;
        // A valid PE overlay makes the baseline distinct without changing its updater protocol.
        fs::OpenOptions::new()
            .append(true)
            .open(&installed)?
            .write_all(b"ACCEPTANCE_BASELINE_MARKER")?;
        let before = update::hash_file(&installed)?;
        println!(
            "single EXE baseline prepared; staging {}",
            if fail { "rollback" } else { "upgrade" }
        );
        let mut staged = if args[3] == "github" && !fail {
            let release = update::check_latest("7.1.0")?
                .ok_or_else(|| message("Expected published single-file release"))?;
            if release.version != update::CURRENT_VERSION {
                return Err(message("Unexpected release version"));
            }
            update::download_update(&release, &installed, |_, _| {})?
        } else {
            update::stage_local_executable(
                if args[3] == "github" {
                    &verified_download
                } else {
                    &source
                },
                update::CURRENT_VERSION,
                &installed,
            )?
        };
        if args[3] == "github" && !fail {
            fs::copy(
                staged
                    .directory
                    .path()
                    .join("payload")
                    .join(update::MAIN_EXE),
                &verified_download,
            )?;
        }
        if fail {
            staged.version = "0.0.0".into();
        }
        let stage = staged.directory.path().to_owned();
        let mut child = update::launch_installer(staged, 0, false)?;
        let exit = child.wait()?;
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(stage.join("result.json"))?)?;
        assert_eq!(exit.success(), !fail, "{receipt}");
        assert_eq!(receipt["success"], !fail, "{receipt}");
        assert_eq!(receipt["can_cleanup"], true, "{receipt}");
        assert_eq!(
            update::hash_file(&installed)?,
            if fail {
                before
            } else {
                update::hash_file(&source)?
            }
        );
        assert_eq!(
            fs::read(data.join("config.json"))?,
            b"{\"name\":\"synthetic-user\",\"group\":\"test\"}"
        );
        assert_eq!(fs::read(&output)?, b"synthetic reimbursement output");
        let notice = update::finish_update(&stage, 0, app.path())?;
        assert!(!stage.exists());
        assert_eq!(
            fs::read_dir(app.path())?.count(),
            1,
            "No adjacent DLL, updater, config or license files"
        );
        println!(
            "{notice}; renamed EXE, private settings and output verified; program directory contains one EXE"
        );
    }
    if args[3] == "github" {
        assert!(update::check_latest(update::CURRENT_VERSION)?.is_none());
    }
    println!("single-file updater acceptance passed");
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Run the Windows cross-compiled acceptance executable.");
}
