#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod desktop {
    use cxweb_runtime::{control::ControlStatus, remote_control::RemoteControl};
    use tauri::Manager;

    pub struct AppState {
        control: Result<RemoteControl, &'static str>,
    }

    #[tauri::command]
    async fn activate_codex(
        state: tauri::State<'_, AppState>,
        client: std::path::PathBuf,
        home: std::path::PathBuf,
        cwd: std::path::PathBuf,
        route: String,
    ) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .activate(cxweb_runtime::setup_owner::ActivationTarget {
                client,
                home,
                cwd,
                route,
            })
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn installed_list() -> Result<cxweb_runtime::installed_control::Inventory, String> {
        cxweb_runtime::installed_control::list()
            .await
            .map_err(str::to_owned)
    }
    #[tauri::command]
    async fn installed_check(
        installation: String,
    ) -> Result<cxweb_runtime::installed_control::Snapshot, String> {
        cxweb_runtime::installed_control::check(&installation)
            .await
            .map_err(str::to_owned)
    }
    #[tauri::command]
    async fn installed_retry_web(
        installation: String,
        instance: String,
    ) -> Result<cxweb_runtime::installed_control::Snapshot, String> {
        cxweb_runtime::installed_control::retry_web(&installation, instance)
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn installed_web_login(
        installation: String,
        instance: String,
        finish: bool,
    ) -> Result<cxweb_runtime::installed_control::Snapshot, String> {
        cxweb_runtime::installed_control::web_login(&installation, instance, finish)
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn installed_qualify_reasoning(
        installation: String,
        instance: String,
    ) -> Result<cxweb_runtime::installed_control::Snapshot, String> {
        cxweb_runtime::installed_control::qualify_reasoning(&installation, instance)
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn installed_disconnect(
        installation: String,
        instance: String,
        allow_active: bool,
    ) -> Result<cxweb_runtime::installed_control::Snapshot, String> {
        cxweb_runtime::installed_control::disconnect(&installation, instance, allow_active)
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn native_discover() -> cxweb_runtime::native_discovery::Report {
        cxweb_runtime::native_discovery::discover().await
    }

    #[tauri::command]
    async fn native_preflight(
        client: std::path::PathBuf,
        home: std::path::PathBuf,
        cwd: std::path::PathBuf,
    ) -> Result<cxweb_runtime::native_preflight::Report, String> {
        cxweb_runtime::native_preflight::inspect(&client, &home, &cwd)
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn native_cancel(
        state: tauri::State<'_, AppState>,
        instance: String,
        operation: String,
    ) -> Result<(), String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .cancel_native(instance, operation)
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn reset_test(state: tauri::State<'_, AppState>) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .reset_test()
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn native_text(
        state: tauri::State<'_, AppState>,
        client: std::path::PathBuf,
        home: std::path::PathBuf,
        cwd: std::path::PathBuf,
        route: String,
        exercise: Option<cxweb_runtime::native_probe::Exercise>,
    ) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .native_text(cxweb_runtime::setup_owner::NativeTarget {
                client,
                home,
                cwd,
                route,
                exercise: exercise.unwrap_or_default(),
            })
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn connect(state: tauri::State<'_, AppState>) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .connect()
            .await
            .map_err(str::to_owned)
    }
    #[tauri::command]
    async fn status(
        state: tauri::State<'_, AppState>,
        refresh: bool,
    ) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .status(refresh)
            .await
            .map_err(str::to_owned)
    }
    #[tauri::command]
    async fn qualify(state: tauri::State<'_, AppState>) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .qualify()
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn qualify_text(state: tauri::State<'_, AppState>) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .qualify_text()
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn qualify_tools(state: tauri::State<'_, AppState>) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .qualify_tools()
            .await
            .map_err(str::to_owned)
    }

    #[tauri::command]
    async fn background(state: tauri::State<'_, AppState>) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .background()
            .await
            .map_err(str::to_owned)
    }

    pub fn run() {
        tauri::Builder::default()
            .setup(|app| {
                app.manage(AppState {
                    control: RemoteControl::new(),
                });
                tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("index.html".into()),
                )
                .title("cxweb · development preview")
                .inner_size(440.0, 540.0)
                .min_inner_size(360.0, 440.0)
                .on_navigation(|url| {
                    url.scheme() == "tauri"
                        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
                })
                .build()?;
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
                activate_codex,
                connect,
                status,
                qualify,
                qualify_text,
                qualify_tools,
                background,
                native_discover,
                native_preflight,
                native_text,
                native_cancel,
                reset_test,
                installed_list,
                installed_check,
                installed_disconnect,
                installed_qualify_reasoning,
                installed_retry_web,
                installed_web_login
            ])
            .run(tauri::generate_context!())
            .expect("cxweb desktop runtime");
    }
}

fn main() {
    #[cfg(windows)]
    desktop::run();
    #[cfg(not(windows))]
    compile_error!(
        "Desktop/browser integration is currently qualified only for Windows development."
    );
}
