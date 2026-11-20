use crate::{Result, message};
use pdfium_render::prelude::*;
#[cfg(not(windows))]
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn engine() -> Result<&'static Pdfium> {
    static PDFIUM: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();
    PDFIUM
        .get_or_init(|| {
            #[cfg(windows)]
            let directory = crate::bundle::pdfium_directory().map_err(|error| error.to_string())?;
            #[cfg(not(windows))]
            let directory = std::env::var_os("PAYMERGE_PDFIUM_DIR")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(Path::to_path_buf))
                })
                .ok_or_else(|| "无法确定 PDF 引擎所在目录".to_owned())?;
            let library =
                Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&directory))
                    .map_err(|e| format!("无法加载 PDF 引擎：{e}"))?;
            Ok(Pdfium::new(library))
        })
        .as_ref()
        .map_err(|error| message(error.clone()))
}
