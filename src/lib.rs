pub mod bundle;
pub mod config;
pub mod extract;
pub mod merge;
pub mod model;
pub mod pdf;
pub mod update;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("无法处理 PDF：{0}")]
    Pdf(#[from] pdfium_render::prelude::PdfiumError),
    #[error("无法读取图片：{0}")]
    Image(#[from] image::ImageError),
    #[error("配置文件格式错误：{0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn message(text: impl Into<String>) -> Error {
    Error::Message(text.into())
}
