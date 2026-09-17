fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["connect", "status", "qualify", "qualify_text"]),
    ))
    .expect("Tauri build configuration");
}
