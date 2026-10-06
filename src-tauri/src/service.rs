use crate::{
    adapters::{sources as cockpit, usage},
    config::{self, Config},
    domain,
    model::*,
    themes,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

#[derive(Clone)]
struct Entry {
    summary: AccountSummary,
    source_id: String,
    root: PathBuf,
    base_name: String,
}
struct Data {
    config: Config,
    source: SourceStatus,
    accounts: Vec<Entry>,
    quotas: HashMap<String, AccountQuota>,
    jobs: HashMap<String, RefreshJob>,
    in_flight: HashMap<String, String>,
    cooldown: HashMap<String, DateTime<Utc>>,
    failures: HashMap<String, u32>,
    generation: u64,
    revision: u64,
    sequence: u64,
    next_auto: Instant,
    refresh_started: HashMap<String, Instant>,
    handled_resets: HashSet<String>,
    samples: HashMap<String, Instant>,
    clock_pending: HashSet<String>,
    last_wall: DateTime<Utc>,
    last_tick: Instant,
    next_source_scan: Instant,
    position_pending: Option<Instant>,
}
#[derive(Clone)]
pub struct Service {
    data: Arc<Mutex<Data>>,
    app: Option<AppHandle>,
    client: reqwest::Client,
    semaphore: Arc<tokio::sync::Semaphore>,
    config_path: PathBuf,
    pub theme_dir: PathBuf,
    pub instance_id: String,
}

impl Service {
    pub fn new(data_dir: PathBuf, app: Option<AppHandle>) -> Result<Self, ApiError> {
        let config_path = data_dir.join("settings.json");
        let mut config = config::load(&config_path)?;
        if config.source_path.is_none() && !cfg!(test) {
            config.source_path = cockpit::discover_root()
        }
        if themes::builtin(&config.settings.active_theme_id).is_none()
            && crate::theme_package::installed(
                &data_dir.join("themes/runtime"),
                &config.settings.active_theme_id,
            )
            .is_err()
        {
            config.settings.active_theme_id = "default".into()
        }
        let next_auto =
            Instant::now() + Duration::from_secs(config.settings.refresh_interval_seconds);
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法初始化配额查询"))?;
        let service = Self {
            data: Arc::new(Mutex::new(Data {
                config,
                source: SourceStatus::default(),
                accounts: vec![],
                quotas: HashMap::new(),
                jobs: HashMap::new(),
                in_flight: HashMap::new(),
                cooldown: HashMap::new(),
                failures: HashMap::new(),
                generation: 0,
                revision: 0,
                sequence: 0,
                next_auto,
                refresh_started: HashMap::new(),
                handled_resets: HashSet::new(),
                samples: HashMap::new(),
                clock_pending: HashSet::new(),
                last_wall: Utc::now(),
                last_tick: Instant::now(),
                next_source_scan: Instant::now(),
                position_pending: None,
            })),
            app,
            client,
            semaphore: Arc::new(tokio::sync::Semaphore::new(2)),
            config_path,
            theme_dir: data_dir.join("themes"),
            instance_id: uuid::Uuid::new_v4().to_string(),
        };
        service.rescan()?;
        Ok(service)
    }
    pub fn envelope<T: Serialize>(&self, result: Result<T, ApiError>) -> ApiResult<T> {
        let revision = self.data.lock().map(|d| d.revision).unwrap_or(0);
        let (ok, data, error) = match result {
            Ok(data) => (true, Some(data), None),
            Err(error) => (false, None, Some(error)),
        };
        ApiResult {
            api_version: "1.0".into(),
            instance_id: self.instance_id.clone(),
            revision,
            request_id: uuid::Uuid::new_v4().to_string(),
            generated_at: Utc::now().to_rfc3339(),
            ok,
            data,
            error,
        }
    }
    fn event(&self, event_type: &str, job_id: Option<String>) {
        let event = {
            let mut d = self.data.lock().expect("service state");
            d.revision += 1;
            d.sequence += 1;
            HudEvent {
                api_version: "1.0".into(),
                instance_id: self.instance_id.clone(),
                revision: d.revision,
                sequence: d.sequence,
                emitted_at: Utc::now().to_rfc3339(),
                event_type: event_type.into(),
                data: EventData { job_id },
            }
        };
        if let Some(app) = &self.app {
            let _ = app.emit_to("hud", "hud://v1/event", &event);
            let _ = app.emit_to("settings", "hud://v1/event", &event);
            let _ = app.emit_to("usb-display", "hud://v1/event", &event);
        }
    }
    pub fn settings(&self) -> Settings {
        self.data
            .lock()
            .expect("service state")
            .config
            .settings
            .clone()
    }
    pub fn source_path(&self) -> Option<PathBuf> {
        self.data
            .lock()
            .expect("service state")
            .config
            .source_path
            .clone()
    }
    fn save(&self, config: &Config) -> Result<(), ApiError> {
        config::atomic_json(&self.config_path, config)
    }
    fn invalidate(d: &mut Data) {
        d.generation += 1;
        d.handled_resets.clear();
        for job in d.jobs.values_mut() {
            if job.finished_at.is_none() {
                job.state = "cancelled".into();
                job.finished_at = Some(Utc::now().to_rfc3339());
                for item in &mut job.items {
                    if item.state == "queued" || item.state == "running" {
                        item.state = "cancelled".into()
                    }
                }
            }
        }
        for quota in d.quotas.values_mut() {
            if quota.status == "refreshing" {
                quota.status = if quota.error.is_some() {
                    "error"
                } else if quota.origin == "network" {
                    "ok"
                } else {
                    "unavailable"
                }
                .into()
            }
        }
    }
    pub fn rescan(&self) -> Result<SourceStatus, ApiError> {
        let (root, sources, generation) = {
            let d = self.data.lock().expect("service state");
            (
                d.config.source_path.clone(),
                d.config.sources.clone(),
                d.generation,
            )
        };
        let roots: Vec<PathBuf> = if sources.is_empty() {
            root.clone().into_iter().collect()
        } else {
            sources
                .iter()
                .filter(|s| s.enabled)
                .filter_map(|s| s.path.clone())
                .collect()
        };
        let mut catalog_roots = HashMap::new();
        let result = if roots.is_empty() {
            None
        } else {
            Some((|| {
                let mut combined = crate::adapters::cockpit::SourceCatalog {
                    accounts: vec![],
                    format: "unknown".into(),
                };
                let mut seen = HashSet::new();
                for path in &roots {
                    let canonical = path.canonicalize().map_err(|_| {
                        ApiError::new("SOURCE_NOT_FOUND", "已选择的数据源目录不可用")
                    })?;
                    if !seen.insert(canonical.clone()) {
                        continue;
                    }
                    for mut account in cockpit::list_accounts(&canonical)?.accounts {
                        let original = account.source_id.clone();
                        let key = format!("{}\n{}", canonical.to_string_lossy(), original);
                        catalog_roots.insert(key.clone(), (canonical.clone(), original));
                        account.source_id = key;
                        combined.accounts.push(account);
                    }
                }
                Ok::<_, ApiError>(combined)
            })())
        };
        let mut d = self.data.lock().expect("service state");
        if d.generation != generation || d.config.source_path != root || d.config.sources != sources
        {
            return Err(ApiError::new("CONFLICT", "账号来源已变化，请重新扫描"));
        }
        match result {
            None => {
                d.source = SourceStatus::default();
                d.accounts.clear();
                d.quotas.clear();
            }
            Some(Err(error)) => {
                d.next_source_scan = Instant::now() + Duration::from_secs(30);
                Self::invalidate(&mut d);
                d.source = SourceStatus {
                    state: if error.code == "SOURCE_UNSUPPORTED" {
                        "unsupported"
                    } else {
                        "unavailable"
                    }
                    .into(),
                    format: "unknown".into(),
                    adapter_version: "0.2.0".into(),
                    error: Some(error),
                };
                for quota in d.quotas.values_mut() {
                    quota.status = "unavailable".into();
                    quota.error = Some(ApiError::new("SOURCE_NOT_FOUND", "账号来源暂不可用"));
                }
            }
            Some(Ok(catalog)) => {
                let mut config = d.config.clone();
                let mut accounts = vec![];
                for (index, account) in catalog.accounts.into_iter().enumerate() {
                    let key = account.source_id.clone();
                    let (account_root, original_id) = catalog_roots
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| ApiError::new("INTERNAL_ERROR", "账号来源失效"))?;
                    let id = config
                        .id_map
                        .entry(key)
                        .or_insert_with(|| uuid::Uuid::new_v4().to_string())
                        .clone();
                    let base_name = account
                        .display_name
                        .split('@')
                        .next()
                        .unwrap_or("账号")
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(80)
                        .collect::<String>();
                    let selected = config.selected_ids.contains(&id);
                    let order = config
                        .selected_ids
                        .iter()
                        .position(|x| x == &id)
                        .unwrap_or(index + 100) as u64;
                    let display_name = config
                        .aliases
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| base_name.clone());
                    accounts.push(Entry {
                        summary: AccountSummary {
                            id: id.clone(),
                            display_name,
                            alias: config.aliases.get(&id).cloned(),
                            is_current: account.is_current,
                            provider_id: account.provider_id,
                            selected,
                            order,
                            support: account.support,
                        },
                        source_id: original_id,
                        root: account_root,
                        base_name,
                    });
                }
                let known = accounts
                    .iter()
                    .map(|a| a.summary.id.clone())
                    .collect::<HashSet<_>>();
                config.selected_ids.retain(|id| known.contains(id));
                if config.selected_ids != d.config.selected_ids {
                    config.settings.settings_revision += 1;
                }
                if config.id_map != d.config.id_map || config.selected_ids != d.config.selected_ids
                {
                    self.save(&config)?;
                }
                let old_ids = d
                    .accounts
                    .iter()
                    .map(|x| x.summary.id.clone())
                    .collect::<HashSet<_>>();
                if old_ids != known {
                    Self::invalidate(&mut d);
                }
                d.config = config;
                d.accounts = accounts;
                d.quotas.retain(|id, _| known.contains(id));
                d.source = SourceStatus {
                    state: "ready".into(),
                    format: catalog.format,
                    adapter_version: "0.2.0".into(),
                    error: None,
                };
            }
        }
        let status = d.source.clone();
        drop(d);
        self.event("source.changed", None);
        self.event("snapshot.changed", None);
        Ok(status)
    }
    pub fn sources(&self) -> Vec<cockpit::SourceOption> {
        let d = self.data.lock().expect("service state");
        if !d.config.sources.is_empty() {
            return d.config.sources.clone();
        }
        let mut options = cockpit::defaults();
        if let Some(path) = &d.config.source_path {
            let kind = if path.join("codex_accounts.json").exists()
                || path.join("accounts.json").exists()
            {
                "cockpit"
            } else {
                "official"
            };
            if let Some(s) = options.iter_mut().find(|s| s.id == kind) {
                s.enabled = true;
                s.path = Some(path.clone());
            }
        }
        options
    }
    pub fn set_sources(
        &self,
        mut sources: Vec<cockpit::SourceOption>,
    ) -> Result<SourceStatus, ApiError> {
        if sources.len() != cockpit::KINDS.len()
            || sources.iter().map(|s| &s.id).collect::<HashSet<_>>().len() != sources.len()
            || sources.iter().any(|s| {
                !cockpit::KINDS.contains(&s.id.as_str()) || (s.enabled && s.path.is_none())
            })
        {
            return Err(ApiError::new("INVALID_ARGUMENT", "请选择有效的数据源目录"));
        }
        for s in &mut sources {
            if s.enabled {
                s.path = Some(cockpit::resolve_root(s.path.as_deref().unwrap(), &s.id)?);
            }
        }
        {
            let mut d = self.data.lock().expect("service state");
            let mut config = d.config.clone();
            config.sources = sources;
            config.settings.settings_revision += 1;
            self.save(&config)?;
            Self::invalidate(&mut d);
            d.config = config;
        }
        self.event("settings.changed", None);
        self.rescan()
    }
    pub fn choose_source(&self, path: PathBuf) -> Result<SourceStatus, ApiError> {
        let root = path
            .canonicalize()
            .map_err(|_| ApiError::new("SOURCE_NOT_FOUND", "所选目录无法读取"))?;
        cockpit::list_accounts(&root)?;
        {
            let mut d = self.data.lock().expect("service state");
            let mut config = d.config.clone();
            config.source_path = Some(root);
            config.sources.clear();
            config.selected_ids.clear();
            config.settings.settings_revision += 1;
            self.save(&config)?;
            Self::invalidate(&mut d);
            d.config = config;
            d.accounts.clear();
            d.quotas.clear();
            d.cooldown.clear();
        }
        self.event("settings.changed", None);
        self.rescan()
    }
    #[cfg(test)]
    pub fn accounts(&self, all: bool) -> Vec<AccountSummary> {
        let d = self.data.lock().expect("service state");
        let mut result = d
            .accounts
            .iter()
            .filter(|a| all || a.summary.selected)
            .map(|a| a.summary.clone())
            .collect::<Vec<_>>();
        result.sort_by_key(|a| a.order);
        if d.config.settings.display.privacy_mode {
            for (i, a) in result.iter_mut().enumerate() {
                a.alias = None;
                a.display_name = format!(
                    "{} {}",
                    if d.config.settings.display.locale == "zh-CN" {
                        "账号"
                    } else {
                        "Account"
                    },
                    i + 1
                )
            }
        }
        result
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut d = self.data.lock().expect("service state");
        Self::snapshot_locked(&mut d)
    }
    fn snapshot_locked(d: &mut Data) -> Snapshot {
        let now = Utc::now();
        let interval = d.config.settings.refresh_interval_seconds;
        let mut accounts = d
            .accounts
            .iter()
            .filter(|a| a.summary.selected)
            .map(|a| a.summary.clone())
            .collect::<Vec<_>>();
        accounts.sort_by_key(|a| a.order);
        if d.config.settings.display.privacy_mode {
            for (i, a) in accounts.iter_mut().enumerate() {
                a.alias = None;
                a.display_name = format!(
                    "{} {}",
                    if d.config.settings.display.locale == "zh-CN" {
                        "账号"
                    } else {
                        "Account"
                    },
                    i + 1
                )
            }
        }
        let source_ready = d.source.state == "ready";
        let quotas = accounts
            .iter()
            .map(|a| {
                let interval = d
                    .config
                    .settings
                    .account_interval(&a.id)
                    .unwrap_or(interval);
                let elapsed = d.samples.get(&a.id).map(Instant::elapsed);
                let pending = d.clock_pending.contains(&a.id);
                let quota = d
                    .quotas
                    .entry(a.id.clone())
                    .or_insert_with(|| AccountQuota::empty(&a.id));
                domain::update_freshness(quota, now, interval);
                if elapsed
                    .is_some_and(|elapsed| elapsed.as_secs() >= interval.saturating_mul(2).max(120))
                {
                    quota.freshness = "stale".into();
                }
                let mut result = quota.clone();
                if !source_ready || pending {
                    result.status = "unavailable".into();
                    result.error = Some(ApiError::new(
                        "SOURCE_NOT_FOUND",
                        if pending {
                            "系统时间变化，等待重新查询"
                        } else {
                            "账号来源暂不可用"
                        },
                    ));
                }
                result
            })
            .collect::<Vec<_>>();
        let availability = quotas
            .iter()
            .map(|q| domain::evaluate(q, now))
            .collect::<Vec<_>>();
        let recommendation = domain::recommend(&accounts, &availability, &quotas);
        let mut display = d.config.settings.display.clone();
        let all_accounts: Vec<_> = d.accounts.iter().map(|a| a.summary.clone()).collect();
        display.quota_provider =
            crate::quota_total::effective_provider(&display.quota_provider, &all_accounts).into();
        let total_quota = crate::quota_total::calculate(&accounts, &quotas, now, &display);
        Snapshot {
            source: d.source.clone(),
            accounts,
            quotas,
            availability,
            recommendation,
            total_quota,
            next_refresh_at: d
                .config
                .selected_ids
                .iter()
                .filter_map(|id| {
                    let interval = d.config.settings.account_interval(id)?;
                    let next = if d.config.settings.account_refresh.contains_key(id) {
                        d.refresh_started
                            .get(id)
                            .map(|at| *at + Duration::from_secs(interval))
                            .unwrap_or_else(Instant::now)
                    } else {
                        d.next_auto
                    };
                    Some(next)
                })
                .min()
                .map(|next| {
                    (now + chrono::Duration::from_std(
                        next.saturating_duration_since(Instant::now()),
                    )
                    .unwrap_or_default())
                    .to_rfc3339()
                }),
        }
    }
    /// Authoritative data and its revision are captured under the same state lock.
    pub fn read_reply(&self, kind: &str, all: bool) -> ApiResult<Value> {
        let mut d = self.data.lock().expect("service state");
        let result = match kind {
            "snapshot" => serde_json::to_value(Self::snapshot_locked(&mut d))
                .map_err(|_| ApiError::new("INTERNAL_ERROR", "快照不可用")),
            "recommendation" => serde_json::to_value(Self::snapshot_locked(&mut d).recommendation)
                .map_err(|_| ApiError::new("INTERNAL_ERROR", "推荐不可用")),
            "settings" => serde_json::to_value(&d.config.settings)
                .map_err(|_| ApiError::new("INTERNAL_ERROR", "设置不可用")),
            "accounts" => {
                let mut values = d
                    .accounts
                    .iter()
                    .filter(|a| all || a.summary.selected)
                    .map(|a| a.summary.clone())
                    .collect::<Vec<_>>();
                values.sort_by_key(|a| a.order);
                if d.config.settings.display.privacy_mode {
                    for (i, a) in values.iter_mut().enumerate() {
                        a.alias = None;
                        a.display_name = format!(
                            "{} {}",
                            if d.config.settings.display.locale == "zh-CN" {
                                "账号"
                            } else {
                                "Account"
                            },
                            i + 1
                        )
                    }
                }
                serde_json::to_value(values)
                    .map_err(|_| ApiError::new("INTERNAL_ERROR", "账号不可用"))
            }
            _ if kind.starts_with("job:") => d
                .jobs
                .get(&kind[4..])
                .ok_or_else(|| ApiError::new("NOT_FOUND", "刷新任务不存在或已过期"))
                .and_then(|j| {
                    serde_json::to_value(j)
                        .map_err(|_| ApiError::new("INTERNAL_ERROR", "任务不可用"))
                }),
            _ => Err(ApiError::new("NOT_FOUND", "接口不存在")),
        };
        let (ok, data, error) = match result {
            Ok(v) => (true, Some(v), None),
            Err(e) => (false, None, Some(e)),
        };
        ApiResult {
            api_version: "1.0".into(),
            instance_id: self.instance_id.clone(),
            revision: d.revision,
            request_id: uuid::Uuid::new_v4().to_string(),
            generated_at: Utc::now().to_rfc3339(),
            ok,
            data,
            error,
        }
    }
    pub fn select_accounts(&self, patch: AccountSelectionPatch) -> Result<Settings, ApiError> {
        let mut d = self.data.lock().expect("service state");
        if patch.expected_revision != d.config.settings.settings_revision {
            return Err(ApiError::new("CONFLICT", "设置已更新，请重新读取后再保存"));
        }
        let ids = patch.account_ids.iter().cloned().collect::<HashSet<_>>();
        if ids.len() != patch.account_ids.len()
            || ids.len() > 100
            || ids
                .iter()
                .any(|id| !d.accounts.iter().any(|a| &a.summary.id == id))
        {
            return Err(ApiError::new(
                "INVALID_ARGUMENT",
                "请选择有效且不重复的账号，最多 100 个",
            ));
        }
        let mut config = d.config.clone();
        config.selected_ids = patch.account_ids;
        if let Some(aliases) = patch.aliases {
            for (id, alias) in aliases {
                if !d.accounts.iter().any(|a| a.summary.id == id)
                    || alias.chars().count() > 80
                    || alias.chars().any(char::is_control)
                {
                    return Err(ApiError::new("INVALID_ARGUMENT", "账号别名无效"));
                }
                if alias.trim().is_empty() {
                    config.aliases.remove(&id);
                } else {
                    config.aliases.insert(id, alias.trim().into());
                }
            }
        }
        config.settings.settings_revision += 1;
        self.save(&config)?;
        Self::invalidate(&mut d);
        d.config = config;
        let selected = d.config.selected_ids.clone();
        let aliases = d.config.aliases.clone();
        for entry in &mut d.accounts {
            entry.summary.selected = selected.contains(&entry.summary.id);
            entry.summary.order = selected
                .iter()
                .position(|id| id == &entry.summary.id)
                .unwrap_or(100) as u64;
            entry.summary.alias = aliases.get(&entry.summary.id).cloned();
            entry.summary.display_name = aliases
                .get(&entry.summary.id)
                .cloned()
                .unwrap_or_else(|| entry.base_name.clone());
        }
        d.quotas.retain(|id, _| ids.contains(id));
        let settings = d.config.settings.clone();
        drop(d);
        self.event("settings.changed", None);
        self.event("snapshot.changed", None);
        Ok(settings)
    }
    pub fn set_alias(&self, id: &str, alias: &str) -> Result<Settings, ApiError> {
        let mut d = self.data.lock().expect("service state");
        if alias.chars().count() > 80 || alias.chars().any(char::is_control) {
            return Err(ApiError::new(
                "INVALID_ARGUMENT",
                "Display name must be at most 80 characters",
            ));
        }
        if !d.accounts.iter().any(|entry| entry.summary.id == id) {
            return Err(ApiError::new("NOT_FOUND", "Account not found"));
        }
        let mut config = d.config.clone();
        if alias.trim().is_empty() {
            config.aliases.remove(id);
        } else {
            config
                .aliases
                .insert(id.to_owned(), alias.trim().to_owned());
        }
        config.settings.settings_revision += 1;
        self.save(&config)?;
        let saved = config.aliases.get(id).cloned();
        for entry in &mut d.accounts {
            if entry.summary.id == id {
                entry.summary.alias = saved.clone();
                entry.summary.display_name =
                    saved.clone().unwrap_or_else(|| entry.base_name.clone());
            }
        }
        d.config = config;
        let settings = d.config.settings.clone();
        drop(d);
        self.event("settings.changed", None);
        self.event("snapshot.changed", None);
        Ok(settings)
    }

    pub fn update_settings(&self, patch: SettingsPatch) -> Result<Settings, ApiError> {
        let mut d = self.data.lock().expect("service state");
        if patch.expected_revision != d.config.settings.settings_revision {
            return Err(ApiError::new("CONFLICT", "设置已更新，请重新读取后再保存"));
        }
        let mut config = d.config.clone();
        if let Some(proxies) = patch.proxies {
            let mut ids = std::collections::HashSet::new();
            if proxies.len() > 100
                || proxies.iter().any(|p| {
                    p.id.is_empty()
                        || !ids.insert(&p.id)
                        || p.name.len() > 200
                        || p.address.len() > 2048
                })
            {
                return Err(ApiError::new("INVALID_ARGUMENT", "代理设置无效"));
            }
            config
                .settings
                .account_proxies
                .retain(|_, id| proxies.iter().any(|p| &p.id == id));
            config.settings.proxies = proxies;
        }
        if let Some(overrides) = patch.account_proxies {
            if overrides.iter().any(|(account, proxy)| {
                !d.accounts.iter().any(|a| &a.summary.id == account)
                    || !config.settings.proxies.iter().any(|p| &p.id == proxy)
            }) {
                return Err(ApiError::new("INVALID_ARGUMENT", "账号代理设置无效"));
            }
            config.settings.account_proxies = overrides;
        }
        if let Some(overrides) = patch.account_refresh {
            if overrides.iter().any(|(id, v)| {
                ![0, 60, 300, 900, 3600].contains(v)
                    || !d.accounts.iter().any(|a| a.summary.id == *id)
            }) {
                return Err(ApiError::new("INVALID_ARGUMENT", "账号刷新设置无效"));
            }
            config.settings.account_refresh = overrides;
        }
        if let Some(usb) = patch.usb_display {
            if crate::themes::builtin(&usb.theme_id).is_none()
                || (usb.enabled && usb.device_id.is_empty())
            {
                return Err(ApiError::new("INVALID_ARGUMENT", "请选择屏幕和内置主题"));
            }
            config.settings.usb_display = usb;
        }
        if let Some(interval) = patch.refresh_interval_seconds {
            if !(60..=1800).contains(&interval) {
                return Err(ApiError::new("INVALID_ARGUMENT", "刷新间隔须为 1–30 分钟"));
            }
            config.settings.refresh_interval_seconds = interval;
        }
        if let Some(enabled) = patch.auto_refresh {
            config.settings.auto_refresh = enabled;
        }
        if let Some(display) = patch.display {
            if let Some(v) = display.quota_provider {
                if !crate::quota_total::PROVIDERS.contains(&v.as_str())
                    || !d
                        .accounts
                        .iter()
                        .any(|a| crate::quota_total::provider(&a.summary.provider_id) == v)
                {
                    return Err(ApiError::new("INVALID_ARGUMENT", "请选择已有账号的平台"));
                }
                config.settings.display.quota_provider = v;
            }
            if let Some(profiles) = display.quota_profiles {
                if profiles.iter().any(|(id, value)| {
                    !d.accounts.iter().any(|a| {
                        a.summary.id == *id
                            && crate::quota_total::valid_profile(
                                crate::quota_total::provider(&a.summary.provider_id),
                                value,
                            )
                    })
                }) {
                    return Err(ApiError::new("INVALID_ARGUMENT", "订阅换算设置无效"));
                }
                config.settings.display.quota_profiles = profiles;
            }
            if let Some(v) = display.position_locked {
                config.settings.display.position_locked = v;
            }
            if let Some(v) = display.always_on_top {
                config.settings.display.always_on_top = v
            }
            if let Some(v) = display.show_hover_details {
                config.settings.display.show_hover_details = v
            }
            if let Some(v) = display.privacy_mode {
                config.settings.display.privacy_mode = v
            }
            if let Some(v) = display.locale {
                if !["system", "zh-CN", "en"].contains(&v.as_str()) {
                    return Err(ApiError::new("INVALID_ARGUMENT", "语言设置无效"));
                }
                config.settings.display.locale = v
            }
        }
        config.settings.settings_revision += 1;
        self.save(&config)?;
        d.next_auto =
            Instant::now() + Duration::from_secs(config.settings.refresh_interval_seconds);
        d.config = config;
        let settings = d.config.settings.clone();
        drop(d);
        self.event("settings.changed", None);
        self.event("snapshot.changed", None);
        Ok(settings)
    }
    pub fn select_theme(&self, id: &str, expected_revision: u64) -> Result<Settings, ApiError> {
        if themes::builtin(id).is_none() {
            crate::theme_package::installed(&self.theme_dir.join("runtime"), id)?;
        }
        let mut d = self.data.lock().expect("service state");
        if expected_revision != d.config.settings.settings_revision {
            return Err(ApiError::new("CONFLICT", "设置已更新，请重新读取后再保存"));
        }
        let mut config = d.config.clone();
        config.settings.active_theme_id = id.into();
        config.settings.settings_revision += 1;
        self.save(&config)?;
        d.config = config;
        let settings = d.config.settings.clone();
        drop(d);
        self.event("theme.changed", None);
        self.event("settings.changed", None);
        Ok(settings)
    }
    #[cfg(test)]
    pub fn job(&self, id: &str) -> Result<RefreshJob, ApiError> {
        self.data
            .lock()
            .expect("service state")
            .jobs
            .get(id)
            .cloned()
            .ok_or_else(|| ApiError::new("NOT_FOUND", "刷新任务不存在或已过期"))
    }
    pub fn refresh(&self, request: RefreshRequest) -> Result<RefreshTicket, ApiError> {
        self.rescan()?;
        let now = Utc::now();
        let mut d = self.data.lock().expect("service state");
        if d.source.state != "ready" {
            return Err(d.source.error.clone().unwrap_or_else(|| {
                ApiError::new("SOURCE_NOT_FOUND", "请先选择 Cockpit 数据目录")
            }));
        }
        let ids = request
            .account_ids
            .unwrap_or_else(|| d.config.selected_ids.clone());
        let set = ids.iter().cloned().collect::<HashSet<_>>();
        if ids.is_empty()
            || ids.len() > 100
            || set.len() != ids.len()
            || ids.iter().any(|id| !d.config.selected_ids.contains(id))
        {
            return Err(ApiError::new(
                "INVALID_ARGUMENT",
                "请先选择需要显示和刷新的账号",
            ));
        }
        let active = ids
            .iter()
            .filter_map(|id| d.in_flight.get(id).cloned())
            .collect::<HashSet<_>>();
        if !active.is_empty() {
            if active.len() == 1 {
                let job_id = active.iter().next().unwrap().clone();
                if let Some(job) = d.jobs.get(&job_id) {
                    let previous = job
                        .items
                        .iter()
                        .map(|i| i.account_id.clone())
                        .collect::<HashSet<_>>();
                    if job.finished_at.is_none() && previous == set {
                        return Ok(RefreshTicket {
                            job_id,
                            joined: true,
                        });
                    }
                }
            }
            return Err(ApiError::new(
                "RATE_LIMITED",
                "部分账号正在刷新，请稍后重试",
            ));
        }
        d.jobs.retain(|_, job| {
            job.finished_at
                .as_ref()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .map(|t| now.signed_duration_since(t).num_minutes() < 30)
                .unwrap_or(true)
        });
        if d.jobs.len() >= 100 {
            if let Some(oldest) = d
                .jobs
                .values()
                .filter(|j| j.finished_at.is_some())
                .min_by_key(|j| j.created_at.clone())
                .map(|j| j.job_id.clone())
            {
                d.jobs.remove(&oldest);
            } else {
                return Err(ApiError::new("RATE_LIMITED", "刷新任务已满，请稍后重试"));
            }
        }
        let mut eligible = vec![];
        let mut items = vec![];
        for id in &ids {
            if let Some(retry) = d.cooldown.get(id).filter(|t| **t > now) {
                let mut error = ApiError::new("RATE_LIMITED", "请等待刷新冷却结束");
                error.retry_at = Some(retry.to_rfc3339());
                items.push(RefreshItem {
                    account_id: id.clone(),
                    state: "failed".into(),
                    error: Some(error),
                });
            } else {
                eligible.push(id.clone());
                items.push(RefreshItem {
                    account_id: id.clone(),
                    state: "queued".into(),
                    error: None,
                });
            }
        }
        if eligible.is_empty() {
            return Err(items
                .into_iter()
                .find_map(|i| i.error)
                .unwrap_or_else(|| ApiError::new("RATE_LIMITED", "请稍后刷新")));
        }
        let job_id = uuid::Uuid::new_v4().to_string();
        let generation = d.generation;

        let work = eligible
            .iter()
            .filter_map(|id| {
                d.accounts
                    .iter()
                    .find(|a| &a.summary.id == id)
                    .map(|a| (id.clone(), a.source_id.clone(), a.root.clone()))
            })
            .collect::<Vec<_>>();
        for id in eligible {
            d.refresh_started.insert(id.clone(), Instant::now());
            d.in_flight.insert(id.clone(), job_id.clone());
            d.cooldown
                .insert(id.clone(), now + chrono::Duration::seconds(15));
            let quota = d
                .quotas
                .entry(id.clone())
                .or_insert_with(|| AccountQuota::empty(id));
            quota.status = "refreshing".into();
            quota.last_attempt_at = Some(now.to_rfc3339());
        }
        d.jobs.insert(
            job_id.clone(),
            RefreshJob {
                job_id: job_id.clone(),
                state: "running".into(),
                created_at: now.to_rfc3339(),
                finished_at: None,
                items,
            },
        );
        drop(d);
        self.event("refresh.progress", Some(job_id.clone()));
        self.event("snapshot.changed", None);
        for (id, source_id, root) in work {
            let service = self.clone();
            let job = job_id.clone();
            tauri::async_runtime::spawn(async move {
                service
                    .run_account(job, id, source_id, root, generation)
                    .await;
            });
        }
        Ok(RefreshTicket {
            job_id,
            joined: false,
        })
    }
    fn account_client(&self, id: &str) -> Result<reqwest::Client, ApiError> {
        let settings = self.settings();
        let Some(proxy_id) = settings.account_proxies.get(id) else {
            return Ok(self.client.clone());
        };
        let proxy = settings
            .proxies
            .iter()
            .find(|p| &p.id == proxy_id)
            .ok_or_else(|| ApiError::new("INVALID_ARGUMENT", "所选代理不存在"))?;
        crate::network::proxy_client(&proxy.address)
    }

    async fn run_account(
        &self,
        job_id: String,
        id: String,
        source_id: String,
        root: PathBuf,
        generation: u64,
    ) {
        let permit = tokio::time::timeout(
            Duration::from_secs(60),
            self.semaphore.clone().acquire_owned(),
        )
        .await;
        let result = match permit {
            Ok(Ok(_permit)) => {
                {
                    let mut d = self.data.lock().expect("service state");
                    if d.generation != generation {
                        if d.in_flight.get(&id) == Some(&job_id) {
                            d.in_flight.remove(&id);
                        }
                        return;
                    }
                    if let Some(job) = d.jobs.get_mut(&job_id) {
                        if let Some(item) = job.items.iter_mut().find(|i| i.account_id == id) {
                            item.state = "running".into()
                        }
                    }
                }
                let root_copy = root.clone();
                let source_copy = source_id.clone();
                match tokio::task::spawn_blocking(move || {
                    cockpit::read_credentials(&root_copy, &source_copy)
                })
                .await
                {
                    Ok(Ok(credentials)) => {
                        {
                            let mut d = self.data.lock().expect("service state");
                            if d.generation != generation {
                                if d.in_flight.get(&id) == Some(&job_id) {
                                    d.in_flight.remove(&id);
                                }
                                return;
                            }
                        }
                        let client = self.account_client(&id);
                        let result = match &client {
                            Ok(client) => usage::fetch_quota(client, &credentials, &id).await,
                            Err(error) => Err(error.clone()),
                        };
                        if result
                            .as_ref()
                            .err()
                            .is_some_and(|e| e.code == "AUTH_EXPIRED")
                        {
                            let root_copy = root.clone();
                            let source_copy = source_id.clone();
                            match tokio::task::spawn_blocking(move || {
                                cockpit::read_credentials(&root_copy, &source_copy)
                            })
                            .await
                            {
                                Ok(Ok(new)) if new.access_token != credentials.access_token => {
                                    let valid = {
                                        let d = self.data.lock().expect("service state");
                                        d.generation == generation
                                    };
                                    if valid {
                                        usage::fetch_quota(
                                            client.as_ref().expect("successful initial client"),
                                            &new,
                                            &id,
                                        )
                                        .await
                                    } else {
                                        result
                                    }
                                }
                                _ => result,
                            }
                        } else {
                            result
                        }
                    }
                    Ok(Err(error)) => Err(error),
                    Err(_) => Err(ApiError::new("IO_ERROR", "读取账号时发生错误")),
                }
            }
            _ => Err(ApiError::new("TIMEOUT", "账号刷新排队超时，请稍后重试")),
        };
        let now = Utc::now();
        let mut d = self.data.lock().expect("service state");
        if d.generation != generation || d.in_flight.get(&id) != Some(&job_id) {
            if d.in_flight.get(&id) == Some(&job_id) {
                d.in_flight.remove(&id);
            }
            return;
        }
        d.in_flight.remove(&id);
        let succeeded = result.is_ok();
        let error = result.as_ref().err().cloned();
        match result {
            Ok(mut quota) => {
                let interval = d
                    .config
                    .settings
                    .account_interval(&id)
                    .unwrap_or(d.config.settings.refresh_interval_seconds);
                domain::update_freshness(&mut quota, now, interval);
                let past_reset = quota.windows.iter().any(|w| {
                    w.scope == "base"
                        && w.applicability == "required"
                        && w.resets_at.as_ref().is_some_and(|t| {
                            DateTime::parse_from_rfc3339(t).is_ok_and(|t| t <= now)
                        })
                });
                if past_reset {
                    let count = d.failures.entry(id.clone()).or_default();
                    *count += 1;
                    let delay = 30_u64
                        .saturating_mul(2_u64.saturating_pow((*count - 1).min(6)))
                        .min(1800);
                    d.cooldown
                        .insert(id.clone(), now + chrono::Duration::seconds(delay as i64));
                } else {
                    d.failures.remove(&id);
                }
                d.samples.insert(id.clone(), Instant::now());
                d.clock_pending.remove(&id);
                d.quotas.insert(id.clone(), quota);
                if let Some(a) = d.accounts.iter_mut().find(|a| a.summary.id == id) {
                    a.summary.support = "supported".into()
                }
            }
            Err(error) => {
                let count = d.failures.entry(id.clone()).or_default();
                *count += 1;
                let seconds = 30_u64
                    .saturating_mul(2_u64.saturating_pow((*count - 1).min(6)))
                    .min(1800);
                let retry = error
                    .retry_at
                    .as_ref()
                    .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                    .map(|t| t.with_timezone(&Utc))
                    .unwrap_or(
                        now + chrono::Duration::seconds(if error.retryable {
                            seconds as i64
                        } else {
                            15
                        }),
                    );
                d.cooldown.insert(id.clone(), retry);
                if error.code == "ACCOUNT_UNSUPPORTED" {
                    if let Some(a) = d.accounts.iter_mut().find(|a| a.summary.id == id) {
                        a.summary.support = "unsupported".into()
                    }
                }
                let quota = d
                    .quotas
                    .entry(id.clone())
                    .or_insert_with(|| AccountQuota::empty(&id));
                quota.status = match error.code.as_str() {
                    "AUTH_EXPIRED" => "auth_expired",
                    "ACCOUNT_UNSUPPORTED" => "unavailable",
                    _ => "error",
                }
                .into();
                quota.last_attempt_at = Some(now.to_rfc3339());
                quota.error = Some(error);
            }
        }
        let mut finished = false;
        if let Some(job) = d.jobs.get_mut(&job_id) {
            if let Some(item) = job.items.iter_mut().find(|i| i.account_id == id) {
                item.state = if succeeded { "succeeded" } else { "failed" }.into();
                item.error = error;
            }
            if job
                .items
                .iter()
                .all(|i| i.state == "succeeded" || i.state == "failed" || i.state == "cancelled")
            {
                let count = job.items.iter().filter(|i| i.state == "succeeded").count();
                job.state = if count == job.items.len() {
                    "completed"
                } else if count == 0 {
                    "failed"
                } else {
                    "partial"
                }
                .into();
                job.finished_at = Some(now.to_rfc3339());
                finished = true;
            }
        }
        drop(d);
        self.event("snapshot.changed", None);
        self.event(
            if finished {
                "refresh.completed"
            } else {
                "refresh.progress"
            },
            Some(job_id),
        );
    }
    pub fn tick(&self) {
        let now = Utc::now();
        let mut d = self.data.lock().expect("service state");
        let interval = d.config.settings.refresh_interval_seconds;
        let mut changed = false;
        let mut due = vec![];
        let mut reset_keys = vec![];
        let rescan_due = d
            .config
            .selected_ids
            .iter()
            .any(|id| d.config.settings.account_interval(id).is_some())
            && d.source.state != "ready"
            && (d.config.source_path.is_some() || d.config.sources.iter().any(|s| s.enabled))
            && Instant::now() >= d.next_source_scan;
        if rescan_due {
            d.next_source_scan = Instant::now() + Duration::from_secs(30);
        }
        let elapsed = d.last_tick.elapsed().as_secs_f64();
        let wall = (now - d.last_wall).num_milliseconds() as f64 / 1000.0;
        if (wall - elapsed).abs() > 5.0 {
            Self::invalidate(&mut d);
            d.clock_pending = d.config.selected_ids.iter().cloned().collect();
            changed = true;
        }
        d.last_tick = Instant::now();
        d.last_wall = now;
        let refresh_settings = d.config.settings.clone();
        for (id, quota) in &mut d.quotas {
            let interval = refresh_settings.account_interval(id).unwrap_or(interval);
            let old = quota.freshness.clone();
            domain::update_freshness(quota, now, interval);
            changed |= old != quota.freshness;
            for window in &quota.windows {
                if window.scope == "base" && window.applicability == "required" {
                    if let Some(reset) = &window.resets_at {
                        if DateTime::parse_from_rfc3339(reset).is_ok_and(|t| t <= now) {
                            reset_keys.push((id.clone(), format!("{id}:{reset}")))
                        }
                    }
                }
            }
        }
        if d.config.settings.auto_refresh && Instant::now() >= d.next_auto {
            d.next_auto = Instant::now() + Duration::from_secs(interval);
            due = d
                .config
                .selected_ids
                .iter()
                .filter(|id| !d.config.settings.account_refresh.contains_key(*id))
                .cloned()
                .collect();
        }
        for id in &d.config.selected_ids {
            if let Some(seconds) = d
                .config
                .settings
                .account_refresh
                .get(id)
                .filter(|s| **s > 0)
            {
                if d.refresh_started
                    .get(id)
                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(*seconds))
                {
                    due.push(id.clone());
                }
            }
        }
        for (id, key) in reset_keys {
            if d.handled_resets.insert(key) {
                changed = true;
            }
            due.push(id);
        }
        due.extend(d.clock_pending.iter().cloned());
        if d.source.state != "ready" {
            due.clear();
        }
        due.sort();
        due.dedup();
        due.retain(|id| {
            d.config.settings.account_interval(id).is_some()
                && !d.in_flight.contains_key(id)
                && d.cooldown.get(id).is_none_or(|t| *t <= now)
                && d.config.selected_ids.contains(id)
        });
        let save_position = d
            .position_pending
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(500));
        drop(d);
        if save_position {
            self.flush_position();
        }
        if rescan_due {
            let _ = self.rescan();
        }
        if changed {
            self.event("snapshot.changed", None)
        }
        if !due.is_empty() {
            let _ = self.refresh(RefreshRequest {
                account_ids: Some(due),
            });
        }
    }
    pub fn start_background(&self) {
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            let ids = {
                let d = service.data.lock().expect("service state");
                d.config
                    .selected_ids
                    .iter()
                    .filter(|id| d.config.settings.account_interval(id).is_some())
                    .cloned()
                    .collect::<Vec<_>>()
            };
            if !ids.is_empty() {
                let _ = service.refresh(RefreshRequest {
                    account_ids: Some(ids),
                });
            }
            let mut timer = tokio::time::interval(Duration::from_secs(1));
            loop {
                timer.tick().await;
                service.tick();
            }
        });
    }
    pub fn diagnostics(&self) -> Diagnostics {
        let d = self.data.lock().expect("service state");
        Diagnostics {
            app_version: env!("CARGO_PKG_VERSION").into(),
            adapter_version: "0.2.0".into(),
            source: d.source.clone(),
            selected_account_count: d.config.selected_ids.len(),
            active_job_count: d.jobs.values().filter(|j| j.finished_at.is_none()).count(),
            recent_error_codes: d
                .quotas
                .values()
                .filter_map(|q| q.error.as_ref().map(|e| e.code.clone()))
                .collect(),
        }
    }
    pub fn remember_position(&self, x: i32, y: i32) {
        let mut d = self.data.lock().expect("service state");
        d.config.window_position = Some((x, y));
        d.position_pending = Some(Instant::now());
    }
    pub fn flush_position(&self) {
        let mut d = self.data.lock().expect("service state");
        if d.position_pending.is_some() && self.save(&d.config).is_ok() {
            d.position_pending = None;
        }
    }
    pub fn saved_position(&self) -> Option<(i32, i32)> {
        self.data
            .lock()
            .expect("service state")
            .config
            .window_position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn multiple_sources_preserve_ids_and_route_each_account_to_its_root() {
        let (dir, service, id) = synthetic_service();
        let root = service.source_path().unwrap();
        let proxy = dir.path().join("proxy");
        std::fs::create_dir(&proxy).unwrap();
        std::fs::write(proxy.join("claude.json"),json!({"type":"claude","email":"synthetic@example.test","access_token":"synthetic-claude"}).to_string()).unwrap();
        let mut options = cockpit::defaults();
        for s in &mut options {
            s.enabled = false;
            s.path = None;
            if s.id == "cockpit" {
                s.enabled = true;
                s.path = Some(root.clone());
            }
            if s.id == "cliproxyapi" {
                s.enabled = true;
                s.path = Some(proxy.clone());
            }
        }
        service.set_sources(options.clone()).unwrap();
        assert_eq!(service.accounts(true).len(), 2);
        assert!(service.accounts(true).iter().any(|a| a.id == id));
        let d = service.data.lock().unwrap();
        let claude = d
            .accounts
            .iter()
            .find(|a| a.summary.provider_id == "claude")
            .unwrap();
        assert_eq!(
            cockpit::read_credentials(&claude.root, &claude.source_id)
                .unwrap()
                .access_token,
            "synthetic-claude"
        );
        drop(d);
        let raw = std::fs::read_to_string(dir.path().join("hud/settings.json")).unwrap();
        assert!(!raw.contains("synthetic-claude"));
        let reopened = Service::new(dir.path().join("hud"), None).unwrap();
        assert_eq!(reopened.accounts(true).len(), 2);
        for s in &mut options {
            s.enabled = false;
        }
        reopened.set_sources(options).unwrap();
        assert!(reopened.accounts(true).is_empty());
        assert_eq!(reopened.snapshot().source.state, "not_configured");
    }
    #[test]
    fn aliases_persist_without_changing_selection_or_refresh_generation() {
        let (dir, service, id) = synthetic_service();
        let generation = service.data.lock().unwrap().generation;
        let selected = service.data.lock().unwrap().config.selected_ids.clone();
        service.set_alias(&id, "  My account  ").unwrap();
        assert_eq!(service.accounts(true)[0].display_name, "My account");
        assert_eq!(service.data.lock().unwrap().generation, generation);
        assert_eq!(service.data.lock().unwrap().config.selected_ids, selected);
        let reopened = Service::new(dir.path().join("hud"), None).unwrap();
        assert_eq!(
            reopened.accounts(true)[0].alias.as_deref(),
            Some("My account")
        );
        assert!(reopened.set_alias(&id, &"x".repeat(81)).is_err());
        assert!(reopened.set_alias(&id, "bad\nname").is_err());
        assert!(reopened.set_alias("missing", "name").is_err());
        reopened
            .data
            .lock()
            .unwrap()
            .config
            .settings
            .display
            .privacy_mode = true;
        let reply = reopened.read_reply("accounts", true).data.unwrap();
        assert_eq!(reply[0]["displayName"], "Account 1");
        assert!(reply[0]["alias"].is_null());
        reopened
            .data
            .lock()
            .unwrap()
            .config
            .settings
            .display
            .privacy_mode = false;
        reopened.set_alias(&id, "").unwrap();
        assert_eq!(reopened.accounts(true)[0].display_name, "synthetic-name");
    }

    #[test]
    fn proxy_profiles_persist_and_deletion_restores_system() {
        let (dir, service, _) = synthetic_service();
        let id = service.accounts(true)[0].id.clone();
        let update = |value| service.update_settings(serde_json::from_value(value).unwrap());
        update(
            json!({"expectedRevision":service.settings().settings_revision,
            "proxies":[{"id":"cloud","name":"Tencent","address":"socks5://127.0.0.1:7893"}],
            "accountProxies":{id.clone():"cloud"}}),
        )
        .unwrap();
        assert!(service.account_client(&id).is_ok());
        let reopened = Service::new(dir.path().join("hud"), None).unwrap();
        assert_eq!(
            reopened
                .settings()
                .account_proxies
                .get(&id)
                .map(String::as_str),
            Some("cloud")
        );
        update(
            json!({"expectedRevision":service.settings().settings_revision,
            "proxies":[{"id":"cloud","name":"Tencent","address":"socks5://"}]}),
        )
        .unwrap();
        assert!(service.account_client(&id).is_err()); // Never fall back to the system path.
        update(json!({"expectedRevision":service.settings().settings_revision,"proxies":[]}))
            .unwrap();
        assert!(service.settings().account_proxies.is_empty());
        assert!(service.account_client(&id).is_ok());
    }

    #[test]
    fn language_defaults_to_english_and_persists_chinese() {
        let (dir, service, _) = synthetic_service();
        assert_eq!(service.settings().display.locale, "en");
        let patch = serde_json::from_value(json!({"expectedRevision":service.settings().settings_revision,"display":{"locale":"zh-CN"}})).unwrap();
        service.update_settings(patch).unwrap();
        assert_eq!(
            Service::new(dir.path().join("hud"), None)
                .unwrap()
                .settings()
                .display
                .locale,
            "zh-CN"
        );
    }
    #[test]
    fn config_conflicts_and_unknown_accounts_do_not_commit() {
        let dir = tempfile::tempdir().unwrap();
        let service = Service::new(dir.path().into(), None).unwrap();
        let patch = SettingsPatch {
            expected_revision: 99,
            proxies: None,
            account_proxies: None,
            account_refresh: None,
            usb_display: None,
            refresh_interval_seconds: Some(60),
            auto_refresh: None,
            display: None,
        };
        assert_eq!(service.update_settings(patch).unwrap_err().code, "CONFLICT");
        assert_eq!(service.settings().refresh_interval_seconds, 300);
        assert!(service
            .select_accounts(AccountSelectionPatch {
                expected_revision: 0,
                account_ids: vec!["missing".into()],
                aliases: None
            })
            .is_err());
    }
    #[test]
    fn catalog_exposes_opaque_ids_and_short_names_without_exposing_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("codex_accounts.json"),json!({"version":"1.0","accounts":[{"id":"source-secret-id","email":"test@example.test"}]}).to_string()).unwrap();
        let service = Service::new(dir.path().join("hud"), None).unwrap();
        std::fs::create_dir(source.join("codex_accounts")).unwrap();
        std::fs::write(
            source.join("codex_accounts/source-secret-id.json"),
            json!({
                "id":"source-secret-id", "auth_mode":"oauth", "account_id":"synthetic-workspace",
                "tokens":{"access_token":"synthetic-test-token"}
            })
            .to_string(),
        )
        .unwrap();
        service.choose_source(source).unwrap();
        let accounts = service.accounts(true);
        assert_eq!(accounts[0].display_name, "test");
        assert_ne!(accounts[0].id, "source-secret-id");
        assert!(service.snapshot().accounts.is_empty());
        let public = serde_json::to_string(&service.envelope(Ok(accounts))).unwrap();
        assert!(!public.contains("example.test"));
        assert!(!public.contains("source-secret-id"));
    }

    fn synthetic_service() -> (tempfile::TempDir, Service, String) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        // Synthetic OAuth details allow the catalog to classify login accounts.
        std::fs::write(
            source.join("codex_accounts.json"),
            json!({"version":"1.0","accounts":[{
                "id":"synthetic-source-id","email":"synthetic-name@example.test"
            }]})
            .to_string(),
        )
        .unwrap();
        let service = Service::new(dir.path().join("hud"), None).unwrap();
        std::fs::create_dir(source.join("codex_accounts")).unwrap();
        std::fs::write(
            source.join("codex_accounts/synthetic-source-id.json"),
            json!({
                "id":"synthetic-source-id", "auth_mode":"oauth", "account_id":"synthetic-workspace",
                "tokens":{"access_token":"synthetic-test-token"}
            })
            .to_string(),
        )
        .unwrap();
        service.choose_source(source).unwrap();
        let id = service.accounts(true)[0].id.clone();
        service
            .select_accounts(AccountSelectionPatch {
                expected_revision: service.settings().settings_revision,
                account_ids: vec![id.clone()],
                aliases: None,
            })
            .unwrap();
        service.data.lock().unwrap().config.settings.auto_refresh = false;
        (dir, service, id)
    }

    fn synthetic_quota(id: &str, exhausted: bool) -> AccountQuota {
        let now = Utc::now();
        usage::parse_usage(
            &json!({"rate_limit":{
                "allowed":!exhausted,"limit_reached":exhausted,
                "primary_window":{
                    "used_percent":if exhausted {100} else {20},
                    "limit_window_seconds":18000,
                    "reset_at":(now + chrono::Duration::seconds(600)).timestamp()
                },
                "secondary_window":{
                    "used_percent":30,"limit_window_seconds":604800,
                    "reset_at":(now + chrono::Duration::seconds(3600)).timestamp()
                }
            }}),
            id,
            now,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn source_loss_cancels_old_work_and_keeps_historical_quota_out_of_recommendation() {
        let (_dir, service, id) = synthetic_service();
        let root = service.source_path().unwrap();
        let old_generation = {
            let mut d = service.data.lock().unwrap();
            d.quotas.insert(id.clone(), synthetic_quota(&id, false));
            d.in_flight.insert(id.clone(), "old-job".into());
            d.jobs.insert(
                "old-job".into(),
                RefreshJob {
                    job_id: "old-job".into(),
                    state: "running".into(),
                    created_at: Utc::now().to_rfc3339(),
                    finished_at: None,
                    items: vec![RefreshItem {
                        account_id: id.clone(),
                        state: "running".into(),
                        error: None,
                    }],
                },
            );
            d.generation
        };
        assert_eq!(service.snapshot().recommendation.state, "now");
        std::fs::write(root.join("codex_accounts.json"), b"invalid catalog").unwrap();
        assert_eq!(service.rescan().unwrap().state, "unavailable");
        assert!(service.data.lock().unwrap().generation > old_generation);
        assert_eq!(service.job("old-job").unwrap().state, "cancelled");

        // A stale queued task returns before it reads a credential file or sends a request.
        service
            .run_account(
                "old-job".into(),
                id.clone(),
                "synthetic-source-id".into(),
                root,
                old_generation,
            )
            .await;
        let snapshot = service.snapshot();
        assert_eq!(snapshot.recommendation.state, "unknown");
        assert_eq!(snapshot.quotas[0].status, "unavailable");
        assert_eq!(snapshot.quotas[0].origin, "network");
        assert_eq!(snapshot.quotas[0].windows[0].remaining_percent, Some(80.0));
        assert!(!service.data.lock().unwrap().in_flight.contains_key(&id));
    }

    #[tokio::test]
    async fn reset_due_during_cooldown_is_retried_when_cooldown_ends() {
        let (_dir, service, id) = synthetic_service();
        // Keep spawned refresh tasks queued throughout the assertions; no I/O can start.
        let permits = service
            .semaphore
            .clone()
            .acquire_many_owned(2)
            .await
            .unwrap();
        {
            let mut d = service.data.lock().unwrap();
            d.config.settings.auto_refresh = true;
            d.next_auto = Instant::now() + Duration::from_secs(1800);
            let mut quota = synthetic_quota(&id, true);
            quota.windows[0].resets_at =
                Some((Utc::now() - chrono::Duration::seconds(1)).to_rfc3339());
            d.quotas.insert(id.clone(), quota);
            d.cooldown
                .insert(id.clone(), Utc::now() + chrono::Duration::seconds(60));
        }
        service.tick();
        assert!(service.data.lock().unwrap().jobs.is_empty());
        assert_eq!(
            service.snapshot().availability[0].reason,
            "awaiting_confirmation"
        );
        assert!(!service.data.lock().unwrap().handled_resets.is_empty());
        service.data.lock().unwrap().cooldown.remove(&id);
        service.tick();
        {
            let mut d = service.data.lock().unwrap();
            assert_eq!(d.jobs.len(), 1);
            assert!(d.in_flight.contains_key(&id));
            assert_eq!(d.jobs.values().next().unwrap().items[0].state, "queued");
            // Cancel before releasing permits so test cleanup also cannot read credentials.
            Service::invalidate(&mut d);
        }
        drop(permits);
    }

    #[tokio::test]
    async fn disabled_auto_refresh_does_not_query_after_resets_or_clock_changes() {
        let (_dir, service, id) = synthetic_service();
        // An accidental request remains observable as a queued job and cannot perform I/O.
        let permits = service
            .semaphore
            .clone()
            .acquire_many_owned(2)
            .await
            .unwrap();
        {
            let mut d = service.data.lock().unwrap();
            let mut quota = synthetic_quota(&id, true);
            quota.windows[0].resets_at =
                Some((Utc::now() - chrono::Duration::seconds(1)).to_rfc3339());
            d.quotas.insert(id.clone(), quota);
            d.next_auto = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
            d.last_wall = Utc::now() + chrono::Duration::seconds(60);
            d.last_tick = Instant::now();
            assert!(!d.config.settings.auto_refresh);
            assert!(d.cooldown.is_empty());
        }
        for _ in 0..3 {
            service.tick();
            let d = service.data.lock().unwrap();
            assert!(d.jobs.is_empty());
            assert!(d.in_flight.is_empty());
        }
        let snapshot = service.snapshot();
        assert_eq!(snapshot.recommendation.state, "unknown");
        assert_eq!(snapshot.next_refresh_at, None);
        assert!(service.data.lock().unwrap().clock_pending.contains(&id));
        drop(permits);
    }

    #[test]
    fn cancelled_job_with_physical_request_lease_is_not_joined() {
        let (_dir, service, id) = synthetic_service();
        {
            let mut d = service.data.lock().unwrap();
            d.in_flight.insert(id.clone(), "cancelled-job".into());
            d.jobs.insert(
                "cancelled-job".into(),
                RefreshJob {
                    job_id: "cancelled-job".into(),
                    state: "cancelled".into(),
                    created_at: Utc::now().to_rfc3339(),
                    finished_at: Some(Utc::now().to_rfc3339()),
                    items: vec![RefreshItem {
                        account_id: id.clone(),
                        state: "cancelled".into(),
                        error: None,
                    }],
                },
            );
        }
        let error = service
            .refresh(RefreshRequest {
                account_ids: Some(vec![id.clone()]),
            })
            .unwrap_err();
        assert_eq!(error.code, "RATE_LIMITED");
        assert_eq!(service.job("cancelled-job").unwrap().state, "cancelled");
        let d = service.data.lock().unwrap();
        assert_eq!(d.jobs.len(), 1);
        assert_eq!(
            d.in_flight.get(&id).map(String::as_str),
            Some("cancelled-job")
        );
    }

    #[test]
    fn elapsed_sample_age_and_clock_changes_cannot_restore_positive_recommendation() {
        let (_dir, service, id) = synthetic_service();
        {
            let mut d = service.data.lock().unwrap();
            d.quotas.insert(id.clone(), synthetic_quota(&id, false));
            // Wall time looks recent after rollback; elapsed time proves this sample is old.
            d.samples.insert(
                id.clone(),
                Instant::now()
                    .checked_sub(Duration::from_secs(601))
                    .unwrap(),
            );
        }
        let snapshot = service.snapshot();
        assert_eq!(snapshot.quotas[0].freshness, "stale");
        assert_eq!(snapshot.recommendation.state, "unknown");
        {
            let mut d = service.data.lock().unwrap();
            d.samples.insert(id.clone(), Instant::now());
            d.last_wall = Utc::now() + chrono::Duration::seconds(60);
            d.last_tick = Instant::now();
            // Clock repair remains pending during cooldown; this test launches no request.
            d.cooldown
                .insert(id.clone(), Utc::now() + chrono::Duration::seconds(120));
        }
        service.tick();
        assert!(service.data.lock().unwrap().clock_pending.contains(&id));
        let snapshot = service.snapshot();
        assert_eq!(snapshot.recommendation.state, "unknown");
        assert_eq!(snapshot.quotas[0].status, "unavailable");
        assert!(snapshot.quotas[0]
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("系统时间"));
        assert!(service.data.lock().unwrap().jobs.is_empty());
    }

    #[test]
    fn account_reply_captures_privacy_mask_and_revision_under_one_lock() {
        let (_dir, service, _id) = synthetic_service();
        {
            let mut d = service.data.lock().unwrap();
            d.revision = 0;
            d.config.settings.display.privacy_mode = false;
        }
        let writer = service.clone();
        let thread = std::thread::spawn(move || {
            for revision in 1..=10_000_u64 {
                let mut d = writer.data.lock().unwrap();
                d.revision = revision;
                d.config.settings.display.privacy_mode = revision % 2 == 1;
            }
        });
        for _ in 0..2_000 {
            let reply = service.read_reply("accounts", true);
            assert!(reply.ok);
            let accounts = reply.data.unwrap();
            let name = accounts[0]["displayName"].as_str().unwrap();
            assert_eq!(
                name,
                if reply.revision % 2 == 1 {
                    "Account 1"
                } else {
                    "synthetic-name"
                }
            );
        }
        thread.join().unwrap();
    }
    #[tokio::test]
    async fn account_override_enables_global_off_and_respects_own_interval() {
        let (_dir, service, id) = synthetic_service();
        let permits = service
            .semaphore
            .clone()
            .acquire_many_owned(2)
            .await
            .unwrap();
        {
            let mut d = service.data.lock().unwrap();
            d.config.settings.account_refresh.insert(id.clone(), 3600);
            d.refresh_started
                .insert(id.clone(), Instant::now() - Duration::from_secs(3599));
            assert!(!d.config.settings.auto_refresh);
        }
        service.tick();
        assert!(service.data.lock().unwrap().jobs.is_empty());
        {
            let mut d = service.data.lock().unwrap();
            d.refresh_started
                .insert(id.clone(), Instant::now() - Duration::from_secs(3601));
        }
        assert!(service.snapshot().next_refresh_at.is_some());
        service.tick();
        let mut d = service.data.lock().unwrap();
        assert!(d.in_flight.contains_key(&id));
        Service::invalidate(&mut d);
        drop(d);
        drop(permits);
    }

    #[tokio::test]
    async fn account_off_blocks_periodic_reset_and_clock_refresh_but_not_manual() {
        let (_dir, service, id) = synthetic_service();
        let permits = service
            .semaphore
            .clone()
            .acquire_many_owned(2)
            .await
            .unwrap();
        {
            let mut d = service.data.lock().unwrap();
            d.config.settings.auto_refresh = true;
            d.config.settings.account_refresh.insert(id.clone(), 0);
            d.next_auto = Instant::now() - Duration::from_secs(1);
            d.quotas.insert(id.clone(), synthetic_quota(&id, true));
            d.clock_pending.insert(id.clone());
        }
        service.tick();
        assert!(service.data.lock().unwrap().jobs.is_empty());
        assert!(service.snapshot().next_refresh_at.is_none());
        assert!(service
            .refresh(RefreshRequest {
                account_ids: Some(vec![id.clone()])
            })
            .is_ok());
        let mut d = service.data.lock().unwrap();
        assert!(d.in_flight.contains_key(&id));
        Service::invalidate(&mut d);
        drop(d);
        drop(permits);
    }

    #[test]
    fn display_and_account_preferences_persist_and_inherit() {
        let (_dir, service, id) = synthetic_service();
        let rev = service.settings().settings_revision;
        let patch: SettingsPatch = serde_json::from_value(serde_json::json!({
            "expectedRevision": rev, "accountRefresh": {id.clone(): 900},
            "display": {"positionLocked": true, "quotaProvider":"chatgpt", "quotaProfiles": {id.clone(): "pro10x"}},
            "usbDisplay": {"enabled": true,"deviceId":"monitor-interface-A","themeId":"paper"}
        }))
        .unwrap();
        let saved = service.update_settings(patch).unwrap();
        assert_eq!(saved.account_interval(&id), Some(900));
        let loaded = crate::config::load(&service.config_path).unwrap().settings;
        assert!(loaded.display.position_locked);
        assert_eq!(loaded.display.quota_provider, "chatgpt");
        assert_eq!(
            loaded.display.quota_profiles.get(&id).map(String::as_str),
            Some("pro10x")
        );
        assert!(service.update_settings(serde_json::from_value(serde_json::json!({"expectedRevision":saved.settings_revision,"display":{"quotaProvider":"claude"}})).unwrap()).is_err());
        assert!(service.update_settings(serde_json::from_value(serde_json::json!({"expectedRevision":saved.settings_revision,"display":{"quotaProfiles":{id.clone():"custom:1:0"}}})).unwrap()).is_err());
        assert_eq!(loaded.usb_display.device_id, "monitor-interface-A");
        assert_eq!(loaded.account_interval(&id), Some(900));
        let inherited = service.update_settings(serde_json::from_value(serde_json::json!({"expectedRevision":saved.settings_revision,"accountRefresh":{}})).unwrap()).unwrap();
        assert_eq!(inherited.account_interval(&id), None);
        assert!(service.update_settings(serde_json::from_value(serde_json::json!({"expectedRevision":inherited.settings_revision,"accountRefresh":{id:42}})).unwrap()).is_err());
    }
}
