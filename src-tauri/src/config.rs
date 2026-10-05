use crate::model::{ApiError, Settings};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub config_version: u32,
    pub settings: Settings,
    pub source_path: Option<PathBuf>,
    pub id_map: HashMap<String, String>,
    pub selected_ids: Vec<String>,
    pub aliases: HashMap<String, String>,
    pub window_position: Option<(i32, i32)>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: 1,
            settings: Settings::default(),
            source_path: None,
            id_map: HashMap::new(),
            selected_ids: vec![],
            aliases: HashMap::new(),
            window_position: None,
        }
    }
}

pub fn load(path: &Path) -> Result<Config, ApiError> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let bytes = fs::read(path).map_err(|_| ApiError::new("IO_ERROR", "无法读取 HUD 设置"))?;
    if bytes.len() > 1024 * 1024 {
        return Err(ApiError::new("IO_ERROR", "HUD 设置文件过大"));
    }
    let config: Config = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::new("IO_ERROR", "HUD 设置损坏，请先保留原文件并重新设置"))?;
    if config.config_version != 1
        || !(60..=1800).contains(&config.settings.refresh_interval_seconds)
        || config.selected_ids.len() > 100
    {
        return Err(ApiError::new(
            "VERSION_UNSUPPORTED",
            "HUD 设置版本或数值不受支持",
        ));
    }
    Ok(config)
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), ApiError> {
    let parent = path
        .parent()
        .ok_or_else(|| ApiError::new("IO_ERROR", "设置位置无效"))?;
    fs::create_dir_all(parent).map_err(|_| ApiError::new("IO_ERROR", "无法创建 HUD 数据目录"))?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法保存设置"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| ApiError::new("IO_ERROR", "无法准备设置文件"))?;
    file.write_all(&bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| ApiError::new("IO_ERROR", "无法写入 HUD 设置"))?;
    file.persist(path)
        .map_err(|_| ApiError::new("IO_ERROR", "无法提交 HUD 设置"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_round_trip_and_overwrite_are_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut config = Config::default();
        atomic_json(&path, &config).unwrap();
        config.settings.refresh_interval_seconds = 120;
        atomic_json(&path, &config).unwrap();
        assert_eq!(load(&path).unwrap().settings.refresh_interval_seconds, 120);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn corrupt_config_is_not_silently_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"invalid").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"invalid");
    }
}
