use crate::{
    config,
    model::{ApiError, ThemeIssue as Issue, ThemeSummary, ThemeValidation as Validation},
};
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{Map, Value};
use std::{fs, path::Path};

const MAX_THEME_BYTES: usize = 64 * 1024;
const RESERVED: [&str; 3] = ["default", "midnight", "paper"];

struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Strict;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Strict, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Strict(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Strict, E> {
                Ok(Strict(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut values = vec![];
                while let Some(Strict(value)) = a.next_element()? {
                    values.push(value)
                }
                Ok(Strict(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut values = Map::new();
                while let Some((key, Strict(value))) = a.next_entry::<String, Strict>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom("duplicate key"));
                    }
                    values.insert(key, value);
                }
                Ok(Strict(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(V)
    }
}

pub fn parse(bytes: &[u8]) -> Result<Value, ApiError> {
    if bytes.len() > MAX_THEME_BYTES {
        return Err(ApiError::new("THEME_INVALID", "主题文件不能超过 64 KiB"));
    }
    let value: Strict = serde_json::from_slice(bytes)
        .map_err(|_| ApiError::new("THEME_INVALID", "主题 JSON 无效或含重复字段"))?;
    Ok(value.0)
}

fn depth(v: &Value) -> usize {
    match v {
        Value::Array(a) => 1 + a.iter().map(depth).max().unwrap_or(0),
        Value::Object(a) => 1 + a.values().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}
fn issue(code: &str, message: &str) -> Issue {
    Issue {
        path: "/".into(),
        code: code.into(),
        message: message.into(),
    }
}
pub fn validate(document: &Value, importing: bool) -> Validation {
    let mut issues = vec![];
    if serde_json::to_vec(document)
        .map(|v| v.len() > MAX_THEME_BYTES)
        .unwrap_or(true)
        || depth(document) > 16
    {
        issues.push(issue("SIZE_LIMIT", "主题文件过大或结构过深"));
    }
    let schema: Value = serde_json::from_str(include_str!("../../schemas/theme.schema.json"))
        .expect("bundled theme schema");
    match jsonschema::validator_for(&schema) {
        Ok(validator) => {
            if !validator.is_valid(document) {
                issues.push(issue(
                    "SCHEMA_INVALID",
                    "主题字段不符合 V1 格式，请检查颜色、数值、布局和必填字段",
                ))
            }
        }
        Err(_) => issues.push(issue("SCHEMA_UNAVAILABLE", "主题校验器暂不可用")),
    }
    if let Some(min) = document["minHudVersion"].as_str() {
        if let (Ok(required), Ok(current)) = (
            semver::Version::parse(min),
            semver::Version::parse(env!("CARGO_PKG_VERSION")),
        ) {
            if required > current {
                issues.push(issue("VERSION_UNSUPPORTED", "主题需要更新版本的 HUD"))
            }
        }
    }
    if importing
        && document["id"]
            .as_str()
            .is_some_and(|id| RESERVED.contains(&id))
    {
        issues.push(issue("RESERVED_ID", "该主题 ID 为内置主题保留"))
    }
    if issues.is_empty() {
        let colors = &document["tokens"]["colors"];
        for foreground in [
            "text",
            "textMuted",
            "success",
            "warning",
            "exhausted",
            "error",
            "unknown",
            "stale",
        ] {
            for background in ["background", "surface"] {
                if contrast(
                    colors[foreground].as_str().unwrap_or(""),
                    colors[background].as_str().unwrap_or(""),
                ) < 4.5
                {
                    issues.push(Issue {
                        path: format!("/tokens/colors/{foreground}"),
                        code: "CONTRAST_LOW".into(),
                        message: format!("{foreground} 与 {background} 的文字对比度不足 4.5:1"),
                    });
                }
            }
        }
        if contrast(
            colors["accentText"].as_str().unwrap_or(""),
            colors["accent"].as_str().unwrap_or(""),
        ) < 4.5
        {
            issues.push(issue("CONTRAST_LOW", "强调文字对比度不足"))
        }
        // Accent is decorative; controls and focus rings use the high-contrast
        // border/text tokens. A pastel brand color need not itself be body text.
        for foreground in ["border"] {
            for background in ["background", "surface"] {
                if contrast(
                    colors[foreground].as_str().unwrap_or(""),
                    colors[background].as_str().unwrap_or(""),
                ) < 3.0
                {
                    issues.push(issue("CONTRAST_LOW", "控件边界或焦点颜色对比度不足 3:1"))
                }
            }
        }
    }
    Validation {
        valid: issues.is_empty(),
        issues,
    }
}
fn luminance(color: &str) -> Option<f64> {
    if color.len() != 7 || !color.starts_with('#') {
        return None;
    }
    let mut channels = vec![];
    for i in [1, 3, 5] {
        let value = u8::from_str_radix(color.get(i..i + 2)?, 16).ok()? as f64 / 255.0;
        channels.push(if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        })
    }
    Some(channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722)
}
fn contrast(a: &str, b: &str) -> f64 {
    match (luminance(a), luminance(b)) {
        (Some(a), Some(b)) => (a.max(b) + 0.05) / (a.min(b) + 0.05),
        _ => 0.0,
    }
}

pub fn builtin(id: &str) -> Option<Value> {
    if id == "default" {
        return serde_json::from_str(include_str!("../../examples/themes/cream/theme.json")).ok();
    }
    let mut theme: Value =
        serde_json::from_str(include_str!("../../examples/themes/midnight/theme.json")).ok()?;
    match id {
        "midnight" => {}
        "default" => {
            theme["id"] = "default".into();
            theme["name"] = "夜航".into();
        }
        "paper" => {
            theme["id"] = "paper".into();
            theme["name"] = "晨光".into();
            theme["tokens"]["colors"] = serde_json::json!({
                "background":"#F8FAFC","surface":"#FFFFFF","text":"#0F172A","textMuted":"#475569","border":"#64748B","accent":"#1D4ED8","accentText":"#FFFFFF","success":"#166534","warning":"#854D0E","exhausted":"#9A3412","error":"#B91C1C","unknown":"#475569","stale":"#6B21A8"
            });
        }
        _ => return None,
    }
    Some(theme)
}
fn safe_id(id: &str) -> bool {
    id.len() <= 64
        && id.bytes().next().is_some_and(|x| x.is_ascii_lowercase())
        && id
            .bytes()
            .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == b'-')
}
pub fn get(dir: &Path, id: &str) -> Result<Value, ApiError> {
    if let Some(value) = builtin(id) {
        return Ok(value);
    }
    if !safe_id(id) {
        return Err(ApiError::new("THEME_INVALID", "主题 ID 无效"));
    }
    let path = dir.join(format!("theme-{id}.json"));
    let meta = fs::symlink_metadata(&path).map_err(|_| ApiError::new("NOT_FOUND", "找不到主题"))?;
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > MAX_THEME_BYTES as u64 {
        return Err(ApiError::new("THEME_INVALID", "主题文件无效"));
    }
    let value = parse(&fs::read(path).map_err(|_| ApiError::new("IO_ERROR", "无法读取主题"))?)?;
    if !validate(&value, true).valid || value["id"].as_str() != Some(id) {
        return Err(ApiError::new("THEME_INVALID", "已安装主题未通过校验"));
    }
    Ok(value)
}
pub fn list(dir: &Path) -> Vec<ThemeSummary> {
    let mut out = RESERVED
        .iter()
        .map(|id| ThemeSummary {
            id: (*id).into(),
            name: builtin(id).unwrap()["name"].as_str().unwrap().into(),
            built_in: true,
        })
        .collect::<Vec<_>>();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten().take(100) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(id) = name
                .strip_prefix("theme-")
                .and_then(|x| x.strip_suffix(".json"))
            {
                if let Ok(theme) = get(dir, id) {
                    out.push(ThemeSummary {
                        id: id.into(),
                        name: theme["name"].as_str().unwrap_or(id).into(),
                        built_in: false,
                    })
                }
            }
        }
    }
    out
}
pub fn import(dir: &Path, path: &Path) -> Result<ThemeSummary, ApiError> {
    let meta = fs::metadata(path).map_err(|_| ApiError::new("IO_ERROR", "无法读取所选主题"))?;
    if !meta.is_file() || meta.len() > MAX_THEME_BYTES as u64 {
        return Err(ApiError::new(
            "THEME_INVALID",
            "主题须为不超过 64 KiB 的 JSON 文件",
        ));
    }
    let document =
        parse(&fs::read(path).map_err(|_| ApiError::new("IO_ERROR", "无法读取所选主题"))?)?;
    let validation = validate(&document, true);
    if !validation.valid {
        return Err(ApiError::new(
            "THEME_INVALID",
            &validation.issues[0].message,
        ));
    }
    let id = document["id"]
        .as_str()
        .ok_or_else(|| ApiError::new("THEME_INVALID", "主题缺少 ID"))?;
    let target = dir.join(format!("theme-{id}.json"));
    if target.exists() {
        return Err(ApiError::new(
            "CONFLICT",
            "该主题 ID 已存在，请修改 ID 后导入",
        ));
    }
    if list(dir).len() >= 103 {
        return Err(ApiError::new("THEME_INVALID", "已达到主题数量上限"));
    }
    config::atomic_json(&target, &document)?;
    Ok(ThemeSummary {
        id: id.into(),
        name: document["name"].as_str().unwrap_or(id).into(),
        built_in: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtins_pass_schema_and_contrast() {
        for id in RESERVED {
            let result = validate(&builtin(id).unwrap(), false);
            assert!(result.valid, "{id}");
        }
    }
    #[test]
    fn scripts_low_contrast_and_traversal_are_rejected() {
        let mut theme = builtin("midnight").unwrap();
        theme["script"] = "alert(1)".into();
        assert!(!validate(&theme, false).valid);
        let mut theme = builtin("midnight").unwrap();
        theme["tokens"]["colors"]["text"] = "#111827".into();
        assert!(!validate(&theme, false).valid);
        let dir = tempfile::tempdir().unwrap();
        assert!(get(dir.path(), "../secret").is_err());
        assert!(!safe_id("valid\n"));
    }
    #[test]
    fn duplicate_keys_are_rejected() {
        assert!(parse(br#"{"id":"first","id":"last"}"#).is_err());
    }
    #[test]
    fn import_preserves_source_and_installs_valid_copy() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.json");
        let output = dir.path().join("themes");
        let mut theme = builtin("midnight").unwrap();
        theme["id"] = "my-theme".into();
        let bytes = serde_json::to_vec(&theme).unwrap();
        fs::write(&input, &bytes).unwrap();
        import(&output, &input).unwrap();
        assert_eq!(fs::read(&input).unwrap(), bytes);
        assert_eq!(get(&output, "my-theme").unwrap(), theme);
        assert!(import(&output, &input).is_err());
    }
}
