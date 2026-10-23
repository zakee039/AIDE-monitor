//! Display entries share one physical account and one network refresh.
use crate::model::*;
use std::collections::BTreeMap;

const SEPARATOR: &str = "::pool:";
pub fn source_id(id: &str) -> &str {
    id.split_once(SEPARATOR).map_or(id, |(source, _)| source)
}
pub fn options(provider: &str) -> &'static [&'static str] {
    match provider {
        "antigravity" => &["gemini3", "claude", "gemini4"],
        "grok" => &["shared"],
        _ => &[],
    }
}
pub fn scope(pool: &str) -> String {
    match pool {
        "gemini3" => "feature:gemini".into(),
        "shared" | "base" => "base".into(),
        _ => format!("feature:{pool}"),
    }
}
pub fn catalog(provider: &str, windows: &[QuotaWindow], saved: &[PoolOption]) -> Vec<PoolOption> {
    let mut result: Vec<_> = options(provider)
        .iter()
        .map(|id| PoolOption {
            id: (*id).into(),
            label: match *id {
                "gemini3" => "Gemini 3",
                "claude" => "Claude",
                "gemini4" => "Gemini 4",
                "shared" => "共享额度",
                _ => id,
            }
            .into(),
        })
        .collect();
    for item in saved {
        if !result.iter().any(|p| p.id == item.id) && result.len() < 100 {
            result.push(item.clone());
        }
    }
    for w in windows {
        let Some(feature) = w.scope.strip_prefix("feature:") else {
            continue;
        };
        let id = if provider == "antigravity" && feature == "gemini" {
            "gemini3"
        } else {
            feature
        };
        if id.is_empty()
            || id.len() > 160
            || id.contains("::pool:")
            || id.chars().any(char::is_control)
        {
            continue;
        }
        if result.iter().any(|p| p.id == id) || result.len() >= 100 {
            continue;
        }
        result.push(PoolOption {
            id: id.into(),
            label: w
                .label
                .split(" · ")
                .next()
                .unwrap_or(id)
                .chars()
                .filter(|c| !c.is_control())
                .take(100)
                .collect(),
        });
    }
    if options(provider).is_empty() && !result.is_empty() && !result.iter().any(|p| p.id == "base")
    {
        result.insert(
            0,
            PoolOption {
                id: "base".into(),
                label: "主额度".into(),
            },
        );
    }
    result
}
pub fn tracks_window(
    provider: &str,
    id: &str,
    choices: &BTreeMap<String, Vec<String>>,
    window: &QuotaWindow,
) -> bool {
    window.applicability == "required"
        && (window.scope == "base"
            || selected(provider, id, choices)
                .into_iter()
                .any(|pool| scope(pool) == window.scope))
}
pub fn refresh_ids(
    requested: Vec<String>,
    physical: &[String],
    displayed: &[AccountSummary],
) -> Result<Vec<String>, ApiError> {
    if requested.is_empty()
        || requested.len() > 100
        || requested
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != requested.len()
    {
        return Err(ApiError::new("INVALID_ARGUMENT", "刷新账号重复或过多"));
    }
    let mut ids = Vec::new();
    for id in requested {
        if !physical.contains(&id) && !displayed.iter().any(|a| a.id == id) {
            return Err(ApiError::new("INVALID_ARGUMENT", "请选择有效的额度池"));
        }
        let source = source_id(&id).to_string();
        if !ids.contains(&source) {
            ids.push(source);
        }
    }
    Ok(ids)
}
pub fn selected<'a>(
    provider: &str,
    id: &str,
    choices: &'a BTreeMap<String, Vec<String>>,
) -> Vec<&'a str> {
    choices
        .get(id)
        .map(|pools| pools.iter().map(String::as_str).collect())
        .unwrap_or_else(|| vec![options(provider).first().copied().unwrap_or("base")])
}
pub fn project(
    accounts: Vec<AccountSummary>,
    quotas: Vec<AccountQuota>,
    choices: &BTreeMap<String, Vec<String>>,
) -> (Vec<AccountSummary>, Vec<AccountQuota>) {
    project_named(accounts, quotas, choices, &BTreeMap::new())
}
pub fn project_named(
    accounts: Vec<AccountSummary>,
    quotas: Vec<AccountQuota>,
    choices: &BTreeMap<String, Vec<String>>,
    aliases: &BTreeMap<String, String>,
) -> (Vec<AccountSummary>, Vec<AccountQuota>) {
    let mut displayed = vec![];
    let mut samples = vec![];
    for account in accounts {
        let quota = quotas
            .iter()
            .find(|q| q.account_id == account.id)
            .cloned()
            .unwrap_or_else(|| AccountQuota::empty(&account.id));
        let pools = catalog(&account.provider_id, &quota.windows, &account.pools);
        if pools.is_empty() {
            displayed.push(account);
            samples.push(quota);
            continue;
        }
        for pool in selected(&account.provider_id, &account.id, choices) {
            if !pools.iter().any(|p| p.id == pool) {
                continue;
            }
            let mut entry = account.clone();
            entry.id = format!("{}{SEPARATOR}{pool}", account.id);
            entry.display_name = if pool == "base" {
                account.display_name.clone()
            } else {
                format!(
                    "{}-{}",
                    account.display_name,
                    if options(&account.provider_id).contains(&pool) {
                        pool
                    } else {
                        pools
                            .iter()
                            .find(|p| p.id == pool)
                            .map(|p| p.label.as_str())
                            .unwrap_or(pool)
                    }
                )
            };
            if let Some(name) = aliases
                .get(&entry.id)
                .filter(|name| !name.trim().is_empty())
            {
                entry.display_name = format!("{}-{name}", account.display_name);
            }
            entry.alias = None;
            let mut sample = quota.clone();
            sample.account_id = entry.id.clone();
            sample.windows.retain(|w| w.scope == scope(pool));
            for w in &mut sample.windows {
                w.scope = "base".into();
            }
            // A pool that has not been returned must remain unknown, including its reset.
            if sample.windows.is_empty() {
                sample.windows.push(QuotaWindow {
                    retained_at: None,
                    id: format!("{pool}-weekly"),
                    scope: "base".into(),
                    kind: "primary".into(),
                    label: pool.into(),
                    duration_seconds: if pool == "gemini4" {
                        Some(604800)
                    } else {
                        None
                    },
                    applicability: "required".into(),
                    measurement: "unknown".into(),
                    remaining_percent: None,
                    exhausted: None,
                    resets_at: None,
                });
            }
            sample.base_coverage_complete =
                sample.windows.iter().all(|w| w.remaining_percent.is_some());
            sample.provider_allowed = sample
                .base_coverage_complete
                .then(|| sample.windows.iter().all(|w| w.exhausted == Some(false)));
            sample.blocking_reason = match sample.provider_allowed {
                Some(true) => "none",
                Some(false) => "quota_windows",
                None => "unknown",
            }
            .into();
            displayed.push(entry);
            samples.push(sample);
        }
    }
    for (order, a) in displayed.iter_mut().enumerate() {
        a.order = order as u64;
    }
    (displayed, samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn account() -> AccountSummary {
        AccountSummary {
            pools: vec![],
            id: "a".into(),
            display_name: "name".into(),
            alias: None,
            is_current: false,
            provider_id: "antigravity".into(),
            selected: true,
            order: 0,
            support: "supported".into(),
        }
    }
    #[test]
    fn independent_pools_preserve_values_ids_errors_and_weekly_only_argon() {
        let mut q = crate::adapters::providers::parse(&json!({"groups":[{"buckets":[
            {"bucketId":"gemini-5h","remainingFraction":0.8}, {"bucketId":"gemini-weekly","remainingFraction":0.7},
            {"bucketId":"3p-5h","remainingFraction":0.3}, {"bucketId":"3p-weekly","remainingFraction":0.2},
            {"bucketId":"gemini-argon-wtu-weekly","remainingFraction":0.6}
        ]}]}), "antigravity", "a", Utc::now()).unwrap();
        q.error = Some(ApiError::new("NETWORK_ERROR", "test"));
        let choices = BTreeMap::from([(
            "a".into(),
            vec!["gemini3".into(), "claude".into(), "gemini4".into()],
        )]);
        let (accounts, quotas) = project(vec![account()], vec![q], &choices);
        assert_eq!(
            accounts
                .iter()
                .map(|a| a.display_name.as_str())
                .collect::<Vec<_>>(),
            ["name-gemini3", "name-claude", "name-gemini4"]
        );
        assert_eq!(
            quotas.iter().map(|q| q.windows.len()).collect::<Vec<_>>(),
            [2, 2, 1]
        );
        assert_eq!(quotas[1].windows[0].remaining_percent, Some(30.));
        assert_eq!(quotas[2].windows[0].remaining_percent, Some(60.));
        assert!(quotas
            .iter()
            .all(|q| q.error.is_some() && q.windows.iter().all(|w| w.scope == "base")));
        assert!(accounts.iter().all(|a| source_id(&a.id) == "a"));
    }
    #[test]
    fn old_settings_default_to_gemini_and_missing_argon_is_unknown() {
        let (a, _) = project(vec![account()], vec![], &BTreeMap::new());
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].display_name, "name-gemini3");
        let choices = BTreeMap::from([("a".into(), vec!["gemini4".into()])]);
        let (_, q) = project(vec![account()], vec![], &choices);
        assert!(!q[0].base_coverage_complete);
        assert_eq!(q[0].windows[0].remaining_percent, None);
        assert_eq!(q[0].windows[0].resets_at, None);
        assert_eq!(q[0].windows[0].duration_seconds, Some(604800));
    }
    #[test]
    fn refresh_resolves_selected_pools_once_and_rejects_unselected_suffixes() {
        let choices = BTreeMap::from([("a".into(), vec!["gemini3".into(), "claude".into()])]);
        let (displayed, _) = project(vec![account()], vec![], &choices);
        let physical = vec!["a".into()];
        assert_eq!(
            refresh_ids(
                vec![
                    "a::pool:gemini3".into(),
                    "a::pool:claude".into(),
                    "a".into()
                ],
                &physical,
                &displayed
            )
            .unwrap(),
            physical
        );
        assert!(refresh_ids(vec!["a::pool:gemini4".into()], &physical, &displayed).is_err());
        assert!(refresh_ids(vec!["a::pool:unknown".into()], &physical, &displayed).is_err());
    }
    #[test]
    fn totals_follow_selected_pools_and_inherit_physical_account_profile() {
        let now = Utc::now();
        let q = crate::adapters::providers::parse(&json!({"groups":[{"buckets":[
            {"bucketId":"gemini-5h","remainingFraction":0.8}, {"bucketId":"gemini-weekly","remainingFraction":0.7},
            {"bucketId":"3p-5h","remainingFraction":0.3}, {"bucketId":"3p-weekly","remainingFraction":0.2},
            {"bucketId":"gemini-argon-wtu-weekly","remainingFraction":0.6}
        ]}]}), "antigravity", "a", now).unwrap();
        let choices = BTreeMap::from([(
            "a".into(),
            vec!["gemini3".into(), "claude".into(), "gemini4".into()],
        )]);
        let (accounts, mut quotas) = project(vec![account()], vec![q], &choices);
        for q in &mut quotas {
            q.five_hour_weekly_ratio = Some(if q.account_id.ends_with("::pool:claude") {
                0.5
            } else {
                0.25
            });
        }
        let mut display = DisplaySettings::default();
        display.quota_provider = "antigravity".into();
        let total = crate::quota_total::calculate(&accounts, &quotas, now, &display);
        assert_eq!(total.percent, Some(350.));
        assert!(!total.partial);
        display
            .quota_profiles
            .insert("a".into(), "custom:2:25".into());
        let total = crate::quota_total::calculate(&accounts, &quotas, now, &display);
        assert_eq!(total.percent, Some(700.));
        assert!(total.estimated);
    }

    #[test]
    fn discovered_pool_can_be_selected_and_retained_without_guessing_total() {
        let now = Utc::now();
        let q=crate::adapters::providers::parse(&json!({"groups":[{"buckets":[
            {"bucketId":"future-5h","remainingFraction":0.8}, {"bucketId":"future-weekly","remainingFraction":0.9}
        ]}]}),"antigravity","a",now).unwrap();
        let mut a = account();
        a.pools = catalog("antigravity", &q.windows, &[]);
        let dynamic = a
            .pools
            .iter()
            .find(|p| p.id.starts_with("bucket-"))
            .unwrap()
            .id
            .clone();
        let choices = BTreeMap::from([("a".into(), vec![dynamic.clone()])]);
        let (accounts, mut quotas) = project(vec![a.clone()], vec![q], &choices);
        assert_eq!(accounts.len(), 1);
        assert_eq!(quotas[0].windows.len(), 2);
        quotas[0].five_hour_weekly_ratio = Some(0.4);
        let mut display = DisplaySettings::default();
        display.quota_provider = "antigravity".into();
        let total = crate::quota_total::calculate(&accounts, &quotas, now, &display);
        assert!(total.partial);
        assert_eq!(total.percent, None);
        let (restored, unknown) = project(vec![a.clone()], vec![], &choices);
        assert_eq!(restored[0].id, accounts[0].id);
        assert_eq!(unknown[0].windows[0].measurement, "unknown");
        assert_eq!(catalog("antigravity", &[], &a.pools), a.pools);
    }
}
