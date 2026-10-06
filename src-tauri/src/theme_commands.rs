use crate::{model::*, service::Service, theme_package as packages, theme_runtime as runtime};
use serde_json::{json, Value};
use tauri::{Manager, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
fn allowed(window: &WebviewWindow) -> Result<(), ApiError> {
    crate::authorize(window, true)
}
#[tauri::command]
pub fn aide_theme_list(window: WebviewWindow, service: State<'_, Service>) -> ApiResult<Value> {
    crate::respond(&service,allowed(&window).map(|_|{
    let mut list:Vec<Value>=packages::BUILTINS.iter().map(|id|json!({"id":id,"name":crate::themes::builtin(id).unwrap()["name"],"builtIn":true,"version":env!("CARGO_PKG_VERSION")})).collect();
    list.extend(packages::list(&runtime::root(&service)).iter().map(|r|json!({"id":r.manifest.id,"name":r.manifest.name,"version":r.manifest.version,"author":r.manifest.author,"builtIn":false})));
    json!({"themes":list,"activeId":service.settings().active_theme_id,"notice":*window.app_handle().state::<runtime::Runtime>().notice.lock().unwrap()})
}))
}
#[tauri::command]
pub fn aide_theme_data(window: WebviewWindow, service: State<'_, Service>) -> ApiResult<Value> {
    match crate::authorize(&window, false) {
        Ok(()) => service.read_reply("snapshot", false),
        Err(e) => crate::respond::<Value>(&service, Err(e)),
    }
}
#[tauri::command]
pub async fn aide_theme_select(
    window: WebviewWindow,
    service: State<'_, Service>,
    id: String,
) -> Result<ApiResult<Value>, String> {
    if let Err(e) = allowed(&window) {
        return Ok(crate::respond::<Value>(&service, Err(e)));
    }
    let app = window.app_handle().clone();
    let result = tauri::async_runtime::spawn_blocking(move || runtime::select(&app, &id))
        .await
        .unwrap_or_else(|_| Err(packages::invalid("无法加载主题")));
    Ok(crate::respond(&service, result))
}
#[tauri::command]
pub async fn aide_theme_install(
    window: WebviewWindow,
    service: State<'_, Service>,
) -> Result<ApiResult<Value>, String> {
    if let Err(e) = allowed(&window) {
        return Ok(crate::respond::<Value>(&service, Err(e)));
    }
    let app = window.app_handle().clone();
    let root = runtime::root(&service);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("AIDE Theme", &["aidetheme"])
            .blocking_pick_file()
        else {
            return Ok(json!({"cancelled":true}));
        };
        let path = file
            .into_path()
            .map_err(|_| packages::invalid("文件位置无效"))?;
        // Review and install the same bytes even if the selected source is replaced.
        let staging = tempfile::tempdir().map_err(|_| packages::invalid("无法暂存主题包"))?;
        let frozen = staging.path().join("selected.aidetheme");
        let input = std::fs::File::open(&path).map_err(|_| packages::invalid("无法读取主题包"))?;
        let mut input = std::io::Read::take(input, 20 * 1024 * 1024 + 1);
        let mut output =
            std::fs::File::create(&frozen).map_err(|_| packages::invalid("无法暂存主题包"))?;
        if std::io::copy(&mut input, &mut output)
            .map_err(|_| packages::invalid("无法暂存主题包"))?
            > 20 * 1024 * 1024
        {
            return Err(packages::invalid("压缩包超过 20 MiB"));
        }
        drop(output);
        let path = frozen;
        let result = packages::install(&root, &path, false, true);
        let record = match result {
            Err(e) if e.code == "CONFLICT" => {
                if !app
                    .dialog()
                    .message(format!("{}\n相同 ID 不保证作者相同。确认替换？", e.message))
                    .title("替换主题")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::YesNo)
                    .blocking_show()
                {
                    return Ok(json!({"cancelled":true}));
                }
                let clear = app
                    .dialog()
                    .message("清空旧主题偏好？选择“否”会把旧偏好交给新主题。")
                    .title("主题偏好")
                    .buttons(MessageDialogButtons::YesNo)
                    .blocking_show();
                packages::install(&root, &path, true, clear)?
            }
            other => other?,
        };
        Ok(json!({"cancelled":false,"theme":record.manifest}))
    })
    .await
    .unwrap_or_else(|_| Err(packages::invalid("安装失败")));
    Ok(crate::respond(&service, result))
}
#[tauri::command]
pub async fn aide_theme_uninstall(
    window: WebviewWindow,
    service: State<'_, Service>,
    id: String,
    clear_storage: bool,
) -> Result<ApiResult<Value>, String> {
    if let Err(e) = allowed(&window) {
        return Ok(crate::respond::<Value>(&service, Err(e)));
    }
    let app = window.app_handle().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        runtime::uninstall(&app, &id, clear_storage)?;
        Ok(Value::Null)
    })
    .await
    .unwrap_or_else(|_| Err(packages::invalid("卸载失败")));
    Ok(crate::respond(&service, result))
}

#[tauri::command]
pub fn aide_theme_resume(window: WebviewWindow) -> Result<(), String> {
    if window.label() != "hud" {
        return Err("FORBIDDEN".into());
    }
    runtime::resume(window.app_handle());
    Ok(())
}
