//! Verified single-executable GitHub updates.
mod install;
use crate::{Result, message};
#[cfg(windows)]
pub use install::copy_program_access;
pub use install::{apply_plan, finish_update, launch_installer};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub const REPOSITORY: &str = "ZhangStudyLife/PayMerge-PDF";
pub const REPOSITORY_URL: &str = "https://github.com/ZhangStudyLife/PayMerge-PDF";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
pub const MAIN_EXE: &str = "PayMerge-PDF.exe";
pub const STAGE_PREFIX: &str = ".paymerge-update-";
const MAX_DOWNLOAD_SIZE: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReleaseInfo {
    pub version: String,
    pub notes: String,
    pub download_url: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
    size: u64,
}
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<GithubAsset>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PackageFile {
    pub path: String,
    pub sha256: String,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct InstallPlan {
    pub version: String,
    pub app_dir: PathBuf,
    pub executable_name: String,
    pub parent_pid: u32,
    pub restart: bool,
    pub files: Vec<PackageFile>,
}
#[derive(Debug)]
pub struct StagedUpdate {
    pub directory: tempfile::TempDir,
    pub version: String,
    pub app_dir: PathBuf,
    pub executable_name: String,
    pub files: Vec<PackageFile>,
}
impl StagedUpdate {
    pub fn write_plan(&self, parent_pid: u32, restart: bool) -> Result<PathBuf> {
        let path = self.directory.path().join("plan.json");
        let plan = InstallPlan {
            version: self.version.clone(),
            app_dir: self.app_dir.clone(),
            executable_name: self.executable_name.clone(),
            parent_pid,
            restart,
            files: self.files.clone(),
        };
        fs::write(&path, serde_json::to_vec_pretty(&plan)?)?;
        Ok(path)
    }
}
pub fn asset_name(_version: &str) -> String {
    MAIN_EXE.into()
}
fn client(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .timeout_connect(Some(Duration::from_secs(15)))
        .user_agent(format!("PayMerge-PDF/{CURRENT_VERSION}"))
        .build()
        .into()
}
pub fn release_from_json(json: &[u8], current: &str) -> Result<Option<ReleaseInfo>> {
    let release: GithubRelease = serde_json::from_slice(json)?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(release.tag_name.trim_start_matches('v'))
        .map_err(|error| message(error.to_string()))?;
    let current = Version::parse(current).map_err(|error| message(error.to_string()))?;
    if !version.pre.is_empty() || version <= current {
        return Ok(None);
    }
    let name = asset_name(&version.to_string());
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| message("新版尚未提供单文件 EXE"))?;
    let expected = format!(
        "{REPOSITORY_URL}/releases/download/{}/{name}",
        release.tag_name
    );
    if asset.browser_download_url != expected {
        return Err(message("更新地址与本仓库不一致"));
    }
    let sha256 = asset
        .digest
        .and_then(|digest| digest.strip_prefix("sha256:").map(str::to_owned))
        .filter(|hash| valid_hash(hash))
        .ok_or_else(|| message("更新摘要暂不可用，请稍后重试"))?;
    if asset.size == 0 || asset.size > MAX_DOWNLOAD_SIZE {
        return Err(message("更新文件大小无效"));
    }
    Ok(Some(ReleaseInfo {
        version: version.to_string(),
        notes: release.body.unwrap_or_default(),
        download_url: asset.browser_download_url,
        sha256: sha256.to_lowercase(),
        size: asset.size,
    }))
}
pub fn check_latest(current: &str) -> Result<Option<ReleaseInfo>> {
    let response = client(Duration::from_secs(25))
        .get(&format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .call();
    let mut response = match response {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(error) => return Err(message(format!("无法检查更新：{error}"))),
    };
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(message("更新响应过大"));
    }
    release_from_json(&bytes, current)
}
fn new_stage(version: &str, executable: &Path) -> Result<StagedUpdate> {
    Version::parse(version).map_err(|error| message(error.to_string()))?;
    let executable = executable.canonicalize()?;
    let app_dir = executable
        .parent()
        .ok_or_else(|| message("程序目录无效"))?
        .to_owned();
    let executable_name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| message("程序文件名无效"))?
        .to_owned();
    safe_executable_name(&executable_name)?;
    let directory = tempfile::Builder::new()
        .prefix(STAGE_PREFIX)
        .tempdir_in(&app_dir)?;
    fs::create_dir(directory.path().join("payload"))?;
    Ok(StagedUpdate {
        directory,
        version: version.into(),
        app_dir,
        executable_name,
        files: Vec::new(),
    })
}
fn finish_stage(mut staged: StagedUpdate, sha256: String) -> Result<StagedUpdate> {
    let path = staged.directory.path().join("payload").join(MAIN_EXE);
    let mut signature = [0_u8; 2];
    fs::File::open(&path)?.read_exact(&mut signature)?;
    if signature != *b"MZ" {
        return Err(message("下载内容不是 Windows 程序"));
    }
    staged.files.push(PackageFile {
        path: MAIN_EXE.into(),
        sha256,
    });
    verify_files(&staged.directory.path().join("payload"), &staged.files)?;
    Ok(staged)
}
pub fn download_update(
    release: &ReleaseInfo,
    executable: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<StagedUpdate> {
    let staged = new_stage(&release.version, executable)?;
    let path = staged.directory.path().join("payload").join(MAIN_EXE);
    let mut response = client(Duration::from_secs(15 * 60))
        .get(&release.download_url)
        .call()
        .map_err(|error| message(format!("下载失败：{error}")))?;
    let mut output = fs::File::create(&path)?;
    let mut reader = response.body_mut().as_reader();
    let mut hash = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > release.size || bytes > MAX_DOWNLOAD_SIZE {
            return Err(message("下载长度与发布记录不一致"));
        }
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        progress(bytes, release.size);
    }
    output.flush()?;
    drop(output);
    let digest = format!("{:x}", hash.finalize());
    if bytes != release.size || digest != release.sha256 {
        return Err(message("SHA256 校验失败，当前版本未更改"));
    }
    finish_stage(staged, digest)
}
pub fn stage_local_executable(
    source: &Path,
    version: &str,
    installed: &Path,
) -> Result<StagedUpdate> {
    let staged = new_stage(version, installed)?;
    if fs::metadata(source)?.len() > MAX_DOWNLOAD_SIZE {
        return Err(message("程序文件过大"));
    }
    fs::copy(
        source,
        staged.directory.path().join("payload").join(MAIN_EXE),
    )?;
    finish_stage(staged, hash_file(source)?)
}
fn safe_executable_name(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains(['\\', '/', ':', '\0'])
        || path.ends_with(['.', ' '])
        || !path.to_ascii_lowercase().ends_with(".exe")
        || !matches!(
            Path::new(path).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
    {
        return Err(message("升级目标文件名无效"));
    }
    Ok(())
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}
pub fn hash_file(path: &Path) -> Result<String> {
    let mut source = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn verify_files(root: &Path, files: &[PackageFile]) -> Result<()> {
    if files.len() != 1 || files[0].path != MAIN_EXE || !valid_hash(&files[0].sha256) {
        return Err(message("升级内容只能包含主程序"));
    }
    ensure_plain_path(root, MAIN_EXE)?;
    if hash_file(&root.join(MAIN_EXE))? != files[0].sha256 {
        return Err(message("升级程序校验失败"));
    }
    Ok(())
}
fn ensure_plain_path(root: &Path, relative: &str) -> Result<()> {
    let path = root.join(relative);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let reparse = false;
        if metadata.file_type().is_symlink() || reparse {
            return Err(message("升级路径不允许符号链接或目录联接"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(version: &str) -> serde_json::Value {
        serde_json::json!({"tag_name":format!("v{version}"),"body":"更新","assets":[{"name":MAIN_EXE,"browser_download_url":format!("{REPOSITORY_URL}/releases/download/v{version}/{MAIN_EXE}"),"size":4096,"digest":format!("sha256:{}","a".repeat(64))}]})
    }
    #[test]
    fn release_checks_version_and_requires_owned_single_executable_and_digest() {
        assert!(
            release_from_json(&serde_json::to_vec(&release("7.10.0")).unwrap(), "7.9.0")
                .unwrap()
                .is_some()
        );
        assert!(
            release_from_json(&serde_json::to_vec(&release("7.2.0")).unwrap(), "7.2.0")
                .unwrap()
                .is_none()
        );
        assert!(
            release_from_json(
                &serde_json::to_vec(&release("7.3.0-beta.1")).unwrap(),
                "7.2.0"
            )
            .unwrap()
            .is_none()
        );
        let mut data = release("7.3.0");
        data["draft"] = true.into();
        assert!(
            release_from_json(&serde_json::to_vec(&data).unwrap(), "7.2.0")
                .unwrap()
                .is_none()
        );
        let mut data = release("7.3.0");
        data["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(release_from_json(&serde_json::to_vec(&data).unwrap(), "7.2.0").is_err());
        data["assets"][0]["digest"] = format!("sha256:{}", "a".repeat(64)).into();
        data["assets"][0]["browser_download_url"] = "https://example.org/app.exe".into();
        assert!(release_from_json(&serde_json::to_vec(&data).unwrap(), "7.2.0").is_err());
    }
    #[test]
    fn single_executable_staging_validates_pe_and_preserves_renamed_destination() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("合票 工具.exe");
        fs::write(&installed, b"old executable").unwrap();
        let new = root.path().join("download.exe");
        fs::write(&new, b"MZ new executable").unwrap();
        let staged = stage_local_executable(&new, "7.3.0", &installed).unwrap();
        assert_eq!(staged.executable_name, "合票 工具.exe");
        assert_eq!(staged.files.len(), 1);
        drop(staged);
        fs::write(&new, b"not an executable").unwrap();
        assert!(stage_local_executable(&new, "7.3.0", &installed).is_err());
        for name in [
            "../config.exe",
            "C:bad.exe",
            "x\\bad.exe",
            "output.pdf",
            "..",
        ] {
            assert!(safe_executable_name(name).is_err());
        }
        assert_eq!(fs::read(&installed).unwrap(), b"old executable");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }
}
