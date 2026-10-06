//! Read-only source routing. Official Codex auth files and Cockpit catalogs stay separate.
use super::cockpit::{self, Credentials, SourceAccount, SourceCatalog};
use crate::model::ApiError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub fn discover_root() -> Option<PathBuf> {
    let official = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|p| PathBuf::from(p).join(".codex"))
        });
    official
        .filter(|p| p.is_absolute() && p.join("auth.json").is_file())
        .or_else(cockpit::discover_root)
}

fn is_official(root: &Path) -> bool {
    root.join("auth.json").is_file() && !root.join("codex_accounts.json").is_file()
}

fn jwt_claims(token: Option<&str>) -> Option<Value> {
    let payload = token?.split('.').nth(1)?;
    if payload.len() > 65536 {
        return None;
    }
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?).ok()
}

// JWT claims only identify the local saved login; the remote service authenticates the token.
fn official(root: &Path) -> Result<Option<(SourceAccount, Credentials)>, ApiError> {
    let path = root.join("auth.json");
    let canonical_root = root.canonicalize().map_err(|_| {
        ApiError::new(
            "SOURCE_NOT_FOUND",
            "Official account directory is unavailable",
        )
    })?;
    let canonical_file = path.canonicalize().map_err(|_| {
        ApiError::new(
            "SOURCE_NOT_FOUND",
            "auth.json is unavailable; sign in with ChatGPT in the official client",
        )
    })?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err(ApiError::new(
            "SOURCE_UNSUPPORTED",
            "Account file is outside the selected directory",
        ));
    }
    let mut bytes = Vec::new();
    File::open(&canonical_file)
        .and_then(|f| f.take(1024 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|_| ApiError::new("IO_ERROR", "Cannot read auth.json"))?;
    if bytes.len() > 1024 * 1024 {
        return Err(ApiError::new(
            "SOURCE_UNSUPPORTED",
            "Account file exceeds the size limit",
        ));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::new("SOURCE_BUSY", "Account file is being updated; retry later"))?;
    if value.get("OPENAI_API_KEY").is_some_and(|v| !v.is_null())
        || value
            .get("auth_mode")
            .and_then(Value::as_str)
            .is_some_and(|mode| !matches!(mode, "chatgpt" | "oauth"))
    {
        return Ok(None);
    }
    let tokens = value.get("tokens").ok_or_else(|| {
        ApiError::new(
            "AUTH_EXPIRED",
            "No saved ChatGPT login; sign in using the official client",
        )
    })?;
    let access = tokens
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 32768 && s.bytes().all(|b| b.is_ascii_graphic()))
        .ok_or_else(|| {
            ApiError::new("AUTH_EXPIRED", "The saved login has no valid access token")
        })?;
    let claims = jwt_claims(tokens.get("id_token").and_then(Value::as_str));
    let access_claims = jwt_claims(Some(access));
    let workspace = tokens
        .get("account_id")
        .and_then(Value::as_str)
        .or_else(|| {
            claims
                .as_ref()?
                .get("https://api.openai.com/auth")?
                .get("chatgpt_account_id")?
                .as_str()
        })
        .or_else(|| {
            access_claims
                .as_ref()?
                .get("https://api.openai.com/auth")?
                .get("chatgpt_account_id")?
                .as_str()
        })
        .filter(|s| !s.is_empty() && s.len() <= 256 && s.bytes().all(|b| b.is_ascii_graphic()))
        .ok_or_else(|| {
            ApiError::new(
                "ACCOUNT_UNSUPPORTED",
                "The official login has no workspace identity",
            )
        })?;
    let email = claims
        .as_ref()
        .and_then(|v| v.get("email"))
        .and_then(Value::as_str)
        .filter(|s| s.len() <= 320);
    let subject = claims
        .as_ref()
        .and_then(|v| v.get("sub"))
        .and_then(Value::as_str)
        .or_else(|| {
            access_claims
                .as_ref()?
                .get("https://api.openai.com/auth")?
                .get("chatgpt_user_id")?
                .as_str()
        })
        .or(email)
        .filter(|s| !s.is_empty() && s.len() <= 512)
        .ok_or_else(|| {
            ApiError::new(
                "ACCOUNT_UNSUPPORTED",
                "Cannot identify the saved login safely",
            )
        })?;
    let id = format!(
        "official-{:x}",
        Sha256::digest(format!("{workspace}\n{subject}"))
    );
    Ok(Some((
        SourceAccount {
            provider_id: "codex_usage".into(),
            source_id: id,
            display_name: email.unwrap_or("Official account").to_owned(),
            support: "supported".into(),
            is_current: true,
        },
        Credentials {
            provider: "codex_usage".into(),
            project_id: None,
            gcp: false,
            access_token: access.to_owned(),
            account_id: Some(workspace.to_owned()),
        },
    )))
}

pub fn legacy_list_accounts(root: &Path) -> Result<SourceCatalog, ApiError> {
    if !is_official(root) {
        return cockpit::list_accounts(root);
    }
    Ok(SourceCatalog {
        accounts: official(root)?.map(|(a, _)| vec![a]).unwrap_or_default(),
        format: "plaintext".into(),
    })
}
pub fn legacy_read_credentials(root: &Path, source_id: &str) -> Result<Credentials, ApiError> {
    if !is_official(root) {
        return cockpit::read_credentials(root, source_id);
    }
    let (account, credentials) = official(root)?
        .ok_or_else(|| ApiError::new("ACCOUNT_UNSUPPORTED", "API accounts are excluded"))?;
    if account.source_id != source_id {
        return Err(ApiError::new(
            "AUTH_EXPIRED",
            "The official account changed; rescan accounts",
        ));
    }
    Ok(credentials)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture(subject: &str) -> Value {
        let claims = URL_SAFE_NO_PAD
            .encode(json!({"email":"synthetic@example.test","sub":subject}).to_string());
        json!({"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"id_token":format!("header.{claims}.signature"),"access_token":"synthetic-token","account_id":"synthetic-workspace"}})
    }
    #[test]
    fn official_login_is_read_only_and_account_switch_does_not_reuse_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(&path, fixture("first").to_string()).unwrap();
        let before = std::fs::read(&path).unwrap();
        let first = legacy_list_accounts(dir.path()).unwrap().accounts.remove(0);
        assert!(first.is_current);
        assert_eq!(first.display_name, "synthetic@example.test");
        assert_eq!(
            legacy_read_credentials(dir.path(), &first.source_id)
                .unwrap()
                .access_token,
            "synthetic-token"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::write(&path, fixture("second").to_string()).unwrap();
        assert_ne!(
            legacy_list_accounts(dir.path()).unwrap().accounts[0].source_id,
            first.source_id
        );
        assert!(legacy_read_credentials(dir.path(), &first.source_id).is_err());
    }
    #[test]
    fn api_keys_and_unknown_official_auth_modes_are_not_imported() {
        let dir = tempfile::tempdir().unwrap();
        for v in [
            json!({"auth_mode":"apikey","OPENAI_API_KEY":"synthetic-key"}),
            json!({"auth_mode":"future"}),
        ] {
            std::fs::write(dir.path().join("auth.json"), v.to_string()).unwrap();
            assert!(legacy_list_accounts(dir.path())
                .unwrap()
                .accounts
                .is_empty());
        }
        let mut bad = fixture("first");
        bad["tokens"]["account_id"] = json!(null);
        std::fs::write(dir.path().join("auth.json"), bad.to_string()).unwrap();
        assert!(legacy_list_accounts(dir.path()).is_err());
    }
}
