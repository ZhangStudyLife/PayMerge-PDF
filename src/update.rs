//! GitHub release checks and verified, staged application updates.
mod install;
#[cfg(windows)]
pub use install::copy_program_access;
pub use install::{apply_plan, finish_update, launch_installer};

use crate::{Result, message};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub const REPOSITORY: &str = "ZhangStudyLife/PayMerge-PDF";
pub const REPOSITORY_URL: &str = "https://github.com/ZhangStudyLife/PayMerge-PDF";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
pub const MAIN_EXE: &str = "ASC_InvoiceMergeTool.exe";
pub const UPDATER_EXE: &str = "PayMergeUpdater.exe";
pub const STAGE_PREFIX: &str = ".paymerge-update-";
const MAX_ARCHIVE_SIZE: u64 = 256 * 1024 * 1024;
const MAX_UNPACKED_SIZE: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReleaseInfo {
    pub version: String,
    pub notes: String,
    pub download_url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
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
    pub parent_pid: u32,
    pub restart: bool,
    pub files: Vec<PackageFile>,
}

#[derive(Debug)]
pub struct StagedUpdate {
    pub directory: tempfile::TempDir,
    pub version: String,
    pub app_dir: PathBuf,
    pub files: Vec<PackageFile>,
}

impl StagedUpdate {
    pub fn write_plan(&self, parent_pid: u32, restart: bool) -> Result<PathBuf> {
        let path = self.directory.path().join("plan.json");
        let plan = InstallPlan {
            version: self.version.clone(),
            app_dir: self.app_dir.clone(),
            parent_pid,
            restart,
            files: self.files.clone(),
        };
        fs::write(&path, serde_json::to_vec_pretty(&plan)?)?;
        Ok(path)
    }
}

pub fn asset_name(version: &str) -> String {
    format!("PayMerge-PDF-{version}-windows-x64.zip")
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
        .map_err(|e| message(format!("GitHub 版本号无效：{e}")))?;
    let current = Version::parse(current).map_err(|e| message(e.to_string()))?;
    if !version.pre.is_empty() || version <= current {
        return Ok(None);
    }
    let name = asset_name(&version.to_string());
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| message("新版尚未提供 Windows x64 安装包"))?;
    let expected = format!("{REPOSITORY_URL}/releases/download/{}/", release.tag_name);
    if !asset.browser_download_url.starts_with(&expected)
        || asset.browser_download_url != format!("{expected}{name}")
    {
        return Err(message("更新包地址与项目的 GitHub Releases 不一致"));
    }
    let sha256 = asset
        .digest
        .and_then(|digest| digest.strip_prefix("sha256:").map(str::to_owned))
        .filter(|hash| valid_hash(hash))
        .ok_or_else(|| message("GitHub 尚未提供更新包 SHA256，请稍后重试"))?;
    if asset.size == 0 || asset.size > MAX_ARCHIVE_SIZE {
        return Err(message("更新包大小无效"));
    }
    Ok(Some(ReleaseInfo {
        version: version.to_string(),
        notes: release.body.unwrap_or_default(),
        download_url: asset.browser_download_url,
        sha256,
        size: asset.size,
    }))
}

pub fn check_latest(current: &str) -> Result<Option<ReleaseInfo>> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let response = client(Duration::from_secs(25))
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .call();
    let mut response = match response {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(error) => return Err(message(format!("暂时无法检查 GitHub 更新：{error}"))),
    };
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(message("GitHub 更新响应过大"));
    }
    release_from_json(&bytes, current)
}

pub fn download_update(
    release: &ReleaseInfo,
    app_dir: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<StagedUpdate> {
    let app_dir = app_dir.canonicalize()?;
    let temporary = tempfile::Builder::new()
        .prefix(STAGE_PREFIX)
        .tempdir_in(&app_dir)?;
    let archive_path = temporary.path().join("package.zip");
    let mut response = client(Duration::from_secs(15 * 60))
        .get(&release.download_url)
        .call()
        .map_err(|e| message(format!("下载更新失败：{e}")))?;
    let mut output = fs::File::create(&archive_path)?;
    let mut hash = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];
    let mut reader = response.body_mut().as_reader();
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > release.size || bytes > MAX_ARCHIVE_SIZE {
            return Err(message("下载文件大小与 GitHub 记录不一致"));
        }
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        progress(bytes, release.size);
    }
    output.flush()?;
    drop(output);
    if bytes != release.size || format!("{:x}", hash.finalize()) != release.sha256 {
        return Err(message("更新包 SHA256 校验失败，已保留当前版本"));
    }
    stage_archive(&archive_path, &release.version, &app_dir, temporary)
}

pub fn stage_local_archive(archive: &Path, version: &str, app_dir: &Path) -> Result<StagedUpdate> {
    let app_dir = app_dir.canonicalize()?;
    let temporary = tempfile::Builder::new()
        .prefix(STAGE_PREFIX)
        .tempdir_in(&app_dir)?;
    stage_archive(archive, version, &app_dir, temporary)
}

fn stage_archive(
    archive: &Path,
    version: &str,
    app_dir: &Path,
    temporary: tempfile::TempDir,
) -> Result<StagedUpdate> {
    Version::parse(version).map_err(|e| message(e.to_string()))?;
    let prefix = format!("PayMerge-PDF-{version}-windows-x64/");
    let payload = temporary.path().join("payload");
    fs::create_dir(&payload)?;
    let mut archive = zip::ZipArchive::new(fs::File::open(archive)?)
        .map_err(|e| message(format!("更新包不是有效的 ZIP：{e}")))?;
    if archive.len() > 2048 {
        return Err(message("更新包文件数量异常"));
    }
    let mut unpacked = 0_u64;
    let mut seen = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| message(e.to_string()))?;
        let name = entry
            .name()
            .map_err(|e| message(e.to_string()))?
            .into_owned();
        let relative = name
            .strip_prefix(&prefix)
            .ok_or_else(|| message("更新包根目录与版本不符"))?;
        if relative.is_empty() || entry.is_dir() {
            continue;
        }
        safe_package_path(relative)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(message("更新包不允许包含符号链接"));
        }
        if !seen.insert(relative.to_lowercase()) {
            return Err(message("更新包包含重复文件名"));
        }
        unpacked = unpacked
            .checked_add(entry.size())
            .ok_or_else(|| message("更新包大小溢出"))?;
        if unpacked > MAX_UNPACKED_SIZE {
            return Err(message("更新包解压后大小异常"));
        }
        let target = payload.join(relative);
        fs::create_dir_all(target.parent().unwrap())?;
        let declared = entry.size();
        let copied = std::io::copy(
            &mut (&mut entry).take(declared + 1),
            &mut fs::File::create(&target)?,
        )?;
        if copied != declared {
            return Err(message("更新包文件长度无效"));
        }
    }
    let text = fs::read_to_string(payload.join("SHA256SUMS.txt"))?;
    let files = parse_manifest(&text)?;
    let listed: BTreeSet<_> = files
        .iter()
        .map(|f| f.path.to_lowercase())
        .chain(std::iter::once("sha256sums.txt".to_owned()))
        .collect();
    if listed != seen {
        return Err(message("更新包文件与 SHA256 清单不一致"));
    }
    for required in [MAIN_EXE, UPDATER_EXE, "pdfium.dll", "使用说明.md"] {
        if !files.iter().any(|f| f.path == required) {
            return Err(message(format!("更新包缺少 {required}")));
        }
    }
    verify_files(&payload, &files)?;
    let mut files = files;
    files.push(PackageFile {
        path: "SHA256SUMS.txt".into(),
        sha256: hash_file(&payload.join("SHA256SUMS.txt"))?,
    });
    Ok(StagedUpdate {
        directory: temporary,
        version: version.into(),
        app_dir: app_dir.to_owned(),
        files,
    })
}

pub fn safe_package_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains(['\\', ':', '\0'])
        || path.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' '])
        })
        || !Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(message("更新包包含不安全的文件路径"));
    }
    if ![
        MAIN_EXE,
        UPDATER_EXE,
        "pdfium.dll",
        "SHA256SUMS.txt",
        "使用说明.md",
    ]
    .contains(&path)
        && !path.starts_with("licenses/")
    {
        return Err(message(format!("更新包试图修改非程序文件：{path}")));
    }
    Ok(())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

fn parse_manifest(text: &str) -> Result<Vec<PackageFile>> {
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let (hash, path) = line
            .split_once("  ")
            .ok_or_else(|| message("更新包 SHA256 清单格式无效"))?;
        safe_package_path(path)?;
        if !valid_hash(hash) || path == "SHA256SUMS.txt" || !seen.insert(path.to_lowercase()) {
            return Err(message("更新包 SHA256 清单无效"));
        }
        files.push(PackageFile {
            path: path.into(),
            sha256: hash.to_ascii_lowercase(),
        });
    }
    Ok(files)
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn verify_files(root: &Path, files: &[PackageFile]) -> Result<()> {
    for file in files {
        safe_package_path(&file.path)?;
        ensure_plain_path(root, &file.path)?;
        if hash_file(&root.join(&file.path))? != file.sha256 {
            return Err(message(format!("更新文件校验失败：{}", file.path)));
        }
    }
    Ok(())
}

fn ensure_plain_path(root: &Path, relative: &str) -> Result<()> {
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        path.push(part.as_os_str());
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            #[cfg(windows)]
            let reparse = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let reparse = false;
            if metadata.file_type().is_symlink() || reparse {
                return Err(message("更新路径不允许包含符号链接或目录联接"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn release(version: &str) -> serde_json::Value {
        serde_json::json!({"tag_name":format!("v{version}"),"draft":false,"prerelease":false,"body":"新版说明", "assets":[{
            "name":asset_name(version), "browser_download_url":format!("{REPOSITORY_URL}/releases/download/v{version}/{}",asset_name(version)),
            "size":1024,"digest":format!("sha256:{}","a".repeat(64))
        }]})
    }

    #[test]
    fn latest_release_uses_semver_and_ignores_preview_versions() {
        assert!(
            release_from_json(&serde_json::to_vec(&release("7.10.0")).unwrap(), "7.9.0")
                .unwrap()
                .is_some()
        );
        assert!(
            release_from_json(&serde_json::to_vec(&release("7.1.0")).unwrap(), "7.1.0")
                .unwrap()
                .is_none()
        );
        assert!(
            release_from_json(&serde_json::to_vec(&release("7.0.0")).unwrap(), "7.1.0")
                .unwrap()
                .is_none()
        );
        assert!(
            release_from_json(
                &serde_json::to_vec(&release("7.2.0-beta.1")).unwrap(),
                "7.1.0"
            )
            .unwrap()
            .is_none()
        );
        let mut draft = release("7.2.0");
        draft["draft"] = true.into();
        assert!(
            release_from_json(&serde_json::to_vec(&draft).unwrap(), "7.1.0")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn update_requires_an_owned_release_asset_and_digest() {
        let mut data = release("7.2.0");
        data["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(release_from_json(&serde_json::to_vec(&data).unwrap(), "7.1.0").is_err());
        let mut data = release("7.2.0");
        data["assets"][0]["browser_download_url"] = "https://example.org/app.zip".into();
        assert!(release_from_json(&serde_json::to_vec(&data).unwrap(), "7.1.0").is_err());
    }

    #[test]
    fn package_paths_cannot_target_user_data_or_escape_the_installation() {
        for path in [
            "config.json",
            "发票.pdf",
            "../config.json",
            "licenses/../../config.json",
            "licenses\\bad",
            "licenses/x:bad",
            "licenses/name. ",
        ] {
            assert!(safe_package_path(path).is_err(), "{path}");
        }
        assert!(safe_package_path("licenses/example/LICENSE-MIT").is_ok());
    }

    fn archive(tamper: bool, extra: bool) -> Vec<u8> {
        let payload: Vec<(&str, &[u8])> = vec![
            (MAIN_EXE, b"application"),
            (UPDATER_EXE, b"updater"),
            ("pdfium.dll", b"pdfium"),
            ("使用说明.md", b"guide"),
        ];
        let mut manifest = String::new();
        for (path, bytes) in &payload {
            manifest.push_str(&format!("{:x}  {path}\n", Sha256::digest(bytes)));
        }
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (path, bytes) in payload {
            zip.start_file(format!("PayMerge-PDF-7.2.0-windows-x64/{path}"), options)
                .unwrap();
            zip.write_all(if tamper && path == MAIN_EXE {
                b"modified"
            } else {
                bytes
            })
            .unwrap();
        }
        zip.start_file("PayMerge-PDF-7.2.0-windows-x64/SHA256SUMS.txt", options)
            .unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        if extra {
            zip.start_file("PayMerge-PDF-7.2.0-windows-x64/config.json", options)
                .unwrap();
            zip.write_all(b"overwrite personal config").unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn complete_archive_is_verified_and_corruption_is_rejected() {
        let app = tempfile::tempdir().unwrap();
        let archive_path = app.path().join("release.zip");
        fs::write(&archive_path, archive(false, false)).unwrap();
        let staged = stage_local_archive(&archive_path, "7.2.0", app.path()).unwrap();
        assert_eq!(staged.files.len(), 5);
        drop(staged);
        fs::write(&archive_path, archive(true, false)).unwrap();
        assert!(stage_local_archive(&archive_path, "7.2.0", app.path()).is_err());
        fs::write(&archive_path, archive(false, true)).unwrap();
        assert!(stage_local_archive(&archive_path, "7.2.0", app.path()).is_err());
        assert_eq!(fs::read_dir(app.path()).unwrap().count(), 1);
    }
}
