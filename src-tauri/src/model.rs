use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Public DTOs deliberately contain no credential or source-file fields.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub retry_at: Option<String>,
    pub retryable: bool,
}

impl ApiError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
            retry_at: None,
            retryable: matches!(
                code,
                "SOURCE_BUSY" | "NETWORK_ERROR" | "TIMEOUT" | "RATE_LIMITED"
            ),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResult<T> {
    pub api_version: String,
    pub instance_id: String,
    pub revision: u64,
    pub request_id: String,
    pub generated_at: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub is_current: bool,
    pub provider_id: String,
    pub selected: bool,
    pub order: u64,
    pub support: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: String,
    pub scope: String,
    pub kind: String,
    pub label: String,
    pub duration_seconds: Option<u64>,
    pub applicability: String,
    pub measurement: String,
    pub remaining_percent: Option<f64>,
    pub exhausted: Option<bool>,
    pub resets_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountQuota {
    pub account_id: String,
    pub origin: String,
    pub freshness: String,
    pub status: String,
    pub last_success_at: Option<String>,
    pub last_attempt_at: Option<String>,
    pub observed_at: Option<String>,
    pub valid_until: Option<String>,
    pub provider_allowed: Option<bool>,
    pub base_coverage_complete: bool,
    pub blocking_reason: String,
    #[serde(default)]
    pub reset_credits_available: Option<u64>,
    pub windows: Vec<QuotaWindow>,
    pub error: Option<ApiError>,
}

impl AccountQuota {
    /// First use must not manufacture a successful network sample.
    pub fn empty(account_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            origin: "none".into(),
            freshness: "unknown".into(),
            status: "unavailable".into(),
            last_success_at: None,
            last_attempt_at: None,
            observed_at: None,
            valid_until: None,
            provider_allowed: None,
            base_coverage_complete: false,
            blocking_reason: "unknown".into(),
            reset_credits_available: None,
            windows: Vec::new(),
            error: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountAvailability {
    pub account_id: String,
    pub state: String,
    pub reason: String,
    pub estimated_available_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub selected: usize,
    pub known: usize,
    pub unknown: usize,
    pub complete: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub state: String,
    pub account_id: Option<String>,
    pub estimated_available_at: Option<String>,
    pub reason: String,
    pub coverage: Coverage,
}

impl Recommendation {
    pub fn empty() -> Self {
        Self {
            state: "empty".into(),
            account_id: None,
            estimated_available_at: None,
            reason: "no_selection".into(),
            coverage: Coverage {
                selected: 0,
                known: 0,
                unknown: 0,
                complete: true,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub state: String,
    pub format: String,
    pub adapter_version: String,
    pub error: Option<ApiError>,
}

impl Default for SourceStatus {
    fn default() -> Self {
        Self {
            state: "not_configured".into(),
            format: "unknown".into(),
            adapter_version: "0.1.0".into(),
            error: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotalQuota {
    pub percent: Option<f64>,
    pub partial: bool,
    pub weekly_scale_percent: f64,
}

impl Default for TotalQuota {
    fn default() -> Self {
        Self {
            percent: None,
            partial: false,
            weekly_scale_percent: 15.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub source: SourceStatus,
    pub accounts: Vec<AccountSummary>,
    pub quotas: Vec<AccountQuota>,
    pub availability: Vec<AccountAvailability>,
    pub recommendation: Recommendation,
    pub total_quota: TotalQuota,
    pub next_refresh_at: Option<String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            source: SourceStatus::default(),
            accounts: Vec::new(),
            quotas: Vec::new(),
            availability: Vec::new(),
            recommendation: Recommendation::empty(),
            total_quota: TotalQuota::default(),
            next_refresh_at: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefreshRequest {
    pub account_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTicket {
    pub job_id: String,
    pub joined: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshItem {
    pub account_id: String,
    pub state: String,
    pub error: Option<ApiError>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshJob {
    pub job_id: String,
    pub state: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub items: Vec<RefreshItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DisplaySettings {
    #[serde(default)]
    pub position_locked: bool,
    pub always_on_top: bool,
    pub show_hover_details: bool,
    pub privacy_mode: bool,
    pub locale: String,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            position_locked: false,
            always_on_top: true,
            show_hover_details: false,
            privacy_mode: false,
            locale: "en".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub proxies: Vec<ProxyProfile>,
    #[serde(default)]
    pub account_proxies: BTreeMap<String, String>,
    #[serde(default)]
    pub account_refresh: BTreeMap<String, u64>,
    #[serde(default)]
    pub usb_display: UsbDisplaySettings,
    pub settings_revision: u64,
    pub refresh_interval_seconds: u64,
    pub auto_refresh: bool,
    pub display: DisplaySettings,
    pub active_theme_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyProfile {
    pub id: String,
    pub name: String,
    pub address: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            proxies: vec![],
            account_proxies: BTreeMap::new(),
            account_refresh: BTreeMap::new(),
            usb_display: UsbDisplaySettings::default(),
            settings_revision: 0,
            refresh_interval_seconds: 300,
            auto_refresh: true,
            display: DisplaySettings::default(),
            active_theme_id: "default".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplaySettingsPatch {
    pub position_locked: Option<bool>,
    pub always_on_top: Option<bool>,
    pub show_hover_details: Option<bool>,
    pub privacy_mode: Option<bool>,
    pub locale: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub proxies: Option<Vec<ProxyProfile>>,
    pub account_proxies: Option<BTreeMap<String, String>>,
    pub account_refresh: Option<BTreeMap<String, u64>>,
    pub usb_display: Option<UsbDisplaySettings>,
    pub expected_revision: u64,
    pub refresh_interval_seconds: Option<u64>,
    pub auto_refresh: Option<bool>,
    pub display: Option<DisplaySettingsPatch>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountSelectionPatch {
    pub expected_revision: u64,
    pub account_ids: Vec<String>,
    pub aliases: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSummary {
    pub id: String,
    pub name: String,
    pub built_in: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub app_version: String,
    pub adapter_version: String,
    pub source: SourceStatus,
    pub selected_account_count: usize,
    pub active_job_count: usize,
    pub recent_error_codes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub app_version: String,
    pub api_version: String,
    pub transport: String,
    pub enabled_methods: Vec<String>,
    pub granted_scopes: Vec<String>,
    pub theme_schema_versions: Vec<u64>,
    pub provider_ids: Vec<String>,
    pub max_refresh_accounts: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EventData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HudEvent {
    pub api_version: String,
    pub instance_id: String,
    pub revision: u64,
    pub sequence: u64,
    pub emitted_at: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: EventData,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct UsbDisplaySettings {
    pub enabled: bool,
    pub device_id: String,
    pub theme_id: String,
}
impl Default for UsbDisplaySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            device_id: String::new(),
            theme_id: "default".into(),
        }
    }
}
impl Settings {
    pub fn account_interval(&self, id: &str) -> Option<u64> {
        match self.account_refresh.get(id) {
            Some(0) => None,
            Some(seconds) => Some(*seconds),
            None if self.auto_refresh => Some(self.refresh_interval_seconds),
            None => None,
        }
    }
}
