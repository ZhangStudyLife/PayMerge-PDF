#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod ui;

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--verify-install") {
        let valid = (|| -> paymerge_pdf::Result<()> {
            if args.get(2).and_then(|arg| arg.to_str())
                != Some(paymerge_pdf::update::CURRENT_VERSION)
            {
                return Err(paymerge_pdf::message("安装版本不一致"));
            }
            let executable = std::env::current_exe()?;
            let directory = executable
                .parent()
                .ok_or_else(|| paymerge_pdf::message("安装目录无效"))?;
            let bindings = pdfium_render::prelude::Pdfium::bind_to_library(
                pdfium_render::prelude::Pdfium::pdfium_platform_library_name_at_path(directory),
            )?;
            let pdf = pdfium_render::prelude::Pdfium::new(bindings);
            let _document = pdf.create_new_pdf()?;
            Ok(())
        })()
        .is_ok();
        std::process::exit(if valid { 0 } else { 1 });
    }
    let status = if args.len() == 4 && args[1] == "--finish-update" {
        let directory = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
        let helper = args[3].to_string_lossy().parse::<u32>();
        match (directory, helper) {
            (Some(directory), Ok(helper)) => Some(
                paymerge_pdf::update::finish_update(
                    std::path::Path::new(&args[2]),
                    helper,
                    &directory,
                )
                .unwrap_or_else(|e| e.to_string()),
            ),
            _ => Some("更新清理参数无效".to_owned()),
        }
    } else {
        None
    };
    if let Err(error) = ui::run(status) {
        rfd::MessageDialog::new()
            .set_title("启动失败")
            .set_description(error.to_string())
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
