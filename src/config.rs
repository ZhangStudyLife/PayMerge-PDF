use crate::{Result, message};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub name: String,
    pub group: String,
    pub output_path_absolute: String,
    pub output_path_relative: String,
}

impl Config {
    pub fn load_migrated(app_dir: &Path, data_dir: &Path) -> Result<Self> {
        if data_dir.join("config.json").is_file() {
            return Self::load(data_dir);
        }
        let config = Self::load(app_dir)?;
        if config.configured() {
            fs::create_dir_all(data_dir)?;
            config.save(data_dir)?;
        }
        Ok(config)
    }

    pub fn load(app_dir: &Path) -> Result<Self> {
        let path = app_dir.join("config.json");
        match fs::read(&path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err.into()),
        }
    }

    pub fn configured(&self) -> bool {
        !self.name.is_empty() && !self.group.is_empty()
    }

    pub fn set_output(&mut self, output: &str, app_dir: &Path) {
        if output.is_empty() {
            self.output_path_absolute.clear();
            self.output_path_relative.clear();
        } else {
            let output = Path::new(output);
            let absolute = if output.is_absolute() {
                output.to_path_buf()
            } else {
                app_dir.join(output)
            };
            self.output_path_absolute = absolute.to_string_lossy().into_owned();
            self.output_path_relative = pathdiff::diff_paths(absolute, app_dir)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
    }

    pub fn save(&self, app_dir: &Path) -> Result<()> {
        validate_personal(&self.name, "姓名")?;
        validate_personal(&self.group, "组别")?;
        let mut temp = tempfile::NamedTempFile::new_in(app_dir)?;
        temp.write_all(&serde_json::to_vec_pretty(self)?)?;
        temp.flush()?;
        temp.persist(app_dir.join("config.json"))
            .map_err(|e| e.error)?;
        Ok(())
    }

    pub fn output_directory(&self, app_dir: &Path) -> Result<PathBuf> {
        if self.output_path_absolute.is_empty() && self.output_path_relative.is_empty() {
            return Ok(app_dir.to_path_buf());
        }
        let absolute = Path::new(&self.output_path_absolute);
        if !self.output_path_absolute.is_empty() && absolute.is_dir() {
            return Ok(absolute.to_path_buf());
        }
        let relative = app_dir.join(&self.output_path_relative);
        if !self.output_path_relative.is_empty() && relative.is_dir() {
            return Ok(relative);
        }
        Err(message("配置的输出路径无效，请在“修改配置”中重新选择"))
    }
}

pub fn validate_personal(text: &str, label: &str) -> Result<()> {
    if text.is_empty()
        || !text.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '_' || ('\u{4e00}'..='\u{9fa5}').contains(&c)
        })
    {
        return Err(message(format!(
            "{label}只能包含中文、英文、数字和下划线，且不能为空"
        )));
    }
    Ok(())
}

pub fn numbered_path(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path.extension().unwrap_or_default().to_string_lossy();
    for index in 1.. {
        let candidate = path.with_file_name(format!("{stem}_{index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}
