use super::cockpit::Credentials;
use crate::model::{AccountQuota, ApiError, QuotaWindow};
use chrono::{DateTime, Duration, Utc};
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    Client, RequestBuilder,
};
use serde_json::{json, Value};

pub fn request(client: &Client, c: &Credentials) -> Result<RequestBuilder, ApiError> {
    let mut auth = HeaderValue::from_str(&format!("Bearer {}", c.access_token))
        .map_err(|_| ApiError::new("AUTH_EXPIRED", "无效登录凭据"))?;
    auth.set_sensitive(true);
    let req = match c.provider.as_str() {
        "claude" => client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("anthropic-beta", "oauth-2025-04-20"),
        "antigravity" => client
            .post(if c.gcp {
                "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels"
            } else {
                "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels"
            })
            .json(
                &c.project_id
                    .as_ref()
                    .map(|p| json!({"project":p}))
                    .unwrap_or(json!({})),
            ),
        "grok" => client
            .get("https://cli-chat-proxy.grok.com/v1/billing?format=credits")
            .header("x-xai-token-auth", "xai-grok-cli")
            .header("x-grok-cli-version", "0.2.93")
            .header("x-grok-client-version", "0.2.93")
            .header("x-grok-client-surface", "grok-cli")
            .header("x-grok-client-identifier", "aide-monitor"),
        _ => return Err(ApiError::new("ACCOUNT_UNSUPPORTED", "不支持此平台")),
    };
    Ok(req
        .header(AUTHORIZATION, auth)
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            match c.provider.as_str() {
                "antigravity" => "antigravity/1.20.5 windows/amd64",
                "grok" => "grok-cli/0.2.93",
                _ => concat!("AIDE-monitor/", env!("CARGO_PKG_VERSION")),
            },
        ))
}
fn percent(v: Option<f64>) -> Option<f64> {
    v.filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
}
fn date(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.to_rfc3339())
}
fn window(
    id: &str,
    label: &str,
    scope: &str,
    remaining: Option<f64>,
    reset: Option<String>,
    duration: Option<u64>,
) -> QuotaWindow {
    let remaining = percent(remaining);
    QuotaWindow {
        id: id.into(),
        label: label.chars().take(100).collect(),
        scope: scope.into(),
        kind: "primary".into(),
        duration_seconds: duration,
        applicability: "required".into(),
        measurement: if remaining.is_some() {
            "percent"
        } else {
            "unknown"
        }
        .into(),
        remaining_percent: remaining,
        exhausted: remaining.map(|v| v == 0.0),
        resets_at: reset,
    }
}
fn number(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.get("val").and_then(Value::as_f64))
}
fn remaining(v: &Value) -> Option<f64> {
    percent(v.get("usagePercent").and_then(Value::as_f64))
        .map(|v| 100.0 - v)
        .or_else(|| {
            let total = number(v.get("total")?)?;
            if total <= 0.0 {
                return None;
            }
            let rem = v
                .get("remaining")
                .and_then(number)
                .or_else(|| number(v.get("used")?).map(|used| total - used))?;
            percent(Some(rem / total * 100.0))
        })
}
pub fn parse(
    v: &Value,
    provider: &str,
    id: &str,
    now: DateTime<Utc>,
) -> Result<AccountQuota, ApiError> {
    let mut windows = vec![];
    match provider {
        "claude" => {
            for (key, label, duration, scope) in [
                ("five_hour", "5h", 18000, "base"),
                ("seven_day", "7d", 604800, "base"),
                ("seven_day_sonnet", "Sonnet", 604800, "feature:sonnet"),
                ("seven_day_opus", "Opus", 604800, "feature:opus"),
            ] {
                if let Some(w) = v.get(key).filter(|v| v.is_object()) {
                    windows.push(window(
                        key,
                        label,
                        scope,
                        percent(w.get("utilization").and_then(Value::as_f64)).map(|u| 100.0 - u),
                        date(w.get("resets_at")),
                        Some(duration),
                    ));
                }
            }
        }
        "antigravity" => {
            if let Some(models) = v.get("models").and_then(Value::as_object) {
                for (key, m) in models.iter().take(100) {
                    if let Some(q) = m.get("quotaInfo") {
                        windows.push(window(
                            key,
                            m.get("displayName").and_then(Value::as_str).unwrap_or(key),
                            &format!("feature:{key}"),
                            q.get("remainingFraction")
                                .and_then(Value::as_f64)
                                .map(|f| f * 100.0),
                            date(q.get("resetTime")),
                            None,
                        ));
                    }
                }
            }
        }
        "grok" => {
            if let Some(c) = v.get("config").filter(|v| v.is_object()) {
                let reset = date(c.pointer("/currentPeriod/end"));
                let p = percent(c.get("creditUsagePercent").and_then(Value::as_f64))
                    .map(|p| 100.0 - p)
                    .or_else(|| c.get("weeklyCredits").and_then(remaining));
                if c.get("creditUsagePercent").is_some() || c.get("weeklyCredits").is_some() {
                    windows.push(window(
                        "credits",
                        "Credits",
                        "base",
                        p,
                        reset.clone(),
                        if c.pointer("/currentPeriod/type").and_then(Value::as_str)
                            == Some("weekly")
                        {
                            Some(604800)
                        } else {
                            None
                        },
                    ));
                }
                if let Some(products) = c.get("productUsage").and_then(Value::as_array) {
                    for (i, p) in products.iter().take(100).enumerate() {
                        let name = p
                            .get("product")
                            .and_then(Value::as_str)
                            .unwrap_or("Product");
                        windows.push(window(
                            &format!("product-{i}"),
                            name,
                            &format!("feature:{i}"),
                            remaining(p),
                            reset.clone(),
                            None,
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    if windows.is_empty() {
        return Err(ApiError::new(
            "RESPONSE_UNSUPPORTED",
            "未返回可识别的额度窗口",
        ));
    }
    let mut q = AccountQuota::empty(id);
    q.origin = "network".into();
    q.status = "ok".into();
    q.freshness = "fresh".into();
    q.last_success_at = Some(now.to_rfc3339());
    q.last_attempt_at = q.last_success_at.clone();
    q.observed_at = q.last_success_at.clone();
    q.valid_until = Some((now + Duration::seconds(300)).to_rfc3339());
    // Model-specific quotas do not establish whole-account availability.
    q.base_coverage_complete = provider == "claude"
        && ["five_hour", "seven_day"].iter().all(|id| {
            windows
                .iter()
                .any(|w| w.id == *id && w.remaining_percent.is_some())
        });
    if q.base_coverage_complete {
        let allowed = windows
            .iter()
            .filter(|w| w.scope == "base")
            .all(|w| w.remaining_percent.is_some_and(|v| v > 0.0));
        q.provider_allowed = Some(allowed);
        q.blocking_reason = if allowed { "none" } else { "quota_windows" }.into();
    }
    q.windows = windows;
    Ok(q)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_measurements_are_not_invented() {
        let q=parse(&json!({"five_hour":{"utilization":23.5,"resets_at":"2026-10-06T00:00:00Z"},"seven_day":{"utilization":100}}),"claude","a",Utc::now()).unwrap();
        assert_eq!(q.windows[0].remaining_percent, Some(76.5));
        assert_eq!(q.provider_allowed, Some(false));
        let q = parse(
            &json!({"models":{"a":{"quotaInfo":{"remainingFraction":0.2}},"b":{"quotaInfo":{}}}}),
            "antigravity",
            "a",
            Utc::now(),
        )
        .unwrap();
        assert_eq!(q.windows[0].remaining_percent, Some(20.0));
        assert_eq!(q.windows[1].remaining_percent, None);
        assert!(!q.base_coverage_complete);
        let q = parse(
            &json!({"config":{"weeklyCredits":{"total":{"val":100},"remaining":{"val":75}}}}),
            "grok",
            "a",
            Utc::now(),
        )
        .unwrap();
        assert_eq!(q.windows[0].remaining_percent, Some(75.0));
        assert_eq!(q.windows[0].duration_seconds, None);
        assert!(parse(&json!({}), "grok", "a", Utc::now()).is_err());
    }
    #[test]
    fn credential_targets_are_fixed_and_redacted() {
        let c = Client::new();
        for p in ["claude", "antigravity", "grok"] {
            let r = request(
                &c,
                &Credentials {
                    provider: p.into(),
                    access_token: "secret-synthetic".into(),
                    account_id: None,
                    project_id: None,
                    gcp: false,
                },
            )
            .unwrap()
            .build()
            .unwrap();
            assert_eq!(r.url().scheme(), "https");
            assert!(r.headers()[AUTHORIZATION].is_sensitive());
            assert!(!format!("{r:?}").contains("secret-synthetic"));
        }
    }
}
