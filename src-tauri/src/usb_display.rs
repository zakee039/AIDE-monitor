pub const THEMES: [&str; 3] = ["usb-mint", "usb-day", "usb-quad"];

use crate::{model::ApiError, service::Service};
use serde::Serialize;
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayDevice {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

// EDD_GET_DEVICE_INTERFACE_NAME returns the monitor interface path, not the
// transient DISPLAY1/2 number (which changes when a display is disconnected).
#[cfg(windows)]
fn identity(name: &str) -> Option<(String, String)> {
    use windows::{
        core::PCWSTR,
        Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW},
    };
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut device = DISPLAY_DEVICEW::default();
    device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    if !unsafe { EnumDisplayDevicesW(PCWSTR(name.as_ptr()), 0, &mut device, 1) }.as_bool() {
        return None;
    }
    let text = |v: &[u16]| {
        String::from_utf16_lossy(&v[..v.iter().position(|c| *c == 0).unwrap_or(v.len())])
    };
    let id = text(&device.DeviceID);
    if id.is_empty() {
        None
    } else {
        Some((id, text(&device.DeviceString)))
    }
}
#[cfg(not(windows))]
fn identity(_name: &str) -> Option<(String, String)> {
    None
}

pub fn devices(app: &tauri::AppHandle) -> Result<Vec<DisplayDevice>, ApiError> {
    let monitors = app
        .available_monitors()
        .map_err(|_| ApiError::new("IO_ERROR", "无法枚举屏幕"))?;
    Ok(monitors
        .into_iter()
        .filter_map(|m| {
            let (id, name) = identity(m.name()?.as_str())?;
            Some(DisplayDevice {
                id,
                name,
                width: m.size().width,
                height: m.size().height,
                x: m.position().x,
                y: m.position().y,
            })
        })
        .collect())
}
#[tauri::command]
pub fn aide_usb_displays(window: WebviewWindow) -> Result<Vec<DisplayDevice>, String> {
    if window.label() != "settings" {
        return Err("FORBIDDEN".into());
    }
    devices(window.app_handle()).map_err(|e| e.message)
}

fn remembered_device(devices: Vec<DisplayDevice>, id: &str) -> Option<DisplayDevice> {
    let mut matches = devices.into_iter().filter(|d| !id.is_empty() && d.id == id);
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}

pub fn sync(app: &tauri::AppHandle) {
    let settings = app.state::<Service>().settings().usb_display;
    let target = if settings.enabled {
        devices(app)
            .ok()
            .and_then(|v| remembered_device(v, &settings.device_id))
    } else {
        None
    };
    let Some(device) = target else {
        if let Some(w) = app.get_webview_window("usb-display") {
            let _ = w.hide();
        }
        return;
    };
    let window = if let Some(w) = app.get_webview_window("usb-display") {
        w
    } else {
        let Ok(w) = WebviewWindowBuilder::new(
            app,
            "usb-display",
            WebviewUrl::App("index.html?view=usb".into()),
        )
        .title("AIDE monitor · 独立监视屏")
        .decorations(false)
        .shadow(false)
        .maximizable(false)
        .minimizable(false)
        .resizable(false)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .focusable(false)
        .always_on_top(true)
        .build() else {
            return;
        };
        w
    };
    let pos = tauri::PhysicalPosition::new(device.x, device.y);
    let size = tauri::PhysicalSize::new(device.width, device.height);
    if window.outer_position().ok() != Some(pos) || window.inner_size().ok() != Some(size) {
        let _ = window.hide();
        if window
            .set_position(pos)
            .and_then(|_| window.set_size(size))
            .is_err()
        {
            return;
        }
    }
    // Re-enumerate immediately before showing. Never substitute another screen.
    if devices(app).is_ok_and(|v| {
        v.iter()
            .any(|d| d.id == device.id && d.x == device.x && d.y == device.y)
    }) {
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
    } else {
        let _ = window.hide();
    }
}
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || sync(&handle));
            tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn display(id: &str, x: i32) -> DisplayDevice {
        DisplayDevice {
            id: id.into(),
            name: "USB monitor".into(),
            width: 800,
            height: 480,
            x,
            y: 0,
        }
    }
    #[test]
    fn disconnect_never_falls_back_and_reconnect_uses_new_coordinates() {
        assert!(remembered_device(vec![display("other", 0)], "saved").is_none());
        assert!(remembered_device(vec![], "saved").is_none());
        assert_eq!(
            remembered_device(vec![display("other", 0), display("saved", -800)], "saved")
                .unwrap()
                .x,
            -800
        );
        assert!(
            remembered_device(vec![display("saved", 0), display("saved", 800)], "saved").is_none()
        );
    }
}

pub fn valid_theme(theme: &crate::model::UsbThemeDefinition) -> bool {
    let keys = ["background", "text", "textMuted", "border", "success"];
    theme.kind == "aide-usb-theme" && theme.version == 1
        && theme.width == 320 && theme.height == 170
        && theme.id.starts_with("usb-custom-") && theme.id.len() <= 80
        && theme.id.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && !theme.name.trim().is_empty() && theme.name.chars().count() <= 40
        && !theme.name.chars().any(char::is_control)
        && matches!(theme.layout.as_str(), "rows" | "columns")
        && theme.palette.len() == keys.len()
        && keys.iter().all(|key| theme.palette.get(*key).is_some_and(|v|
            v.len() == 7 && v.starts_with('#') && v.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)))
}
pub fn valid_settings(settings: &crate::model::UsbDisplaySettings) -> bool {
    let mut ids = std::collections::HashSet::new();
    settings.custom_themes.len() <= 20
        && settings.custom_themes.iter().all(|theme| valid_theme(theme) && ids.insert(&theme.id))
        && (THEMES.contains(&settings.theme_id.as_str()) || settings.custom_themes.iter().any(|t| t.id == settings.theme_id))
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    #[test]
    fn usb_import_rejects_desktop_documents_and_css_and_preserves_custom_theme() {
        let theme: crate::model::UsbThemeDefinition = serde_json::from_str(include_str!("../../examples/usb-themes/sage.json")).unwrap();
        assert!(valid_theme(&theme));
        let mut settings = crate::model::UsbDisplaySettings::default();
        settings.theme_id = theme.id.clone();
        settings.custom_themes.push(theme.clone());
        assert!(valid_settings(&settings));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut config = crate::config::Config::default();
        config.settings.usb_display = settings.clone();
        crate::config::atomic_json(&path, &config).unwrap();
        assert_eq!(crate::config::load(&path).unwrap().settings.usb_display, settings);
        settings.custom_themes.push(theme.clone());
        assert!(!valid_settings(&settings));
        let mut invalid = theme.clone();
        invalid.palette.insert("background".into(), "url(https://example.com)".into());
        assert!(!valid_theme(&invalid));
        invalid = theme.clone(); invalid.id = "default".into();
        assert!(!valid_theme(&invalid));
        invalid = theme; invalid.width = 640;
        assert!(!valid_theme(&invalid));
        assert!(serde_json::from_str::<crate::model::UsbThemeDefinition>(include_str!("../../examples/themes/cream/theme.json")).is_err());
    }
}
