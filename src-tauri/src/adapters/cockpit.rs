use crate::model::ApiError;
use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_JSON_BYTES: usize = 1024 * 1024;
const MAX_KEY_BYTES: usize = 256;
const MAX_SOURCE_ACCOUNTS: usize = 500;

#[derive(Clone)]
pub struct SourceAccount {
    pub provider_id: String,
    pub source_id: String,
    pub display_name: String,
    pub support: String,
    pub is_current: bool,
}

pub struct SourceCatalog {
    pub accounts: Vec<SourceAccount>,
    pub format: String,
}

// Deliberately lacks Debug and Serialize: credentials must not enter logs or transports.
pub struct Credentials {
    pub(crate) provider: String,
    pub(crate) project_id: Option<String>,
    pub(crate) gcp: bool,
    pub(crate) access_token: String,
    pub(crate) account_id: Option<String>,
}

/// Discovery examines path existence only; account/key contents are read on explicit use.
pub fn discover_root() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("COCKPIT_TOOLS_DATA_DIR") {
        let root = PathBuf::from(configured);
        if root.is_absolute() && root.join("codex_accounts.json").is_file() {
            return Some(root);
        }
        return None;
    }
    let profile = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    [
        ".cockpit_tools",
        ".cockpit_tools_dev",
        ".antigravity_cockpit",
    ]
    .iter()
    .map(|name| PathBuf::from(&profile).join(name))
    .find(|root| root.join("codex_accounts.json").is_file())
}

pub fn list_accounts(root: &Path) -> Result<SourceCatalog, ApiError> {
    let mut catalog = read_catalog(root)?;
    let mut login_accounts = Vec::new();
    for mut account in catalog.accounts {
        let value = read_detail(root, &account.source_id)?;
        match credentials_from_detail(&value, &account.source_id) {
            Ok(_) => account.support = "supported".into(),
            // An expired login remains selectable so its authentication error is visible.
            Err(error) if error.code == "AUTH_EXPIRED" => account.support = "supported".into(),
            Err(error) if error.code == "ACCOUNT_UNSUPPORTED" => continue,
            Err(error) => return Err(error),
        }
        login_accounts.push(account);
    }
    catalog.accounts = login_accounts;
    Ok(catalog)
}

fn read_catalog(root: &Path) -> Result<SourceCatalog, ApiError> {
    let bytes = read_in_root(
        root,
        &root.join("codex_accounts.json"),
        MAX_JSON_BYTES,
        "SOURCE_NOT_FOUND",
    )?;
    let index: Value = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::new("SOURCE_BUSY", "账号索引暂时无法读取，请稍后重试"))?;
    if index.get("version").and_then(Value::as_str) != Some("1.0") {
        return Err(unsupported_source());
    }
    if let Some(schema) = index.get("detail_schema_version") {
        if !schema.as_u64().is_some_and(|version| version <= 2) {
            return Err(unsupported_source());
        }
    }
    let entries = index
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or_else(unsupported_source)?;
    if entries.len() > MAX_SOURCE_ACCOUNTS {
        return Err(ApiError::new(
            "SOURCE_UNSUPPORTED",
            "账号索引超出当前版本支持的大小",
        ));
    }
    let mut seen = HashSet::new();
    let mut accounts = Vec::with_capacity(entries.len());
    for (order, entry) in entries.iter().enumerate() {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(unsupported_source)?;
        if !safe_source_id(id) || !seen.insert(id.to_owned()) {
            return Err(unsupported_source());
        }
        let name = entry
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty() && name.len() <= 320)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("账号 {}", order + 1));
        accounts.push(SourceAccount {
            provider_id: "codex_usage".into(),
            source_id: id.to_owned(),
            display_name: name,
            support: "unknown".into(),
            is_current: false,
        });
    }
    // Index schema alone cannot establish whether individual details are encrypted or mixed.
    Ok(SourceCatalog {
        accounts,
        format: "unknown".into(),
    })
}

pub fn read_credentials(root: &Path, source_id: &str) -> Result<Credentials, ApiError> {
    if !safe_source_id(source_id) {
        return Err(ApiError::new("INVALID_ARGUMENT", "账号标识无效"));
    }
    // Require index membership as well as path containment; never use arbitrary file names.
    if !read_catalog(root)?
        .accounts
        .iter()
        .any(|account| account.source_id == source_id)
    {
        return Err(ApiError::new("NOT_FOUND", "账号不在当前来源的索引中"));
    }
    credentials_from_detail(&read_detail(root, source_id)?, source_id)
}

fn read_detail(root: &Path, source_id: &str) -> Result<Value, ApiError> {
    let detail = root
        .join("codex_accounts")
        .join(format!("{source_id}.json"));
    let first = decode_detail(root, &detail);
    let value = match first {
        Err(error) if error.code == "STORAGE_DECRYPT_FAILED" || error.code == "SOURCE_BUSY" => {
            // Cockpit may atomically replace its key/detail pair. A single re-read is bounded.
            decode_detail(root, &detail)?
        }
        other => other?,
    };
    Ok(value)
}

fn credentials_from_detail(value: &Value, source_id: &str) -> Result<Credentials, ApiError> {
    if value.get("id").and_then(Value::as_str) != Some(source_id) {
        return Err(ApiError::new(
            "SOURCE_UNSUPPORTED",
            "账号详情与索引标识不一致",
        ));
    }
    let mode = match value.get("auth_mode") {
        None => "oauth", // The pinned historical model defaults a missing mode to OAuth.
        Some(Value::String(mode)) => mode.as_str(),
        Some(_) => return Err(unsupported_source()),
    };
    let unsupported = mode != "oauth"
        || value.get("token_source_mode").and_then(Value::as_str) == Some("chatgpt_web_session")
        || [
            "agent_identity",
            "openai_api_key",
            "personal_access_token",
            "web_session",
            "session_token",
        ]
        .iter()
        .any(|key| value.get(*key).is_some_and(|v| !v.is_null()));
    if unsupported {
        return Err(ApiError::new(
            "ACCOUNT_UNSUPPORTED",
            "第一版仅支持标准 OAuth 账号",
        ));
    }
    if value.get("requires_reauth").and_then(Value::as_bool) == Some(true) {
        return Err(ApiError::new(
            "AUTH_EXPIRED",
            "请在 Cockpit 中重新登录该账号",
        ));
    }
    let tokens = value
        .get("tokens")
        .and_then(Value::as_object)
        .ok_or_else(|| ApiError::new("ACCOUNT_UNSUPPORTED", "账号缺少标准 OAuth 凭据"))?;
    let access_token = tokens
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|token| {
            !token.is_empty()
                && token.len() <= 32 * 1024
                && token.bytes().all(|byte| byte.is_ascii_graphic())
        })
        .ok_or_else(|| {
            ApiError::new(
                "AUTH_EXPIRED",
                "账号缺少可用的访问凭据，请在 Cockpit 中登录",
            )
        })?;
    if access_token.starts_with("at-") {
        return Err(ApiError::new(
            "ACCOUNT_UNSUPPORTED",
            "第一版仅支持标准 OAuth 账号",
        ));
    }
    let account_id = value
        .get("account_id")
        .and_then(Value::as_str)
        .filter(|id| {
            !id.is_empty() && id.len() <= 256 && id.bytes().all(|byte| byte.is_ascii_graphic())
        })
        .ok_or_else(|| {
            ApiError::new(
                "ACCOUNT_UNSUPPORTED",
                "账号缺少工作区标识，请在 Cockpit 中更新账号",
            )
        })?;
    Ok(Credentials {
        provider: "codex_usage".into(),
        project_id: None,
        gcp: false,
        access_token: access_token.into(),
        account_id: Some(account_id.into()),
    })
}

fn decode_detail(root: &Path, path: &Path) -> Result<Value, ApiError> {
    decode_provider(root, path, "codex")
}
pub(super) fn decode_provider(root: &Path, path: &Path, kind: &str) -> Result<Value, ApiError> {
    let bytes = read_in_root(root, path, MAX_JSON_BYTES, "IO_ERROR")?;
    let document: Value = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::new("SOURCE_BUSY", "账号详情暂时无法读取，请稍后重试"))?;
    if !document.is_object() {
        return Err(unsupported_source());
    }
    let envelope_marker = ["algorithm", "ciphertext", "nonce", "key_id", "encrypted_at"]
        .iter()
        .any(|field| document.get(*field).is_some());
    if !envelope_marker {
        return Ok(document);
    }
    if document.get("version").and_then(Value::as_u64) != Some(1)
        || document.get("kind").and_then(Value::as_str) != Some(kind)
        || document.get("algorithm").and_then(Value::as_str) != Some("AES-256-GCM")
        || document.get("key_id").and_then(Value::as_str) != Some("local-secure-account-storage-v1")
    {
        return Err(unsupported_source());
    }
    let raw_key = read_in_root(
        root,
        &root.join("secure-account-storage.key"),
        MAX_KEY_BYTES,
        "STORAGE_KEY_UNAVAILABLE",
    )?;
    let encoded_key = std::str::from_utf8(&raw_key).map_err(|_| unavailable_key())?;
    let key = STANDARD
        .decode(encoded_key.trim())
        .map_err(|_| unavailable_key())?;
    if key.len() != 32 {
        return Err(unavailable_key());
    }
    let nonce = document
        .get("nonce")
        .and_then(Value::as_str)
        .and_then(|raw| STANDARD.decode(raw.trim()).ok())
        .filter(|decoded| decoded.len() == 12)
        .ok_or_else(decrypt_failed)?;
    let ciphertext = document
        .get("ciphertext")
        .and_then(Value::as_str)
        .and_then(|raw| STANDARD.decode(raw.trim()).ok())
        .filter(|decoded| decoded.len() >= 16)
        .ok_or_else(decrypt_failed)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| unavailable_key())?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
        .map_err(|_| decrypt_failed())?;
    if plaintext.len() > MAX_JSON_BYTES {
        return Err(decrypt_failed());
    }
    serde_json::from_slice(&plaintext).map_err(|_| decrypt_failed())
}

fn safe_source_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub(super) fn read_in_root(
    root: &Path,
    candidate: &Path,
    limit: usize,
    code: &str,
) -> Result<Vec<u8>, ApiError> {
    let error = || {
        ApiError::new(
            code,
            match code {
                "STORAGE_KEY_UNAVAILABLE" => "无法读取 Cockpit 已有的加密密钥",
                "SOURCE_NOT_FOUND" => "未找到可读取的 Cockpit 账号索引",
                _ => "无法读取账号详情",
            },
        )
    };
    let resolved_root = root.canonicalize().map_err(|_| error())?;
    let resolved_file = candidate.canonicalize().map_err(|_| error())?;
    if !resolved_file.starts_with(&resolved_root) {
        return Err(ApiError::new(
            "SOURCE_UNSUPPORTED",
            "账号文件必须位于所选来源目录内",
        ));
    }
    let file = File::open(&resolved_file).map_err(|_| error())?;
    let metadata = file.metadata().map_err(|_| error())?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(ApiError::new(
            "SOURCE_UNSUPPORTED",
            "来源文件类型或大小不受支持",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error())?;
    if bytes.len() > limit {
        return Err(ApiError::new("SOURCE_UNSUPPORTED", "来源文件超过大小限制"));
    }
    Ok(bytes)
}

fn unsupported_source() -> ApiError {
    ApiError::new("SOURCE_UNSUPPORTED", "当前 Cockpit 存储格式尚不受支持")
}
fn unavailable_key() -> ApiError {
    ApiError::new("STORAGE_KEY_UNAVAILABLE", "Cockpit 加密密钥缺失或格式无效")
}
fn decrypt_failed() -> ApiError {
    ApiError::new(
        "STORAGE_DECRYPT_FAILED",
        "无法解密账号详情，请检查 Cockpit 中的账号状态",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::fs;
    use tempfile::TempDir;

    fn fixture() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("codex_accounts")).unwrap();
        fs::write(
            dir.path().join("codex_accounts.json"),
            json!({
                "version":"1.0", "detail_schema_version":2,
                "accounts":[{"id":"synthetic-account", "email":"sample@example.test"}]
            })
            .to_string(),
        )
        .unwrap();
        dir
    }
    fn detail() -> Value {
        json!({"id":"synthetic-account","auth_mode":"oauth","account_id":"synthetic-workspace",
            "tokens":{"access_token":"synthetic-access-token"}})
    }
    fn detail_path(dir: &TempDir) -> PathBuf {
        dir.path().join("codex_accounts/synthetic-account.json")
    }
    fn encrypted(value: &Value) -> Value {
        let key = [7_u8; 32];
        let nonce = [9_u8; 12];
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                serde_json::to_vec(value).unwrap().as_slice(),
            )
            .unwrap();
        json!({"version":1,"kind":"codex","algorithm":"AES-256-GCM","key_id":"local-secure-account-storage-v1",
            "nonce":STANDARD.encode(nonce),"ciphertext":STANDARD.encode(ciphertext),"encrypted_at":1})
    }
    fn hashes(dir: &TempDir) -> Vec<(PathBuf, Vec<u8>)> {
        let mut paths: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        paths.extend(
            fs::read_dir(dir.path().join("codex_accounts"))
                .unwrap()
                .map(|entry| entry.unwrap().path()),
        );
        paths.sort();
        paths
            .into_iter()
            .filter(|path| path.is_file())
            .map(|path| {
                let hash = Sha256::digest(fs::read(&path).unwrap()).to_vec();
                (path, hash)
            })
            .collect()
    }

    #[test]
    fn encrypted_and_plaintext_reads_never_write_source() {
        for encrypt in [false, true] {
            let dir = fixture();
            let document = if encrypt {
                encrypted(&detail())
            } else {
                detail()
            };
            fs::write(detail_path(&dir), document.to_string()).unwrap();
            if encrypt {
                fs::write(
                    dir.path().join("secure-account-storage.key"),
                    STANDARD.encode([7_u8; 32]),
                )
                .unwrap();
            }
            let before = hashes(&dir);
            let catalog = list_accounts(dir.path()).unwrap();
            assert_eq!(catalog.accounts.len(), 1);
            assert_eq!(catalog.accounts[0].support, "supported");
            let credentials = read_credentials(dir.path(), "synthetic-account").unwrap();
            assert_eq!(credentials.access_token, "synthetic-access-token");
            assert_eq!(before, hashes(&dir));
        }
    }

    #[test]
    fn missing_key_is_reported_without_creation() {
        let dir = fixture();
        fs::write(detail_path(&dir), encrypted(&detail()).to_string()).unwrap();
        let before = hashes(&dir);
        let error = read_credentials(dir.path(), "synthetic-account")
            .err()
            .unwrap();
        assert_eq!(error.code, "STORAGE_KEY_UNAVAILABLE");
        assert_eq!(before, hashes(&dir));
    }

    #[test]
    fn corrupt_envelope_is_not_reinterpreted_as_plaintext() {
        let dir = fixture();
        let mut value = encrypted(&detail());
        value["ciphertext"] = json!(STANDARD.encode([0_u8; 32]));
        fs::write(detail_path(&dir), value.to_string()).unwrap();
        fs::write(
            dir.path().join("secure-account-storage.key"),
            STANDARD.encode([7_u8; 32]),
        )
        .unwrap();
        let before = hashes(&dir);
        assert_eq!(
            read_credentials(dir.path(), "synthetic-account")
                .err()
                .unwrap()
                .code,
            "STORAGE_DECRYPT_FAILED"
        );
        assert_eq!(before, hashes(&dir));
    }

    #[test]
    fn future_envelope_and_account_types_fail_closed() {
        let dir = fixture();
        for (field, value) in [
            ("auth_mode", json!("apikey")),
            ("agent_identity", json!({"account_id":"any"})),
            ("token_source_mode", json!("chatgpt_web_session")),
        ] {
            let mut document = detail();
            document[field] = value;
            fs::write(detail_path(&dir), document.to_string()).unwrap();
            assert_eq!(
                read_credentials(dir.path(), "synthetic-account")
                    .err()
                    .unwrap()
                    .code,
                "ACCOUNT_UNSUPPORTED"
            );
        }
        let mut envelope = encrypted(&detail());
        envelope["version"] = json!(2);
        fs::write(detail_path(&dir), envelope.to_string()).unwrap();
        assert_eq!(
            read_credentials(dir.path(), "synthetic-account")
                .err()
                .unwrap()
                .code,
            "SOURCE_UNSUPPORTED"
        );
    }

    #[test]
    fn index_and_workspace_identity_are_required() {
        let dir = fixture();
        let mut document = detail();
        document["id"] = json!("wrong-account");
        fs::write(detail_path(&dir), document.to_string()).unwrap();
        assert_eq!(
            read_credentials(dir.path(), "synthetic-account")
                .err()
                .unwrap()
                .code,
            "SOURCE_UNSUPPORTED"
        );
        let mut document = detail();
        document.as_object_mut().unwrap().remove("account_id");
        fs::write(detail_path(&dir), document.to_string()).unwrap();
        assert_eq!(
            read_credentials(dir.path(), "synthetic-account")
                .err()
                .unwrap()
                .code,
            "ACCOUNT_UNSUPPORTED"
        );
        assert_eq!(
            read_credentials(dir.path(), "../synthetic-account")
                .err()
                .unwrap()
                .code,
            "INVALID_ARGUMENT"
        );
        assert_eq!(
            read_credentials(dir.path(), "absent").err().unwrap().code,
            "NOT_FOUND"
        );
    }

    #[test]
    fn listing_requires_details_to_identify_login_accounts() {
        let dir = fixture();
        assert!(list_accounts(dir.path()).is_err());
        assert!(!dir.path().join("secure-account-storage.key").exists());
    }

    #[test]
    fn listing_filters_api_credentials_in_plaintext_and_encrypted_details() {
        for encrypt in [false, true] {
            let dir = fixture();
            fs::write(
                dir.path().join("secure-account-storage.key"),
                STANDARD.encode([7_u8; 32]),
            )
            .unwrap();
            for (mode, expected) in [("oauth", 1), ("apikey", 0)] {
                let mut value = detail();
                value["auth_mode"] = json!(mode);
                value["requires_reauth"] = json!(true);
                let document = if encrypt { encrypted(&value) } else { value };
                fs::write(detail_path(&dir), document.to_string()).unwrap();
                assert_eq!(list_accounts(dir.path()).unwrap().accounts.len(), expected);
            }
            let mut value = detail();
            value["openai_api_key"] = json!("synthetic-api-key");
            let document = if encrypt { encrypted(&value) } else { value };
            fs::write(detail_path(&dir), document.to_string()).unwrap();
            assert!(list_accounts(dir.path()).unwrap().accounts.is_empty());
        }
    }

    #[test]
    fn file_reads_enforce_root_containment_and_size_bounds() {
        let dir = fixture();
        let outside = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(
            read_in_root(dir.path(), outside.path(), MAX_JSON_BYTES, "IO_ERROR")
                .err()
                .unwrap()
                .code,
            "SOURCE_UNSUPPORTED"
        );
        fs::write(detail_path(&dir), vec![b'x'; MAX_JSON_BYTES + 1]).unwrap();
        assert_eq!(
            read_credentials(dir.path(), "synthetic-account")
                .err()
                .unwrap()
                .code,
            "SOURCE_UNSUPPORTED"
        );
    }
}
