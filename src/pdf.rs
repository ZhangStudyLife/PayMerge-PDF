use crate::{Result, message};
use pdfium_render::prelude::*;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub fn engine() -> Result<&'static Pdfium> {
    static PDFIUM: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();
    PDFIUM
        .get_or_init(|| {
            let directory = std::env::var_os("PAYMERGE_PDFIUM_DIR")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(Path::to_path_buf))
                })
                .ok_or_else(|| "无法确定 PDF 引擎所在目录".to_owned())?;
            let library = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
                &directory,
            ))
            .map_err(|e| format!("无法加载 PDF 引擎，请保留程序旁的 pdfium.dll。详情：{e}"))?;
            Ok(Pdfium::new(library))
        })
        .as_ref()
        .map_err(|error| message(error.clone()))
}
