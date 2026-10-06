fn main() {
    // Link one manifest for both the application and native-dialog test executables.
    // Embedding it again in the resource library creates duplicate manifest resources.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    }
    static COMMANDS: &[&str] = &[
        "aide_usb_displays",
        "hud_v1_capabilities_get",
        "hud_v1_accounts_list",
        "hud_v1_accounts_selection_update",
        "hud_v1_accounts_alias_update",
        "hud_internal_docs_open",
        "hud_internal_update_check",
        "hud_internal_update_open",
        "hud_v1_recommendation_get",
        "hud_v1_refresh_request",
        "hud_v1_refresh_status_get",
        "hud_v1_settings_get",
        "hud_v1_settings_update",
        "aide_theme_builtins",
        "hud_v1_window_control",
        "hud_v1_diagnostics_get",
        "hud_internal_sources_get",
        "hud_internal_sources_save",
        "hud_internal_sources_pick",
        "hud_internal_startup",
        "hud_internal_source_get",
        "hud_internal_source_choose",
        "hud_internal_source_rescan",
        "aide_theme_builtin",
        "hud_internal_window_layout",
        "aide_hud_context_menu",
        "aide_theme_list",
        "aide_theme_data",
        "aide_theme_install",
        "aide_theme_select",
        "aide_theme_uninstall",
        "aide_theme_resume",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("Unable to prepare Tauri application");
}
