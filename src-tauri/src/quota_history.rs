//! Retain measurements for display without claiming a new successful sample for that window.
use crate::model::AccountQuota;

pub fn retain_missing(current: &mut AccountQuota, previous: &AccountQuota) {
    if current.account_id != previous.account_id || previous.origin != "network" {
        return;
    }
    let Some(sample_time) = previous.last_success_at.as_ref() else {
        return;
    };
    for old in &previous.windows {
        if old.applicability != "required"
            || !matches!(old.measurement.as_str(), "percent" | "unlimited")
        {
            continue;
        }
        let index = current
            .windows
            .iter()
            .position(|w| w.id == old.id && w.scope == old.scope);
        if index.is_some_and(|i| {
            current.windows[i].applicability == "not_applicable"
                || current.windows[i].measurement != "unknown"
        }) {
            continue;
        }
        let mut retained = old.clone();
        retained.retained_at = Some(old.retained_at.as_ref().unwrap_or(sample_time).clone());
        if let Some(i) = index {
            current.windows[i] = retained;
        } else {
            current.windows.push(retained);
        }
    }
    // A mixed-age base snapshot remains ineligible in domain::evaluate, but its
    // measurements must still support the historical display/total calculation.
    if current
        .windows
        .iter()
        .any(|w| w.scope == "base" && w.retained_at.is_some())
    {
        let base: Vec<_> = current
            .windows
            .iter()
            .filter(|w| w.scope == "base" && w.applicability != "not_applicable")
            .collect();
        current.base_coverage_complete = !base.is_empty()
            && base.iter().all(|w| {
                w.applicability == "required"
                    && matches!(w.measurement.as_str(), "percent" | "unlimited")
            });
        if current.base_coverage_complete && current.blocking_reason != "other" {
            let allowed = base.iter().all(|w| w.exhausted == Some(false));
            current.provider_allowed = Some(allowed);
            current.blocking_reason = if allowed { "none" } else { "quota_windows" }.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use serde_json::json;
    fn quota(now: chrono::DateTime<Utc>, argon: bool) -> AccountQuota {
        let buckets = if argon {
            json!([{"bucketId":"gemini-argon-wtu-weekly","remainingFraction":0.6,"resetTime":(now + Duration::days(3)).to_rfc3339()}])
        } else {
            json!([])
        };
        crate::adapters::providers::parse(
            &json!({"groups":[{"buckets":buckets}]}),
            "antigravity",
            "a",
            now,
        )
        .unwrap()
    }
    #[test]
    fn missing_pool_keeps_balance_and_original_time_across_refreshes_and_reset() {
        let now = Utc::now();
        let old = quota(now, true);
        let mut next = quota(now + Duration::days(1), false);
        retain_missing(&mut next, &old);
        let mut later = quota(now + Duration::days(4), false);
        retain_missing(&mut later, &next);
        let w = later
            .windows
            .iter()
            .find(|w| w.scope == "feature:gemini4")
            .unwrap();
        assert_eq!(w.remaining_percent, Some(60.));
        assert_eq!(w.retained_at, old.last_success_at);
        assert_eq!(w.resets_at, old.windows.last().unwrap().resets_at);
        let mut recovered = quota(now + Duration::days(5), true);
        retain_missing(&mut recovered, &later);
        assert!(recovered.windows.iter().all(|w| w.retained_at.is_none()));
    }
    #[test]
    fn missing_measurement_retains_history_but_not_new_zero_or_inapplicable() {
        let now = Utc::now();
        let old = quota(now, true);
        let mut next = old.clone();
        let index = next.windows.len() - 1;
        next.windows[index].measurement = "unknown".into();
        next.windows[index].remaining_percent = None;
        retain_missing(&mut next, &old);
        assert_eq!(next.windows[index].remaining_percent, Some(60.));
        next.windows[index].retained_at = None;
        next.windows[index].remaining_percent = Some(0.);
        next.windows[index].exhausted = Some(true);
        retain_missing(&mut next, &old);
        assert_eq!(next.windows[index].remaining_percent, Some(0.));
        assert!(next.windows[index].retained_at.is_none());
        next.windows[index].applicability = "not_applicable".into();
        next.windows[index].measurement = "unknown".into();
        retain_missing(&mut next, &old);
        assert_eq!(next.windows[index].applicability, "not_applicable");
    }
    #[test]
    fn missing_base_measurement_retains_balance_without_restoring_live_eligibility() {
        let now = Utc::now();
        let payload = |five: serde_json::Value, week: f64| json!({"five_hour":five,"seven_day":{"utilization":week,"resets_at":(now + Duration::days(3)).to_rfc3339()}});
        let old = crate::adapters::providers::parse(
            &payload(
                json!({"utilization":15,"resets_at":(now + Duration::hours(4)).to_rfc3339()}),
                30.,
            ),
            "claude",
            "a",
            now,
        )
        .unwrap();
        let mut next = crate::adapters::providers::parse(
            &payload(serde_json::Value::Null, 100.),
            "claude",
            "a",
            now + Duration::minutes(1),
        )
        .unwrap();
        retain_missing(&mut next, &old);
        assert_eq!(next.windows.iter().find(|w| w.id == "five_hour").unwrap().remaining_percent, Some(85.));
        assert_eq!(next.windows.iter().find(|w| w.id == "seven_day").unwrap().remaining_percent, Some(0.));
        assert!(next.base_coverage_complete);
        assert_eq!(next.provider_allowed, Some(false));
        assert_eq!(
            crate::domain::evaluate(&next, now + Duration::minutes(1)).reason,
            "stale"
        );
        let mut other = next.clone();
        other.account_id = "b".into();
        other.windows.clear();
        retain_missing(&mut other, &old);
        assert!(other.windows.is_empty());
    }
}
