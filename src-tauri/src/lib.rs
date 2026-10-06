mod adapters;
mod config;
mod domain;
mod model;
mod service;
mod smoke;
mod startup;
mod themes;

use model::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use service::Service;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_dialog::DialogExt;

fn authorize(window: &WebviewWindow, settings_only: bool) -> Result<(), ApiError> {
    if (settings_only && window.label() == "settings")
        || (!settings_only && ["hud", "settings"].contains(&window.label()))
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
                    "themes.validate",
                    "themes.preview",
                    "themes.import",
                    "themes.select",
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
async fn hud_internal_theme_export(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> Result<ApiResult<Value>, String> {
    let service = service.inner().clone();
    if let Err(e) = authorize(&window, true).and_then(|_| empty(&request)) {
        return Ok(respond::<Value>(&service, Err(e)));
    }
    let app = window.app_handle().clone();
    let zh = service.settings().display.locale == "zh-CN";
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(if zh {
                "保存主题示例"
            } else {
                "Save theme example"
            })
            .set_file_name("theme.json")
            .add_filter("JSON", &["json"])
            .blocking_save_file()
    })
    .await;
    let result = match picked {
        Ok(None) => Ok(json!({"cancelled":true})),
        Ok(Some(file)) => file
            .into_path()
            .map_err(|_| ApiError::new("IO_ERROR", "Invalid save path"))
            .and_then(|path| {
                std::fs::write(path, include_bytes!("../../public/theme-template.json"))
                    .map_err(|_| ApiError::new("IO_ERROR", "Cannot save theme example"))
            })
            .map(|_| json!({"cancelled":false})),
        Err(_) => Err(ApiError::new("IO_ERROR", "Cannot open save dialog")),
    };
    Ok(respond(&service, result))
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
                std::fs::write(
                    dir.join("theme-template.json"),
                    include_bytes!("../../public/theme-template.json"),
                )
                .map_err(|_| ApiError::new("IO_ERROR", "Cannot prepare theme example"))?;
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
fn hud_v1_quota_snapshot_get(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    if let Err(e) = authorize(&window, false).and_then(|_| empty(&request)) {
        respond::<Value>(&service, Err(e))
    } else {
        service.read_reply("snapshot", false)
    }
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
fn hud_v1_themes_list(
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
struct ThemeDocumentRequest {
    document: Value,
}
#[tauri::command]
fn hud_v1_themes_validate(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| parse::<ThemeDocumentRequest>(request))
            .map(|r| themes::validate(&r.document, true)),
    )
}
#[tauri::command]
fn hud_v1_themes_preview(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| parse::<ThemeDocumentRequest>(request))
            .and_then(|r| {
                let validation = themes::validate(&r.document, false);
                if !validation.valid {
                    Err(ApiError::new("THEME_INVALID", "主题未通过校验，无法预览"))
                } else {
                    Ok(json!({"previewId":uuid::Uuid::new_v4().to_string()}))
                }
            }),
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeSelectRequest {
    id: String,
    expected_revision: u64,
}
#[tauri::command]
fn hud_v1_themes_select(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, true)
            .and_then(|_| parse::<ThemeSelectRequest>(request))
            .and_then(|r| service.select_theme(&r.id, r.expected_revision)),
    )
}
#[tauri::command]
async fn hud_v1_themes_import(
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
                "导入 HUD 主题"
            } else {
                "Import HUD theme"
            })
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
    })
    .await;
    let result = match picked {
        Ok(None) => Ok(json!({"cancelled":true,"theme":null})),
        Ok(Some(file)) => file
            .into_path()
            .map_err(|_| ApiError::new("IO_ERROR", "主题文件位置无效"))
            .and_then(|path| themes::import(&service.theme_dir, &path))
            .and_then(|theme| {
                serde_json::to_value(theme)
                    .map(|v| json!({"cancelled":false,"theme":v}))
                    .map_err(|_| ApiError::new("INTERNAL_ERROR", "主题导入失败"))
            }),
        Err(_) => Err(ApiError::new("IO_ERROR", "无法打开主题选择器")),
    };
    Ok(respond(&service, result))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeGetRequest {
    id: Option<String>,
}
#[tauri::command]
fn hud_internal_theme_get(
    window: WebviewWindow,
    service: State<'_, Service>,
    request: Value,
) -> ApiResult<Value> {
    respond(
        &service,
        authorize(&window, false)
            .and_then(|_| parse::<ThemeGetRequest>(request))
            .and_then(|r| {
                themes::get(
                    &service.theme_dir,
                    &r.id.unwrap_or_else(|| service.settings().active_theme_id),
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
        .inner_size(720.0, 620.0)
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

    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.set_always_on_top(settings.display.always_on_top);
    }
    if let Some(item) = app.try_state::<CheckMenuItem<tauri::Wry>>() {
        let _ = item.set_checked(settings.display.always_on_top);
    }
}
fn window_action(app: &tauri::AppHandle, action: &str) -> Result<Value, ApiError> {
    let hud = app
        .get_webview_window("hud")
        .ok_or_else(|| ApiError::new("NOT_FOUND", "悬浮窗不可用"))?;
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
            .and_then(|_| window.set_resizable(!request.collapsed))
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
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = window_action(app, "show");
        }))
        .plugin(tauri_plugin_dialog::init())
        .on_page_load(smoke::on_page_load)
        .invoke_handler(tauri::generate_handler![
            hud_v1_capabilities_get,
            hud_v1_accounts_list,
            hud_v1_accounts_selection_update,
            hud_v1_accounts_alias_update,
            hud_internal_theme_export,
            hud_internal_docs_open,
            hud_v1_quota_snapshot_get,
            hud_v1_recommendation_get,
            hud_v1_refresh_request,
            hud_v1_refresh_status_get,
            hud_v1_settings_get,
            hud_v1_settings_update,
            hud_v1_themes_list,
            hud_v1_themes_validate,
            hud_v1_themes_preview,
            hud_v1_themes_import,
            hud_v1_themes_select,
            hud_v1_window_control,
            hud_v1_diagnostics_get,
            hud_internal_sources_get,
            hud_internal_sources_save,
            hud_internal_sources_pick,
            hud_internal_startup,
            hud_internal_source_get,
            hud_internal_source_choose,
            hud_internal_source_rescan,
            hud_internal_theme_get,
            hud_internal_window_layout
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
                if smoke::is_interactive() {
                    let _ = open_settings_from_tray(app.handle());
                } else {
                    smoke::start_poll(app.handle());
                }
                return Ok(());
            }
            let dir = app.path().app_data_dir()?;
            if !dir.join("settings.json").exists() {
                let legacy = dir
                    .parent()
                    .map(|p| p.join("dev.cockpit-quota-hud.desktop"));
                if let Some(legacy) = legacy.filter(|p| p.join("settings.json").is_file()) {
                    let mut previous = config::load(&legacy.join("settings.json"))
                        .map_err(|e| std::io::Error::other(e.message))?;
                    previous.settings.display.locale = "en".into();
                    config::atomic_json(&dir.join("settings.json"), &previous)
                        .map_err(|e| std::io::Error::other(e.message))?;
                    if let Ok(files) = std::fs::read_dir(legacy.join("themes")) {
                        std::fs::create_dir_all(dir.join("themes"))?;
                        for file in files.flatten() {
                            if file.path().extension().and_then(|x| x.to_str()) == Some("json") {
                                std::fs::copy(
                                    file.path(),
                                    dir.join("themes").join(file.file_name()),
                                )?;
                            }
                        }
                    }
                }
            }
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
            app.manage(service.clone());
            let show = MenuItem::with_id(app, "show", "显示悬浮窗", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "hide", "隐藏悬浮窗", true, None::<&str>)?;
            let pin = pin_item(app.handle(), &service)?;
            let refresh = MenuItem::with_id(app, "refresh", "刷新全部", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
            let restore = MenuItem::with_id(app, "restore", "恢复窗口位置", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            app.manage(TrayLabels(vec![
                (show.clone(), "Show HUD", "显示悬浮窗"),
                (hide.clone(), "Hide HUD", "隐藏悬浮窗"),
                (refresh.clone(), "Refresh all", "刷新全部"),
                (settings.clone(), "Settings…", "设置…"),
                (restore.clone(), "Restore position", "恢复窗口位置"),
                (quit.clone(), "Quit", "退出"),
            ]));
            sync_window_preferences(app.handle(), &service.settings());
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(
                app,
                &[
                    &pin, &settings, &refresh, &separator, &show, &hide, &restore, &quit,
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
