use crate::{
    model::{ApiError, RefreshRequest},
    service::Service,
    theme_package::{self as packages, Installed},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Clone)]
struct Session {
    token: String,
    _profile: std::sync::Arc<tempfile::TempDir>,
    label: String,
    record: Installed,
    created: Instant,
    last_seen: Instant,
    ready: bool,
    hidden: bool,
    calls: HashMap<String, (Instant, u32)>,
}
#[derive(Default)]
pub struct Runtime {
    pub folded: AtomicBool,
    sessions: Mutex<HashMap<String, Session>>,
    pub notice: Mutex<Option<String>>,
    pub transition: Mutex<()>,
}
pub fn root(service: &Service) -> std::path::PathBuf {
    service.theme_dir.join("runtime")
}
fn error(message: &str) -> ApiError {
    ApiError::new("THEME_INVALID", message)
}
fn api_error(_: tauri::Error) -> ApiError {
    error("主题窗口操作失败")
}
pub fn is_session(app: &tauri::AppHandle, label: &str) -> bool {
    app.state::<Runtime>()
        .sessions
        .lock()
        .unwrap()
        .values()
        .any(|s| s.label == label)
}
pub fn visible(app: &tauri::AppHandle) -> Option<WebviewWindow> {
    let runtime = app.state::<Runtime>();
    let sessions = runtime.sessions.lock().unwrap();
    sessions
        .values()
        .find(|s| s.ready)
        .and_then(|s| app.get_webview_window(&s.label))
}
pub fn set_hidden(app: &tauri::AppHandle, hidden: bool) {
    for s in app.state::<Runtime>().sessions.lock().unwrap().values_mut() {
        if s.ready {
            s.hidden = hidden;
            s.last_seen = Instant::now();
        }
    }
}
pub fn target(app: &tauri::AppHandle) -> Option<WebviewWindow> {
    if app.state::<Runtime>().folded.load(Ordering::SeqCst) {
        app.get_webview_window("hud")
    } else {
        visible(app)
    }
}
pub fn toggle(app: &tauri::AppHandle) -> bool {
    let Some(w) = visible(app) else {
        return false;
    };
    let runtime = app.state::<Runtime>();
    let folded = !runtime.folded.fetch_xor(true, Ordering::SeqCst);
    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.emit("aide://set-collapse", folded);
        if folded {
            let _ = w.hide();
            set_hidden(app, true);
            // Prepare the hidden orb at the theme's right edge. The trusted HUD
            // reveals it only after its React content and native layout agree.
            let _ = hud.hide();
            if let (Ok(position), Ok(size), Ok(scale)) =
                (w.outer_position(), w.inner_size(), hud.scale_factor())
            {
                if let Ok(mut layout) = app.state::<crate::HudLayoutState>().0.lock() {
                    layout.collapsed = true;
                    layout.expanded_width = size.width as f64 / scale;
                }
                let _ = hud.set_size(tauri::LogicalSize::new(
                    crate::HUD_ORB_SIZE,
                    crate::HUD_ORB_SIZE,
                ));
                let _ = hud.set_position(tauri::PhysicalPosition::new(
                    position.x + size.width as i32 - (crate::HUD_ORB_SIZE * scale).round() as i32,
                    position.y,
                ));
            }
        } else {
            let _ = hud.hide();
            set_hidden(app, false);
            if let (Ok(position), Ok(orb), Ok(expanded)) =
                (hud.outer_position(), hud.inner_size(), w.inner_size())
            {
                let _ = w.set_position(tauri::PhysicalPosition::new(
                    position.x + orb.width as i32 - expanded.width as i32,
                    position.y,
                ));
            }
            let _ = w.show();
        }
    }
    true
}
pub fn retire_id(app: &tauri::AppHandle, id: &str) {
    let removed: Vec<_> = {
        let state = app.state::<Runtime>();
        let mut sessions = state.sessions.lock().unwrap();
        let tokens: Vec<_> = sessions
            .iter()
            .filter(|(_, s)| s.record.manifest.id == id)
            .map(|(k, _)| k.clone())
            .collect();
        tokens
            .into_iter()
            .filter_map(|k| sessions.remove(&k))
            .collect()
    };
    for s in removed {
        if let Some(w) = app.get_webview_window(&s.label) {
            let _ = w.destroy();
        }
    }
}
pub fn resume(app: &tauri::AppHandle) {
    if app.state::<Runtime>().folded.load(Ordering::SeqCst) {
        toggle(app);
    }
}
pub fn revoke(app: &tauri::AppHandle) {
    app.state::<Runtime>().folded.store(false, Ordering::SeqCst);
    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.emit("aide://set-collapse", false);
    }
    let removed: Vec<_> = app
        .state::<Runtime>()
        .sessions
        .lock()
        .unwrap()
        .drain()
        .map(|(_, s)| s)
        .collect();
    for session in removed {
        if let Some(w) = app.get_webview_window(&session.label) {
            let _ = w.destroy();
        }
    }
}
pub fn uninstall(app: &tauri::AppHandle, id: &str, clear: bool) -> Result<(), ApiError> {
    let runtime = app.state::<Runtime>();
    let _guard = runtime.transition.lock().unwrap();
    let service = app.state::<Service>();
    if service.settings().active_theme_id == id {
        restore_inner(app, None);
    }
    retire_id(app, id);
    packages::uninstall(&root(&service), id, clear)
}
pub fn restore(app: &tauri::AppHandle, message: Option<&str>) {
    let runtime = app.state::<Runtime>();
    let _guard = runtime.transition.lock().unwrap();
    restore_inner(app, message);
}
fn restore_inner(app: &tauri::AppHandle, message: Option<&str>) {
    revoke(app);
    let service = app.state::<Service>();
    let _ = service.select_theme("default", service.settings().settings_revision);
    *app.state::<Runtime>().notice.lock().unwrap() = message.map(str::to_string);
    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.show();
    }
    let _ = app.emit("aide://themes-changed", ());
}
pub fn select(app: &tauri::AppHandle, id: &str) -> Result<Value, ApiError> {
    let _transition = app.state::<Runtime>();
    let _guard = _transition.transition.lock().unwrap();
    let service = app.state::<Service>();
    if packages::BUILTINS.contains(&id) {
        revoke(app);
        let settings = service.select_theme(id, service.settings().settings_revision)?;
        if let Some(hud) = app.get_webview_window("hud") {
            hud.show().map_err(api_error)?;
        }
        return Ok(json!(settings));
    }
    let record = packages::installed(&root(&service), id)?;
    // Only one pending load; keep the current ready theme alive until commit.
    let stale: Vec<_> = {
        let mut sessions = _transition.sessions.lock().unwrap();
        let keys: Vec<_> = sessions
            .iter()
            .filter(|(_, s)| !s.ready)
            .map(|(k, _)| k.clone())
            .collect();
        keys.into_iter()
            .filter_map(|k| sessions.remove(&k))
            .collect()
    };
    for s in stale {
        if let Some(w) = app.get_webview_window(&s.label) {
            let _ = w.destroy();
        }
    }
    let token = uuid::Uuid::new_v4().to_string();
    let label = format!("aide-theme-{token}");
    let prefix = format!("http://aide.localhost/{token}/");
    let entry = format!("{prefix}index.html");
    let sdk = include_str!("../../theme-sdk/bootstrap.js")
        .replace("__AIDE_SESSION__", &token)
        .replace("__AIDE_ENDPOINT__", &format!("{prefix}rpc"));
    let profile = std::sync::Arc::new(
        tempfile::Builder::new()
            .prefix("aide-theme-profile-")
            .tempdir()
            .map_err(|_| error("无法创建隔离浏览器目录"))?,
    );
    let now = Instant::now();
    _transition.sessions.lock().unwrap().insert(
        token.clone(),
        Session {
            token: token.clone(),
            _profile: profile.clone(),
            label: label.clone(),
            record,
            created: now,
            last_seen: now,
            ready: false,
            hidden: false,
            calls: HashMap::new(),
        },
    );
    let allowed = entry.clone();
    let navigation_started = AtomicBool::new(false);
    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .data_directory(profile.path().to_path_buf())
    .incognito(true)
    .title("AIDE monitor · Theme")
    .visible(false)
    .focused(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .skip_taskbar(true)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .always_on_top(service.settings().display.always_on_top)
    .inner_size(380., 128.)
    .devtools(false)
    .initialization_script(sdk)
    .on_navigation(move |url| {
        if url.as_str() == "about:blank" {
            return !navigation_started.load(Ordering::SeqCst);
        }
        url.as_str() == allowed && !navigation_started.swap(true, Ordering::SeqCst)
    })
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .on_download(|_, _| false)
    .build()
    .map_err(|e| {
        _transition.sessions.lock().unwrap().remove(&token);
        api_error(e)
    })?;
    // Install native request filtering before loading any third-party resource.
    if let Err(e) = harden(&window, &prefix) {
        _transition.sessions.lock().unwrap().remove(&token);
        let _ = window.destroy();
        return Err(e);
    }
    if let Some(hud) = target(app).or_else(|| app.get_webview_window("hud")) {
        if let Ok(position) = hud.outer_position() {
            let _ = window.set_position(position);
        }
    }
    if let Err(e) = window.navigate(entry.parse().unwrap()) {
        _transition.sessions.lock().unwrap().remove(&token);
        let _ = window.destroy();
        return Err(api_error(e));
    }
    let app = app.clone();
    let token2 = token.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let expired = {
                let runtime = app.state::<Runtime>();
                let sessions = runtime.sessions.lock().unwrap();
                match sessions.get(&token2) {
                    None => return,
                    Some(s) => {
                        (!s.ready && s.created.elapsed() > Duration::from_secs(8))
                            || (s.ready
                                && !s.hidden
                                && s.last_seen.elapsed() > Duration::from_secs(15))
                    }
                }
            };
            if expired {
                let app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    fail_session(&app, &token2, "主题加载超时或停止响应，已恢复可用界面")
                });
                return;
            }
        }
    });
    Ok(json!({"loading":true}))
}
fn fail_session(app: &tauri::AppHandle, token: &str, message: &str) {
    let runtime = app.state::<Runtime>();
    let _guard = runtime.transition.lock().unwrap();
    let removed = app
        .state::<Runtime>()
        .sessions
        .lock()
        .unwrap()
        .remove(token);
    if let Some(s) = removed {
        if let Some(w) = app.get_webview_window(&s.label) {
            let _ = w.destroy();
        }
        *app.state::<Runtime>().notice.lock().unwrap() = Some(message.into());
        if visible(app).is_none() {
            let service = app.state::<Service>();
            if packages::BUILTINS.contains(&service.settings().active_theme_id.as_str()) {
                if let Some(hud) = app.get_webview_window("hud") {
                    let _ = hud.show();
                }
            } else {
                restore_inner(app, Some(message));
            }
        }
        let _ = app.emit("aide://themes-changed", ());
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    api_version: String,
    session_id: String,
    request_id: String,
    method: String,
    params: Value,
}
fn fields(value: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    let map = value.as_object().ok_or_else(|| error("参数须为对象"))?;
    if map.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(error("未知参数"));
    }
    Ok(())
}
pub fn response(
    app: &tauri::AppHandle,
    label: &str,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let path = request.uri().path();
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    let token = parts.next().unwrap_or("");
    let file = parts.next().unwrap_or("");
    let session = app
        .state::<Runtime>()
        .sessions
        .lock()
        .unwrap()
        .get(token)
        .filter(|s| s.label == label)
        .cloned();
    let denied = || {
        tauri::http::Response::builder()
            .status(403)
            .body(Vec::new())
            .unwrap()
    };
    let Some(session) = session else {
        return denied();
    };
    if file == "rpc" && request.method() == "POST" {
        let mut correlation = String::new();
        let mut snapshot = false;
        let result = if request.body().len() > 96 * 1024 {
            Err(error("请求过大"))
        } else {
            serde_json::from_slice::<Request>(request.body())
                .map_err(|_| error("请求格式错误"))
                .and_then(|r| {
                    if r.api_version != "1"
                        || r.session_id != token
                        || r.request_id.is_empty()
                        || r.request_id.len() > 100
                    {
                        return Err(error("会话或协议无效"));
                    }
                    correlation = r.request_id;
                    snapshot = r.method == "usage.get";
                    dispatch(app, &session, &r.method, r.params)
                })
        };
        let service = app.state::<Service>();
        let envelope = if snapshot && result.is_ok() {
            service.read_reply("snapshot", false)
        } else {
            service.envelope(result)
        };
        let mut value = serde_json::to_value(envelope).unwrap();
        value["apiVersion"] = json!("1");
        value["requestId"] = json!(correlation);
        value["sessionId"] = json!(token);
        value["sequence"] = value["revision"].clone();
        let body = serde_json::to_vec(&value).unwrap();
        return tauri::http::Response::builder()
            .header("Content-Type", "application/json")
            .header("Cache-Control", "no-store")
            .header("Access-Control-Allow-Origin", "http://aide.localhost")
            .body(body)
            .unwrap();
    }
    if request.method() != "GET" || !packages::safe_path(file) || file == "manifest.json" {
        return denied();
    }
    let service = app.state::<Service>();
    let base = packages::content(&root(&service), &session.record);
    let resource = base.join(file);
    let valid = resource
        .canonicalize()
        .ok()
        .zip(base.canonicalize().ok())
        .is_some_and(|(p, b)| p.starts_with(b));
    if !valid {
        return denied();
    }
    let bytes = match std::fs::read(resource) {
        Ok(b) if b.len() <= 20 * 1024 * 1024 => b,
        _ => return denied(),
    };
    let mime = match file.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "json" => "application/json",
        _ => "application/octet-stream",
    };
    let origin = format!("http://aide.localhost/{token}/");
    let csp=format!("default-src 'none'; script-src {origin}; style-src {origin} 'unsafe-inline'; img-src {origin} data:; font-src {origin}; connect-src {origin}rpc; object-src 'none'; frame-src 'none'; worker-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'");
    tauri::http::Response::builder().header("Content-Type",mime).header("Content-Security-Policy",csp).header("X-Content-Type-Options","nosniff").header("Cache-Control","no-store").header("Permissions-Policy","camera=(), microphone=(), geolocation=(), usb=(), serial=(), bluetooth=(), display-capture=()").body(bytes).unwrap()
}
fn dispatch(
    app: &tauri::AppHandle,
    session: &Session,
    method: &str,
    params: Value,
) -> Result<Value, ApiError> {
    if ![
        "usage.get",
        "accounts.list",
        "accounts.refresh",
        "window.resize",
        "window.drag",
        "window.hide",
        "window.menu",
        "app.openSettings",
        "theme.failed",
        "theme.ready",
        "storage.get",
        "storage.set",
        "storage.remove",
    ]
    .contains(&method)
    {
        return Err(ApiError::new("FORBIDDEN", "未公开的主题方法"));
    }
    let runtime = app.state::<Runtime>();
    {
        let mut sessions = runtime.sessions.lock().unwrap();
        let current = sessions
            .get_mut(&session.token)
            .ok_or_else(|| error("会话已关闭"))?;
        let limit = if method == "window.resize" {
            20
        } else if method.starts_with("storage.") {
            30
        } else {
            10
        };
        let rate = current
            .calls
            .entry(method.into())
            .or_insert((Instant::now(), 0));
        if rate.0.elapsed() > Duration::from_secs(1) {
            *rate = (Instant::now(), 0);
        }
        rate.1 += 1;
        if rate.1 > limit {
            return Err(ApiError::new("RATE_LIMITED", "主题调用过于频繁"));
        }
        current.last_seen = Instant::now();
    }
    let service = app.state::<Service>();
    let window = app
        .get_webview_window(&session.label)
        .ok_or_else(|| error("主题窗口不存在"))?;
    match method {
        "usage.get" => {
            fields(&params, &[])?;
            Ok(json!(service.snapshot()))
        }
        "accounts.list" => {
            fields(&params, &[])?;
            Ok(json!(service.snapshot().accounts))
        }
        "accounts.refresh" => {
            fields(&params, &["accountIds"])?;
            let r: RefreshRequest =
                serde_json::from_value(params).map_err(|_| error("刷新参数无效"))?;
            let selected = service.snapshot().accounts;
            if r.account_ids
                .as_ref()
                .is_some_and(|ids| ids.iter().any(|id| !selected.iter().any(|a| a.id == *id)))
            {
                return Err(ApiError::new("FORBIDDEN", "只能刷新所选账号"));
            }
            Ok(json!(service.refresh(r)?))
        }
        "window.resize" => {
            fields(&params, &["width", "height"])?;
            let w = params["width"].as_f64().ok_or_else(|| error("宽度无效"))?;
            let h = params["height"].as_f64().ok_or_else(|| error("高度无效"))?;
            resize(&window, w, h)
        }
        "window.drag" => {
            if app.state::<Service>().settings().display.position_locked {
                return Err(ApiError::new("FORBIDDEN", "窗口位置已锁定"));
            }
            fields(&params, &[])?;
            #[cfg(windows)]
            {
                if unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(1) } >= 0
                {
                    return Err(ApiError::new("FORBIDDEN", "拖动需要鼠标按下"));
                }
            }
            #[cfg(windows)]
            {
                if unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }
                    != window.hwnd().map_err(api_error)?
                {
                    return Err(ApiError::new("FORBIDDEN", "请在当前主题窗口内拖动"));
                }
            }
            window.start_dragging().map_err(api_error)?;
            Ok(Value::Null)
        }
        "window.hide" => {
            fields(&params, &[])?;
            window.hide().map_err(api_error)?;
            if let Some(s) = runtime.sessions.lock().unwrap().get_mut(&session.token) {
                s.hidden = true;
            }
            Ok(Value::Null)
        }
        "window.menu" => {
            fields(&params, &[])?;
            crate::aide_hud_context_menu(window, service, app.state())
                .map_err(|_| error("无法打开菜单"))?;
            Ok(Value::Null)
        }
        "app.openSettings" => {
            fields(&params, &[])?;
            crate::open_settings(app)?;
            Ok(Value::Null)
        }
        "theme.failed" => {
            fields(&params, &[])?;
            fail_session(app, &session.token, "主题脚本出错，已恢复可用界面");
            Ok(Value::Null)
        }
        "theme.ready" => {
            fields(&params, &[])?;
            let _gate = runtime.transition.lock().unwrap();
            if runtime
                .sessions
                .lock()
                .unwrap()
                .get(&session.token)
                .is_some_and(|s| s.ready)
            {
                return Ok(Value::Null);
            } // repeated ready is harmless
            if !runtime
                .sessions
                .lock()
                .unwrap()
                .contains_key(&session.token)
            {
                return Err(error("会话已关闭"));
            }
            if packages::installed(&root(&service), &session.record.manifest.id)?.generation
                != session.record.generation
            {
                return Err(error("安装版本已改变，请重新加载"));
            }
            window.show().map_err(api_error)?;
            service.select_theme(
                &session.record.manifest.id,
                service.settings().settings_revision,
            )?;
            let obsolete: Vec<_> = {
                let mut sessions = runtime.sessions.lock().unwrap();
                sessions.get_mut(&session.token).unwrap().ready = true;
                let keys: Vec<_> = sessions
                    .keys()
                    .filter(|k| **k != session.token)
                    .cloned()
                    .collect();
                keys.into_iter()
                    .filter_map(|k| sessions.remove(&k))
                    .collect()
            };
            runtime.folded.store(false, Ordering::SeqCst);
            if let Some(hud) = app.get_webview_window("hud") {
                let _ = hud.hide();
            }
            for old in obsolete {
                if let Some(w) = app.get_webview_window(&old.label) {
                    let _ = w.destroy();
                }
            }
            *runtime.notice.lock().unwrap() = None;
            let _ = app.emit("aide://themes-changed", ());
            Ok(Value::Null)
        }
        "storage.get" | "storage.set" | "storage.remove" => {
            fields(
                &params,
                if method == "storage.set" {
                    &["key", "value"]
                } else {
                    &["key"]
                },
            )?;
            let key = params["key"].as_str().ok_or_else(|| error("偏好键无效"))?;
            let _lock = runtime.sessions.lock().unwrap();
            packages::storage(
                &root(&service),
                &session.record,
                method,
                key,
                params.get("value").cloned(),
            )
        }
        _ => Err(ApiError::new("FORBIDDEN", "未公开的主题方法")),
    }
}
pub fn resize(window: &WebviewWindow, width: f64, height: f64) -> Result<Value, ApiError> {
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return Err(error("尺寸须为正数"));
    }
    let scale = window.scale_factor().map_err(api_error)?;
    let monitor = window.current_monitor().map_err(api_error)?;
    let (maxw, maxh) = monitor
        .as_ref()
        .map(|m| {
            (
                m.work_area().size.width as f64 / scale,
                m.work_area().size.height as f64 / scale,
            )
        })
        .unwrap_or((1000., 560.));
    let width = width.max(40.).min(1000.).min(maxw);
    let height = height.max(40.).min(560.).min(maxh);
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(api_error)?;
    if let Some(m) = monitor {
        let area = m.work_area();
        let p = window.outer_position().map_err(api_error)?;
        let x = p.x.clamp(
            area.position.x,
            (area.position.x + area.size.width as i32 - (width * scale).ceil() as i32)
                .max(area.position.x),
        );
        let y = p.y.clamp(
            area.position.y,
            (area.position.y + area.size.height as i32 - (height * scale).ceil() as i32)
                .max(area.position.y),
        );
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(api_error)?;
    }
    Ok(json!({"width":width,"height":height}))
}
#[cfg(windows)]
fn harden(window: &WebviewWindow, prefix: &str) -> Result<(), ApiError> {
    let prefix = prefix.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    window
        .with_webview(move |platform| {
            let result = (|| unsafe {
                use webview2_com::{
                    Microsoft::Web::WebView2::Win32::*, PermissionRequestedEventHandler,
                    WebResourceRequestedEventHandler,
                };
                use windows::core::{w, HSTRING, PWSTR};
                let webview = platform.controller().CoreWebView2()?;
                let env = platform.environment();
                let settings = webview.Settings()?;
                settings.SetAreDefaultContextMenusEnabled(false)?;
                settings.SetAreDevToolsEnabled(false)?;
                settings.SetIsWebMessageEnabled(false)?;
                webview.AddWebResourceRequestedFilter(
                    w!("*"),
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                )?;
                let mut cookie = 0;
                webview.add_WebResourceRequested(
                    &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let mut raw = PWSTR::null();
                        args.Request()?.Uri(&mut raw)?;
                        let url = raw.to_string().unwrap_or_default();
                        windows::Win32::System::Com::CoTaskMemFree(Some(raw.0 as _));
                        if !url.starts_with(&prefix) {
                            let response = env.CreateWebResourceResponse(
                                None,
                                403,
                                w!("Forbidden"),
                                &HSTRING::from("Content-Type: text/plain"),
                            )?;
                            args.SetResponse(&response)?;
                        }
                        Ok(())
                    })),
                    &mut cookie,
                )?;
                webview.add_PermissionRequested(
                    &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                        if let Some(args) = args {
                            args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                        }
                        Ok(())
                    })),
                    &mut cookie,
                )?;
                Ok::<(), windows::core::Error>(())
            })();
            let _ = tx.send(result.is_ok());
        })
        .map_err(api_error)?;
    if rx.recv_timeout(Duration::from_secs(3)) != Ok(true) {
        return Err(error("无法建立主题网络隔离"));
    }
    Ok(())
}
#[cfg(not(windows))]
fn harden(_: &WebviewWindow, _: &str) -> Result<(), ApiError> {
    Err(error("当前第三方主题运行时仅支持 Windows"))
}
