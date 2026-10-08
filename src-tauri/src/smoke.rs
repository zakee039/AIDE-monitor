//! Opt-in native WebView/IPC check, using only an isolated empty source.
//!
//! Normal launches never call this module. Keep the `SmokeState` managed by the
//! application until it exits so its temporary source and settings remain alive.

use crate::{config, model::ApiError};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{webview::PageLoadEvent, AppHandle, Manager, Webview, Wry};

pub struct SmokeState {
    _temporary: tempfile::TempDir,
    pub data_dir: PathBuf,
    pub report_path: PathBuf,
    results: Arc<Mutex<BTreeMap<String, Value>>>,
    polling_started: AtomicBool,
}

pub fn is_enabled() -> bool {
    std::env::args_os()
        .any(|argument| argument == "--smoke-test" || argument == "--smoke-interactive")
}

pub fn is_interactive() -> bool {
    std::env::args_os().any(|argument| argument == "--smoke-interactive")
}

fn io_error(error: ApiError) -> io::Error {
    io::Error::other(error.message)
}

pub fn prepare_data() -> Result<SmokeState, io::Error> {
    let temporary = tempfile::Builder::new()
        .prefix("aide-monitor-smoke-")
        .tempdir()?;
    let source = temporary.path().join("source");
    let data_dir = temporary.path().join("hud");
    std::fs::create_dir_all(&source)?;
    std::fs::create_dir_all(&data_dir)?;
    config::atomic_json(
        &source.join("codex_accounts.json"),
        &json!({"version":"1.0","detail_schema_version":2,"accounts":[]}),
    )
    .map_err(io_error)?;
    let mut configuration = config::Config::default();
    configuration.source_path = Some(source);
    configuration.settings.auto_refresh = false;
    config::atomic_json(&data_dir.join("settings.json"), &configuration).map_err(io_error)?;
    let report_path = std::env::current_dir()?.join("artifacts/native-smoke.json");
    Ok(SmokeState {
        _temporary: temporary,
        data_dir,
        report_path,
        results: Arc::new(Mutex::new(BTreeMap::new())),
        polling_started: AtomicBool::new(false),
    })
}

pub fn on_page_load(webview: &Webview<Wry>, payload: &tauri::webview::PageLoadPayload<'_>) {
    if !is_enabled()
        || is_interactive()
        || crate::theme_smoke::enabled()
        || payload.event() != PageLoadEvent::Finished
        || !matches!(webview.label(), "hud" | "settings")
    {
        return;
    }
    let label = serde_json::to_string(webview.label()).expect("window label JSON");
    let script = SCRIPT.replace("__SMOKE_WINDOW_LABEL__", &label);
    if webview.eval(script).is_err() {
        record(
            webview.app_handle(),
            webview.label(),
            json!({"ok":false,"error":"EVALUATION_DISPATCH_FAILED"}),
        );
    }
}

fn record(app: &AppHandle, label: &str, value: Value) {
    if let Some(state) = app.try_state::<SmokeState>() {
        if let Ok(mut results) = state.results.lock() {
            results.entry(label.to_owned()).or_insert(value);
        }
    }
}

/// Call once after managing `SmokeState` and creating both hidden windows.
pub fn start_poll(app: &AppHandle) {
    if !is_enabled() {
        return;
    }
    let Some(state) = app.try_state::<SmokeState>() else {
        app.exit(1);
        return;
    };
    if state.polling_started.swap(true, Ordering::SeqCst) {
        return;
    }
    let results = state.results.clone();
    let report_path = state.report_path.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let mut reopened = false;
        let mut pin_round_trip = false;
        loop {
            for label in ["hud", "settings"] {
                let already_finished = results
                    .lock()
                    .map(|values| values.contains_key(label))
                    .unwrap_or(true);
                if already_finished {
                    continue;
                }
                if let Some(window) = app.get_webview_window(label) {
                    let callback_app = app.clone();
                    if window
                        .eval_with_callback("window.__HUD_SMOKE_RESULT || null", move |value| {
                            if let Ok(result) = serde_json::from_str::<Value>(&value) {
                                if result.is_object() && result.get("ok").is_some() {
                                    record(&callback_app, label, result);
                                }
                            }
                        })
                        .is_err()
                    {
                        record(
                            &app,
                            label,
                            json!({"ok":false,"error":"POLL_DISPATCH_FAILED"}),
                        );
                    }
                }
            }
            let timed_out = started.elapsed() >= Duration::from_secs(20);
            let current = results.lock().map(|v| v.clone()).unwrap_or_default();
            if current.len() == 2
                && !reopened
                && !timed_out
                && current.values().all(|value| value["ok"] == true)
            {
                // Same handler as the tray checkbox; verify all three copies agree.
                let check_pin = |expected: bool| {
                    app.state::<crate::service::Service>()
                        .settings()
                        .display
                        .always_on_top
                        == expected
                        && app
                            .state::<tauri::menu::CheckMenuItem<tauri::Wry>>()
                            .is_checked()
                            .ok()
                            == Some(expected)
                        && app
                            .get_webview_window("hud")
                            .and_then(|w| w.is_always_on_top().ok())
                            == Some(expected)
                };
                let before = app
                    .state::<crate::service::Service>()
                    .settings()
                    .display
                    .always_on_top;
                crate::toggle_pin(&app);
                let first = check_pin(!before);
                crate::toggle_pin(&app);
                pin_round_trip = first && check_pin(before);
                reopened = true;
                results.lock().unwrap().remove("settings");
                if let Some(window) = app.get_webview_window("settings") {
                    let _ = window.destroy();
                }
                for _ in 0..20 {
                    if app.get_webview_window("settings").is_none() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                // Reopen through exactly the worker used by the tray event.
                if !matches!(crate::open_settings_from_tray(&app).await, Ok(Ok(()))) {
                    record(
                        &app,
                        "settings",
                        json!({"ok":false,"error":"TRAY_REOPEN_FAILED"}),
                    );
                }
                continue;
            }
            if current.len() == 2 || timed_out {
                let ok = !timed_out
                    && reopened
                    && pin_round_trip
                    && app.get_webview_window("hud").is_some_and(|w| {
                        w.is_resizable().ok() == Some(false)
                            && w.is_maximizable().ok() == Some(false)
                            && w.is_maximized().ok() == Some(false)
                    })
                    && ["hud", "settings"].iter().all(|label| {
                        current.get(*label).and_then(|value| value.get("ok"))
                            == Some(&Value::Bool(true))
                    });
                let report = json!({
                    "schemaVersion":1,
                    "appVersion":env!("CARGO_PKG_VERSION"),
                    "generatedAt":chrono::Utc::now().to_rfc3339(),
                    "ok":ok,
                    "isolatedSource":true,
                    "networkQueries":0,
                    "timedOut":timed_out,
                    "runtimeSettingsReopened":reopened,
                    "trayPinRoundTrip":pin_round_trip,
                    "hudWindowLocked": app.get_webview_window("hud").is_some_and(|w|
                        w.is_resizable().ok() == Some(false)
                        && w.is_maximizable().ok() == Some(false)
                        && w.is_maximized().ok() == Some(false)),
                    "elapsedMilliseconds":started.elapsed().as_millis(),
                    "windows":current,
                });
                let saved = config::atomic_json(&report_path, &report).is_ok();
                app.exit(if ok && saved { 0 } else { 1 });
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}

const SCRIPT: &str = r#"
(() => {
  if (window.__HUD_SMOKE_STARTED) return;
  window.__HUD_SMOKE_STARTED = true;
  const label = __SMOKE_WINDOW_LABEL__;
  const checks = [];
  let currentCheck = 'page-render';
  const violations = [];
  window.addEventListener('securitypolicyviolation', () => violations.push(true));
  const pause = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
  function assert(condition, code) {
    if (!condition) throw new Error(code);
  }
  async function call(command, request = {}) {
    currentCheck = command;
    let result;
    try { result = await window.__TAURI_INTERNALS__.invoke(command, {request}); }
    catch { throw new Error('IPC_REJECTED'); }
    assert(result && result.ok === true && result.apiVersion === '1.0', 'API_ENVELOPE_FAILED');
    assert(typeof result.instanceId === 'string' && Number.isSafeInteger(result.revision), 'API_METADATA_FAILED');
    assert(Number.isFinite(Date.parse(result.generatedAt)), 'API_TIMESTAMP_FAILED');
    checks.push(command);
    return result.data;
  }
  (async () => {
    const until = Date.now() + 15000;
    while (!document.querySelector('main.app-stage.desktop') && Date.now() < until) await pause(100);
    const main = document.querySelector('main.app-stage.desktop');
    assert(main && main.getBoundingClientRect().width > 0, 'NATIVE_MAIN_NOT_RENDERED');
    assert(main.classList.contains(label === 'settings' ? 'settings-stage' : 'hud-stage'), 'WINDOW_ROUTE_MISMATCH');
    assert(!document.querySelector('.demo-banner, .browser-caption'), 'BROWSER_DEMO_RENDERED');
    assert(!/浏览器预览|浏览器演示|全部账号与配额均为虚构/.test(main.textContent), 'BROWSER_DEMO_TEXT_RENDERED');
    assert(document.querySelector('script[type="module"]'), 'MODULE_ASSET_MISSING');
    assert(document.styleSheets.length > 0, 'STYLE_ASSET_MISSING');
    checks.push('native-page-and-assets');

    const capabilities = await call('hud_v1_capabilities_get');
    assert(capabilities.transport === 'tauri', 'TRANSPORT_MISMATCH');
    assert(capabilities.enabledMethods.length === (label === 'settings' ? 13 : 9), 'METHOD_COUNT_MISMATCH');
    const commonScopes = ['quota.read', 'quota.refresh', 'events.read', 'settings.read', 'themes.read', 'window.control'];
    const requiredScopes = label === 'settings' ? commonScopes.concat(['settings.write', 'themes.write', 'diagnostics.read']) : commonScopes;
    assert(requiredScopes.every(scope => capabilities.grantedScopes.includes(scope)), 'SCOPE_MISMATCH');
    if (label === 'hud') assert(!capabilities.grantedScopes.includes('settings.write'), 'HUD_WRITE_SCOPE_GRANTED');
    const settings = await call('hud_v1_settings_get');
    assert(settings.autoRefresh === false, 'ISOLATED_AUTO_REFRESH_NOT_DISABLED');
    const accounts = await call('hud_v1_accounts_list');
    assert(Array.isArray(accounts) && accounts.length === 0, 'ISOLATED_ACCOUNTS_NOT_EMPTY');
    const snapshot = await call('aide_theme_data');
    assert(snapshot.accounts.length === 0 && snapshot.quotas.length === 0, 'ISOLATED_SNAPSHOT_NOT_EMPTY');
    assert(snapshot.source.state === 'ready', 'ISOLATED_SOURCE_NOT_READY');
    assert(snapshot.totalQuota.percent === null && snapshot.totalQuota.partial === false && snapshot.totalQuota.weeklyScalePercent === 15, 'TOTAL_QUOTA_CONTRACT_MISMATCH');
    const themes = await call('aide_theme_builtins');
    assert(Array.isArray(themes) && themes.some(theme => theme.id === 'paper'), 'BUILT_IN_THEMES_MISSING');
    const theme = await call('aide_theme_builtin');
    assert(theme && typeof theme.id === 'string', 'ACTIVE_THEME_MISSING');
    if (theme.id === 'default') assert(theme.tokens.colors.background === '#FAF6EC' && theme.tokens.colors.accent === '#39C5BB', 'CREAM_THEME_MISMATCH');

    if (label === 'settings') {
      const sources = await call('hud_internal_sources_get');
      assert(sources.length === 5 && sources.some(s => s.id === 'cockpit' && s.enabled), 'SOURCE_OPTIONS_MISSING');
      const startup = await call('hud_internal_startup');
      assert(typeof startup.enabled === 'boolean', 'STARTUP_STATUS_MISSING');
      const diagnostics = await call('hud_v1_diagnostics_get');
      assert(diagnostics.selectedAccountCount === 0 && diagnostics.activeJobCount === 0, 'DIAGNOSTICS_NOT_ISOLATED');
      const beforeUpdate = await call('hud_v1_settings_get');
      const updated = await call('hud_v1_settings_update', {
        expectedRevision:beforeUpdate.settingsRevision,
        proxies:[{id:"smoke-proxy",name:"Smoke",address:"socks5://127.0.0.1:7893"}],
        accountProxies:{},
        refreshIntervalSeconds:61,
        autoRefresh:false,
        display:{privacyMode:true,alwaysOnTop:false,positionLocked:true},
        usbDisplay:{enabled:false,deviceId:"smoke-disconnected-monitor",themeId:"usb-day"}
      });
      assert(updated.refreshIntervalSeconds === 61 && updated.display.privacyMode === true && updated.autoRefresh === false, 'SETTINGS_UPDATE_NOT_APPLIED');
      assert(updated.proxies.length === 1 && updated.proxies[0].address === 'socks5://127.0.0.1:7893', 'PROXY_PREFERENCES_NOT_APPLIED');
      checks.push('proxy-preferences-native-persistence');
      assert(updated.display.positionLocked && updated.usbDisplay.deviceId === 'smoke-disconnected-monitor', 'DISPLAY_PREFERENCES_NOT_APPLIED');
      const monitors = await window.__TAURI_INTERNALS__.invoke('aide_usb_displays');
      assert(Array.isArray(monitors) && monitors.every(m => typeof m.id === 'string' && m.width > 0 && m.height > 0), 'MONITOR_ENUMERATION_FAILED');
      checks.push('monitor-enumeration-and-display-preferences');
      const selected = await window.__TAURI_INTERNALS__.invoke('aide_theme_select', {id:'paper'});
      assert(selected.ok, 'THEME_SELECT_FAILED');
      const activeTheme = await call('aide_theme_builtin');
      assert(activeTheme.id === 'paper', 'THEME_SELECT_NOT_APPLIED');
    } else {
      currentCheck = 'hud-write-ACL-rejection';
      let denied = false;
      try {
        const result = await window.__TAURI_INTERNALS__.invoke('hud_v1_settings_update', {request:{
          expectedRevision:settings.settingsRevision, refreshIntervalSeconds:62
        }});
        denied = result?.ok === false && result.error?.code === 'FORBIDDEN';
      } catch { denied = true; }
      assert(denied, 'HUD_WRITE_NOT_REJECTED');
      checks.push('hud-write-ACL-rejection');
      currentCheck = 'native-collapse-expand';
      while (!document.querySelector('.hud-collapse') && Date.now() < until) await pause(50);
      const collapse = document.querySelector('.hud-collapse');
      assert(collapse, 'COLLAPSE_CONTROL_MISSING');
      collapse.click();
      const foldedDeadline = Date.now() + 3000;
      // Physical window sizes round to whole pixels at fractional system DPI.
      const orbViewportMatches = () => Math.abs(innerWidth - 43) <= 1 && Math.abs(innerHeight - 43) <= 1;
      while ((!document.querySelector('.quota-orb') || !orbViewportMatches()) && Date.now() < foldedDeadline) await pause(50);
      const orb = document.querySelector('.quota-orb');
      assert(orb && orbViewportMatches(), 'NATIVE_ORB_SIZE_MISMATCH');
      const foldedBounds = document.querySelector('.hud-shell').getBoundingClientRect();
      assert(Math.abs(foldedBounds.width - 43) < .1 && Math.abs(foldedBounds.height - 43) < .1, 'ORB_CONTENT_SIZE_MISMATCH');
      window.__HUD_SMOKE_ORB_VIEWPORT = {width:innerWidth, height:innerHeight, contentWidth:foldedBounds.width, contentHeight:foldedBounds.height, scale:devicePixelRatio};
      assert(orb.textContent === '—', 'UNKNOWN_ORB_SHOWN_AS_ZERO');
      assert(getComputedStyle(orb).fontSize === '16px', 'ORB_FONT_SIZE_MISMATCH');
      assert(getComputedStyle(document.documentElement).backgroundColor === 'rgba(0, 0, 0, 0)', 'ORB_BACKGROUND_NOT_TRANSPARENT');
      orb.click();
      const expandedDeadline = Date.now() + 3000;
      const expandedViewportMatches = () => !document.querySelector('.quota-orb') && innerWidth > 44 && Math.abs(document.querySelector('.hud-shell').getBoundingClientRect().width - innerWidth) < 2;
      while (!expandedViewportMatches() && Date.now() < expandedDeadline) await pause(50);
      assert(expandedViewportMatches() && document.querySelector('.hud-collapse'), 'NATIVE_EXPAND_FAILED');
      assert(Math.abs(document.querySelector('.hud-shell').getBoundingClientRect().width - innerWidth) < 2, 'NATIVE_CONTENT_SIZE_MISMATCH');
      checks.push('native-collapse-expand');
      await call('hud_v1_window_control', {action:'open_settings'});
      checks.push('runtime-settings-open');
    }
    await pause(150);
    currentCheck = 'asset-and-CSP-check';
    assert(violations.length === 0, 'CSP_VIOLATION');
    const resources = performance.getEntriesByType('resource');
    assert(!resources.some(entry => ['script','link','css'].includes(entry.initiatorType) && entry.responseStatus >= 400), 'ASSET_HTTP_FAILURE');
    checks.push('asset-and-CSP-check');
    window.__HUD_SMOKE_RESULT = {ok:true, label, checks, methodCount:capabilities.enabledMethods.length,
      scopeCount:capabilities.grantedScopes.length, nativeMain:true, demoVisible:false, cspViolations:violations.length, orbViewport:window.__HUD_SMOKE_ORB_VIEWPORT, viewport:{width:innerWidth,height:innerHeight}};
  })().catch(error => {
    window.__HUD_SMOKE_RESULT = {ok:false, label, checks, failedCheck:currentCheck,
      error:/^[A-Z_]+$/.test(error.message || '') ? error.message : 'JAVASCRIPT_CHECK_FAILED', orbViewport:window.__HUD_SMOKE_ORB_VIEWPORT, viewport:{width:innerWidth,height:innerHeight,scale:devicePixelRatio}, panel:document.querySelector('.hud-shell')?.getBoundingClientRect().toJSON()};
  });
})();
"#;
