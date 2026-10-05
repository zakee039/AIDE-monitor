//! Quota decisions are pure and conservative. UI formatting never affects them.
use crate::model::{
    AccountAvailability, AccountQuota, AccountSummary, Coverage, QuotaWindow, Recommendation,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use std::collections::HashSet;

fn timestamp(value: &str) -> Option<DateTime<Utc>> {
    // The public contract requires UTC, including an explicit offset.
    DateTime::parse_from_rfc3339(value)
        .ok()
        .filter(|value| value.offset().local_minus_utc() == 0)
        .map(|value| value.with_timezone(&Utc))
}

fn utc(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn unknown(quota: &AccountQuota, reason: &str) -> AccountAvailability {
    AccountAvailability {
        account_id: quota.account_id.clone(),
        state: "unknown".into(),
        reason: reason.into(),
        estimated_available_at: None,
    }
}

/// Recompute the age deadline without extending the time of the last successful sample.
/// Callers use a monotonic clock to detect clock jumps; this function also rejects future samples.
pub fn update_freshness(quota: &mut AccountQuota, now: DateTime<Utc>, interval_seconds: u64) {
    quota.valid_until = None;
    quota.freshness = "unknown".into();
    if quota.origin != "network" {
        return;
    }
    let Some(success) = quota.last_success_at.as_deref().and_then(timestamp) else {
        return;
    };
    if success > now {
        return;
    }
    let limit = interval_seconds.saturating_mul(2).max(120);
    let Ok(limit) = i64::try_from(limit) else {
        return;
    };
    let Some(duration) = Duration::try_seconds(limit) else {
        return;
    };
    let Some(deadline) = success.checked_add_signed(duration) else {
        return;
    };
    quota.valid_until = Some(utc(deadline));
    quota.freshness = if now >= deadline { "stale" } else { "fresh" }.into();
}

fn measurement_known(window: &QuotaWindow) -> bool {
    if window.id.is_empty()
        || window.duration_seconds == Some(0)
        || !matches!(window.kind.as_str(), "primary" | "secondary" | "other")
    {
        return false;
    }
    match window.measurement.as_str() {
        "percent" => window.remaining_percent.is_some_and(|percent| {
            percent.is_finite()
                && (0.0..=100.0).contains(&percent)
                && window.exhausted == Some(percent == 0.0)
        }),
        "unlimited" => window.remaining_percent.is_none() && window.exhausted == Some(false),
        _ => false,
    }
}

// HUD eligibility floors, independent of the provider's actual exhaustion flag.
fn below_recommendation_floor(window: &QuotaWindow) -> bool {
    let floor = match window.duration_seconds {
        Some(18_000) => 5.0,
        Some(604_800) => 2.0,
        _ => return false,
    };
    window.measurement == "percent" && window.remaining_percent.is_some_and(|p| p < floor)
}

pub fn evaluate(quota: &AccountQuota, now: DateTime<Utc>) -> AccountAvailability {
    match quota.origin.as_str() {
        "none" => return unknown(quota, "no_data"),
        "cockpit_cache" => return unknown(quota, "cache_only"),
        "network" => {}
        _ => return unknown(quota, "no_data"),
    }
    if quota.error.is_some() || !matches!(quota.status.as_str(), "ok" | "refreshing") {
        return unknown(quota, "query_failed");
    }
    let Some(success) = quota.last_success_at.as_deref().and_then(timestamp) else {
        return unknown(quota, "clock_uncertain");
    };
    if success > now {
        return unknown(quota, "clock_uncertain");
    }
    if quota.freshness == "stale" {
        return unknown(quota, "stale");
    }
    if quota.freshness != "fresh" {
        return unknown(quota, "clock_uncertain");
    }
    if let Some(deadline) = &quota.valid_until {
        let Some(deadline) = timestamp(deadline) else {
            return unknown(quota, "clock_uncertain");
        };
        if deadline <= now {
            return unknown(quota, "stale");
        }
    }

    let base: Vec<_> = quota
        .windows
        .iter()
        .filter(|window| window.scope == "base")
        .collect();
    // A past reset invalidates an otherwise young sample. It never restores credit locally.
    for window in &base {
        if window.applicability != "required" {
            continue;
        }
        if let Some(reset) = &window.resets_at {
            let Some(reset) = timestamp(reset) else {
                return unknown(quota, "unknown_reset");
            };
            if reset <= now {
                return unknown(quota, "awaiting_confirmation");
            }
        }
    }
    if !quota.base_coverage_complete || base.is_empty() {
        return unknown(quota, "incomplete_quota");
    }
    let mut ids = HashSet::new();
    let mut required = Vec::new();
    for window in base {
        if !ids.insert(window.id.as_str()) {
            return unknown(quota, "incomplete_quota");
        }
        match window.applicability.as_str() {
            "not_applicable" => {}
            "required" if measurement_known(window) => required.push(window),
            _ => return unknown(quota, "incomplete_quota"),
        }
    }
    if required.is_empty() {
        return unknown(quota, "incomplete_quota");
    }
    let Some(allowed) = quota.provider_allowed else {
        return unknown(quota, "incomplete_quota");
    };
    let below_floor = required
        .iter()
        .any(|window| below_recommendation_floor(window));
    let has_exhausted = required.iter().any(|window| window.exhausted == Some(true));
    let blocked: Vec<_> = required
        .iter()
        .filter(|window| window.exhausted == Some(true) || below_recommendation_floor(window))
        .collect();
    if blocked.is_empty() {
        if allowed && quota.blocking_reason == "none" {
            return AccountAvailability {
                account_id: quota.account_id.clone(),
                state: "now".into(),
                reason: "available".into(),
                estimated_available_at: None,
            };
        }
        return unknown(quota, "other_limit");
    }
    if (has_exhausted && quota.blocking_reason != "quota_windows")
        || (!has_exhausted && (!allowed || quota.blocking_reason != "none"))
    {
        return unknown(quota, "other_limit");
    }
    let mut latest = None;
    for window in blocked {
        let Some(reset) = window.resets_at.as_deref().and_then(timestamp) else {
            return unknown(quota, "unknown_reset");
        };
        // Already checked above, retained here to keep this invariant explicit.
        if reset <= now {
            return unknown(quota, "awaiting_confirmation");
        }
        latest = Some(latest.map_or(reset, |value: DateTime<Utc>| value.max(reset)));
    }
    AccountAvailability {
        account_id: quota.account_id.clone(),
        state: "waiting".into(),
        reason: if below_floor && !has_exhausted {
            "below_threshold"
        } else {
            "quota_exhausted"
        }
        .into(),
        estimated_available_at: latest.map(utc),
    }
}

fn weekly_reset(quota: Option<&AccountQuota>) -> Option<DateTime<Utc>> {
    quota?
        .windows
        .iter()
        .filter(|window| {
            window.scope == "base"
                && window.applicability == "required"
                && window.kind == "secondary"
                && window.duration_seconds == Some(604_800)
        })
        .filter_map(|window| window.resets_at.as_deref().and_then(timestamp))
        .min()
}

/// Availability is produced by `evaluate` at the caller's common sampling time.
/// Only selected accounts participate; ties use user order and then opaque ID.
pub fn recommend(
    accounts: &[AccountSummary],
    availability: &[AccountAvailability],
    quotas: &[AccountQuota],
) -> Recommendation {
    let mut selected: Vec<_> = accounts.iter().filter(|account| account.selected).collect();
    selected.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    if selected.is_empty() {
        return Recommendation::empty();
    }
    let entry = |id: &str| availability.iter().find(|value| value.account_id == id);
    let valid_wait =
        |value: &AccountAvailability| value.estimated_available_at.as_deref().and_then(timestamp);
    let known = selected
        .iter()
        .filter(|account| {
            entry(&account.id).is_some_and(|value| {
                value.state == "now" || (value.state == "waiting" && valid_wait(value).is_some())
            })
        })
        .count();
    let coverage = Coverage {
        selected: selected.len(),
        known,
        unknown: selected.len() - known,
        complete: known == selected.len(),
    };
    let mut now: Vec<_> = selected
        .iter()
        .copied()
        .filter(|account| entry(&account.id).is_some_and(|value| value.state == "now"))
        .collect();
    now.sort_by(|a, b| {
        let a_reset = weekly_reset(quotas.iter().find(|value| value.account_id == a.id));
        let b_reset = weekly_reset(quotas.iter().find(|value| value.account_id == b.id));
        // Some known reset is preferred to None, then the earliest reset is preferred.
        a_reset
            .is_none()
            .cmp(&b_reset.is_none())
            .then_with(|| a_reset.cmp(&b_reset))
            .then_with(|| a.order.cmp(&b.order))
            .then_with(|| a.id.cmp(&b.id))
    });
    if let Some(account) = now.first() {
        return Recommendation {
            state: "now".into(),
            account_id: Some(account.id.clone()),
            estimated_available_at: None,
            reason: "available".into(),
            coverage,
        };
    }
    let mut waiting: Vec<_> = selected
        .iter()
        .filter_map(|account| {
            let value = entry(&account.id)?;
            if value.state != "waiting" {
                return None;
            }
            Some((*account, valid_wait(value)?))
        })
        .collect();
    waiting.sort_by(|(a, a_time), (b, b_time)| {
        a_time
            .cmp(b_time)
            .then_with(|| a.order.cmp(&b.order))
            .then_with(|| a.id.cmp(&b.id))
    });
    if let Some((account, time)) = waiting.first() {
        return Recommendation {
            state: "waiting".into(),
            account_id: Some(account.id.clone()),
            estimated_available_at: Some(utc(*time)),
            reason: entry(&account.id)
                .map(|value| value.reason.clone())
                .unwrap_or_else(|| "quota_exhausted".into()),
            coverage,
        };
    }
    let reason = selected
        .iter()
        .find_map(|account| entry(&account.id).map(|value| value.reason.clone()))
        .unwrap_or_else(|| "no_data".into());
    Recommendation {
        state: "unknown".into(),
        account_id: None,
        estimated_available_at: None,
        reason,
        coverage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApiError;

    fn time() -> DateTime<Utc> {
        timestamp("2026-10-05T00:00:00Z").unwrap()
    }
    fn at(seconds: i64) -> String {
        utc(time() + Duration::seconds(seconds))
    }
    fn window(id: &str, exhausted: bool, reset: Option<i64>) -> QuotaWindow {
        QuotaWindow {
            id: id.into(),
            scope: "base".into(),
            kind: if id == "secondary" {
                "secondary"
            } else {
                "primary"
            }
            .into(),
            label: id.into(),
            duration_seconds: Some(if id == "secondary" { 604_800 } else { 18_000 }),
            applicability: "required".into(),
            measurement: "percent".into(),
            remaining_percent: Some(if exhausted { 0.0 } else { 50.0 }),
            exhausted: Some(exhausted),
            resets_at: reset.map(at),
        }
    }
    fn quota(id: &str, windows: Vec<QuotaWindow>) -> AccountQuota {
        let exhausted = windows
            .iter()
            .any(|window| window.scope == "base" && window.exhausted == Some(true));
        AccountQuota {
            account_id: id.into(),
            origin: "network".into(),
            freshness: "fresh".into(),
            status: "ok".into(),
            last_success_at: Some(at(-1)),
            last_attempt_at: Some(at(-1)),
            observed_at: Some(at(-1)),
            valid_until: Some(at(599)),
            provider_allowed: Some(!exhausted),
            base_coverage_complete: true,
            blocking_reason: if exhausted { "quota_windows" } else { "none" }.into(),
            reset_credits_available: None,
            windows,
            error: None,
        }
    }
    fn account(id: &str, order: u64) -> AccountSummary {
        AccountSummary {
            id: id.into(),
            display_name: "Same name".into(),
            alias: None,
            is_current: false,
            provider_id: "codex".into(),
            selected: true,
            order,
            support: "supported".into(),
        }
    }
    fn result(windows: Vec<QuotaWindow>) -> AccountAvailability {
        evaluate(&quota("a", windows), time())
    }

    #[test]
    fn first_use_and_cache_do_not_manufacture_network_results() {
        let q = AccountQuota::empty("a");
        assert_eq!(q.origin, "none");
        assert_eq!(q.last_success_at, None);
        assert_eq!(evaluate(&q, time()).reason, "no_data");
        let mut q = quota("a", vec![window("primary", false, None)]);
        q.origin = "cockpit_cache".into();
        assert_eq!(evaluate(&q, time()).reason, "cache_only");
    }

    #[test]
    fn available_balance_and_subpercent_floor_are_distinguished() {
        assert_eq!(
            result(vec![
                window("primary", false, Some(3600)),
                window("secondary", false, Some(86400))
            ])
            .state,
            "now"
        );
        let mut w = window("primary", false, None);
        w.remaining_percent = Some(0.4);
        assert_eq!(result(vec![w]).reason, "unknown_reset");
    }

    #[test]
    fn recommendation_floors_include_exact_boundaries_and_wait_for_both_windows() {
        for (hourly, weekly, expected, reset) in [
            (5.0, 2.0, "now", None),
            (4.99, 2.0, "waiting", Some(3600)),
            (5.0, 1.99, "waiting", Some(86400)),
            (1.0, 1.0, "waiting", Some(86400)),
            (0.0, 1.0, "waiting", Some(86400)),
        ] {
            let mut primary = window("primary", hourly == 0.0, Some(3600));
            primary.remaining_percent = Some(hourly);
            let mut secondary = window("secondary", false, Some(86400));
            secondary.remaining_percent = Some(weekly);
            let q = quota("low", vec![primary, secondary]);
            let value = evaluate(&q, time());
            assert_eq!(value.state, expected);
            assert_eq!(value.estimated_available_at, reset.map(at));
            // Local thresholds never rewrite the provider measurement.
            assert_eq!(q.windows[0].remaining_percent, Some(hourly));
        }
    }

    #[test]
    fn recommendation_ignores_low_balance_even_if_its_weekly_reset_is_earlier() {
        let mut low = quota(
            "low",
            vec![
                window("primary", false, Some(3600)),
                window("secondary", false, Some(86400)),
            ],
        );
        low.windows[0].remaining_percent = Some(1.0);
        let good = quota(
            "good",
            vec![
                window("primary", false, Some(3600)),
                window("secondary", false, Some(172800)),
            ],
        );
        let quotas = vec![low, good];
        let availability: Vec<_> = quotas.iter().map(|q| evaluate(q, time())).collect();
        let recommended = recommend(
            &[account("low", 0), account("good", 1)],
            &availability,
            &quotas,
        );
        assert_eq!(recommended.state, "now");
        assert_eq!(recommended.account_id.as_deref(), Some("good"));
        let waiting = recommend(&[account("low", 0)], &availability, &quotas);
        assert_eq!(waiting.state, "waiting");
        assert_eq!(waiting.reason, "below_threshold");
        // Reset time alone never restores quota or promotes this account to now.
        assert_eq!(
            evaluate(&quotas[0], time() + Duration::seconds(3600)).state,
            "unknown"
        );
    }

    #[test]
    fn waiting_uses_latest_of_all_exhausted_base_windows() {
        for resets in [
            vec![3600],
            vec![172800],
            vec![3600, 172800],
            vec![3600, 7200, 10800],
        ] {
            let expected = *resets.iter().max().unwrap();
            let windows = resets
                .iter()
                .enumerate()
                .map(|(index, reset)| window(&index.to_string(), true, Some(*reset)))
                .collect();
            let availability = result(windows);
            assert_eq!(availability.state, "waiting");
            assert_eq!(availability.estimated_available_at, Some(at(expected)));
        }
        let available_other = window("secondary", false, Some(172800));
        assert_eq!(
            result(vec![window("primary", true, Some(3600)), available_other])
                .estimated_available_at,
            Some(at(3600))
        );
    }

    #[test]
    fn unknown_or_missing_reset_never_produces_waiting() {
        assert_eq!(
            result(vec![window("primary", true, None)]).reason,
            "unknown_reset"
        );
        let mut unknown = window("secondary", false, Some(7200));
        unknown.measurement = "unknown".into();
        unknown.remaining_percent = None;
        unknown.exhausted = None;
        assert_eq!(
            result(vec![window("primary", true, Some(3600)), unknown]).state,
            "unknown"
        );
        let mut invalid = window("primary", true, Some(3600));
        invalid.resets_at = Some("not-a-time".into());
        assert_eq!(result(vec![invalid]).state, "unknown");
    }

    #[test]
    fn reset_boundary_requires_confirmation_even_for_unexhausted_credit() {
        for reset in [0, -1] {
            let value = result(vec![window("primary", false, Some(reset))]);
            assert_eq!(value.reason, "awaiting_confirmation");
            assert_eq!(value.state, "unknown");
            assert_eq!(value.estimated_available_at, None);
        }
    }

    #[test]
    fn inapplicable_and_feature_windows_do_not_block_base_usage() {
        let mut secondary = window("secondary", true, Some(-1));
        secondary.applicability = "not_applicable".into();
        let mut feature = window("feature", true, Some(-1));
        feature.scope = "feature:image".into();
        let mut q = quota(
            "a",
            vec![window("primary", false, None), secondary, feature],
        );
        q.provider_allowed = Some(true);
        q.blocking_reason = "none".into();
        assert_eq!(evaluate(&q, time()).state, "now");
        q.windows[1].applicability = "unknown".into();
        assert_eq!(evaluate(&q, time()).state, "unknown");
    }

    #[test]
    fn unlimited_requires_explicit_nonexhausted_evidence() {
        let mut w = window("primary", false, None);
        w.measurement = "unlimited".into();
        w.remaining_percent = None;
        assert_eq!(result(vec![w.clone()]).state, "now");
        w.exhausted = None;
        assert_eq!(result(vec![w]).state, "unknown");
        assert_eq!(result(Vec::new()).state, "unknown");
    }

    #[test]
    fn corrupt_measurements_and_duplicate_ids_are_unknown() {
        for value in [-1.0, 101.0, f64::NAN, f64::INFINITY, 0.0] {
            let mut w = window("primary", false, None);
            w.remaining_percent = Some(value);
            assert_eq!(result(vec![w]).state, "unknown");
        }
        let mut w = window("primary", true, None);
        w.remaining_percent = Some(1.0);
        assert_eq!(result(vec![w]).state, "unknown");
        assert_eq!(
            result(vec![
                window("primary", false, None),
                window("primary", false, None)
            ])
            .state,
            "unknown"
        );
    }

    #[test]
    fn complete_windows_cannot_override_unknown_or_other_provider_limit() {
        let mut q = quota("a", vec![window("primary", false, None)]);
        q.provider_allowed = Some(false);
        assert_eq!(evaluate(&q, time()).reason, "other_limit");
        q.provider_allowed = None;
        assert_eq!(evaluate(&q, time()).state, "unknown");
        q.provider_allowed = Some(true);
        q.base_coverage_complete = false;
        assert_eq!(evaluate(&q, time()).state, "unknown");
        q = quota("a", vec![window("primary", true, Some(3600))]);
        q.blocking_reason = "other".into();
        assert_eq!(evaluate(&q, time()).reason, "other_limit");
    }

    #[test]
    fn refreshing_retains_only_reliable_history() {
        let mut q = quota("a", vec![window("primary", false, None)]);
        q.status = "refreshing".into();
        assert_eq!(evaluate(&q, time()).state, "now");
        q.error = Some(ApiError::new("NETWORK_ERROR", "Could not refresh"));
        assert_eq!(evaluate(&q, time()).reason, "query_failed");
        q.error = None;
        for status in ["auth_expired", "error", "unavailable"] {
            q.status = status.into();
            assert_eq!(evaluate(&q, time()).reason, "query_failed");
        }
    }

    #[test]
    fn freshness_expires_at_exact_deadline_and_recomputes_after_interval_change() {
        for (interval, age) in [(300, 600), (60, 120), (1800, 3600)] {
            let mut q = quota("a", vec![window("primary", false, None)]);
            q.last_success_at = Some(at(-age));
            update_freshness(&mut q, time(), interval);
            assert_eq!(q.freshness, "stale");
            assert_eq!(evaluate(&q, time()).reason, "stale");
            update_freshness(&mut q, time() - Duration::seconds(1), interval);
            assert_eq!(q.freshness, "fresh");
        }
        let mut q = quota("a", vec![window("primary", false, None)]);
        q.last_success_at = Some(at(-180));
        update_freshness(&mut q, time(), 300);
        assert_eq!(q.freshness, "fresh");
        update_freshness(&mut q, time(), 60);
        assert_eq!(q.freshness, "stale");
    }

    #[test]
    fn future_samples_and_invalid_times_do_not_extend_freshness() {
        let mut q = quota("a", vec![window("primary", false, None)]);
        q.last_success_at = Some(at(1));
        update_freshness(&mut q, time(), 300);
        assert_eq!(q.freshness, "unknown");
        assert_eq!(q.valid_until, None);
        assert_eq!(evaluate(&q, time()).reason, "clock_uncertain");
        q.last_success_at = Some("2026-10-05T08:00:00+08:00".into());
        update_freshness(&mut q, time(), 300);
        assert_eq!(q.freshness, "unknown");
    }

    #[test]
    fn now_precedes_waiting_and_errors_reduce_coverage() {
        let accounts = vec![
            account("waiting", 0),
            account("now", 1),
            account("error", 2),
        ];
        let mut error = quota("error", vec![window("primary", false, None)]);
        error.status = "auth_expired".into();
        let quotas = vec![
            quota("waiting", vec![window("primary", true, Some(3600))]),
            quota("now", vec![window("primary", false, None)]),
            error,
        ];
        let availability: Vec<_> = quotas.iter().map(|q| evaluate(q, time())).collect();
        let recommendation = recommend(&accounts, &availability, &quotas);
        assert_eq!(recommendation.account_id.as_deref(), Some("now"));
        assert_eq!(
            recommendation.coverage,
            Coverage {
                selected: 3,
                known: 2,
                unknown: 1,
                complete: false
            }
        );
    }

    #[test]
    fn earliest_real_weekly_reset_then_user_order_then_id() {
        let accounts = vec![
            account("no-week", 0),
            account("later", 1),
            account("earlier", 2),
        ];
        let quotas = vec![
            quota("no-week", vec![window("primary", false, None)]),
            quota("later", vec![window("secondary", false, Some(172800))]),
            quota("earlier", vec![window("secondary", false, Some(86400))]),
        ];
        let availability: Vec<_> = quotas.iter().map(|q| evaluate(q, time())).collect();
        assert_eq!(
            recommend(&accounts, &availability, &quotas)
                .account_id
                .as_deref(),
            Some("earlier")
        );
        let mut quotas = quotas;
        quotas[2].windows[0].duration_seconds = Some(86400);
        assert_eq!(
            recommend(&accounts, &availability, &quotas)
                .account_id
                .as_deref(),
            Some("later")
        );
        quotas[1].windows[0].duration_seconds = Some(86400);
        assert_eq!(
            recommend(&accounts, &availability, &quotas)
                .account_id
                .as_deref(),
            Some("no-week")
        );
        let tied = vec![account("z", 1), account("a", 1)];
        let availability = vec![
            AccountAvailability {
                account_id: "z".into(),
                state: "now".into(),
                reason: "available".into(),
                estimated_available_at: None,
            },
            AccountAvailability {
                account_id: "a".into(),
                state: "now".into(),
                reason: "available".into(),
                estimated_available_at: None,
            },
        ];
        assert_eq!(
            recommend(&tied, &availability, &[]).account_id.as_deref(),
            Some("a")
        );
    }

    #[test]
    fn earliest_waiting_candidate_reports_incomplete_coverage() {
        let accounts = vec![
            account("later", 0),
            account("earlier", 1),
            account("missing", 2),
        ];
        let quotas = vec![
            quota("later", vec![window("primary", true, Some(10800))]),
            quota("earlier", vec![window("primary", true, Some(7200))]),
        ];
        let availability: Vec<_> = quotas.iter().map(|q| evaluate(q, time())).collect();
        let result = recommend(&accounts, &availability, &quotas);
        assert_eq!(result.account_id.as_deref(), Some("earlier"));
        assert_eq!(result.estimated_available_at, Some(at(7200)));
        assert!(!result.coverage.complete);
    }

    #[test]
    fn deselected_accounts_and_empty_selection_are_not_recommended() {
        let mut a = account("a", 0);
        a.selected = false;
        let quotas = vec![quota("a", vec![window("primary", false, None)])];
        let availability = vec![evaluate(&quotas[0], time())];
        assert_eq!(recommend(&[a], &availability, &quotas).state, "empty");
        let result = recommend(&[account("b", 0)], &availability, &quotas);
        assert_eq!(result.state, "unknown");
        assert_eq!(result.coverage.unknown, 1);
    }
}
