use crate::model::{ApiError, ThemeSummary};
use serde_json::Value;
use std::path::Path;
const RESERVED: [&str; 3] = ["default", "midnight", "paper"];
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
            theme["name"] = "明亮".into();
            theme["tokens"]["colors"] = serde_json::json!({
                "background":"#F8FAFC","surface":"#FFFFFF","text":"#0F172A","textMuted":"#475569","border":"#64748B","accent":"#1D4ED8","accentText":"#FFFFFF","success":"#166534","warning":"#854D0E","exhausted":"#9A3412","error":"#B91C1C","unknown":"#475569","stale":"#6B21A8"
            });
        }
        _ => return None,
    }
    Some(theme)
}
pub fn get(dir: &Path, id: &str) -> Result<Value, ApiError> {
    if let Some(value) = builtin(id) {
        return Ok(value);
    }
    crate::theme_package::installed(&dir.join("runtime"), id)?;
    Ok(builtin("default").unwrap())
}
pub fn list(_dir: &Path) -> Vec<ThemeSummary> {
    RESERVED
        .iter()
        .map(|id| ThemeSummary {
            id: (*id).into(),
            name: builtin(id).unwrap()["name"].as_str().unwrap().into(),
            built_in: true,
        })
        .collect()
}
