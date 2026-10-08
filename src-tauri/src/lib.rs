mod adapters;
mod config;
mod domain;
mod model;
mod network;
mod quota_total;
mod service;
mod smoke;
mod startup;
mod theme_commands;
mod theme_package;
mod theme_runtime;
mod theme_smoke;
mod themes;
mod updates;
mod usb_display;
use theme_commands::*;

use model::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use service::Service;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_dialog::DialogExt;

fn authorize(window: &WebviewWindow, settings_only: bool) -> Result<(), ApiError> {
    if (settings_only && window.label() == "settings")
        || (!settings_only && ["hud", "settings", "usb-display"].contains(&window.label()))
    {
        Ok(())
    } else {
        Err(ApiError::new("FORBIDDEN", "此窗口没有该操作权限"))
    }
}
fn empty(request: &Value) -> Result<(), ApiError> {
    if request.as_object().is_some_and(|o| o.is_empty()) {
        Ok(())
    } else {
        Err(ApiError::new("INVALID_ARGUMENT", "该接口不接收参数"))
    }
}
fn parse<T: serde::de::DeserializeOwned>(request: Value) -> Result<T, ApiError> {
    serde_json::from_value(request).map_err(|_| ApiError::new("INVALID_ARGUMENT", "请求参数无效"))
}
fn respond<T: Serialize>(service: &Service, result: Result<T, ApiError>) -> ApiResult<Value> {
    service.envelope(result.and_then(|v| {
        serde_json::to_value(v).map_err(|_| ApiError::new("INTERNAL_ERROR", "无法返回操作结果"))
    }))
}

#[tauri::command]
fn hud_v1_capabilities_get(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    let result = authorize(&window, false)
        .and_then(|_| empty(&request))
        .map(|_| {
            let mut methods = vec![
                "capabilities.get",
                "accounts.list",
                "quota.snapshot.get",
                "recommendation.get",
                "refresh.request",
                "refresh.status.get",
                "settings.get",
                "themes.list",
                "window.control",
            ];
            if window.label() == "settings" {
                methods.extend([
                    "accounts.selection.update",
                    "accounts.alias.update",
                    "settings.update",
                    "diagnostics.get",
                ])
            }
            let mut scopes = vec![
                "quota.read",
                "quota.refresh",
                "events.read",
                "settings.read",
                "themes.read",
                "window.control",
            ];
            if window.label() == "settings" {
                scopes.extend(["settings.write", "themes.write", "diagnostics.read"]);
            }
            Capabilities {
                app_version: env!("CARGO_PKG_VERSION").into(),
                api_version: "1.0".into(),
                transport: "tauri".into(),
                enabled_methods: methods.into_iter().map(str::to_owned).collect(),
                granted_scopes: scopes.into_iter().map(str::to_owned).collect(),
                theme_schema_versions: vec![1],
                provider_ids: vec![
                    "codex_usage".into(),
                    "claude".into(),
                    "antigravity".into(),
                    "grok".into(),
                ],
                max_refresh_accounts: 100,
            }
        });
    respond(&service, result)
}
#[tauri::command]
fn hud_v1_accounts_list(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    if let Err(e) = authorize(&window, false).and_then(|_| empty(&request)) {
        respond::<Value>(&service, Err(e))
    } else {
        service.read_reply("accounts", window.label() == "settings")
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AliasRequest {
    account_id: String,
    alias: String,
}
#[tauri::command]
fn hud_v1_accounts_alias_update(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| parse::<AliasRequest>(request))
            .and_then(|r| service.set_alias(&r.account_id, &r.alias)),
    )
}

#[tauri::command]
async fn hud_internal_update_check(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> Result<ApiResult<Value>, String> {
    let result = match authorize(&window, true).and_then(|_| empty(&request)) {
        Ok(()) => updates::check().await,
        Err(error) => Err(error),
    };
    Ok(respond(&service, result))
}

#[tauri::command]
fn hud_internal_update_open(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| empty(&request))
            .and_then(|_| {
                std::process::Command::new("explorer.exe")
                    .arg(updates::RELEASE_URL)
                    .spawn()
                    .map_err(|_| ApiError::new("IO_ERROR", "无法打开下载页面"))?;
                Ok(json!({"opened":true}))
            }),
    )
}

#[tauri::command]
fn hud_internal_docs_open(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| empty(&request))
            .and_then(|_| {
                let dir = window
                    .app_handle()
                    .path()
                    .app_data_dir()
                    .map_err(|_| {
                        ApiError::new("IO_ERROR", "Cannot locate documentation directory")
                    })?
                    .join("docs");
                std::fs::create_dir_all(&dir).map_err(|_| {
                    ApiError::new("IO_ERROR", "Cannot create documentation directory")
                })?;
                let file = dir.join("api.html");
                std::fs::write(&file, include_bytes!("../../public/api.html"))
                    .map_err(|_| ApiError::new("IO_ERROR", "Cannot prepare documentation"))?;
                #[cfg(windows)]
                std::process::Command::new("explorer.exe")
                    .arg(&file)
                    .spawn()
                    .map_err(|_| ApiError::new("IO_ERROR", "Cannot open documentation"))?;
                #[cfg(not(windows))]
                return Err(ApiError::new(
                    "UNSUPPORTED",
                    "Documentation opener is currently supported on Windows",
                ));
                Ok(json!({"opened":true}))
            }),
    )
}

#[tauri::command]
fn hud_v1_recommendation_get(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    if let Err(e) = authorize(&window, false).and_then(|_| empty(&request)) {
        respond::<Value>(&service, Err(e))
    } else {
        service.read_reply("recommendation", false)
    }
}
#[tauri::command]
fn hud_v1_settings_get(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    if let Err(e) = authorize(&window, false).and_then(|_| empty(&request)) {
        respond::<Value>(&service, Err(e))
    } else {
        service.read_reply("settings", false)
    }
}
#[tauri::command]
fn hud_v1_refresh_request(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, false)
            .and_then(|_| parse::<RefreshRequest>(request))
            .and_then(|request| service.refresh(request)),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobRequest {
    job_id: String,
}
#[tauri::command]
fn hud_v1_refresh_status_get(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    match authorize(&window, false).and_then(|_| parse::<JobRequest>(request)) {
        Err(e) => respond::<Value>(&service, Err(e)),
        Ok(r) => service.read_reply(&format!("job:{}", r.job_id), false),
    }
}
#[tauri::command]
fn hud_v1_accounts_selection_update(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    let result = authorize(&window, true)
        .and_then(|_| parse::<AccountSelectionPatch>(request))
        .and_then(|patch| service.select_accounts(patch));
    if result.is_ok() {
        let _ = service.refresh(RefreshRequest { account_ids: None });
    }
    respond(&service, result)
}
#[tauri::command]
fn hud_v1_settings_update(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    let result = authorize(&window, true)
        .and_then(|_| parse::<SettingsPatch>(request))
        .and_then(|patch| service.update_settings(patch));
    if let Ok(settings) = &result {
        sync_window_preferences(window.app_handle(), settings);
    }
    respond(&service, result)
}
#[tauri::command]
fn aide_theme_builtins(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, false)
            .and_then(|_| empty(&request))
            .map(|_| themes::list(&service.theme_dir)),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeGetRequest {
    id: Option<String>,
}
#[tauri::command]
fn aide_theme_builtin(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    if window.label() == "usb-display" {
        return respond::<Value>(&service, Err(ApiError::new("FORBIDDEN", "独立监视屏使用独立主题")));
    }
    respond(
        &service,
        authorize(&window, false)
            .and_then(|_| parse::<ThemeGetRequest>(request))
            .and_then(|r| {
                themes::get(
                    &service.theme_dir,
                    &r.id.unwrap_or_else(|| {
                        service.settings().active_theme_id
                    }),
                )
            }),
    )
}
#[tauri::command]
fn hud_internal_sources_get(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| empty(&request))
            .map(|_| service.sources()),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcesRequest {
    sources: Vec<adapters::sources::SourceOption>,
}
#[tauri::command]
fn hud_internal_sources_save(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| parse::<SourcesRequest>(request))
            .and_then(|r| service.set_sources(r.sources)),
    )
}
#[tauri::command]
async fn hud_internal_sources_pick(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> Result<ApiResult<Value>, String> {
    if let Err(e) = authorize(&window, true).and_then(|_| empty(&request)) {
        return Ok(respond::<Value>(&service, Err(e)));
    }
    let app = window.app_handle().clone();
    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
            .await;
    Ok(respond(
        &service,
        match picked {
            Ok(None) => Ok(json!({"path":null})),
            Ok(Some(file)) => file
                .into_path()
                .map(|p| json!({"path":p}))
                .map_err(|_| ApiError::new("IO_ERROR", "目录不可用")),
            Err(_) => Err(ApiError::new("IO_ERROR", "无法选择目录")),
        },
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartupRequest {
    enabled: Option<bool>,
}
#[tauri::command]
fn hud_internal_startup(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| parse::<StartupRequest>(request))
            .and_then(|r| match r.enabled {
                Some(v) => startup::set(v),
                None => startup::enabled(),
            })
            .map(|v| json!({"enabled":v})),
    )
}
#[tauri::command]
fn hud_internal_source_get(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true).and_then(|_| empty(&request)).map(
            |_| json!({"path":service.source_path().map(|p|p.to_string_lossy().into_owned())}),
        ),
    )
}
#[tauri::command]
async fn hud_internal_source_choose(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> Result<ApiResult<Value>, String> {
    let service = service.inner().clone();
    if let Err(error) = authorize(&window, true).and_then(|_| empty(&request)) {
        return Ok(respond::<Value>(&service, Err(error)));
    }
    let app = window.app_handle().clone();
    let zh = service.settings().display.locale == "zh-CN";
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(if zh {
                "选择官方 auth.json 或 Cockpit codex_accounts.json 所在目录"
            } else {
                "Select the folder containing auth.json or codex_accounts.json"
            })
            .blocking_pick_folder()
    })
    .await;
    let result=match picked {Ok(None)=>Ok(json!({"cancelled":true,"path":null,"source":service.snapshot().source})),Ok(Some(file))=>file.into_path().map_err(|_|ApiError::new("SOURCE_NOT_FOUND","所选目录无效")).and_then(|path|service.choose_source(path)).map(|source|json!({"cancelled":false,"path":service.source_path().map(|p|p.to_string_lossy().into_owned()),"source":source})),Err(_)=>Err(ApiError::new("IO_ERROR","无法打开目录选择器"))};
    Ok(respond(&service, result))
}
#[tauri::command]
fn hud_internal_source_rescan(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| empty(&request))
            .and_then(|_| service.rescan()),
    )
}
#[tauri::command]
fn hud_v1_diagnostics_get(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| empty(&request))
            .map(|_| service.diagnostics()),
    )
}
fn open_settings(app: &tauri::AppHandle) -> Result<(), ApiError> {
    // Always called from an async command or a worker: WebView2 creation must
    // not block Windows' event handler / synchronous IPC thread.
    static CREATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = CREATION
        .lock()
        .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法打开设置"))?;
    if let Some(window) = app.get_webview_window("settings") {
        if !smoke::is_enabled() || smoke::is_interactive() {
            window
                .show()
                .and_then(|_| window.set_focus())
                .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法显示设置"))?;
        }
    } else {
        WebviewWindowBuilder::new(
            app,
            "settings",
            WebviewUrl::App("index.html?view=settings".into()),
        )
        .title(
            if app.state::<Service>().settings().display.locale == "zh-CN" {
                "AIDE monitor · 设置"
            } else {
                "AIDE monitor · Settings"
            },
        )
        .skip_taskbar(false)
        .inner_size(740.0, 620.0)
        .min_inner_size(600.0, 480.0)
        .background_color(tauri::webview::Color(250, 246, 236, 255))
        .visible(!smoke::is_enabled() || smoke::is_interactive())
        .focused(!smoke::is_enabled() || smoke::is_interactive())
        .center()
        .build()
        .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法打开设置窗口"))?;
    }
    Ok(())
}
fn open_settings_from_tray(
    app: &tauri::AppHandle,
) -> tauri::async_runtime::JoinHandle<Result<(), ApiError>> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || open_settings(&app))
}
fn pin_item(app: &tauri::AppHandle, service: &Service) -> tauri::Result<CheckMenuItem<tauri::Wry>> {
    let item = CheckMenuItem::with_id(
        app,
        "pin",
        "置顶",
        true,
        service.settings().display.always_on_top,
        None::<&str>,
    )?;
    app.manage(item.clone());
    Ok(item)
}
fn toggle_pin(app: &tauri::AppHandle) {
    let service = app.state::<Service>();
    let before = service.settings();
    let result = service.update_settings(SettingsPatch {
        expected_revision: before.settings_revision,
        proxies: None,
        account_proxies: None,
        account_refresh: None,
        usb_display: None,
        refresh_interval_seconds: None,
        auto_refresh: None,
        display: Some(DisplaySettingsPatch {
            always_on_top: Some(!before.display.always_on_top),
            ..Default::default()
        }),
    });
    sync_window_preferences(app, &result.unwrap_or(before));
}
struct TrayLabels(Vec<(MenuItem<tauri::Wry>, &'static str, &'static str)>);

fn sync_window_preferences(app: &tauri::AppHandle, settings: &Settings) {
    let zh = settings.display.locale == "zh-CN";
    if let Some(items) = app.try_state::<TrayLabels>() {
        for (item, english, chinese) in &items.0 {
            let _ = item.set_text(if zh { chinese } else { english });
        }
    }
    if let Some(pin) = app.try_state::<CheckMenuItem<tauri::Wry>>() {
        let _ = pin.set_text(if zh { "置顶" } else { "Always on top" });
    }
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_title(if zh {
            "AIDE monitor · 设置"
        } else {
            "AIDE monitor · Settings"
        });
    }

    if let Some(theme) = theme_runtime::visible(app) {
        let _ = theme.set_always_on_top(settings.display.always_on_top);
    }
    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.set_always_on_top(settings.display.always_on_top);
    }
    if let Some(item) = app.try_state::<CheckMenuItem<tauri::Wry>>() {
        let _ = item.set_checked(settings.display.always_on_top);
    }
}
fn window_action(app: &tauri::AppHandle, action: &str) -> Result<Value, ApiError> {
    let hud = theme_runtime::target(app)
        .or_else(|| app.get_webview_window("hud"))
        .ok_or_else(|| ApiError::new("NOT_FOUND", "悬浮窗不可用"))?;
    if action == "show" {
        theme_runtime::set_hidden(
            app,
            app.state::<theme_runtime::Runtime>()
                .folded
                .load(std::sync::atomic::Ordering::SeqCst),
        );
    } else if action == "hide" {
        theme_runtime::set_hidden(app, true);
    }
    match action {
        "show" => hud
            .show()
            .and_then(|_| hud.set_focus())
            .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法显示悬浮窗"))?,
        "hide" => hud
            .hide()
            .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法隐藏悬浮窗"))?,
        "restore_position" => hud
            .center()
            .and_then(|_| hud.show())
            .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法恢复窗口位置"))?,
        "open_settings" => open_settings(app)?,
        _ => return Err(ApiError::new("INVALID_ARGUMENT", "窗口操作无效")),
    }
    Ok(json!({"accepted":true}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowRequest {
    action: String,
}

#[derive(Default)]
struct HudLayoutState(std::sync::Mutex<HudLayout>);

const HUD_ORB_SIZE: f64 = 43.0;

#[tauri::command]
fn aide_hud_context_menu(
    window: WebviewWindow,
    service: State<'_, Service>,
    layout: State<'_, HudLayoutState>,
) -> Result<(), String> {
    if window.label() != "hud" && !theme_runtime::is_session(window.app_handle(), window.label()) {
        return Err("FORBIDDEN".into());
    }
    let zh = service.settings().display.locale == "zh-CN";
    let collapsed = layout.0.lock().map_err(|_| "INTERNAL_ERROR")?.collapsed;
    let app = window.app_handle();
    let labels = [
        (
            "aide-refresh",
            if zh { "刷新额度" } else { "Refresh quota" },
        ),
        (
            "aide-toggle",
            if collapsed {
                if zh {
                    "展开"
                } else {
                    "Expand"
                }
            } else if zh {
                "收起"
            } else {
                "Collapse"
            },
        ),
        ("aide-hide", if zh { "隐藏" } else { "Hide" }),
        ("aide-quit", if zh { "退出" } else { "Quit" }),
    ];
    let items: Vec<_> = labels
        .into_iter()
        .map(|(id, label)| MenuItem::with_id(app, id, label, true, None::<&str>))
        .collect::<tauri::Result<_>>()
        .map_err(|_| "MENU_ERROR")?;
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        items.iter().map(|item| item as _).collect();
    let menu = Menu::with_items(app, &refs).map_err(|_| "MENU_ERROR")?;
    window.popup_menu(&menu).map_err(|_| "MENU_ERROR".into())
}

#[derive(Default)]
struct HudLayout {
    collapsed: bool,
    expanded_width: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HudLayoutRequest {
    collapsed: bool,
    width: f64,
    height: f64,
}

// Internal trusted-HUD sizing: content determines the expanded size, while the
// orb has a genuinely small native window with transparent corners.
#[tauri::command]
async fn hud_internal_window_layout(
    window: WebviewWindow,
    service: State<'_, Service>,
    layout: State<'_, HudLayoutState>,
    request: Value,
) -> Result<ApiResult<Value>, String> {
    let result = (|| {
        if window.label() != "hud" {
            return Err(ApiError::new("FORBIDDEN", "此窗口没有该操作权限"));
        }
        let request: HudLayoutRequest = parse(request)?;
        if !request.width.is_finite()
            || !request.height.is_finite()
            || !(40.0..=1000.0).contains(&request.width)
            || !(40.0..=560.0).contains(&request.height)
            || (request.collapsed
                && (request.width != HUD_ORB_SIZE || request.height != HUD_ORB_SIZE))
        {
            return Err(ApiError::new("INVALID_ARGUMENT", "窗口尺寸无效"));
        }
        let mut layout = layout
            .0
            .lock()
            .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法调整悬浮窗"))?;
        let before = window
            .inner_size()
            .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法读取窗口尺寸"))?;
        let position = window
            .outer_position()
            .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法读取窗口位置"))?;
        let scale = window
            .scale_factor()
            .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法读取显示缩放"))?;
        window
            .set_size(tauri::LogicalSize::new(request.width, request.height))
            .and_then(|_| window.set_resizable(false))
            .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法调整悬浮窗"))?;
        if layout.collapsed != request.collapsed {
            // Keep the right edge in place when the far-right fold control is used.
            let width = (request.width * scale).round() as i32;
            window
                .set_position(tauri::PhysicalPosition::new(
                    position.x + before.width as i32 - width,
                    position.y,
                ))
                .map_err(|_| ApiError::new("INTERNAL_ERROR", "无法调整窗口位置"))?;
        }
        layout.collapsed = request.collapsed;
        if !request.collapsed {
            layout.expanded_width = request.width;
        }
        if (request.collapsed
            || theme_package::BUILTINS.contains(&service.settings().active_theme_id.as_str()))
            && !smoke::is_enabled()
        {
            let _ = window.show();
        }
        Ok(json!({"accepted":true}))
    })();
    Ok(respond(&service, result))
}
#[tauri::command]
async fn hud_v1_window_control(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> Result<ApiResult<Value>, String> {
    Ok(respond(
        &service,
        authorize(&window, false)
            .and_then(|_| parse::<WindowRequest>(request))
            .and_then(|r| window_action(window.app_handle(), &r.action)),
    ))
}

pub fn run() {
    let mut context = tauri::generate_context!();
    if smoke::is_enabled() {
        context.config_mut().identifier.push_str(".smoke");
        for window in &mut context.config_mut().app.windows {
            window.visible = smoke::is_interactive();
        }
    }
    tauri::Builder::default()
        .manage(HudLayoutState::default())
        .manage(theme_runtime::Runtime::default())
        .register_asynchronous_uri_scheme_protocol("aide", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let label = ctx.webview_label().to_string();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(theme_runtime::response(&app, &label, request))
            });
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "aide-refresh" => {
                let _ = app
                    .state::<Service>()
                    .refresh(RefreshRequest { account_ids: None });
            }
            "aide-toggle" => {
                if theme_runtime::toggle(app) {
                    return;
                }
                if let Some(hud) = app.get_webview_window("hud") {
                    let _ = hud.emit("aide://toggle-collapse", ());
                }
            }
            "aide-hide" => {
                let _ = window_action(app, "hide");
            }
            "aide-quit" => {
                app.state::<Service>().flush_position();
                app.exit(0);
            }
            _ => {}
        })
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = window_action(app, "show");
        }))
        .plugin(tauri_plugin_dialog::init())
        .on_page_load(smoke::on_page_load)
        .invoke_handler(tauri::generate_handler![
            usb_display::aide_usb_displays,
            hud_v1_capabilities_get,
            hud_v1_accounts_list,
            hud_v1_accounts_selection_update,
            hud_v1_accounts_alias_update,
            hud_internal_docs_open,
            hud_internal_update_check,
            hud_internal_update_open,
            hud_v1_recommendation_get,
            hud_v1_refresh_request,
            hud_v1_refresh_status_get,
            hud_v1_settings_get,
            hud_v1_settings_update,
            aide_theme_builtins,
            hud_v1_window_control,
            hud_v1_diagnostics_get,
            hud_internal_sources_get,
            hud_internal_sources_save,
            hud_internal_sources_pick,
            hud_internal_startup,
            hud_internal_source_get,
            hud_internal_source_choose,
            hud_internal_source_rescan,
            aide_theme_builtin,
            hud_internal_window_layout,
            aide_hud_context_menu,
            aide_theme_list,
            aide_theme_data,
            aide_theme_install,
            aide_theme_select,
            aide_theme_uninstall,
            aide_theme_resume
        ])
        .setup(|app| {
            if smoke::is_enabled() {
                let state = smoke::prepare_data()?;
                let service = Service::new(state.data_dir.clone(), Some(app.handle().clone()))
                    .map_err(|error| std::io::Error::other(error.message))?;
                app.manage(state);
                pin_item(app.handle(), &service)?;
                app.manage(service);
                // The HUD smoke script opens settings through the real IPC path.
                if theme_smoke::enabled() {
                    theme_smoke::start(app.handle());
                } else if smoke::is_interactive() {
                    let _ = open_settings_from_tray(app.handle());
                } else {
                    smoke::start_poll(app.handle());
                }
                return Ok(());
            }
            let dir = app.path().app_data_dir()?;
            let service = Service::new(dir, Some(app.handle().clone()))
                .map_err(|error| std::io::Error::other(error.message))?;
            if let Some(hud) = app.get_webview_window("hud") {
                let _ = hud.set_always_on_top(service.settings().display.always_on_top);
                if let Some((x, y)) = service.saved_position() {
                    let monitors = hud.available_monitors().unwrap_or_default();
                    if monitors.iter().any(|m| {
                        let p = m.position();
                        let s = m.size();
                        x >= p.x
                            && y >= p.y
                            && x < p.x + s.width as i32 - 80
                            && y < p.y + s.height as i32 - 40
                    }) {
                        let _ = hud.set_position(tauri::PhysicalPosition::new(x, y));
                    }
                }
            }
            let _ = theme_package::collect_orphans(&theme_runtime::root(&service));
            app.manage(service.clone());
            let startup_theme = service.settings().active_theme_id.clone();
            if !theme_package::BUILTINS.contains(&startup_theme.as_str()) {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if theme_runtime::select(&handle, &startup_theme).is_err() {
                        theme_runtime::restore(&handle, Some("主题启动失败，已恢复默认主题"));
                    }
                });
            }
            let show = MenuItem::with_id(app, "show", "显示悬浮窗", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "hide", "隐藏悬浮窗", true, None::<&str>)?;
            let pin = pin_item(app.handle(), &service)?;
            let refresh = MenuItem::with_id(app, "refresh", "刷新全部", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
            let restore = MenuItem::with_id(app, "restore", "恢复窗口位置", true, None::<&str>)?;
            let recover = MenuItem::with_id(app, "recover", "恢复内置主题", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            app.manage(TrayLabels(vec![
                (show.clone(), "Show HUD", "显示悬浮窗"),
                (hide.clone(), "Hide HUD", "隐藏悬浮窗"),
                (refresh.clone(), "Refresh all", "刷新全部"),
                (settings.clone(), "Settings…", "设置…"),
                (restore.clone(), "Restore position", "恢复窗口位置"),
                (recover.clone(), "Restore built-in theme", "恢复内置主题"),
                (quit.clone(), "Quit", "退出"),
            ]));
            sync_window_preferences(app.handle(), &service.settings());
            usb_display::start(app.handle());
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(
                app,
                &[
                    &pin, &settings, &refresh, &separator, &show, &hide, &restore, &recover, &quit,
                ],
            )?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("AIDE monitor")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        let _ = window_action(app, "show");
                    }
                    "hide" => {
                        let _ = window_action(app, "hide");
                    }
                    "restore" => {
                        let _ = window_action(app, "restore_position");
                    }
                    "settings" => {
                        open_settings_from_tray(app);
                    }
                    "recover" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            theme_runtime::restore(&app, None)
                        });
                    }
                    "pin" => toggle_pin(app),
                    "refresh" => {
                        let service = app.state::<Service>();
                        let _ = service.refresh(RefreshRequest { account_ids: None });
                    }
                    "quit" => {
                        app.state::<Service>().flush_position();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = window_action(tray.app_handle(), "show");
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            service.start_background();
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "usb-display" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
                if let tauri::WindowEvent::Moved(_) = event {
                    let id = window
                        .app_handle()
                        .state::<Service>()
                        .settings()
                        .usb_display
                        .device_id;
                    let valid = usb_display::devices(window.app_handle()).is_ok_and(|v| {
                        v.iter().any(|d| {
                            d.id == id
                                && window.outer_position().ok()
                                    == Some(tauri::PhysicalPosition::new(d.x, d.y))
                        })
                    });
                    if !valid {
                        let _ = window.hide();
                    }
                }
            }
            if window.label().starts_with("aide-theme-") {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                    theme_runtime::set_hidden(window.app_handle(), true);
                }
                if let tauri::WindowEvent::Moved(position) = event {
                    window
                        .app_handle()
                        .state::<Service>()
                        .remember_position(position.x, position.y);
                }
            }
            if window.label() == "hud" {
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    tauri::WindowEvent::Moved(position) => {
                        if let Some(service) = window.app_handle().try_state::<Service>() {
                            if let Some(layout) = window.app_handle().try_state::<HudLayoutState>()
                            {
                                // Programmatic size changes may dispatch a move while
                                // the layout lock is held. Never block the UI thread.
                                if let Ok(layout) = layout.0.try_lock() {
                                    let offset = if layout.collapsed {
                                        ((layout.expanded_width - HUD_ORB_SIZE).max(0.0)
                                            * window.scale_factor().unwrap_or(1.0))
                                        .round() as i32
                                    } else {
                                        0
                                    };
                                    service.remember_position(position.x - offset, position.y);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        })
        .run(context)
        .expect("Unable to run AIDE monitor");
}
