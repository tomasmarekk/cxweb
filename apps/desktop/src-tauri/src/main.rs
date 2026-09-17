#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod desktop {
    use cxweb_runtime::control::{Control, ControlStatus};
    use tauri::Manager;

    pub struct AppState {
        control: Result<Control, &'static str>,
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
    async fn status(state: tauri::State<'_, AppState>) -> Result<ControlStatus, String> {
        state
            .control
            .as_ref()
            .map_err(|e| e.to_string())?
            .status()
            .await
            .map_err(str::to_owned)
    }

    pub fn run() {
        tauri::Builder::default()
            .setup(|app| {
                app.manage(AppState {
                    control: Control::start(),
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
            .invoke_handler(tauri::generate_handler![connect, status])
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
