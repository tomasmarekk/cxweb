fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "activate_codex",
            "connect",
            "status",
            "qualify",
            "qualify_text",
            "qualify_tools",
            "background",
            "native_discover",
            "native_preflight",
            "native_text",
            "native_cancel",
            "reset_test",
            "installed_list",
            "installed_check",
            "installed_disconnect",
            "installed_retry_web",
            "installed_qualify_reasoning",
        ]),
    ))
    .expect("Tauri build configuration");
}
