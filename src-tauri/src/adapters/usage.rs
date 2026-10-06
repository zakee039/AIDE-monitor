use super::cockpit::Credentials;
use crate::model::{AccountQuota, ApiError, QuotaWindow};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use futures_util::StreamExt;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, RETRY_AFTER},
    Client, RequestBuilder, Response, StatusCode,
};
use serde_json::Value;

const USAGE_ENDPOINT: &str = "https://chatgpt.com/backend-api/wham/usage";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// The caller must supply a client with redirects disabled. This adapter never accepts a URL.
pub async fn fetch_quota(
    client: &Client,
    credentials: &Credentials,
    hud_account_id: &str,
) -> Result<AccountQuota, ApiError> {
    let response = (if credentials.provider == "codex_usage" {
        quota_request(client, credentials)
    } else {
        super::providers::request(client, credentials)
    })?
    .send()
    .await
    .map_err(network_error)?;
    read_provider_response(response, hud_account_id, &credentials.provider).await
}

fn quota_request(client: &Client, credentials: &Credentials) -> Result<RequestBuilder, ApiError> {
    let workspace = credentials
        .account_id
        .as_deref()
        .ok_or_else(|| ApiError::new("ACCOUNT_UNSUPPORTED", "账号缺少工作区标识"))?;
    let mut workspace = HeaderValue::from_str(workspace)
        .map_err(|_| ApiError::new("ACCOUNT_UNSUPPORTED", "账号工作区标识格式无效"))?;
    workspace.set_sensitive(true);
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", credentials.access_token))
        .map_err(|_| ApiError::new("AUTH_EXPIRED", "账号访问凭据格式无效"))?;
    authorization.set_sensitive(true);
    Ok(client
        .get(USAGE_ENDPOINT)
        .timeout(std::time::Duration::from_secs(15))
        .header(AUTHORIZATION, authorization)
        .header("ChatGPT-Account-Id", workspace)
        .header("originator", "AIDE monitor")
        .header(
            "User-Agent",
            concat!("AIDE-monitor/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/json"))
}

#[cfg(test)]
async fn read_response(response: Response, hud_account_id: &str) -> Result<AccountQuota, ApiError> {
    read_provider_response(response, hud_account_id, "codex_usage").await
}

async fn read_provider_response(
    response: Response,
    hud_account_id: &str,
    provider: &str,
) -> Result<AccountQuota, ApiError> {
    let observed_at = Utc::now();
    let status = response.status();
    if !status.is_success() {
        // Do not consume or expose upstream error bodies: they may contain identity or credentials.
        return Err(status_error(status, response.headers(), observed_at));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(unsupported_response());
    }
    let mut bytes = Vec::new();
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(network_error)?;
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(unsupported_response());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| unsupported_response())?;
    if provider == "codex_usage" {
        parse_usage(&value, hud_account_id, observed_at)
    } else {
        super::providers::parse(&value, provider, hud_account_id, observed_at)
    }
}

/// Missing or invalid measurements stay unknown. No missing window is treated as full quota.
pub fn parse_usage(
    value: &Value,
    hud_account_id: &str,
    observed_at: DateTime<Utc>,
) -> Result<AccountQuota, ApiError> {
    if !value.is_object() {
        return Err(unsupported_response());
    }
    let rate = value.get("rate_limit").filter(|value| !value.is_null());
    if rate.is_some_and(|value| !value.is_object()) {
        return Err(unsupported_response());
    }
    let mut windows = vec![
        parse_window(
            rate.and_then(|rate| rate.get("primary_window")),
            "base",
            "primary",
            observed_at,
        ),
        parse_window(
            rate.and_then(|rate| rate.get("secondary_window")),
            "base",
            "secondary",
            observed_at,
        ),
    ];
    // Some plans return a weekly window with the other slot absent/null.
    // Only a recognized, measured weekly window establishes this shape;
    // a malformed present slot must remain unknown.
    let weekly_only = windows
        .iter()
        .any(|window| window.duration_seconds == Some(604_800) && window.measurement == "percent")
        && ["primary", "secondary"].iter().any(|kind| {
            rate.and_then(|rate| rate.get(format!("{kind}_window")))
                .is_none_or(Value::is_null)
        });
    if weekly_only {
        for (window, kind) in windows.iter_mut().zip(["primary", "secondary"]) {
            if rate
                .and_then(|rate| rate.get(format!("{kind}_window")))
                .is_none_or(Value::is_null)
            {
                window.applicability = "not_applicable".into();
            }
        }
    }
    let base_coverage_complete = windows.iter().all(|window| {
        window.applicability == "not_applicable"
            || (window.applicability == "required"
                && window.measurement == "percent"
                && window.remaining_percent.is_some()
                && window.duration_seconds.is_some())
    });
    let has_exhausted = windows.iter().any(|window| window.exhausted == Some(true));
    let raw_allowed = rate
        .and_then(|rate| rate.get("allowed"))
        .and_then(Value::as_bool);
    let limit_reached = rate
        .and_then(|rate| rate.get("limit_reached"))
        .and_then(Value::as_bool);
    let mut provider_allowed = match (raw_allowed, limit_reached) {
        (Some(true), Some(false)) => Some(true),
        (Some(false), Some(true)) => Some(false),
        (Some(false), None) | (None, Some(true)) => Some(false),
        _ => None, // Absent or conflicting flags do not assert availability.
    };
    if provider_allowed == Some(true) && has_exhausted {
        provider_allowed = None;
    }
    // A measured exhausted base window explains an estimated wait even when
    // the provider's aggregate permission flags lag behind its quota windows.
    // Preserve provider_allowed: these flags must still gate a positive "now".
    let blocking_reason = if base_coverage_complete && has_exhausted {
        "quota_windows"
    } else {
        match provider_allowed {
            Some(true) => "none",
            Some(false)
                if base_coverage_complete && !has_exhausted && limit_reached == Some(false) =>
            {
                "other"
            }
            _ => "unknown",
        }
    }
    .to_owned();
    if let Some(review) = value
        .get("code_review_rate_limit")
        .and_then(Value::as_object)
    {
        for kind in ["primary", "secondary"] {
            let key = format!("{kind}_window");
            if let Some(window) = review.get(&key).filter(|value| !value.is_null()) {
                windows.push(parse_window(
                    Some(window),
                    "feature:code_review",
                    kind,
                    observed_at,
                ));
            }
        }
    }
    let observed = timestamp(observed_at);
    let valid_until = observed_at
        .checked_add_signed(Duration::seconds(300))
        .map(timestamp);
    Ok(AccountQuota {
        account_id: hud_account_id.to_owned(),
        origin: "network".into(),
        freshness: "fresh".into(),
        status: "ok".into(),
        last_success_at: Some(observed.clone()),
        last_attempt_at: Some(observed.clone()),
        observed_at: Some(observed),
        valid_until,
        provider_allowed,
        base_coverage_complete,
        blocking_reason,
        reset_credits_available: value
            .get("rate_limit_reset_credits")
            .and_then(|credits| credits.get("available_count"))
            .and_then(Value::as_u64)
            .filter(|count| *count <= 9_007_199_254_740_991),
        windows,
        error: None,
    })
}

fn parse_window(
    value: Option<&Value>,
    scope: &str,
    kind: &str,
    observed_at: DateTime<Utc>,
) -> QuotaWindow {
    let object = value.and_then(Value::as_object);
    let remaining = object
        .and_then(|window| window.get("used_percent"))
        .and_then(Value::as_f64)
        .filter(|used| used.is_finite() && (0.0..=100.0).contains(used))
        .map(|used| 100.0 - used);
    let duration = object
        .and_then(|window| window.get("limit_window_seconds"))
        .and_then(Value::as_u64)
        .filter(|duration| *duration > 0 && *duration <= 9_007_199_254_740_991);
    let resets_at = object
        .and_then(|window| match window.get("reset_at") {
            Some(value) if !value.is_null() => value
                .as_i64()
                .filter(|seconds| (0..=253_402_300_799).contains(seconds))
                .and_then(|seconds| DateTime::from_timestamp(seconds, 0)),
            _ => window
                .get("reset_after_seconds")
                .and_then(Value::as_i64)
                .filter(|seconds| *seconds >= 0 && *seconds <= 253_402_300_799)
                .and_then(|seconds| observed_at.checked_add_signed(Duration::seconds(seconds))),
        })
        .filter(|date| date.timestamp() <= 253_402_300_799)
        .map(timestamp);
    QuotaWindow {
        id: format!("{}-{kind}", scope.replace(':', "-")),
        scope: scope.into(),
        kind: kind.into(),
        label: match (scope, kind) {
            ("base", "primary") => "主窗口",
            ("base", _) => "次窗口",
            (_, "primary") => "代码审查主窗口",
            _ => "代码审查次窗口",
        }
        .into(),
        duration_seconds: duration,
        applicability: if object.is_some() {
            "required"
        } else {
            "unknown"
        }
        .into(),
        measurement: if remaining.is_some() {
            "percent"
        } else {
            "unknown"
        }
        .into(),
        remaining_percent: remaining,
        exhausted: remaining.map(|percent| percent <= 0.0),
        resets_at,
    }
}

fn status_error(status: StatusCode, headers: &HeaderMap, now: DateTime<Utc>) -> ApiError {
    match status.as_u16() {
        401 => ApiError::new("AUTH_EXPIRED", "登录凭据已过期，请在 Cockpit 中更新账号"),
        403 => ApiError::new("UPSTREAM_FORBIDDEN", "配额服务拒绝了该账号的请求"),
        429 => {
            let mut error = ApiError::new("RATE_LIMITED", "配额服务要求稍后重试");
            error.retryable = true;
            error.retry_at = Some(timestamp(retry_after(headers, now)));
            error
        }
        _ if status.is_server_error() => {
            let mut error = ApiError::new("NETWORK_ERROR", "配额服务暂时不可用");
            error.retryable = true;
            error
        }
        _ => unsupported_response(),
    }
}

fn retry_after(headers: &HeaderMap, now: DateTime<Utc>) -> DateTime<Utc> {
    if let Some(raw) = headers
        .get(RETRY_AFTER)
        .and_then(|header| header.to_str().ok())
    {
        if let Ok(seconds) = raw.trim().parse::<i64>() {
            if seconds >= 0 && seconds <= 253_402_300_799 {
                if let Some(retry) = now.checked_add_signed(Duration::seconds(seconds)) {
                    return retry;
                }
            }
        }
        if let Ok(date) = httpdate::parse_http_date(raw) {
            let retry: DateTime<Utc> = date.into();
            if retry > now {
                return retry;
            }
        }
    }
    now + Duration::seconds(60)
}

fn network_error(error: reqwest::Error) -> ApiError {
    let mut safe_error = if error.is_timeout() {
        ApiError::new("TIMEOUT", "配额查询超时，请稍后重试")
    } else {
        ApiError::new("NETWORK_ERROR", "无法连接配额服务，请检查网络")
    };
    safe_error.retryable = true;
    safe_error
}
fn unsupported_response() -> ApiError {
    ApiError::new("RESPONSE_UNSUPPORTED", "配额响应格式或大小不受支持")
}
fn timestamp(date: DateTime<Utc>) -> String {
    date.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn observed() -> DateTime<Utc> {
        DateTime::from_timestamp(1_791_216_000, 0).unwrap()
    }
    fn normal() -> Value {
        json!({"rate_limit":{"allowed":true,"limit_reached":false,
            "primary_window":{"used_percent":12.5,"limit_window_seconds":18000,"reset_after_seconds":900},
            "secondary_window":{"used_percent":20,"limit_window_seconds":604800,"reset_at":1791226000}}})
    }

    #[test]
    fn reset_credits_preserve_zero_and_unknown_without_guessing() {
        for (raw, expected) in [
            (json!(0), Some(0)),
            (json!(1), Some(1)),
            (json!(-1), None),
            (json!(1.5), None),
            (json!("2"), None),
            (json!(null), None),
            (json!(9_007_199_254_740_992_u64), None),
        ] {
            let parsed = parse_usage(
                &json!({"rate_limit_reset_credits":{"available_count":raw}}),
                "test",
                Utc::now(),
            )
            .unwrap();
            assert_eq!(parsed.reset_credits_available, expected);
        }
        assert_eq!(
            parse_usage(&json!({}), "test", Utc::now())
                .unwrap()
                .reset_credits_available,
            None
        );
    }

    #[test]
    fn request_target_and_credential_headers_are_fixed_and_sensitive() {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let credentials = Credentials {
            provider: "codex_usage".into(),
            project_id: None,
            gcp: false,
            access_token: "synthetic-access-token".into(),
            account_id: Some("synthetic-workspace".into()),
        };
        let request = quota_request(&client, &credentials)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.url().as_str(), USAGE_ENDPOINT);
        assert_eq!(request.method(), reqwest::Method::GET);
        assert_eq!(
            request.headers().get(AUTHORIZATION).unwrap(),
            "Bearer synthetic-access-token"
        );
        assert_eq!(
            request.headers().get("ChatGPT-Account-Id").unwrap(),
            "synthetic-workspace"
        );
        assert!(request.headers().get(AUTHORIZATION).unwrap().is_sensitive());
        assert!(request
            .headers()
            .get("ChatGPT-Account-Id")
            .unwrap()
            .is_sensitive());
        let debug = format!("{request:?}");
        assert!(!debug.contains("synthetic-access-token"));
        assert!(!debug.contains("synthetic-workspace"));
    }

    #[test]
    fn normal_windows_preserve_precision_duration_and_times() {
        let quota = parse_usage(&normal(), "opaque-hud-account", observed()).unwrap();
        assert!(quota.base_coverage_complete);
        assert_eq!(quota.provider_allowed, Some(true));
        assert_eq!(quota.windows[0].remaining_percent, Some(87.5));
        assert_eq!(quota.windows[0].duration_seconds, Some(18000));
        assert_eq!(
            quota.windows[0].resets_at,
            Some(timestamp(observed() + Duration::seconds(900)))
        );
        assert_eq!(
            quota.windows[1].resets_at,
            Some(timestamp(
                DateTime::from_timestamp(1_791_226_000, 0).unwrap()
            ))
        );
    }

    #[test]
    fn missing_windows_and_measurements_are_unknown() {
        for value in [
            json!({}),
            json!({"rate_limit":{"allowed":true,"limit_reached":false,"primary_window":{"limit_window_seconds":18000}}}),
        ] {
            let quota = parse_usage(&value, "synthetic", observed()).unwrap();
            assert!(!quota.base_coverage_complete);
            assert_eq!(quota.windows[0].measurement, "unknown");
            assert_eq!(quota.windows[0].remaining_percent, None);
            assert_eq!(quota.windows[1].applicability, "unknown");
        }
    }

    #[test]
    fn weekly_only_accounts_omit_null_or_absent_five_hour_limits() {
        for (known, absent) in [
            ("primary_window", "secondary_window"),
            ("secondary_window", "primary_window"),
        ] {
            for null in [true, false] {
                let mut value = json!({"rate_limit":{"allowed":true,"limit_reached":false}});
                value["rate_limit"][known] = json!({"used_percent":23,"limit_window_seconds":604800,"reset_after_seconds":86400});
                if null {
                    value["rate_limit"][absent] = Value::Null;
                }
                let q = parse_usage(&value, "synthetic", observed()).unwrap();
                assert!(q.base_coverage_complete);
                assert_eq!(
                    q.windows
                        .iter()
                        .filter(|w| w.applicability == "required")
                        .count(),
                    1
                );
                assert_eq!(crate::domain::evaluate(&q, observed()).state, "now");
                value["rate_limit"][known]["used_percent"] = json!(100);
                value["rate_limit"]["allowed"] = json!(false);
                value["rate_limit"]["limit_reached"] = json!(true);
                let q = parse_usage(&value, "synthetic", observed()).unwrap();
                assert_eq!(
                    crate::domain::evaluate(&q, observed()).estimated_available_at,
                    Some(
                        (observed() + Duration::seconds(86400))
                            .to_rfc3339_opts(SecondsFormat::Millis, true)
                    )
                );
            }
        }
    }

    #[test]
    fn malformed_other_limit_is_not_mistaken_for_weekly_only() {
        for other in [
            json!({}),
            json!({"used_percent":"unknown","limit_window_seconds":18000}),
        ] {
            let value = json!({"rate_limit":{"allowed":true,"limit_reached":false,
                "primary_window":other,"secondary_window":{"used_percent":23,"limit_window_seconds":604800,"reset_after_seconds":86400}}});
            let q = parse_usage(&value, "synthetic", observed()).unwrap();
            assert!(!q.base_coverage_complete);
            assert_eq!(q.windows[0].applicability, "required");
            assert_eq!(crate::domain::evaluate(&q, observed()).state, "unknown");
        }
    }

    #[test]
    fn invalid_percentage_and_millisecond_resets_are_not_guessed() {
        let mut value = normal();
        value["rate_limit"]["primary_window"]["used_percent"] = json!(101);
        value["rate_limit"]["secondary_window"]["reset_at"] = json!(1_791_226_000_000_i64);
        value["rate_limit"]["secondary_window"]["reset_after_seconds"] = json!(600);
        let quota = parse_usage(&value, "synthetic", observed()).unwrap();
        assert_eq!(quota.windows[0].measurement, "unknown");
        assert_eq!(quota.windows[1].resets_at, None);
        assert!(!quota.base_coverage_complete);
    }

    #[test]
    fn feature_exhaustion_does_not_block_base_quota() {
        let mut value = normal();
        value["code_review_rate_limit"] = json!({"allowed":false,"limit_reached":true,
            "primary_window":{"used_percent":100,"limit_window_seconds":604800,"reset_after_seconds":100}});
        let quota = parse_usage(&value, "synthetic", observed()).unwrap();
        assert_eq!(quota.provider_allowed, Some(true));
        assert_eq!(quota.blocking_reason, "none");
        assert_eq!(quota.windows[2].scope, "feature:code_review");
        assert_eq!(quota.windows[2].exhausted, Some(true));
    }

    #[test]
    fn exhausted_window_with_lagging_flags_still_has_an_estimated_reset() {
        let mut value = normal();
        value["rate_limit"]["primary_window"]["used_percent"] = json!(100);
        value["rate_limit"]["secondary_window"]["used_percent"] = json!(77);
        let q = parse_usage(&value, "synthetic", observed()).unwrap();
        assert_eq!(q.provider_allowed, None);
        assert_eq!(q.blocking_reason, "quota_windows");
        let result = crate::domain::evaluate(&q, observed());
        assert_eq!(result.state, "waiting");
        assert_eq!(
            result
                .estimated_available_at
                .as_deref()
                .map(|v| DateTime::parse_from_rfc3339(v).unwrap()),
            q.windows[0]
                .resets_at
                .as_deref()
                .map(|v| DateTime::parse_from_rfc3339(v).unwrap())
        );
    }

    #[test]
    fn only_explained_base_limits_support_waiting() {
        let mut value = normal();
        value["rate_limit"]["allowed"] = json!(false);
        value["rate_limit"]["limit_reached"] = json!(true);
        value["rate_limit"]["primary_window"]["used_percent"] = json!(100);
        assert_eq!(
            parse_usage(&value, "synthetic", observed())
                .unwrap()
                .blocking_reason,
            "quota_windows"
        );
        value["rate_limit"]["secondary_window"] = Value::Null;
        assert_eq!(
            parse_usage(&value, "synthetic", observed())
                .unwrap()
                .blocking_reason,
            "unknown"
        );
        value["rate_limit"]["allowed"] = json!(true);
        assert_eq!(
            parse_usage(&value, "synthetic", observed())
                .unwrap()
                .provider_allowed,
            None
        );
    }

    #[test]
    fn retry_after_defaults_and_status_errors_are_redacted() {
        let mut headers = HeaderMap::new();
        let error = status_error(StatusCode::TOO_MANY_REQUESTS, &headers, observed());
        assert_eq!(error.code, "RATE_LIMITED");
        assert_eq!(
            error.retry_at,
            Some(timestamp(observed() + Duration::seconds(60)))
        );
        headers.insert(RETRY_AFTER, HeaderValue::from_static("120"));
        assert_eq!(
            status_error(StatusCode::TOO_MANY_REQUESTS, &headers, observed()).retry_at,
            Some(timestamp(observed() + Duration::seconds(120)))
        );
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Thu, 05 Oct 2028 00:00:00 GMT"),
        );
        let retry = retry_after(&headers, observed());
        assert!(retry > observed());
        assert_eq!(
            status_error(StatusCode::UNAUTHORIZED, &headers, observed()).code,
            "AUTH_EXPIRED"
        );
    }

    #[tokio::test]
    async fn mock_http_body_is_bounded_and_errors_do_not_expose_secrets() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        for (status, body, expected) in [
            ("200 OK", normal().to_string(), None),
            (
                "200 OK",
                "x".repeat(MAX_RESPONSE_BYTES + 1),
                Some("RESPONSE_UNSUPPORTED"),
            ),
            (
                "429 Too Many Requests",
                "synthetic-secret-token sample@example.test".into(),
                Some("RATE_LIMITED"),
            ),
        ] {
            let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/usage", server.local_addr().unwrap());
            let task = tokio::spawn(async move {
                let (mut socket, _) = server.accept().await.unwrap();
                let mut request = vec![0_u8; 4096];
                socket.read(&mut request).await.unwrap();
                let reply = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nRetry-After: 90\r\nConnection: close\r\n\r\n{body}",body.len());
                // The consumer can intentionally close early for a bounded/error response.
                let _ = socket.write_all(reply.as_bytes()).await;
            });
            let response = Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap()
                .get(endpoint)
                .send()
                .await
                .unwrap();
            let result = read_response(response, "synthetic").await;
            if let Some(code) = expected {
                let error = result.err().unwrap();
                assert_eq!(error.code, code);
                assert!(!error.message.contains("synthetic-secret-token"));
                assert!(!error.message.contains("sample@example.test"));
            } else {
                assert_eq!(result.unwrap().windows[0].remaining_percent, Some(87.5));
            }
            task.await.unwrap();
        }
    }
}
