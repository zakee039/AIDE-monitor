use crate::{config, model::ApiError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const BUILTINS: [&str; 3] = ["default", "midnight", "paper"];
const FILE_LIMIT: u64 = 20 * 1024 * 1024;
const TOTAL_LIMIT: u64 = 80 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub api_version: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Installed {
    pub manifest: Manifest,
    pub generation: String,
}
pub fn invalid(message: &str) -> ApiError {
    ApiError::new("THEME_INVALID", message)
}
fn io(_: std::io::Error) -> ApiError {
    ApiError::new("IO_ERROR", "无法读写主题文件")
}
pub fn safe_id(id: &str) -> bool {
    id.len() <= 100
        && id.bytes().next().is_some_and(|c| c.is_ascii_lowercase())
        && !id.ends_with('.')
        && !id.contains("..")
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b".-".contains(&c))
        && !BUILTINS.contains(&id)
}
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 180
        && !path.contains(['\\', ':', '\0', '%', '?', '#'])
        && path.is_ascii()
        && path.split('/').all(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part.chars().any(|c| c.is_control() || "<>\"|*".contains(c))
                && ![
                    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                    "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
                    "LPT8", "LPT9",
                ]
                .contains(&stem.as_str())
        })
}
impl Manifest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.schema_version != 1
            || self.api_version != "1"
            || !safe_id(&self.id)
            || self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self
                .author
                .as_ref()
                .is_some_and(|a| a.chars().count() > 100)
            || semver::Version::parse(&self.version).is_err()
        {
            return Err(invalid("主题清单格式或版本不受支持"));
        }
        Ok(())
    }
}
pub fn installed(root: &Path, id: &str) -> Result<Installed, ApiError> {
    if !safe_id(id) {
        return Err(invalid("主题 ID 无效"));
    }
    let bytes = fs::read(root.join("registry").join(format!("{id}.json"))).map_err(io)?;
    let record: Installed = serde_json::from_slice(&bytes).map_err(|_| invalid("主题记录损坏"))?;
    record.manifest.validate()?;
    if record.manifest.id != id || uuid::Uuid::parse_str(&record.generation).is_err() {
        return Err(invalid("主题记录无效"));
    }
    Ok(record)
}
pub fn content(root: &Path, record: &Installed) -> PathBuf {
    root.join("packages").join(&record.generation)
}
pub fn list(root: &Path) -> Vec<Installed> {
    fs::read_dir(root.join("registry"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|id| id.to_str())
                .and_then(|id| installed(root, id).ok())
        })
        .take(100)
        .collect()
}
// Immutable generations plus an atomically replaced registry pointer: an interrupted
// install can leave an unused generation, but never a partially installed active one.
pub fn install(
    root: &Path,
    source: &Path,
    replace: bool,
    clear_storage: bool,
) -> Result<Installed, ApiError> {
    if source.extension().and_then(|e| e.to_str()) != Some("aidetheme") {
        return Err(invalid("请选择 .aidetheme 主题包"));
    }
    let file = fs::File::open(source).map_err(io)?;
    if file.metadata().map_err(io)?.len() > FILE_LIMIT {
        return Err(invalid("压缩包超过 20 MiB"));
    }
    let mut archive = zip::ZipArchive::new(file).map_err(|_| invalid("主题包不是有效 ZIP"))?;
    if archive.len() > 2000 {
        return Err(invalid("主题包文件数量超限"));
    }
    fs::create_dir_all(root.join("packages")).map_err(io)?;
    let stage = tempfile::tempdir_in(root.join("packages")).map_err(io)?;
    let mut seen = HashSet::new();
    let mut total = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| invalid("无法读取压缩条目"))?;
        let path = entry.name().trim_end_matches('/').to_string();
        if !safe_path(&path)
            || !seen.insert(path.to_ascii_lowercase())
            || entry.unix_mode().is_some_and(|m| {
                m & 0o170000 != 0 && m & 0o170000 != 0o100000 && m & 0o170000 != 0o040000
            })
        {
            return Err(invalid("主题包存在不安全或重复路径"));
        }
        let target = stage.path().join(&path);
        if entry.is_dir() {
            fs::create_dir_all(target).map_err(io)?;
            continue;
        }
        if entry.size() > FILE_LIMIT {
            return Err(invalid("单文件超过 20 MiB"));
        }
        fs::create_dir_all(target.parent().unwrap()).map_err(io)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(io)?;
        let count =
            std::io::copy(&mut (&mut entry).take(FILE_LIMIT + 1), &mut output).map_err(io)?;
        total += count;
        if count > FILE_LIMIT || total > TOTAL_LIMIT {
            return Err(invalid("解压体积超限"));
        }
        output.flush().map_err(io)?;
    }
    let bytes = fs::read(stage.path().join("manifest.json")).map_err(io)?;
    if bytes.len() > 65536 {
        return Err(invalid("manifest 过大"));
    }
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| invalid("manifest 无效"))?;
    manifest.validate()?;
    if !stage.path().join("index.html").is_file() {
        return Err(invalid("缺少根目录 index.html"));
    }
    let previous = installed(root, &manifest.id)
        .ok()
        .or_else(|| retained(root, &manifest.id));
    let exists = previous.is_some();
    if exists && !replace {
        return Err(ApiError::new(
            "CONFLICT",
            &format!(
                "{} ({}) {}\n将替换已有主题或使用其保留偏好。",
                manifest.name, manifest.id, manifest.version
            ),
        ));
    }
    if installed(root, &manifest.id).is_err() && list(root).len() >= 100 {
        return Err(invalid("最多安装 100 个主题"));
    }
    let record = Installed {
        manifest,
        generation: uuid::Uuid::new_v4().to_string(),
    };
    fs::rename(stage.path(), content(root, &record)).map_err(io)?;
    // Storage is tied to an installation generation. Explicitly preserving preferences
    // copies only this theme's JSON into the new generation before committing the pointer.
    if exists && !clear_storage {
        let old = previous.as_ref().unwrap();
        let path = root
            .join("storage")
            .join(format!("{}.json", old.generation));
        if path.exists() {
            let data = fs::read(path).map_err(io)?;
            if data.len() <= 1024 * 1024 {
                let value: Value =
                    serde_json::from_slice(&data).map_err(|_| invalid("主题偏好损坏"))?;
                config::atomic_json(
                    &root
                        .join("storage")
                        .join(format!("{}.json", record.generation)),
                    &value,
                )?;
            }
        }
    }
    config::atomic_json(
        &root
            .join("registry")
            .join(format!("{}.json", record.manifest.id)),
        &record,
    )?;
    Ok(record)
}
fn retained(root: &Path, id: &str) -> Option<Installed> {
    if !safe_id(id) {
        return None;
    }
    let bytes = fs::read(root.join("retained").join(format!("{id}.json"))).ok()?;
    let record: Installed = serde_json::from_slice(&bytes).ok()?;
    if record.manifest.id != id
        || record.manifest.validate().is_err()
        || uuid::Uuid::parse_str(&record.generation).is_err()
    {
        return None;
    }
    Some(record)
}
pub fn uninstall(root: &Path, id: &str, clear: bool) -> Result<(), ApiError> {
    let record = installed(root, id)?;
    if !clear {
        config::atomic_json(&root.join("retained").join(format!("{id}.json")), &record)?;
    }
    fs::remove_file(root.join("registry").join(format!("{id}.json"))).map_err(io)?;
    if clear {
        let path = root
            .join("storage")
            .join(format!("{}.json", record.generation));
        if path.exists() {
            fs::remove_file(path).map_err(io)?;
        }
        let pointer = root.join("retained").join(format!("{id}.json"));
        if pointer.exists() {
            fs::remove_file(pointer).map_err(io)?;
        }
    }
    remove_generation(root, &record.generation)?;
    Ok(())
}
fn remove_generation(root: &Path, generation: &str) -> Result<(), ApiError> {
    if uuid::Uuid::parse_str(generation).is_err() {
        return Err(invalid("安装标识无效"));
    }
    let base = root.join("packages");
    let target = base.join(generation);
    if !target.exists() {
        return Ok(());
    }
    let resolved = target.canonicalize().map_err(io)?;
    let base = base.canonicalize().map_err(io)?;
    if resolved.parent() != Some(base.as_path())
        || fs::symlink_metadata(&target)
            .map_err(io)?
            .file_type()
            .is_symlink()
    {
        return Err(invalid("安装目录异常"));
    }
    fs::remove_dir_all(target).map_err(io)
}
/// Only call before theme sessions start: no running WebView may refer to an orphan.
pub fn collect_orphans(root: &Path) -> Result<(), ApiError> {
    let keep: HashSet<_> = list(root).into_iter().map(|r| r.generation).collect();
    for entry in fs::read_dir(root.join("packages"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if uuid::Uuid::parse_str(&name).is_ok() && !keep.contains(&name) {
            remove_generation(root, &name)?;
        }
    }
    Ok(())
}
pub fn storage(
    root: &Path,
    record: &Installed,
    method: &str,
    key: &str,
    value: Option<Value>,
) -> Result<Value, ApiError> {
    if key.is_empty() || key.len() > 128 {
        return Err(invalid("偏好键长度无效"));
    }
    let path = root
        .join("storage")
        .join(format!("{}.json", record.generation));
    let mut map: serde_json::Map<String, Value> = if path.exists() {
        let bytes = fs::read(&path).map_err(io)?;
        if bytes.len() > 1024 * 1024 {
            return Err(invalid("主题偏好超过配额"));
        }
        serde_json::from_slice(&bytes).map_err(|_| invalid("主题偏好损坏"))?
    } else {
        Default::default()
    };
    match method {
        "storage.get" => return Ok(map.get(key).cloned().unwrap_or(Value::Null)),
        "storage.set" => {
            let value = value.ok_or_else(|| invalid("缺少 value"))?;
            if serde_json::to_vec(&value).unwrap().len() > 65536 {
                return Err(invalid("单值超过 64 KiB"));
            }
            map.insert(key.into(), value);
        }
        "storage.remove" => {
            map.remove(key);
        }
        _ => return Err(invalid("未知存储方法")),
    }
    if serde_json::to_vec_pretty(&map).unwrap().len() > 1024 * 1024 {
        return Err(invalid("主题偏好超过 1 MiB"));
    }
    config::atomic_json(&path, &map)?;
    Ok(Value::Null)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn archive(root: &Path, entries: Vec<(&str, Vec<u8>)>) -> PathBuf {
        let path = root.join("test.aidetheme");
        let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (name, bytes) in entries {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
        path
    }
    fn manifest() -> Vec<u8> {
        br#"{"schemaVersion":1,"id":"dev.test.theme","name":"Test","version":"1.0.0","apiVersion":"1"}"#.to_vec()
    }
    #[test]
    fn paths_reject_windows_escape_and_aliases() {
        for path in [
            "../x",
            "/x",
            "C:/x",
            "a\\x",
            "a:stream",
            "a/CON.txt",
            "a/file.",
            "a/file ",
            "a//b",
            "a/%2e%2e/b",
            "a/../../b",
            "a/LPT1",
            "x?y",
        ] {
            assert!(!safe_path(path), "{path}");
        }
        assert!(safe_path("assets/app.js"));
    }
    #[test]
    fn install_replace_storage_and_uninstall_are_isolated() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runtime");
        let path = archive(
            temp.path(),
            vec![
                ("manifest.json", manifest()),
                ("index.html", b"<html>test</html>".to_vec()),
            ],
        );
        let first = install(&root, &path, false, true).unwrap();
        storage(
            &root,
            &first,
            "storage.set",
            "mode",
            Some(Value::Bool(true)),
        )
        .unwrap();
        assert!(install(&root, &path, false, true).is_err());
        let second = install(&root, &path, true, false).unwrap();
        assert_ne!(first.generation, second.generation);
        assert_eq!(
            storage(&root, &second, "storage.get", "mode", None).unwrap(),
            Value::Bool(true)
        );
        let third = install(&root, &path, true, true).unwrap();
        assert_eq!(
            storage(&root, &third, "storage.get", "mode", None).unwrap(),
            Value::Null
        );
        assert!(storage(
            &root,
            &third,
            "storage.set",
            "large",
            Some(Value::String("x".repeat(65537)))
        )
        .is_err());
        uninstall(&root, "dev.test.theme", true).unwrap();
        assert!(installed(&root, "dev.test.theme").is_err());
    }
    #[test]
    fn failed_install_preserves_pointer() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runtime");
        let path = archive(
            temp.path(),
            vec![("manifest.json", manifest()), ("index.html", vec![])],
        );
        let first = install(&root, &path, false, true).unwrap();
        for bad in ["../escape", "assets/CON.txt", "assets/a:stream"] {
            let path = archive(
                temp.path(),
                vec![
                    ("manifest.json", manifest()),
                    ("index.html", vec![]),
                    (bad, vec![]),
                ],
            );
            assert!(install(&root, &path, true, true).is_err());
            assert_eq!(
                installed(&root, "dev.test.theme").unwrap().generation,
                first.generation
            );
        }
    }
    #[test]
    fn retained_preferences_require_consent_and_remain_isolated() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runtime");
        let path = archive(
            temp.path(),
            vec![("manifest.json", manifest()), ("index.html", vec![])],
        );
        let first = install(&root, &path, false, true).unwrap();
        storage(&root, &first, "storage.set", "saved", Some(json!(42))).unwrap();
        uninstall(&root, &first.manifest.id, false).unwrap();
        assert_eq!(
            install(&root, &path, false, false).unwrap_err().code,
            "CONFLICT"
        );
        let second = install(&root, &path, true, false).unwrap();
        assert_eq!(
            storage(&root, &second, "storage.get", "saved", None).unwrap(),
            json!(42)
        );
        collect_orphans(&root).unwrap();
        assert!(content(&root, &second).is_dir());
        uninstall(&root, &second.manifest.id, true).unwrap();
        let third = install(&root, &path, false, true).unwrap();
        assert_eq!(
            storage(&root, &third, "storage.get", "saved", None).unwrap(),
            Value::Null
        );
    }
    #[test]
    fn rejects_symlinks_and_oversized_expansion() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runtime");
        let path = temp.path().join("link.aidetheme");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        zip.add_symlink(
            "index.html",
            "../outside",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.finish().unwrap();
        assert!(install(&root, &path, false, true).is_err());
        let path = archive(
            temp.path(),
            vec![
                ("manifest.json", manifest()),
                ("index.html", vec![b' '; FILE_LIMIT as usize + 1]),
            ],
        );
        assert!(install(&root, &path, false, true).is_err());
    }
    #[test]
    fn reject_case_collisions_missing_entry_and_future_api() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runtime");
        let path = archive(
            temp.path(),
            vec![
                ("manifest.json", manifest()),
                ("index.html", vec![]),
                ("A.js", vec![]),
                ("a.js", vec![]),
            ],
        );
        assert!(install(&root, &path, false, true).is_err());
        let path = archive(temp.path(), vec![("manifest.json", manifest())]);
        assert!(install(&root, &path, false, true).is_err());
        let mut m: Manifest = serde_json::from_slice(&manifest()).unwrap();
        m.api_version = "2".into();
        assert!(m.validate().is_err());
        m.api_version = "1".into();
        m.id = "default".into();
        assert!(m.validate().is_err());
    }
}
