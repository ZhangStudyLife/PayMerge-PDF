//! Embedded runtime resources. The distributed program needs no adjacent files.
use crate::{Result, message};
use std::path::PathBuf;
#[cfg(windows)]
use std::{
    fs,
    io::{Read, Write},
};

pub fn user_data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("PAYMERGE_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|path| PathBuf::from(path).join("PayMerge-PDF"))
        .ok_or_else(|| message("无法确定用户数据目录"))
}

#[cfg(windows)]
fn decode(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    flate2::read::ZlibDecoder::new(bytes).read_to_end(&mut output)?;
    Ok(output)
}

#[cfg(windows)]
pub fn font_data() -> Vec<u8> {
    decode(include_bytes!("../.cache/bundle/font.bin")).expect("embedded font is valid")
}

#[cfg(windows)]
pub fn notices() -> Result<String> {
    String::from_utf8(decode(include_bytes!("../.cache/bundle/notices.bin"))?)
        .map_err(|error| message(error.to_string()))
}

#[cfg(windows)]
pub fn pdfium_directory() -> Result<PathBuf> {
    materialize_pdfium(&user_data_dir()?.join("runtime"))
}

#[cfg(windows)]
fn materialize_pdfium(root: &std::path::Path) -> Result<PathBuf> {
    use sha2::{Digest, Sha256};
    const EXPECTED: &str = include_str!("../.cache/bundle/pdfium.sha256");
    let directory = root.join(EXPECTED.trim());
    fs::create_dir_all(&directory)?;
    let target = directory.join("pdfium.dll");
    if target.is_file() && crate::update::hash_file(&target)? == EXPECTED.trim() {
        return Ok(directory);
    }
    let bytes = decode(include_bytes!("../.cache/bundle/pdfium.bin"))?;
    if format!("{:x}", Sha256::digest(&bytes)) != EXPECTED.trim() {
        return Err(message("内嵌 PDF 引擎校验失败"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    // Samba-created files need executable access; local caches keep their inherited user ACL.
    let canonical = temporary.path().canonicalize()?;
    if matches!(canonical.components().next(), Some(std::path::Component::Prefix(prefix))
        if matches!(prefix.kind(), std::path::Prefix::UNC(..) | std::path::Prefix::VerbatimUNC(..)))
    {
        crate::update::copy_program_access(&std::env::current_exe()?, temporary.path())?;
    }
    // The content-addressed cache can be shared by simultaneous instances.
    if target.is_file() && crate::update::hash_file(&target)? == EXPECTED.trim() {
        return Ok(directory);
    }
    temporary.persist(&target).map_err(|error| error.error)?;
    Ok(directory)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn embedded_pdfium_is_verified_and_reused_in_a_private_cache() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = materialize_pdfium(temporary.path()).unwrap();
        let library = directory.join("pdfium.dll");
        let modified = fs::metadata(&library).unwrap().modified().unwrap();
        assert_eq!(directory, materialize_pdfium(temporary.path()).unwrap());
        assert_eq!(
            modified,
            fs::metadata(&library).unwrap().modified().unwrap()
        );
        fs::write(&library, b"corrupted private cache").unwrap();
        materialize_pdfium(temporary.path()).unwrap();
        assert_eq!(
            crate::update::hash_file(&library).unwrap(),
            include_str!("../.cache/bundle/pdfium.sha256").trim()
        );
        let bindings = pdfium_render::prelude::Pdfium::bind_to_library(&library).unwrap();
        let engine = pdfium_render::prelude::Pdfium::new(bindings);
        assert!(engine.create_new_pdf().is_ok());
    }
}
