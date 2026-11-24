use super::cockpit::Credentials;
use crate::model::{AccountQuota, ApiError, QuotaWindow};
use chrono::{DateTime, Duration, Utc};
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    Client, RequestBuilder,
};
use serde_json::{json, Value};

const ANTIGRAVITY_USER_AGENT: &str =
    "antigravity/2.25.0 (aidev_client; os_type=windows; arch=amd64; cl=1003475615)";

fn antigravity_post(
    client: &Client,
    c: &Credentials,
    method: &str,
) -> Result<RequestBuilder, ApiError> {
    let mut auth = HeaderValue::from_str(&format!("Bearer {}", c.access_token))
        .map_err(|_| ApiError::new("AUTH_EXPIRED", "无效登录凭据"))?;
    auth.set_sensitive(true);
    let base = if c.gcp && valid_project(c.project_id.as_deref()).is_some() {
        "https://cloudcode-pa.googleapis.com"
    } else {
        "https://daily-cloudcode-pa.googleapis.com"
    };
    Ok(client
        .post(format!("{base}/v1internal:{method}"))
        .json(
            &valid_project(c.project_id.as_deref())
                .map(|p| json!({"project":p}))
                .unwrap_or(json!({})),
        )
        .header(AUTHORIZATION, auth)
        .header("Accept", "application/json")
        .header("User-Agent", ANTIGRAVITY_USER_AGENT))
}

pub fn antigravity_models_request(
    client: &Client,
    c: &Credentials,
) -> Result<RequestBuilder, ApiError> {
    if c.provider != "antigravity" {
        return Err(ApiError::new("ACCOUNT_UNSUPPORTED", "不支持此平台"));
    }
    antigravity_post(client, c, "fetchAvailableModels")
}

pub fn request(client: &Client, c: &Credentials) -> Result<RequestBuilder, ApiError> {
    if c.provider == "antigravity" {
        return antigravity_post(client, c, "retrieveUserQuotaSummary");
    }
    let mut auth = HeaderValue::from_str(&format!("Bearer {}", c.access_token))
        .map_err(|_| ApiError::new("AUTH_EXPIRED", "无效登录凭据"))?;
    auth.set_sensitive(true);
    let req = match c.provider.as_str() {
        "claude" => client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("anthropic-beta", "oauth-2025-04-20"),
        "grok" => client
            .get("https://cli-chat-proxy.grok.com/v1/billing?format=credits")
            .header("x-xai-token-auth", "xai-grok-cli")
            .header("x-grok-cli-version", "1.0.52")
            .header("x-grok-client-version", "1.0.52")
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
                "grok" => "grok-cli/1.0.52",
                _ => concat!("AIDE-monitor/", env!("CARGO_PKG_VERSION")),
            },
        ))
}
fn valid_project(project: Option<&str>) -> Option<&str> {
    project
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "aicode-consumers")
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
        retained_at: None,
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
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .or_else(|| v.get("val").and_then(number))
        .filter(|n| n.is_finite())
}
fn remaining(v: &Value) -> Option<f64> {
    percent(v.get("usagePercent").and_then(number))
        .map(|v| 100.0 - v)
        .or_else(|| {
            let total = number(
                v.get("total")
                    .or_else(|| v.get("limit"))
                    .or_else(|| v.get("allocation"))?,
            )?;
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
// Only copy bounded plan identifiers, never whole billing/auth objects into the public DTO.
pub fn plan_type(v: &Value) -> Option<String> {
    [
        "/plan_type",
        "/planType",
        "/subscription_type",
        "/subscriptionTier",
        "/subscription_tier",
        "/config/subscriptionTier",
        "/tier/id",
        "/config/planType",
    ]
    .iter()
    .find_map(|path| v.pointer(path).and_then(Value::as_str))
    .filter(|s| {
        !s.is_empty()
            && s.len() <= 64
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "_- .".contains(c))
    })
    .map(str::to_owned)
}

// Pool identity comes from the bucket ID; display labels preserve the server's original names.
fn antigravity_windows(v: &Value) -> Result<Vec<QuotaWindow>, ApiError> {
    use sha2::{Digest, Sha256};
    use std::collections::{BTreeMap, HashSet};
    let groups = v
        .get("groups")
        .or_else(|| v.pointer("/response/groups"))
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::new("RESPONSE_UNSUPPORTED", "未返回额度汇总"))?;
    let mut grouped: BTreeMap<(String, String), Vec<&Value>> = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    let mut count = 0;
    for g in groups {
        let Some(buckets) = g.get("buckets").and_then(Value::as_array) else {
            continue;
        };
        let group_name = g
            .get("displayName")
            .or_else(|| g.get("display_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= 160 && !s.chars().any(char::is_control));

        let mut parsed_buckets = Vec::new();
        let mut group_families = HashSet::new();
        for b in buckets {
            count += 1;
            if count > 500 {
                return Err(ApiError::new(
                    "RESPONSE_UNSUPPORTED",
                    "额度桶数量超出支持范围",
                ));
            }
            let Some(id) = b
                .get("bucketId")
                .or_else(|| b.get("id"))
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 160 && !id.chars().any(char::is_control))
            else {
                continue;
            };
            let (root, period, mut duration) = bucket_period(id);
            if duration.is_none() {
                duration = b
                    .get("durationSeconds")
                    .or_else(|| b.get("duration_seconds"))
                    .and_then(Value::as_u64)
                    .filter(|n| (1..=366 * 86400).contains(n));
            }
            let family = match root {
                "gemini" => "gemini".to_owned(),
                "3p" | "claude" => "claude".to_owned(),
                "argon" | "gemini-argon-wtu" | "gemini-argon" | "gemini-4-argon" | "gemini-4" => {
                    "gemini4".to_owned()
                }
                _ if root.starts_with("gemini-argon-")
                    || root.starts_with("gemini-4-argon-")
                    || root.starts_with("argon-") =>
                {
                    "gemini4".to_owned()
                }
                _ => format!("bucket-{:x}", Sha256::digest(root.as_bytes())),
            };
            let period = if period.is_empty() && family == "gemini4" {
                duration = Some(604800);
                "weekly"
            } else {
                period
            };
            group_families.insert(family.clone());
            parsed_buckets.push((b, root, family, period, duration));
        }

        let use_group_name = group_families.len() == 1;
        for (b, root, family, period, duration) in parsed_buckets {
            let label = if use_group_name {
                group_name.unwrap_or(root).to_owned()
            } else {
                root.to_owned()
            };
            let key = (family, period.to_owned());
            if !metadata.contains_key(&key) {
                order.push(key.clone());
            }
            metadata.entry(key.clone()).or_insert((label, duration));
            grouped.entry(key).or_default().push(b);
        }
    }
    Ok(order
        .into_iter()
        .map(|key| {
            let (family, period) = &key;
            let buckets = &grouped[&key];
            let (label, duration) = &metadata[&key];
            let values: Option<Vec<f64>> = buckets
                .iter()
                .map(|b| {
                    percent(
                        b.get("remainingFraction")
                            .or_else(|| b.get("remaining_fraction"))
                            .and_then(number)
                            .map(|v| v * 100.),
                    )
                })
                .collect();
            let remaining = values.and_then(|v| v.into_iter().reduce(f64::min));
            let dates: Option<Vec<String>> = buckets
                .iter()
                .map(|b| date(b.get("resetTime").or_else(|| b.get("reset_time"))))
                .collect();
            let reset = dates.and_then(|v| {
                v.into_iter()
                    .max_by_key(|d| DateTime::parse_from_rfc3339(d).ok())
            });
            let mut w = window(
                &format!("{family}-{period}"),
                &if period.is_empty() {
                    label.clone()
                } else {
                    format!("{label} · {period}")
                },
                &format!("feature:{family}"),
                remaining,
                reset,
                *duration,
            );
            w.kind = if family == "gemini4" || period != "weekly" {
                "primary"
            } else {
                "secondary"
            }
            .into();
            w
        })
        .collect())
}

/// When `retrieveUserQuotaSummary` omits the Gemini 5h pool, supplement `gemini-5h` from
/// `fetchAvailableModels` without inventing weekly or Gemini 4 Argon windows.
pub fn supplement_antigravity_models(quota: &mut AccountQuota, models_response: &Value) {
    if quota
        .windows
        .iter()
        .any(|w| w.id == "gemini-5h" && w.remaining_percent.is_some())
    {
        return;
    }
    let Some(models) = models_response
        .get("models")
        .or_else(|| models_response.pointer("/response/models"))
        .and_then(Value::as_object)
    else {
        return;
    };
    let mut keys: Vec<&String> = models.keys().collect();
    keys.sort();
    for key in keys.into_iter().take(500) {
        let Some(model) = models.get(key).filter(|m| m.is_object()) else {
            continue;
        };
        if model.get("isInternal").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let display_name = model
            .get("displayName")
            .and_then(Value::as_str)
            .unwrap_or(key);
        let key_lower = key.to_ascii_lowercase();
        let name_lower = display_name.to_ascii_lowercase();
        if (!key_lower.contains("gemini") && !name_lower.contains("gemini"))
            || key_lower.contains("argon")
            || name_lower.contains("argon")
        {
            continue;
        }
        let Some(info) = model
            .get("quotaInfo")
            .or_else(|| model.get("quota_info"))
            .filter(|v| v.is_object())
        else {
            continue;
        };
        let Some(rem) = percent(
            info.get("remainingFraction")
                .or_else(|| info.get("remaining_fraction"))
                .and_then(number)
                .map(|v| v * 100.0),
        ) else {
            continue;
        };
        let reset = date(info.get("resetTime").or_else(|| info.get("reset_time")));
        if let Some(target) = quota
            .windows
            .iter_mut()
            .find(|w| w.id == "gemini-5h" && w.remaining_percent.is_none())
        {
            target.remaining_percent = Some(rem);
            target.measurement = "percent".into();
            target.exhausted = Some(rem == 0.0);
            if target.resets_at.is_none() {
                target.resets_at = reset;
            }
        } else {
            let pool_label = quota
                .windows
                .iter()
                .find(|w| w.scope == "feature:gemini")
                .and_then(|w| w.label.split(" · ").next())
                .unwrap_or("gemini")
                .to_owned();
            quota.windows.push(window(
                "gemini-5h",
                &format!("{pool_label} · 5h"),
                "feature:gemini",
                Some(rem),
                reset,
                Some(18000),
            ));
        }
        break;
    }
}
fn bucket_period(id: &str) -> (&str, &str, Option<u64>) {
    for (period, seconds) in [
        ("weekly", Some(604800)),
        ("daily", Some(86400)),
        ("monthly", None),
        ("5h", Some(18000)),
        ("7d", Some(604800)),
    ] {
        for separator in ['-', ':'] {
            if let Some(root) = id
                .strip_suffix(&format!("{separator}{period}"))
                .filter(|root| !root.is_empty())
            {
                return (
                    root,
                    if period == "7d" { "weekly" } else { period },
                    seconds,
                );
            }
        }
    }
    if let Some((root, period)) = id.rsplit_once(['-', ':']) {
        if !root.is_empty() {
            let unit = if period.ends_with('h') {
                3600
            } else if period.ends_with('d') {
                86400
            } else {
                0
            };
            let seconds = period
                .get(..period.len().saturating_sub(1))
                .and_then(|n| n.parse::<u64>().ok())
                .and_then(|n| n.checked_mul(unit));
            if let Some(seconds) = seconds.filter(|n| (1..=366 * 86400).contains(n)) {
                return (root, period, Some(seconds));
            }
        }
    }
    (id, "", None)
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
            if let Some(fields) = v.as_object() {
                for (key, value) in fields {
                    if windows.iter().any(|w| w.id == *key)
                        || !value.is_object()
                        || value.get("utilization").is_none()
                    {
                        continue;
                    }
                    if windows.len() >= 100 {
                        break;
                    }
                    let (family, duration) = if let Some(f) = key.strip_prefix("five_hour_") {
                        (f, Some(18000))
                    } else if let Some(f) = key.strip_prefix("seven_day_") {
                        (f, Some(604800))
                    } else {
                        (key.as_str(), None)
                    };
                    use sha2::{Digest, Sha256};
                    let scope = if matches!(family, "sonnet" | "opus") {
                        format!("feature:{family}")
                    } else {
                        format!("feature:bucket-{:x}", Sha256::digest(family.as_bytes()))
                    };
                    windows.push(window(
                        key,
                        family,
                        &scope,
                        percent(value.get("utilization").and_then(number)).map(|v| 100. - v),
                        date(value.get("resets_at")),
                        duration,
                    ));
                }
            }
        }
        "antigravity" => {
            windows = antigravity_windows(v)?;
        }
        "grok" => {
            if let Some(c) = v.get("config").or(Some(v)).filter(|v| v.is_object()) {
                let reset = date(c.pointer("/currentPeriod/end"))
                    .or_else(|| date(c.get("billingPeriodEnd")));
                let unified = c.get("isUnifiedBillingUser").and_then(Value::as_bool) == Some(true);
                let period = c
                    .pointer("/currentPeriod/type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_uppercase();
                let legacy_total = c.get("monthlyLimit").and_then(number);
                let duration = match period.as_str() {
                    "WEEKLY" | "USAGE_PERIOD_TYPE_WEEKLY" => Some(604800),
                    "MONTHLY" | "USAGE_PERIOD_TYPE_MONTHLY" => period_duration(c),
                    _ if !unified && legacy_total.is_some_and(|n| n > 0.) => period_duration(c),
                    _ => None,
                };
                let p = percent(c.get("creditUsagePercent").and_then(number))
                    .map(|p| 100.0 - p)
                    .or_else(|| {
                        [
                            "sharedPool",
                            "weeklyCredits",
                            "subscriptionCredits",
                            "includedCredits",
                        ]
                        .iter()
                        .find_map(|key| c.get(key).and_then(remaining))
                    })
                    .or_else(|| {
                        if unified {
                            None
                        } else {
                            [
                                c.get("credits"),
                                v.get("credits"),
                                v.get("usage"),
                                v.get("creditBalance"),
                            ]
                            .into_iter()
                            .flatten()
                            .find_map(remaining)
                        }
                    })
                    .or_else(|| {
                        if unified {
                            return None;
                        }
                        let total = legacy_total.filter(|n| *n > 0.)?;
                        let used = c.get("used").and_then(|v| {
                            if v.as_object().is_some_and(|o| o.is_empty()) {
                                Some(0.)
                            } else {
                                number(v)
                            }
                        })?;
                        percent(Some(100. * (1. - used / total)))
                    })
                    // Proto3 can omit zero usage, but only a complete known period supports that inference.
                    .or_else(|| {
                        (!c.get("creditUsagePercent").is_some()
                            && matches!(
                                period.as_str(),
                                "WEEKLY"
                                    | "MONTHLY"
                                    | "USAGE_PERIOD_TYPE_WEEKLY"
                                    | "USAGE_PERIOD_TYPE_MONTHLY"
                            )
                            && period_duration(c).is_some())
                        .then_some(100.)
                    });
                if p.is_some()
                    || c.get("creditUsagePercent").is_some()
                    || c.get("sharedPool").is_some()
                    || c.get("weeklyCredits").is_some()
                {
                    windows.push(window(
                        "credits",
                        "Shared quota",
                        "base",
                        p,
                        reset.clone(),
                        duration,
                    ));
                }
                // productUsage is consumption attribution, not an independent allowance.
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
    q.plan_type = plan_type(v);
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
fn period_duration(c: &Value) -> Option<u64> {
    let start =
        date(c.pointer("/currentPeriod/start")).or_else(|| date(c.get("billingPeriodStart")))?;
    let end = date(c.pointer("/currentPeriod/end")).or_else(|| date(c.get("billingPeriodEnd")))?;
    let seconds = (DateTime::parse_from_rfc3339(&end).ok()?
        - DateTime::parse_from_rfc3339(&start).ok()?)
    .num_seconds();
    (seconds > 0).then_some(seconds as u64)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grok_shared_pool_does_not_use_purchased_credits_or_product_shares() {
        let q = parse(&json!({"credits":{"total":100,"used":100},"config":{
            "isUnifiedBillingUser":true,"sharedPool":{"total":"100","remaining":{"val":"75"}},
            "currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","start":"2026-10-20T00:00:00Z","end":"2026-10-27T00:00:00Z"},
            "productUsage":[{"product":"Grok Build","usagePercent":90}]
        }}), "grok", "a", Utc::now()).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].remaining_percent, Some(75.));
        assert_eq!(q.windows[0].duration_seconds, Some(604800));
        assert!(parse(&json!({"config":{"isUnifiedBillingUser":true,"productUsage":[{"usagePercent":20}],"credits":{"total":100,"remaining":70}}}), "grok", "a", Utc::now()).is_err());
    }
    #[test]
    fn grok_monthly_period_and_omitted_zero_are_not_assumed_weekly() {
        let q = parse(&json!({"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_MONTHLY","start":"2026-10-01T00:00:00Z","end":"2026-11-01T00:00:00Z"}}}), "grok", "a", Utc::now()).unwrap();
        assert_eq!(q.windows[0].remaining_percent, Some(100.));
        assert_eq!(q.windows[0].duration_seconds, Some(31 * 86400));
        assert!(parse(
            &json!({"config":{"currentPeriod":{"type":"weekly"}}}),
            "grok",
            "a",
            Utc::now()
        )
        .is_err());
        let q = parse(&json!({"config":{"monthlyLimit":{"val":100},"used":{},"billingPeriodStart":"2026-10-01T00:00:00Z","billingPeriodEnd":"2026-11-01T00:00:00Z"}}), "grok", "a", Utc::now()).unwrap();
        assert_eq!(q.windows[0].remaining_percent, Some(100.));
        assert_eq!(q.windows[0].duration_seconds, Some(31 * 86400));
    }
    #[test]
    fn consumer_placeholder_and_orphan_gcp_flag_do_not_route_to_enterprise() {
        for project in [None, Some("aicode-consumers"), Some("  ")] {
            let c = Credentials {
                account_email: None,
                provider: "antigravity".into(),
                access_token: "synthetic".into(),
                account_id: None,
                project_id: project.map(str::to_string),
                gcp: true,
            };
            let r = request(&Client::new(), &c).unwrap().build().unwrap();
            assert_eq!(
                r.url().host_str(),
                Some("daily-cloudcode-pa.googleapis.com")
            );
            assert_eq!(r.body().unwrap().as_bytes().unwrap(), b"{}");
        }
    }
    #[test]
    fn antigravity_preserves_family_and_period() {
        let q = parse(&json!({"groups":[{"buckets":[
            {"bucketId":"gemini-5h","remainingFraction":1,"resetTime":"2026-10-06T15:36:00Z"},
            {"bucketId":"gemini-weekly","remainingFraction":0.96,"resetTime":"2026-10-07T14:00:00Z"},
            {"bucketId":"3p-5h","remainingFraction":0.8},
            {"bucketId":"claude:weekly","remainingFraction":0.4},
            {"bucketId":"chat_123","remainingFraction":0}
        ]}]}), "antigravity", "a", Utc::now()).unwrap();
        assert_eq!(q.windows.len(), 5); // Unknown buckets are retained as independent discoveries.
        let gemini = q.windows.iter().find(|w| w.id == "gemini-5h").unwrap();
        let week = q.windows.iter().find(|w| w.id == "gemini-weekly").unwrap();
        assert_eq!(gemini.remaining_percent, Some(100.0));
        assert_eq!(week.remaining_percent, Some(96.0));
        assert_eq!(week.duration_seconds, Some(604800));
        assert_eq!(week.resets_at.as_deref(), Some("2026-10-07T14:00:00+00:00"));
        assert_eq!(
            q.windows
                .iter()
                .find(|w| w.id == "claude-5h")
                .unwrap()
                .remaining_percent,
            Some(80.0)
        );
        assert_eq!(
            q.windows
                .iter()
                .find(|w| w.id == "claude-weekly")
                .unwrap()
                .remaining_percent,
            Some(40.0)
        );
        assert!(!q.base_coverage_complete);
        let q = parse(
            &json!({"groups":[{"buckets":[{"bucketId":"gemini:5h","remainingFraction":2}]}]}),
            "antigravity",
            "a",
            Utc::now(),
        )
        .unwrap();
        assert!(q.windows.iter().all(|w| w.remaining_percent.is_none()));
        assert!(parse(&json!({"models":{}}), "antigravity", "a", Utc::now()).is_err());
    }

    #[test]
    fn discovers_future_buckets_cycles_and_model_names_without_allowlist() {
        let v = json!({"groups":[{"displayName":"Translated title", "buckets":[
            {"bucketId":"future-model-5h","remainingFraction":0.8},
            {"bucketId":"future-model-weekly","remainingFraction":0.7},
            {"bucketId":"future-model-2h","remainingFraction":0.6},
            {"bucketId":"other-model-monthly","remainingFraction":0.5},
            {"bucketId":"opaque-bucket","remainingFraction":0.4},
            {"bucketId":"gemini-argon-wtu-5h","remainingFraction":0.3}
        ]}]});
        let q = parse(&v, "antigravity", "a", Utc::now()).unwrap();
        let future: Vec<_> = q
            .windows
            .iter()
            .filter(|w| w.label.starts_with("future-model ·"))
            .collect();
        assert_eq!(future.len(), 3);
        assert!(future.iter().all(|w| w.scope == future[0].scope));
        assert!(future.iter().any(|w| w.duration_seconds == Some(7200)));
        assert!(q
            .windows
            .iter()
            .any(|w| w.scope == "feature:gemini4" && w.duration_seconds == Some(18000)));
        assert!(q
            .windows
            .iter()
            .filter(|w| w.label.starts_with("other-model") || w.label.starts_with("opaque-bucket"))
            .all(|w| w.duration_seconds.is_none()));
        let mut renamed = v.clone();
        renamed["groups"][0]["displayName"] = json!("Another language");
        assert_eq!(
            q.windows,
            parse(&renamed, "antigravity", "a", Utc::now())
                .unwrap()
                .windows
        );
        let claude = parse(
            &json!({"five_hour_future":{"utilization":25},"seven_day_future":{"utilization":10}}),
            "claude",
            "a",
            Utc::now(),
        )
        .unwrap();
        assert_eq!(claude.windows.len(), 2);
        assert_eq!(claude.windows[0].scope, claude.windows[1].scope);
    }
    #[test]
    fn provider_measurements_are_not_invented() {
        let q=parse(&json!({"five_hour":{"utilization":23.5,"resets_at":"2026-10-06T00:00:00Z"},"seven_day":{"utilization":100}}),"claude","a",Utc::now()).unwrap();
        assert_eq!(q.windows[0].remaining_percent, Some(76.5));
        assert_eq!(q.provider_allowed, Some(false));
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
                    account_email: None,
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
            if p == "antigravity" {
                assert_eq!(
                    r.headers()["user-agent"],
                    "antigravity/2.25.0 (aidev_client; os_type=windows; arch=amd64; cl=1003475615)"
                );
                let models_req = antigravity_models_request(
                    &c,
                    &Credentials {
                        account_email: None,
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
                assert_eq!(
                    models_req.url().as_str(),
                    "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels"
                );
                assert_eq!(
                    models_req.headers()["user-agent"],
                    "antigravity/2.25.0 (aidev_client; os_type=windows; arch=amd64; cl=1003475615)"
                );
            }
            assert!(!format!("{r:?}").contains("secret-synthetic"));
        }
    }
    #[test]
    fn antigravity_parses_gemini4_argon_variants() {
        for bucket in [
            json!({"bucketId":"gemini-argon-wtu-weekly","remainingFraction":0,"resetTime":"2026-10-27T10:17:34Z"}),
            json!({"bucketId":"argon:weekly","remainingFraction":0,"resetTime":"2026-10-27T10:17:34Z"}),
            json!({"bucketId":"gemini-argon-custom-weekly","remainingFraction":0,"resetTime":"2026-10-27T10:17:34Z"}),
            json!({"id":"gemini-argon-wtu-weekly","remaining_fraction":0,"reset_time":"2026-10-27T10:17:34Z"}),
        ] {
            let q = parse(
                &json!({"groups":[
                    {"displayName":"Gemini Models","buckets":[
                        {"bucketId":"gemini-weekly","remainingFraction":0.94,"resetTime":"2026-10-24T06:33:44Z"},
                        {"bucketId":"gemini-5h","remainingFraction":0.95,"resetTime":"2026-10-22T11:20:20Z"}
                    ]},
                    {"displayName":"Claude and GPT models","buckets":[
                        {"bucketId":"3p-weekly","remainingFraction":0.55,"resetTime":"2026-10-26T07:45:47Z"},
                        {"bucketId":"3p-5h","remainingFraction":1.0,"resetTime":"2026-10-22T12:06:07Z"}
                    ]},
                    {"displayName":"Gemini 4 Argon","buckets":[bucket]}
                ]}),
                "antigravity",
                "a",
                Utc::now(),
            )
            .unwrap();
            assert_eq!(q.windows.len(), 5);
            let g4 = q.windows.iter().find(|w| w.id == "gemini4-weekly").unwrap();
            assert_eq!(g4.remaining_percent, Some(0.0));
            assert_eq!(g4.exhausted, Some(true));
            assert_eq!(g4.duration_seconds, Some(604800));
            assert_eq!(g4.scope, "feature:gemini4");
            assert_eq!(g4.resets_at.as_deref(), Some("2026-10-27T10:17:34+00:00"));
        }
    }
    #[test]
    fn antigravity_supplements_missing_gemini_5h_from_models_list_only() {
        let mut q = parse(
            &json!({"groups":[{"displayName":"Claude and GPT models","buckets":[
                {"bucketId":"3p-weekly","remainingFraction":0.48,"resetTime":"2026-11-07T07:55:38Z"},
                {"bucketId":"3p-5h","remainingFraction":0.0,"resetTime":"2026-10-31T12:55:38Z"}
            ]}]}),
            "antigravity",
            "a",
            Utc::now(),
        )
        .unwrap();
        assert!(q.windows.iter().all(|w| w.scope == "feature:claude"));
        assert_eq!(q.windows[0].label, "Claude and GPT models · weekly");
        let models = json!({
            "models": {
                "a_internal_gemini": {"isInternal": true, "quotaInfo": {"remainingFraction": 0.1}},
                "b_gemini_argon": {"displayName": "Gemini 4 Argon", "quotaInfo": {"remainingFraction": 0.2}},
                "gemini-3-flash": {
                    "displayName": "Gemini 3 Flash",
                    "quotaInfo": {"remainingFraction": 0.95, "resetTime": "2026-10-31T12:46:31Z"}
                }
            }
        });
        supplement_antigravity_models(&mut q, &models);
        let g5h = q.windows.iter().find(|w| w.id == "gemini-5h").unwrap();
        assert_eq!(g5h.remaining_percent, Some(95.0));
        assert_eq!(g5h.measurement, "percent");
        assert_eq!(g5h.exhausted, Some(false));
        assert_eq!(g5h.resets_at.as_deref(), Some("2026-10-31T12:46:31+00:00"));
        assert!(q.windows.iter().all(|w| w.id != "gemini-weekly"));
        assert!(q.windows.iter().all(|w| w.scope != "feature:gemini4"));
    }
    #[test]
    fn antigravity_parses_language_server_response_with_exhausted_argon() {
        let json = serde_json::json!({
            "response": {
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "buckets": [
                            {"bucketId": "gemini-5h", "remainingFraction": 0.75, "resetTime": "2026-10-22T11:20:20Z"},
                            {"bucketId": "gemini-weekly", "remainingFraction": 0.88, "resetTime": "2026-10-24T06:33:44Z"}
                        ]
                    },
                    {
                        "displayName": "Claude and GPT models",
                        "buckets": [
                            {"bucketId": "3p-5h", "remainingFraction": 1.0, "resetTime": "2026-10-22T13:20:21Z"},
                            {"bucketId": "3p-weekly", "remainingFraction": 0.55, "resetTime": "2026-10-26T07:45:47Z"}
                        ]
                    },
                    {
                        "displayName": "Gemini 4 Argon",
                        "buckets": [
                            {"bucketId": "gemini-argon-wtu-weekly", "remainingFraction": 0, "resetTime": "2026-10-27T10:17:34Z"}
                        ]
                    }
                ]
            }
        });
        let q = parse(&json, "antigravity", "a", chrono::Utc::now()).unwrap();
        let argon = q
            .windows
            .iter()
            .find(|w| w.scope == "feature:gemini4")
            .unwrap();
        assert_eq!(argon.remaining_percent, Some(0.0));
        assert_eq!(argon.exhausted, Some(true));
        assert_eq!(argon.kind, "primary");
        assert_eq!(
            argon.resets_at.as_deref(),
            Some("2026-10-27T10:17:34+00:00")
        );
    }
}
