use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "cxweb local integration diagnostics (development)")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compare manual login in the cxweb profile without CDP. Close cxweb first.
    ManualLoginProbe,
    /// Prove private browser transport in a NEW dedicated test profile.
    BrowserProbe {
        #[arg(long)]
        browser: PathBuf,
        #[arg(long)]
        profile: PathBuf,
        #[arg(long, default_value_t = 0)]
        hold_seconds: u8,
        #[arg(long)]
        open_login: bool,
        /// Leave the page idle before inspecting it, to reproduce manual login startup.
        #[arg(long, default_value_t = 0, requires = "open_login")]
        login_idle_seconds: u8,
    },
    /// Inspect one explicitly selected configuration without changing it.
    Inspect {
        #[arg(long)]
        config: PathBuf,
    },
    /// Run an isolated synthetic loopback server. Never connect a real account.
    Probe {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        tools_output: Option<PathBuf>,
    },
    /// Emit diagnostic model metadata for an isolated model_catalog_json.
    ProbeCatalog,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Command::ManualLoginProbe => {
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let paths = cxweb_platform::state::StatePaths::open()?;
                let _lock = paths.lock().map_err(|_| {
                    std::io::Error::other(
                        "Close cxweb before the manual login comparison. No browser was launched.",
                    )
                })?;
                let executable = cxweb_platform::state::installed_browser()?;
                // Diagnostic only: ordinary browser navigation, no CDP or login
                // automation, no cookie access and no personal-profile copying.
                // Keep the installation lock until this browser process exits.
                let mut browser = std::process::Command::new(executable)
                    .arg(format!("--user-data-dir={}", paths.profile.display()))
                    .args([
                        "--no-first-run",
                        "--no-default-browser-check",
                        "https://chatgpt.com/",
                    ])
                    .creation_flags(0x08000000) // CREATE_NO_WINDOW hides console, not browser UI.
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()?;
                print_json(&serde_json::json!({
                    "mode":"manual_login_comparison", "cdp":false,
                    "profile":"existing_cxweb_profile", "pid":browser.id(),
                    "instruction":"Complete login manually, then close this browser before restarting cxweb. Keep this diagnostic process running."
                }));
                let status = browser.wait()?;
                print_json(
                    &serde_json::json!({"browser_exited":true,"success":status.success(),"authentication_verified":false}),
                );
            }
            #[cfg(not(windows))]
            return Err("manual login comparison is only available on Windows".into());
        }
        Command::BrowserProbe {
            browser,
            profile,
            hold_seconds,
            open_login,
            login_idle_seconds,
        } => {
            if profile.exists() {
                return Err("browser probe requires a new profile directory".into());
            }
            #[cfg(windows)]
            {
                cxweb_platform::state::protected_directory(&profile)?;
                let mut browser =
                    cxweb_browser_adapter::ManagedBrowser::launch(&browser, &profile, false)?;
                let version = browser.version()?;
                let startup = browser.probe_startup_page()?;
                let dom = browser.probe_dom()?;
                let login_page = if open_login {
                    let page = browser.open_login()?;
                    std::thread::sleep(std::time::Duration::from_secs(u64::from(
                        login_idle_seconds,
                    )));
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                    let mut observation;
                    loop {
                        observation = browser.login_observation(&page)?;
                        if observation.composer
                            || observation.login_action
                            || std::time::Instant::now() >= deadline
                        {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    Some(observation)
                } else {
                    None
                };
                print_json(
                    &serde_json::json!({"transport":"inherited_pipe", "pid":browser.pid(),"version":version["product"],"protocol":version["protocolVersion"],"login":"NOT RUN","login_page":login_page,"startup":startup,"dom":dom}),
                );
                std::thread::sleep(std::time::Duration::from_secs(u64::from(hold_seconds)));
                browser.close()?;
                if login_page.is_some_and(|page| !page.composer && !page.login_action) {
                    return Err("E_LOGIN_PAGE_NOT_READY: no usable ChatGPT interface within the probe deadline".into());
                }
            }
            #[cfg(not(windows))]
            {
                let _ = browser;
                return Err("browser transport not qualified on this OS".into());
            }
        }
        Command::Inspect { config } => {
            let text = std::fs::read_to_string(config)?;
            let result = cxweb_codex_adapter::config::inspect(&text)?;
            print_json(&serde_json::to_value(result)?);
        }
        Command::ProbeCatalog => print_json(
            &serde_json::json!({"models":[cxweb_codex_adapter::catalog::synthetic_model()]}),
        ),
        Command::Probe {
            output,
            tools_output,
        } => {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let mut state = cxweb_runtime::ProbeState::new(listener.local_addr()?.port());
            if let Some(path) = tools_output {
                state = state.with_registry_capture(path);
            }
            // The diagnostic descriptor is exclusively created in a caller-owned
            // test directory; it contains no account data or native credentials.
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            use std::io::Write;
            file.write_all(serde_json::json!({"base_url": state.base_url(), "pid":std::process::id(),"synthetic":true}).to_string().as_bytes())?;
            file.sync_all()?;
            axum::serve(listener, cxweb_runtime::diagnostic_router(state))
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
    }
    Ok(())
}

#[allow(clippy::print_stdout)]
fn print_json(value: &serde_json::Value) {
    println!("{value}");
}
