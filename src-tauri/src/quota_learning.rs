//! Learn capacity ratios from paired consumption, never from elapsed time alone.
use crate::model::{AccountQuota, RatioCalibration};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Sample {
    at: i64,
    five: f64,
    week: f64,
    five_reset: i64,
    week_reset: i64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Learner {
    history: Vec<Sample>,
    ratio: Option<f64>,
    pending: Option<f64>,
    validated: Option<i64>,
    last_check: Option<i64>,
    retained: bool,
}
const TOLERANCE: f64 = 0.021;
fn ratio_valid(r: f64) -> bool {
    r.is_finite() && r > 0. && r <= 1.
}
fn agrees(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE + 1e-10
}
pub fn key(id: &str) -> &str {
    id.strip_suffix("::pool:base").unwrap_or(id)
}
fn sample(q: &AccountQuota) -> Option<Sample> {
    if q.origin != "network" || q.status != "ok" || q.freshness != "fresh" || q.error.is_some() {
        return None;
    }
    let at = DateTime::parse_from_rfc3339(q.last_success_at.as_deref()?)
        .ok()?
        .timestamp();
    let get = |duration| {
        let mut windows = q
            .windows
            .iter()
            .filter(|w| w.scope == "base" && w.duration_seconds == Some(duration));
        let w = windows.next()?;
        if windows.next().is_some()
            || w.retained_at.is_some()
            || w.measurement != "percent"
            || w.applicability != "required"
        {
            return None;
        }
        let n = w
            .remaining_percent
            .filter(|n| n.is_finite() && (0.0..=100.0).contains(n))?;
        let reset = DateTime::parse_from_rfc3339(w.resets_at.as_deref()?)
            .ok()?
            .timestamp();
        (reset > at).then_some((n, reset))
    };
    let (five, five_reset) = get(18000)?;
    let (week, week_reset) = get(604800)?;
    Some(Sample {
        at,
        five,
        week,
        five_reset,
        week_reset,
    })
}

impl Learner {
    pub fn sample_count(&self) -> usize {
        self.history.len()
    }
    pub fn last_observed(&self) -> i64 {
        self.last_check.unwrap_or(0)
    }
    pub fn discard_history(&mut self) {
        self.history.clear();
        self.pending = None;
        self.retained = true;
    }
    pub fn observe(&mut self, q: &AccountQuota) {
        let Some(current) = sample(q) else {
            self.pending = None;
            self.retained = true;
            return;
        };
        if self.last_check.is_some_and(|at| current.at <= at) {
            if self.last_check.is_some_and(|at| current.at < at) {
                self.history.clear();
                self.pending = None;
                self.retained = true;
                self.last_check = Some(current.at);
            }
            return;
        }
        self.last_check = Some(current.at);
        let reset = self.history.last().is_some_and(|old| {
            old.five_reset != current.five_reset
                || old.week_reset != current.week_reset
                || current.five > old.five + 0.01
                || current.week > old.week + 0.01
        });
        if reset {
            self.history.clear();
            self.pending = None;
        }
        self.history.retain(|s| {
            s.at < current.at && s.five_reset > current.at && s.week_reset > current.at
        });
        // Nearest qualifying anchor first: n-1, n-2, ... in this same reset cycle.
        let candidate = self
            .history
            .iter()
            .rev()
            .find(|old| old.five - current.five >= 50. - 1e-10)
            .and_then(|old| {
                let delta_week = old.week - current.week;
                let ratio = delta_week / (old.five - current.five);
                // At zero the upstream may have clipped further consumption; do not train it.
                (current.five > 0. && current.week > 0. && delta_week > 0. && ratio_valid(ratio))
                    .then_some(ratio)
            });
        self.apply(candidate, current.at);
        // Keep the newest fresh observation of an unchanged pair. Each successful
        // refresh still validates the candidate; cached/error responses cannot confirm it.
        if self
            .history
            .last()
            .is_some_and(|old| old.five == current.five && old.week == current.week)
        {
            self.history.pop();
        }
        self.history.push(current);
        if self.history.len() > 512 {
            self.history.remove(0);
        }
    }
    fn apply(&mut self, candidate: Option<f64>, at: i64) {
        let Some(candidate) = candidate else {
            self.pending = None;
            self.retained = true;
            return;
        };
        match self.ratio {
            None => {
                self.ratio = Some(candidate);
                self.validated = Some(at);
                self.pending = None;
                self.retained = false;
            }
            Some(old) if agrees(candidate, old) => {
                self.validated = Some(at);
                self.pending = None;
                self.retained = false;
            }
            Some(_)
                if self
                    .pending
                    .is_some_and(|previous| agrees(candidate, previous)) =>
            {
                self.ratio = Some(candidate);
                self.validated = Some(at);
                self.pending = None;
                self.retained = false;
            }
            Some(_) => {
                self.pending = Some(candidate);
                self.retained = true;
            }
        }
    }
    pub fn estimate(&self, now: DateTime<Utc>) -> RatioCalibration {
        let ratio = self
            .ratio
            .filter(|r| ratio_valid(*r) && self.validated.is_some_and(|t| t <= now.timestamp()));
        RatioCalibration {
            state: if ratio.is_none() {
                "learning"
            } else if self.retained || self.validated.is_some_and(|t| now.timestamp() - t > 3600) {
                "retained"
            } else {
                "calibrated"
            }
            .into(),
            samples: self.history.len(),
            ratio,
            validated_at: self
                .validated
                .and_then(|t| DateTime::from_timestamp(t, 0))
                .map(|t| t.to_rfc3339()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::QuotaWindow;
    fn now(at: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(at, 0).unwrap()
    }
    fn quota(at: i64, five: f64, week: f64) -> AccountQuota {
        let mut q = AccountQuota::empty("a");
        q.origin = "network".into();
        q.status = "ok".into();
        q.freshness = "fresh".into();
        q.last_success_at = Some(now(at).to_rfc3339());
        q.windows = [(18000, five), (604800, week)]
            .into_iter()
            .map(|(d, n)| QuotaWindow {
                retained_at: None,
                id: d.to_string(),
                scope: "base".into(),
                kind: "primary".into(),
                label: "test".into(),
                duration_seconds: Some(d),
                applicability: "required".into(),
                measurement: "percent".into(),
                remaining_percent: Some(n),
                exhausted: Some(n == 0.),
                resets_at: Some(now(100000 + d as i64).to_rfc3339()),
            })
            .collect();
        q
    }
    #[test]
    fn user_examples_backtrack_to_nearest_fifty_point_anchor() {
        let mut l = Learner::default();
        l.observe(&quota(100000, 80., 90.));
        l.observe(&quota(100300, 20., 60.));
        assert_eq!(l.estimate(now(100300)).ratio, Some(0.5));
        let mut l = Learner::default();
        l.observe(&quota(100000, 80., 90.));
        l.observe(&quota(100300, 60., 80.));
        assert_eq!(l.estimate(now(100300)).ratio, None);
        l.observe(&quota(100600, 20., 60.));
        assert_eq!(l.estimate(now(100600)).ratio, Some(0.5));
        let mut l = Learner::default();
        l.observe(&quota(100000, 100., 100.));
        l.observe(&quota(100300, 80., 90.));
        l.observe(&quota(100600, 30., 65.));
        assert_eq!(l.estimate(now(100600)).ratio, Some(0.5)); // latest 80/90, not oldest 100/100
    }
    #[test]
    fn absolute_tolerance_and_two_consecutive_values_follow_user_example() {
        let mut l = Learner::default();
        for (at, candidate, expected) in [
            (1, 0.30, 0.30),
            (2, 0.28, 0.30),
            (3, 0.20, 0.30),
            (4, 0.20, 0.20),
        ] {
            l.apply(Some(candidate), at);
            assert_eq!(l.ratio, Some(expected));
        }
        assert!(agrees(0.30, 0.321));
        assert!(!agrees(0.30, 0.322));
        l.apply(Some(0.4), 5);
        l.apply(None, 6);
        l.apply(Some(0.4), 7);
        assert_eq!(l.ratio, Some(0.2));
        l.apply(Some(0.41), 8);
        assert_eq!(l.ratio, Some(0.41));
    }
    #[test]
    fn fresh_refreshes_confirm_changes_but_errors_interrupt_confirmation() {
        let mut l = Learner::default();
        l.observe(&quota(100000, 90., 90.));
        l.observe(&quota(100300, 40., 75.));
        assert_eq!(l.ratio, Some(0.3));
        // A reset starts a new evidence window, while preserving the established ratio.
        l.observe(&quota(100600, 100., 100.));
        l.observe(&quota(100900, 50., 90.));
        assert_eq!(l.ratio, Some(0.3));
        assert_eq!(l.pending, Some(0.2));
        let mut error = quota(101000, 49., 89.8);
        error.error = Some(crate::model::ApiError::new("NETWORK_ERROR", "test"));
        l.observe(&error);
        assert!(l.pending.is_none());
        l.observe(&quota(101100, 45., 89.));
        assert_eq!(l.ratio, Some(0.3));
        l.observe(&quota(101400, 40., 88.));
        assert_eq!(l.ratio, Some(0.2));
    }
    #[test]
    fn reset_missing_clipped_and_clock_reversal_do_not_corrupt_estimate() {
        let mut l = Learner::default();
        l.observe(&quota(100000, 80., 90.));
        l.observe(&quota(100300, 20., 60.));
        let mut q = quota(100600, 100., 60.);
        q.windows[0].resets_at = Some(now(140000).to_rfc3339());
        l.observe(&q);
        assert_eq!(l.history.len(), 1);
        assert_eq!(l.ratio, Some(0.5));
        q.windows[0].retained_at = Some(now(100300).to_rfc3339());
        l.observe(&q);
        assert!(l.pending.is_none());
        l.observe(&quota(100900, 0., 10.));
        assert_eq!(l.ratio, Some(0.5));
        l.observe(&quota(100800, 10., 20.));
        assert!(l.history.is_empty());
    }
    #[test]
    fn persistence_and_pool_keys_preserve_isolation() {
        let mut l = Learner::default();
        l.observe(&quota(100000, 80., 90.));
        l.observe(&quota(100300, 20., 60.));
        let restored: Learner = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(restored.estimate(now(100300)), l.estimate(now(100300)));
        assert_eq!(restored.estimate(now(900000)).ratio, Some(0.5));
        assert_eq!(key("a::pool:base"), "a");
        assert_ne!(key("a::pool:x"), key("a::pool:y"));
    }
}
