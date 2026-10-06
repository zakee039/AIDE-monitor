//! Local estimates in base-subscription units. Ratios are policy, not provider guarantees.
use crate::{domain, model::*};
use chrono::{DateTime, Utc};
use std::collections::HashSet;

pub const PROVIDERS: [&str; 4] = ["chatgpt", "antigravity", "grok", "claude"];
pub fn provider(id: &str) -> &str {
    match id {
        "codex" | "codex_usage" => "chatgpt",
        other => other,
    }
}

// (5h capacity multiplier, fraction of own weekly budget consumed by a full 5h window).
// For weekly-only subscriptions, the configured multiplier describes weekly capacity.
// Their own 5h ratio is unused; mixed pools convert using the platform base ratio.
pub fn profile(platform: &str, name: &str) -> Option<(f64, f64)> {
    let name = name.to_ascii_lowercase().replace(['-', ' ', '_'], "");
    match (platform, name.as_str()) {
        ("chatgpt", "plus" | "plus5h" | "plus7d" | "team" | "team5h" | "team7d" | "business") => {
            Some((1., 0.15))
        }
        ("chatgpt", "pro5x") => Some((5., 0.15)),
        ("chatgpt", "pro10x") => Some((10., 0.15)),
        ("chatgpt", "pro20x") => Some((20., 0.15)),
        ("claude", "pro") => Some((1., 0.15)),
        ("claude", "max5x" | "defaultclaudemax5x") => Some((5., 0.10)),
        ("claude", "max20x" | "defaultclaudemax20x") => Some((20., 0.20)),
        ("antigravity", "pro" | "googleaipro") => Some((1., 0.25)),
        ("antigravity", "ultra5x") => Some((5., 0.25)),
        ("grok", "supergrok") => Some((1., 0.15)),
        _ => None,
    }
}

pub fn valid_profile(platform: &str, value: &str) -> bool {
    profile(platform, value).is_some() || custom(value).is_some()
}
fn custom(value: &str) -> Option<(f64, f64)> {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 3 || parts[0] != "custom" {
        return None;
    }
    let m: f64 = parts[1].parse().ok()?;
    let r: f64 = parts[2].parse().ok()?;
    (m.is_finite() && r.is_finite() && (0.01..=1000.).contains(&m) && (0.01..=100.).contains(&r))
        .then_some((m, r / 100.))
}
fn policy(platform: &str, q: &AccountQuota, display: &DisplaySettings) -> Option<(f64, f64)> {
    if let Some(value) = display.quota_profiles.get(&q.account_id) {
        return profile(platform, value).or_else(|| custom(value));
    }
    if let Some(name) = &q.plan_type {
        return profile(platform, name);
    }
    // The UI explicitly identifies this fallback as a base-plan estimate.
    Some((1., base_ratio(platform)))
}
fn base_ratio(platform: &str) -> f64 {
    if platform == "antigravity" {
        0.25
    } else {
        0.15
    }
}

pub fn effective_provider<'a>(requested: &'a str, accounts: &'a [AccountSummary]) -> &'a str {
    if PROVIDERS.contains(&requested)
        && accounts
            .iter()
            .any(|a| a.selected && provider(&a.provider_id) == requested)
    {
        return requested;
    }
    accounts
        .iter()
        .filter(|a| a.selected && PROVIDERS.contains(&provider(&a.provider_id)))
        .min_by_key(|a| a.order)
        .map(|a| provider(&a.provider_id))
        .unwrap_or("")
}

fn scoped_quota(q: &AccountQuota, platform: &str) -> AccountQuota {
    let mut q = q.clone();
    if platform == "antigravity" {
        // Match the Gemini pool already displayed in the expanded HUD. Never add model pools.
        q.windows.retain(|w| w.scope == "feature:gemini");
        for w in &mut q.windows {
            w.scope = "base".into();
        }
        let five = q.windows.iter().any(|w| w.duration_seconds == Some(18000));
        let week = q.windows.iter().any(|w| w.duration_seconds == Some(604800));
        q.base_coverage_complete = five && week;
    } else if platform == "grok" {
        q.windows.retain(|w| w.scope == "base");
        q.base_coverage_complete =
            q.windows.len() == 1 && q.windows[0].duration_seconds == Some(604800);
    } else {
        return q;
    }
    if q.base_coverage_complete {
        let exhausted = q.windows.iter().any(|w| w.exhausted == Some(true));
        q.provider_allowed = Some(!exhausted);
        q.blocking_reason = if exhausted { "quota_windows" } else { "none" }.into();
    }
    q
}

pub fn calculate(
    accounts: &[AccountSummary],
    quotas: &[AccountQuota],
    now: DateTime<Utc>,
    display: &DisplaySettings,
) -> TotalQuota {
    let platform = display.quota_provider.as_str();
    let mut seen = HashSet::new();
    let accounts: Vec<_> = accounts
        .iter()
        .filter(|a| a.selected && provider(&a.provider_id) == platform && seen.insert(&a.id))
        .cloned()
        .collect();
    let quotas: Vec<_> = accounts
        .iter()
        .filter_map(|a| quotas.iter().find(|q| q.account_id == a.id))
        .map(|q| scoped_quota(q, platform))
        .collect();
    let availability: Vec<_> = quotas.iter().map(|q| domain::evaluate(q, now)).collect();
    let recommendation = domain::recommend(&accounts, &availability, &quotas);
    // Decide units before threshold exclusion. A temporarily depleted 5h account must
    // not switch the total from 5h units to weekly units.
    let has_five = quotas.iter().any(|q| {
        q.windows.iter().any(|w| {
            w.scope == "base" && w.duration_seconds == Some(18000) && w.applicability == "required"
        })
    });
    let mut total = 0.;
    let mut known = 0;
    let mut estimated = false;
    for q in &quotas {
        if !availability
            .iter()
            .any(|a| a.account_id == q.account_id && matches!(a.state.as_str(), "now" | "waiting"))
        {
            continue;
        }
        let required: Vec<_> = q
            .windows
            .iter()
            .filter(|w| w.scope == "base" && w.applicability == "required")
            .collect();
        let five = required.iter().find(|w| w.duration_seconds == Some(18000));
        let week = required.iter().find(|w| w.duration_seconds == Some(604800));
        let Some(b) = week.and_then(|w| w.remaining_percent) else {
            continue;
        };
        let Some((m, ratio)) = policy(platform, q, display) else {
            continue;
        };
        let contribution = match five {
            Some(w) if required.len() == 2 => w.remaining_percent.map(|a| {
                if a < 5. || b < 2. {
                    0.
                } else {
                    m * a.min(b / ratio)
                }
            }),
            None if required.len() == 1 => Some(if has_five {
                b * m / base_ratio(platform)
            } else {
                b * m
            }),
            _ => None,
        };
        if let Some(value) = contribution {
            total += value;
            known += 1;
            estimated |=
                q.plan_type.is_none() && !display.quota_profiles.contains_key(&q.account_id);
        }
    }
    TotalQuota {
        provider_id: platform.into(),
        recommendation: Some(recommendation),
        estimated,
        percent: (known > 0).then_some(total),
        partial: known < accounts.len(),
        weekly_scale_percent: base_ratio(platform) * 100.,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    fn account(id: &str, p: &str) -> AccountSummary {
        AccountSummary {
            id: id.into(),
            provider_id: p.into(),
            display_name: id.into(),
            alias: None,
            is_current: false,
            selected: true,
            order: 0,
            support: "supported".into(),
        }
    }
    #[test]
    fn provider_fallback_uses_only_selected_accounts_in_user_order() {
        let mut accounts = vec![
            account("a", "codex"),
            account("b", "claude"),
            account("c", "grok"),
        ];
        accounts[0].selected = false;
        accounts[1].order = 2;
        accounts[2].order = 1;
        assert_eq!(effective_provider("chatgpt", &accounts), "grok");
        assert_eq!(effective_provider("claude", &accounts), "claude");
        accounts[1].selected = false;
        accounts[2].selected = false;
        assert_eq!(effective_provider("grok", &accounts), "");
        accounts[0].selected = true;
        assert_eq!(effective_provider("", &accounts), "chatgpt");
    }

    fn quota(id: &str, five: Option<f64>, week: f64, now: DateTime<Utc>) -> AccountQuota {
        let mut q = AccountQuota::empty(id);
        q.origin = "network".into();
        q.freshness = "fresh".into();
        q.status = "ok".into();
        q.last_success_at = Some((now - Duration::seconds(1)).to_rfc3339());
        q.valid_until = Some((now + Duration::seconds(120)).to_rfc3339());
        q.base_coverage_complete = true;
        q.provider_allowed = Some(five != Some(0.) && week > 0.);
        q.blocking_reason = if q.provider_allowed == Some(true) {
            "none"
        } else {
            "quota_windows"
        }
        .into();
        q.windows = [(18000, five), (604800, Some(week))]
            .into_iter()
            .filter_map(|(duration, value)| {
                value.map(|value| QuotaWindow {
                    id: duration.to_string(),
                    scope: "base".into(),
                    kind: if duration == 18000 {
                        "primary"
                    } else {
                        "secondary"
                    }
                    .into(),
                    label: duration.to_string(),
                    duration_seconds: Some(duration),
                    applicability: "required".into(),
                    measurement: "percent".into(),
                    remaining_percent: Some(value),
                    exhausted: Some(value == 0.),
                    resets_at: Some((now + Duration::seconds(duration as i64)).to_rfc3339()),
                })
            })
            .collect();
        q
    }
    #[test]
    fn floors_weekly_bottleneck_and_exact_boundaries() {
        let now = Utc::now();
        let a = [account("a", "codex")];
        for (h, w, expected) in [
            (80., 20., 80.),
            (80., 6., 40.),
            (4.99, 80., 0.),
            (80., 1.99, 0.),
            (5., 2., 5.),
        ] {
            assert_eq!(
                calculate(
                    &a,
                    &[quota("a", Some(h), w, now)],
                    now,
                    &DisplaySettings::default()
                )
                .percent,
                Some(expected)
            );
        }
    }
    #[test]
    fn mixed_and_weekly_only_use_base_plan_units() {
        let now = Utc::now();
        let a = [account("a", "codex"), account("b", "codex_usage")];
        let mut b = quota("b", None, 30., now);
        b.plan_type = Some("pro5x".into());
        assert_eq!(
            calculate(
                &a,
                &[quota("a", Some(80.), 6., now), b.clone()],
                now,
                &DisplaySettings::default()
            )
            .percent,
            Some(1040.)
        );
        assert_eq!(
            calculate(
                &a,
                &[quota("a", None, 80., now), b.clone()],
                now,
                &DisplaySettings::default()
            )
            .percent,
            Some(230.)
        );
        // Threshold exclusion does not change the scale of weekly-only accounts.
        assert_eq!(
            calculate(
                &a,
                &[quota("a", Some(0.), 90., now), b],
                now,
                &DisplaySettings::default()
            )
            .percent,
            Some(1000.)
        );
    }
    #[test]
    fn claude_tiers_apply_independent_session_to_week_ratios() {
        let now = Utc::now();
        let a = [account("a", "claude")];
        let mut display = DisplaySettings::default();
        display.quota_provider = "claude".into();
        for (plan, expected) in [("pro", 40.), ("max5x", 300.), ("max20x", 600.)] {
            let mut q = quota("a", Some(80.), 6., now);
            q.plan_type = Some(plan.into());
            assert!((calculate(&a, &[q], now, &display).percent.unwrap() - expected).abs() < 1e-8);
        }
    }
    #[test]
    fn platform_selection_scopes_coverage_and_countdown() {
        let now = Utc::now();
        let a = [account("c", "codex"), account("g", "grok")];
        let mut display = DisplaySettings::default();
        display.quota_provider = "grok".into();
        let result = calculate(
            &a,
            &[quota("c", Some(80.), 90., now), quota("g", None, 0., now)],
            now,
            &display,
        );
        assert_eq!(result.percent, Some(0.));
        assert!(!result.partial);
        assert_eq!(
            result.recommendation.unwrap().account_id.as_deref(),
            Some("g")
        );
        assert_eq!(effective_provider("claude", &a), "chatgpt");
    }
    #[test]
    fn antigravity_uses_only_gemini_and_rejects_missing_or_stale_data() {
        let now = Utc::now();
        let a = [account("g", "antigravity")];
        let mut display = DisplaySettings::default();
        display.quota_provider = "antigravity".into();
        let mut q = quota("g", Some(80.), 10., now);
        for w in &mut q.windows {
            w.scope = "feature:gemini".into();
        }
        let mut other = q.windows[0].clone();
        other.scope = "feature:claude".into();
        other.remaining_percent = Some(0.);
        other.exhausted = Some(true);
        q.windows.push(other);
        assert_eq!(
            calculate(&a, &[q.clone()], now, &display).percent,
            Some(40.)
        );
        q.freshness = "stale".into();
        assert!(calculate(&a, &[q.clone()], now, &display).percent.is_none());
        q.freshness = "fresh".into();
        q.windows.remove(0);
        assert!(calculate(&a, &[q], now, &display).percent.is_none());
    }
    #[test]
    fn unknown_tier_needs_override_and_custom_values_are_bounded() {
        let now = Utc::now();
        let a = [account("g", "grok")];
        let mut q = quota("g", None, 50., now);
        q.plan_type = Some("heavy".into());
        let mut display = DisplaySettings::default();
        display.quota_provider = "grok".into();
        assert!(calculate(&a, &[q.clone()], now, &display).partial);
        display
            .quota_profiles
            .insert("g".into(), "custom:10:15".into());
        assert_eq!(calculate(&a, &[q], now, &display).percent, Some(500.));
        for value in [
            "custom:NaN:15",
            "custom:1:0",
            "custom:-1:15",
            "custom:1001:15",
            "custom:1:101",
        ] {
            assert!(!valid_profile("grok", value));
        }
    }
}
