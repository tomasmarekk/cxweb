fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "connect",
            "status",
            "qualify",
            "qualify_text",
            "qualify_tools",
            "background",
            "native_discover",
            "native_preflight",
            "native_text",
        ]),
    ))
    .expect("Tauri build configuration");
}
