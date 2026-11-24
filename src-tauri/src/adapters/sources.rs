//! Bounded, read-only imports. No source URLs or scripts are ever executed.
use super::{
    cockpit::{self, Credentials, SourceAccount, SourceCatalog},
    source,
};
use crate::model::ApiError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceOption {
    pub id: String,
    pub enabled: bool,
    pub path: Option<PathBuf>,
}
pub const KINDS: [&str; 5] = ["official", "cockpit", "ccswitch", "cliproxyapi", "sub2api"];
pub fn defaults() -> Vec<SourceOption> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    KINDS
        .iter()
        .zip([
            ".codex",
            ".cockpit_tools",
            ".cc-switch",
            ".cli-proxy-api",
            ".sub2api",
        ])
        .map(|(id, dir)| {
            let path = if *id == "official" {
                std::env::var_os("CODEX_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(dir))
            } else {
                home.join(dir)
            };
            SourceOption {
                id: (*id).into(),
                enabled: false,
                path: path.is_dir().then_some(path),
            }
        })
        .collect()
}
fn err() -> ApiError {
    ApiError::new(
        "SOURCE_UNSUPPORTED",
        "未找到受支持的账号文件；Sub2API 请选择含账号导出 JSON 的目录",
    )
}
fn json(root: &Path, file: &Path) -> Result<Value, ApiError> {
    cockpit::parse_json_slice(&cockpit::read_in_root(
        root,
        file,
        4 * 1024 * 1024,
        "SOURCE_BUSY",
    )?)
    .map_err(|_| err())
}
fn safe(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn provider(s: &str) -> Option<&'static str> {
    match s {
        "codex" | "openai" | "codex_usage" => Some("codex_usage"),
        "claude" | "anthropic" => Some("claude"),
        "antigravity" => Some("antigravity"),
        "grok" => Some("grok"),
        _ => None,
    }
}
fn token<'a>(v: &'a Value, paths: &[&str]) -> Option<&'a str> {
    paths
        .iter()
        .find_map(|p| v.pointer(p).and_then(Value::as_str))
        .filter(|s| !s.is_empty() && s.len() <= 32768 && s.bytes().all(|b| b.is_ascii_graphic()))
}
fn credentials(v: &Value, p: &str) -> Result<Credentials, ApiError> {
    let paths: &[&str] = match p {
        "claude" => &[
            "/claude_credentials_raw/claudeAiOauth/accessToken",
            "/claudeAiOauth/accessToken",
            "/access_token",
        ],
        "antigravity" => &["/token/access_token", "/access_token"],
        _ => &["/tokens/access_token", "/access_token"],
    };
    let access = token(v, paths).ok_or_else(|| {
        ApiError::new(
            "AUTH_EXPIRED",
            "账号没有有效的登录凭据，请在原客户端重新登录",
        )
    })?;
    Ok(Credentials {
        account_email: v.get("email").and_then(Value::as_str).map(str::to_owned),
        provider: p.into(),
        access_token: access.into(),
        account_id: token(
            v,
            &["/tokens/account_id", "/account_id", "/chatgpt_account_id"],
        )
        .map(str::to_owned),
        project_id: token(v, &["/token/project_id", "/project_id"]).map(str::to_owned),
        gcp: v
            .pointer("/token/is_gcp_tos")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}
struct Record {
    account: SourceAccount,
    credentials: Option<Credentials>,
}
fn record(id: String, p: &str, v: Value, name: Option<&str>, current: bool) -> Option<Record> {
    if v.get("auth_mode")
        .and_then(Value::as_str)
        .is_some_and(|m| !matches!(m, "oauth" | "o_auth" | "chatgpt"))
        || v.get("type").and_then(Value::as_str) == Some("apikey")
    {
        return None;
    }
    let creds = credentials(&v, p).ok();
    let name = name
        .or_else(|| v.get("email").and_then(Value::as_str))
        .unwrap_or(p)
        .chars()
        .filter(|c| !c.is_control())
        .take(320)
        .collect();
    Some(Record {
        account: SourceAccount {
            source_id: id,
            provider_id: p.into(),
            display_name: name,
            support: if creds.is_some() {
                "supported"
            } else {
                "unsupported"
            }
            .into(),
            is_current: current,
        },
        credentials: creds,
    })
}
fn records(root: &Path) -> Result<Vec<Record>, ApiError> {
    let mut out = vec![];
    let mut recognized = false;
    for (p, sqlite_index, json_index, folder, kind) in [
        (
            "claude",
            "claude_accounts.sqlite",
            "claude_accounts.json",
            "claude_accounts",
            "claude",
        ),
        (
            "grok",
            "grok_accounts.sqlite",
            "grok_accounts.json",
            "grok_accounts",
            "grok",
        ),
        (
            "antigravity",
            "antigravity_accounts.sqlite",
            "accounts.json",
            "accounts",
            "antigravity",
        ),
    ] {
        let (entries, current_id): (Vec<(String, Option<String>)>, Option<String>) =
            if root.join(sqlite_index).is_file() {
                recognized = true;
                let index = cockpit::read_sqlite_index(root, sqlite_index)?;
                (index.entries, index.current_id)
            } else if root.join(json_index).is_file() {
                recognized = true;
                let v = json(root, &root.join(json_index))?;
                let raw_entries = v
                    .get("accounts")
                    .and_then(Value::as_array)
                    .ok_or_else(err)?;
                if raw_entries.len() > 500 {
                    return Err(err());
                }
                let mut parsed = Vec::with_capacity(raw_entries.len());
                for e in raw_entries {
                    let id = e
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|s| safe(s))
                        .ok_or_else(err)?
                        .to_owned();
                    let email = e
                        .get("email")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    parsed.push((id, email));
                }
                (
                    parsed,
                    v.get("current_account_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                )
            } else {
                continue;
            };
        let total = entries.len();
        let mut missing = 0usize;
        for (id, email) in entries {
            let detail_path = root.join(folder).join(format!("{id}.json"));
            if !detail_path.exists() {
                missing += 1;
                continue;
            }
            let detail = cockpit::decode_provider(root, &detail_path, kind)?;
            if detail.get("id").and_then(Value::as_str) != Some(id.as_str()) {
                return Err(err());
            }
            if let Some(mut r) = record(
                format!("{p}:{id}"),
                p,
                detail,
                email.as_deref(),
                current_id.as_deref() == Some(id.as_str()),
            ) {
                // This is the account index identity, not an editable HUD alias or export name.
                if let Some(credentials) = r.credentials.as_mut() {
                    if credentials.account_email.is_none() {
                        credentials.account_email = email;
                    }
                }
                out.push(r);
            }
        }
        if total > 0 && missing == total {
            return Err(ApiError::new("IO_ERROR", "无法读取账号详情"));
        }
    }
    if root.join("cc-switch.db").is_file() {
        recognized = true;
        let path = root
            .join("cc-switch.db")
            .canonicalize()
            .map_err(|_| err())?;
        if !path.starts_with(root.canonicalize().map_err(|_| err())?) {
            return Err(err());
        }
        let db = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| err())?;
        db.busy_timeout(std::time::Duration::from_millis(500))
            .map_err(|_| err())?;
        let mut q = db
            .prepare(
                "SELECT id, app_type, name, settings_config, is_current FROM providers LIMIT 501",
            )
            .map_err(|_| err())?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, bool>(4)?,
                ))
            })
            .map_err(|_| err())?;
        for row in rows {
            let (id, app, name, raw, current) = row.map_err(|_| err())?;
            if raw.len() > 1024 * 1024 {
                return Err(err());
            }
            let Some(p) = provider(&app) else {
                continue;
            };
            let v: Value = serde_json::from_str(&raw).map_err(|_| err())?;
            let auth = v.get("auth").cloned().unwrap_or(v);
            let auth = if let Some(s) = auth.as_str() {
                serde_json::from_str(s).map_err(|_| err())?
            } else {
                auth
            };
            if let Some(r) = record(format!("cc:{app}:{id}"), p, auth, Some(&name), current) {
                out.push(r);
            }
        }
    }
    // CLIProxyAPI stores one OAuth JSON per account; Sub2API exports a bounded accounts array.
    let mut files = std::fs::read_dir(root)
        .map_err(|_| err())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .take(501)
        .collect::<Vec<_>>();
    if files.len() > 500 {
        return Err(err());
    }
    files.sort();
    for path in files {
        let filename = path.file_name().unwrap().to_string_lossy().into_owned();
        if matches!(
            filename.as_str(),
            "accounts.json" | "codex_accounts.json" | "claude_accounts.json" | "grok_accounts.json"
        ) {
            continue;
        }
        let v = match json(root, &path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(p) = v.get("type").and_then(Value::as_str).and_then(provider) {
            recognized = true;
            if let Some(r) = record(format!("file:{p}:{filename}"), p, v, None, false) {
                out.push(r);
            }
            continue;
        }
        if let Some(entries) = v
            .get("accounts")
            .or_else(|| v.pointer("/data/accounts"))
            .and_then(Value::as_array)
        {
            if entries.len() > 500 {
                return Err(err());
            }
            for e in entries.iter() {
                let Some(p) = e.get("platform").and_then(Value::as_str).and_then(provider) else {
                    continue;
                };
                recognized = true;
                if e.get("type").and_then(Value::as_str) != Some("oauth") {
                    continue;
                }
                let identity = e
                    .get("id")
                    .filter(|v| v.is_string() || v.is_number())
                    .map(Value::to_string)
                    .or_else(|| e.get("name").and_then(Value::as_str).map(str::to_owned))
                    .ok_or_else(err)?;
                if let Some(r) = record(
                    format!("export:{filename}:{p}:{:x}", Sha256::digest(identity)),
                    p,
                    e.get("credentials").cloned().unwrap_or(Value::Null),
                    e.get("name").and_then(Value::as_str),
                    false,
                ) {
                    out.push(r);
                }
            }
        }
    }
    if root.join(".credentials.json").is_file() {
        recognized = true;
        let v = json(root, &root.join(".credentials.json"))?;
        if let Some(r) = record("claude:official".into(), "claude", v, Some("Claude"), true) {
            out.push(r);
        }
    }
    let unique = out
        .iter()
        .map(|r| &r.account.source_id)
        .collect::<std::collections::HashSet<_>>();
    if out.len() > 500 || unique.len() != out.len() {
        return Err(err());
    }
    if !recognized
        && !root.join("auth.json").is_file()
        && !root.join("codex_accounts.json").is_file()
        && !root.join("codex_accounts.sqlite").is_file()
    {
        return Err(err());
    }
    Ok(out)
}
pub fn list_accounts(root: &Path) -> Result<SourceCatalog, ApiError> {
    let mut accounts = vec![];
    if root.join("codex_accounts.sqlite").is_file()
        || root.join("codex_accounts.json").is_file()
        || root.join("auth.json").is_file()
    {
        accounts = source::legacy_list_accounts(root)?.accounts;
    }
    accounts.extend(records(root)?.into_iter().map(|r| r.account));
    Ok(SourceCatalog {
        accounts,
        format: "unknown".into(),
    })
}
pub fn read_credentials(root: &Path, id: &str) -> Result<Credentials, ApiError> {
    if !id.contains(':') {
        return source::legacy_read_credentials(root, id);
    }
    records(root)?
        .into_iter()
        .find(|r| r.account.source_id == id)
        .and_then(|r| r.credentials)
        .ok_or_else(|| ApiError::new("AUTH_EXPIRED", "账号凭据不可用，请在原客户端更新并重新扫描"))
}
pub fn discover_root() -> Option<PathBuf> {
    source::discover_root()
}

/// Scan only the chosen directory and conventional immediate children, never the disk.
pub fn resolve_root(root: &Path, kind: &str) -> Result<PathBuf, ApiError> {
    let matches = |p: &Path| match kind {
        "official" => p.join("auth.json").is_file() || p.join(".credentials.json").is_file(),
        "cockpit" => [
            "codex_accounts.sqlite",
            "claude_accounts.sqlite",
            "grok_accounts.sqlite",
            "antigravity_accounts.sqlite",
            "codex_accounts.json",
            "claude_accounts.json",
            "grok_accounts.json",
            "accounts.json",
        ]
        .iter()
        .any(|f| p.join(f).is_file()),
        "ccswitch" => p.join("cc-switch.db").is_file(),
        "cliproxyapi" | "sub2api" => p.is_dir(),
        _ => false,
    };
    let mut last_error = None;
    for child in [
        "",
        ".codex",
        ".claude",
        ".cockpit_tools",
        ".antigravity_cockpit",
        ".cc-switch",
        ".cli-proxy-api",
        "auths",
        "data",
    ] {
        let p = root.join(child);
        if matches(&p) {
            match list_accounts(&p) {
                Ok(_) => return p.canonicalize().map_err(|_| err()),
                Err(error) => last_error = Some(error),
            }
        }
    }
    Err(last_error.unwrap_or_else(err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn write(root: &Path, name: &str, v: Value) {
        let p = root.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, v.to_string()).unwrap();
    }
    #[test]
    fn cockpit_platforms_route_without_writing_credentials() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        for (p, index, folder, v) in [
            (
                "claude",
                "claude_accounts.json",
                "claude_accounts",
                json!({"id":"a","claude_credentials_raw":{"claudeAiOauth":{"accessToken":"synthetic-claude"}}}),
            ),
            (
                "grok",
                "grok_accounts.json",
                "grok_accounts",
                json!({"id":"a","access_token":"synthetic-grok"}),
            ),
            (
                "antigravity",
                "accounts.json",
                "accounts",
                json!({"id":"a","token":{"access_token":"synthetic-google","project_id":"test-project"}}),
            ),
        ] {
            write(
                root,
                index,
                json!({"accounts":[{"id":"a","email":"test@example.test"}],"current_account_id":"a"}),
            );
            write(root, &format!("{folder}/a.json"), v.clone());
            let a = list_accounts(root)
                .unwrap()
                .accounts
                .into_iter()
                .find(|a| a.provider_id == p)
                .unwrap();
            assert!(a.is_current);
            assert_eq!(read_credentials(root, &a.source_id).unwrap().provider, p);
            assert_eq!(read_credentials(root, &a.source_id).unwrap().account_email.as_deref(), Some("test@example.test"));
            assert_eq!(json(root, &root.join(folder).join("a.json")).unwrap(), v);
        }
        assert_eq!(list_accounts(root).unwrap().accounts.len(), 3);
        assert!(read_credentials(root, "claude:../outside").is_err());
    }
    #[test]
    fn cc_switch_opens_existing_database_read_only() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("cc-switch.db");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE providers(id TEXT,app_type TEXT,name TEXT,settings_config TEXT,is_current BOOLEAN);").unwrap();
        db.execute(
            "INSERT INTO providers VALUES('one','claude','Example',?1,1)",
            [json!({"claudeAiOauth":{"accessToken":"synthetic"}}).to_string()],
        )
        .unwrap();
        drop(db);
        let before = std::fs::read(&path).unwrap();
        let a = list_accounts(t.path()).unwrap().accounts.remove(0);
        assert_eq!(a.provider_id, "claude");
        assert!(a.is_current);
        assert_eq!(
            read_credentials(t.path(), &a.source_id)
                .unwrap()
                .access_token,
            "synthetic"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1);
    }
    #[test]
    fn proxy_and_export_routing_and_export_reorder_are_stable() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        write(
            root,
            "claude.json",
            json!({"type":"claude","access_token":"synthetic","email":"test@example.test"}),
        );
        let a = json!({"id":1,"platform":"openai","type":"oauth","name":"one","credentials":{"access_token":"synthetic-one","account_id":"workspace"}});
        let b = json!({"id":2,"platform":"anthropic","type":"oauth","name":"two","credentials":{"access_token":"synthetic-two"}});
        write(root, "export.json", json!({"accounts":[a,b]}));
        let first = list_accounts(root).unwrap();
        let id = first
            .accounts
            .iter()
            .find(|a| a.display_name == "one")
            .unwrap()
            .source_id
            .clone();
        assert_eq!(first.accounts.len(), 3);
        write(root, "export.json", json!({"accounts":[b,a]}));
        assert_eq!(
            read_credentials(root, &id).unwrap().access_token,
            "synthetic-one"
        );
    }
    #[test]
    fn missing_sources_are_not_created_and_api_keys_are_not_oauth() {
        let t = tempfile::tempdir().unwrap();
        assert!(list_accounts(&t.path().join("missing")).is_err());
        assert!(!t.path().join("missing").exists());
        write(
            t.path(),
            "export.json",
            json!({"accounts":[{"platform":"anthropic","type":"apikey","name":"key","credentials":{"api_key":"synthetic-key"}}]}),
        );
        assert!(list_accounts(t.path()).unwrap().accounts.is_empty());
    }
    #[test]
    fn antigravity_account_files_accept_utf8_bom() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        std::fs::create_dir_all(root.join("accounts")).unwrap();
        std::fs::write(
            root.join("accounts.json"),
            format!(
                "\u{FEFF}{}",
                json!({"accounts":[{"id":"a","email":"bom@example.test"}],"current_account_id":"a"})
            ),
        )
        .unwrap();
        std::fs::write(
            root.join("accounts/a.json"),
            format!(
                "\u{FEFF}{}",
                json!({"id":"a","token":{"access_token":"synthetic-bom-token","project_id":"test-project"}})
            ),
        )
        .unwrap();
        let accounts = list_accounts(root).unwrap().accounts;
        assert_eq!(accounts.len(), 1);
        let creds = read_credentials(root, &accounts[0].source_id).unwrap();
        assert_eq!(creds.access_token, "synthetic-bom-token");
        assert_eq!(creds.account_email.as_deref(), Some("bom@example.test"));
    }

    #[test]
    fn cockpit_sqlite_indices_load_all_platforms_and_preserve_errors() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        let create_sqlite = |name: &str, version: &str, current: Option<&str>, id: &str, email: &str| {
            let db_path = root.join(name);
            let db = rusqlite::Connection::open(&db_path).unwrap();
            db.execute_batch(
                "CREATE TABLE summaries (
                    source INTEGER NOT NULL, position INTEGER NOT NULL,
                    account_id TEXT NOT NULL, email TEXT NOT NULL, name TEXT,
                    created_at INTEGER NOT NULL, last_used INTEGER NOT NULL,
                    plan_type TEXT, subscription_active_until TEXT,
                    provider_login TEXT, provider_user_id TEXT, payload TEXT,
                    PRIMARY KEY(source, position)
                );
                CREATE TABLE account_index_state (
                    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
                    state TEXT NOT NULL
                );",
            )
            .unwrap();
            db.execute(
                "INSERT INTO account_index_state VALUES (1, ?1)",
                [json!({
                    "source": 0,
                    "version": version,
                    "current_id": current,
                    "detail_schema": 2,
                    "complete": true
                })
                .to_string()],
            )
            .unwrap();
            db.execute(
                "INSERT INTO summaries (source, position, account_id, email, created_at, last_used) VALUES (0, 0, ?1, ?2, 1, 1)",
                [id, email],
            )
            .unwrap();
        };

        create_sqlite(
            "codex_accounts.sqlite",
            "1.0",
            Some("codex-a"),
            "codex-a",
            "codex@example.test",
        );
        write(
            root,
            "codex_accounts/codex-a.json",
            json!({"id":"codex-a","auth_mode":"oauth","account_id":"ws-1","tokens":{"access_token":"tok-codex"}}),
        );
        create_sqlite(
            "antigravity_accounts.sqlite",
            "2.0",
            Some("ag-a"),
            "ag-a",
            "ag@example.test",
        );
        write(
            root,
            "accounts/ag-a.json",
            json!({"id":"ag-a","token":{"access_token":"tok-ag","project_id":"proj-1"}}),
        );
        create_sqlite(
            "grok_accounts.sqlite",
            "",
            None,
            "grok-a",
            "grok@example.test",
        );
        write(
            root,
            "grok_accounts/grok-a.json",
            json!({"id":"grok-a","auth_mode":"oauth","access_token":"tok-grok"}),
        );

        let before = std::fs::read(root.join("codex_accounts.sqlite")).unwrap();
        let resolved = resolve_root(root, "cockpit").unwrap();
        assert_eq!(resolved, root.canonicalize().unwrap());
        let accounts = list_accounts(root).unwrap().accounts;
        assert_eq!(accounts.len(), 3);
        assert!(accounts.iter().any(|a| a.provider_id == "codex_usage" && a.is_current));
        assert!(accounts.iter().any(|a| a.provider_id == "antigravity" && a.is_current));
        assert!(accounts.iter().any(|a| a.provider_id == "grok"));
        assert_eq!(read_credentials(root, "codex-a").unwrap().access_token, "tok-codex");
        assert_eq!(
            read_credentials(root, "antigravity:ag-a").unwrap().access_token,
            "tok-ag"
        );
        assert_eq!(read_credentials(root, "grok:grok-a").unwrap().access_token, "tok-grok");
        assert_eq!(std::fs::read(root.join("codex_accounts.sqlite")).unwrap(), before);

        // Encrypted detail without key should surface STORAGE_KEY_UNAVAILABLE via resolve_root.
        write(
            root,
            "codex_accounts/codex-a.json",
            json!({
                "version": 1,
                "kind": "codex",
                "algorithm": "AES-256-GCM",
                "key_id": "local-secure-account-storage-v1",
                "nonce": "AAAAAAAAAAAAAAAA",
                "ciphertext": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
            }),
        );
        assert_eq!(
            resolve_root(root, "cockpit").err().unwrap().code,
            "STORAGE_KEY_UNAVAILABLE"
        );
    }
}
