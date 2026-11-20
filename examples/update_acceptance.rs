//! Native Windows updater acceptance. Uses only synthetic personal files, never opens a desktop UI.
#[cfg(windows)]
fn main() -> paymerge_pdf::Result<()> {
    use paymerge_pdf::{
        message,
        update::{self, MAIN_EXE, UPDATER_EXE},
    };
    use std::{fs, path::PathBuf};
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 5 {
        return Err(message(
            "usage: update_acceptance <old-package> <new-package> <zip-or-github> <temp-root>",
        ));
    }
    let old = PathBuf::from(&args[1]);
    let new = PathBuf::from(&args[2]);
    let source = PathBuf::from(&args[3]);
    let temporary = PathBuf::from(&args[4]);
    fs::create_dir_all(&temporary)?;
    for fail_verification in [false, true] {
        let app = tempfile::Builder::new()
            .prefix("升级 验收-")
            .tempdir_in(&temporary)?;
        for name in [MAIN_EXE, "pdfium.dll"] {
            fs::copy(old.join(name), app.path().join(name))?;
            update::copy_program_access(&old.join(name), &app.path().join(name))?;
        }
        fs::copy(new.join(UPDATER_EXE), app.path().join(UPDATER_EXE))?;
        update::copy_program_access(&new.join(UPDATER_EXE), &app.path().join(UPDATER_EXE))?;
        fs::write(
            app.path().join("config.json"),
            b"{\"name\":\"synthetic-user\"}",
        )?;
        fs::write(
            app.path().join("synthetic-output.pdf"),
            b"synthetic user output",
        )?;
        let old_hash = update::hash_file(&app.path().join(MAIN_EXE))?;
        println!(
            "synthetic installation prepared; staging {}",
            if fail_verification {
                "rollback case"
            } else {
                "upgrade case"
            }
        );
        let staged = if source == std::path::Path::new("github") {
            let release = update::check_latest("7.0.0")?
                .ok_or_else(|| message("Expected a newer GitHub release"))?;
            if release.version != update::CURRENT_VERSION {
                return Err(message("Unexpected release"));
            }
            update::download_update(&release, app.path(), |_, _| {})?
        } else {
            update::stage_local_archive(&source, update::CURRENT_VERSION, app.path())?
        };
        let mut staged = staged;
        if fail_verification {
            staged.version = "0.0.0".into();
        }
        let stage_path = staged.directory.path().to_owned();
        println!("verified payload; launching independent update helper");
        let mut child = update::launch_installer(staged, 0, false)?;
        let exit = child.wait()?;
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(stage_path.join("result.json"))?)?;
        assert_eq!(exit.success(), !fail_verification, "{receipt}");
        assert_eq!(receipt["success"], !fail_verification, "{receipt}");
        assert_eq!(receipt["can_cleanup"], true, "{receipt}");
        if fail_verification {
            assert_eq!(update::hash_file(&app.path().join(MAIN_EXE))?, old_hash);
        } else {
            for name in [MAIN_EXE, UPDATER_EXE, "pdfium.dll"] {
                assert_eq!(
                    update::hash_file(&app.path().join(name))?,
                    update::hash_file(&new.join(name))?
                );
            }
        }
        assert_eq!(
            fs::read(app.path().join("config.json"))?,
            b"{\"name\":\"synthetic-user\"}"
        );
        assert_eq!(
            fs::read(app.path().join("synthetic-output.pdf"))?,
            b"synthetic user output"
        );
        let notice = update::finish_update(&stage_path, 0, app.path())?;
        assert!(!stage_path.exists());
        println!(
            "{}: {notice}; configuration and synthetic output preserved; stage removed",
            if fail_verification {
                "rollback"
            } else {
                "upgrade"
            }
        );
    }
    assert!(update::check_latest(update::CURRENT_VERSION)?.is_none());
    println!("native updater acceptance passed; no windows or OS input automation");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Run the cross-compiled acceptance executable on Windows.");
}
