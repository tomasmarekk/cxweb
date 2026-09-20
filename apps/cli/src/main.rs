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
    /// Verify a fixed checkpoint and continuation in the installed browser. Uses ChatGPT allowance.
    RuntimeVerifyCompaction {
        #[arg(long)]
        installation: String,
        /// Reviewed native executable to exercise actual compaction transport.
        #[arg(long)]
        client: Option<std::path::PathBuf>,
        #[arg(long, requires = "client")]
        websocket: bool,
        /// Save one failed browser surface privately for local development inspection.
        #[arg(long, requires = "client")]
        capture_failure: bool,
    },
    /// Qualify every observed reasoning choice in the installed family. Uses ChatGPT allowance.
    RuntimeQualifyReasoning {
        #[arg(long)]
        installation: String,
    },
    /// Connect a verified background ChatGPT route to the selected Codex home.
    ConnectCodex {
        #[arg(long)]
        client: PathBuf,
        #[arg(long)]
        home: PathBuf,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        route: String,
    },
    /// Read installed runtime health through private IPC. Does not launch or probe anything.
    RuntimeHealth {
        #[arg(long)]
        installation: String,
    },
    /// Explicitly retry an eligible installed browser recovery without sending a message.
    RuntimeRetryWeb {
        #[arg(long)]
        installation: String,
    },
    /// Inventory native executable candidates without launching clients or reading credentials.
    NativeDiscover,
    /// Inspect a reviewed native backend's selected home/cwd without generating or changing configuration.
    NativePreflight {
        #[arg(long)]
        client: PathBuf,
        #[arg(long)]
        home: PathBuf,
        #[arg(long)]
        cwd: PathBuf,
    },
    /// Isolated real browser gateway. Close the cxweb runtime first. Uses ChatGPT allowance.
    LiveProbe {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        websocket: bool,
        #[arg(long)]
        client_build: String,
        #[arg(long)]
        coding: bool,
        #[arg(long)]
        compaction: bool,
    },
    /// Invoke a fixed operation through the same private runtime as the desktop.
    BrowserControl {
        #[arg(value_enum)]
        action: BrowserAction,
    },
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
        /// Test an off-screen native render surface in the fresh fixture profile.
        #[arg(long, conflicts_with = "open_login")]
        offscreen: bool,
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
        #[arg(long)]
        identity_output: Option<PathBuf>,
        #[arg(long)]
        websocket_output: Option<PathBuf>,
    },
    /// Emit diagnostic model metadata for an isolated model_catalog_json.
    ProbeCatalog {
        #[arg(long)]
        client_build: Option<String>,
        #[arg(long)]
        coding: bool,
        #[arg(long)]
        compaction: bool,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum BrowserAction {
    Background,
    Status,
    Connect,
    Refresh,
    Discover,
    TestText,
    TestTools,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Command::RuntimeVerifyCompaction {
            installation,
            client,
            websocket,
            capture_failure,
        } => {
            #[cfg(windows)]
            {
                let current = cxweb_runtime::installed_control::check(&installation).await?;
                let result = cxweb_runtime::installed_control::verify_compaction(
                    &installation,
                    current.instance,
                    client.map(|client| cxweb_runtime::native_probe::CheckpointTarget {
                        client,
                        websocket,
                        capture_failure,
                    }),
                )
                .await?;
                print_json(&serde_json::to_value(result.health)?);
            }
            #[cfg(not(windows))]
            {
                let _ = (installation, client, websocket, capture_failure);
                return Err("runtime verification requires Windows".into());
            }
        }
        Command::RuntimeQualifyReasoning { installation } => {
            #[cfg(windows)]
            {
                let current = cxweb_runtime::installed_control::check(&installation).await?;
                let result = cxweb_runtime::installed_control::qualify_reasoning(
                    &installation,
                    current.instance,
                )
                .await?;
                print_json(&serde_json::to_value(result.health)?);
            }
            #[cfg(not(windows))]
            {
                let _ = installation;
                return Err("runtime qualification requires Windows".into());
            }
        }
        Command::ConnectCodex {
            client,
            home,
            cwd,
            route,
        } => {
            #[cfg(windows)]
            print_json(&serde_json::to_value(
                cxweb_runtime::remote_control::RemoteControl::new()?
                    .activate(cxweb_runtime::setup_owner::ActivationTarget {
                        client,
                        home,
                        cwd,
                        route,
                    })
                    .await?,
            )?);
            #[cfg(not(windows))]
            {
                let _ = (client, home, cwd, route);
                return Err("Codex integration requires Windows".into());
            }
        }
        Command::RuntimeHealth { installation } => {
            #[cfg(windows)]
            {
                use cxweb_runtime::control_protocol::{Command, Reply, Request, exchange};
                let response = exchange(
                    &installation,
                    &Request {
                        version: 1,
                        command: Command::Health {},
                    },
                )
                .await?;
                let Reply::Health { health, .. } = response else {
                    return Err("E_RUNTIME_HEALTH_UNAVAILABLE".into());
                };
                print_json(&serde_json::to_value(health)?);
            }
            #[cfg(not(windows))]
            {
                let _ = installation;
                return Err("runtime health requires Windows".into());
            }
        }
        Command::RuntimeRetryWeb { installation } => {
            #[cfg(windows)]
            {
                let current = cxweb_runtime::installed_control::check(&installation).await?;
                let result =
                    cxweb_runtime::installed_control::retry_web(&installation, current.instance)
                        .await?;
                print_json(&serde_json::to_value(result.health)?);
            }
            #[cfg(not(windows))]
            {
                let _ = installation;
                return Err("runtime recovery requires Windows".into());
            }
        }
        Command::NativeDiscover => {
            #[cfg(windows)]
            print_json(&serde_json::to_value(
                cxweb_runtime::native_discovery::discover().await,
            )?);
            #[cfg(not(windows))]
            return Err("native discovery requires Windows".into());
        }
        Command::NativePreflight { client, home, cwd } => {
            #[cfg(windows)]
            print_json(&serde_json::to_value(
                cxweb_runtime::native_preflight::inspect(&client, &home, &cwd).await?,
            )?);
            #[cfg(not(windows))]
            {
                let _ = (client, home, cwd);
                return Err("native preflight requires Windows".into());
            }
        }
        Command::LiveProbe {
            output,
            websocket,
            client_build,
            coding,
            compaction,
        } => {
            let codec = cxweb_codex_adapter::catalog_codec::CatalogCodec::for_build(&client_build)
                .ok_or("E_CATALOG_CLIENT_BUILD")?;
            #[cfg(windows)]
            {
                let report = cxweb_runtime::live_probe::serve(&output, websocket, codec, coding, compaction, async {
                    use tokio::io::AsyncReadExt;
                    let mut stdin = tokio::io::stdin();
                    let mut byte = [0];
                    tokio::select! { _ = tokio::signal::ctrl_c() => (), _ = stdin.read(&mut byte) => () }
                }).await?;
                print_json(&report);
            }
            #[cfg(not(windows))]
            {
                let _ = (output, websocket, codec, coding, compaction);
                return Err("live browser qualification requires Windows".into());
            }
        }
        Command::BrowserControl { action } => {
            #[cfg(windows)]
            {
                let control = cxweb_runtime::remote_control::RemoteControl::new()?;
                let status = match action {
                    BrowserAction::Background => control.background().await?,
                    BrowserAction::Status => control.status(false).await?,
                    BrowserAction::Connect => control.connect().await?,
                    BrowserAction::Refresh => control.status(true).await?,
                    BrowserAction::Discover => control.qualify().await?,
                    BrowserAction::TestText => control.qualify_text().await?,
                    BrowserAction::TestTools => control.qualify_tools().await?,
                };
                // Fixed summary only: no account metadata, prompt or response body.
                print_json(&serde_json::json!({
                    "phase": status.phase,
                    "background_session": status.background_session,
                    "candidate_count": status.candidate_models.len(),
                    "candidate_models": status.candidate_models,
                    "temporary_chat_verified": status.temporary_chat_available,
                    "text_verified": status.text_qualified_model.is_some(),
                    "tool_protocol_verified": status.tool_qualified_model.is_some(),
                    "routing_installed": status.routing_installed,
                    "qualification_diagnostic": status.qualification_diagnostic,
                    "scope_diagnostic": status.scope_diagnostic,
                    "model_discovery_diagnostic": status.model_discovery_diagnostic,
                    "session_surface": status.observation.as_ref().map(|observation| serde_json::json!({
                        "official_page":observation.official_page,"composer":observation.composer,
                        "account_surface":observation.account_surface,"login_action":observation.login_action,
                        "verification_required":observation.verification_required,"document_ready":observation.document_ready
                    })),
                    "language": status.observation.as_ref().map(|observation| serde_json::json!({
                        "browser": observation.browser_language, "page": observation.page_language,
                    })),
                }));
            }
            #[cfg(not(windows))]
            {
                let _ = action;
                return Err("browser control requires Windows".into());
            }
        }
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
                        "--lang=en-US",
                        "--accept-lang=en-US,en",
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
            offscreen,
            login_idle_seconds,
        } => {
            if profile.exists() {
                return Err("browser probe requires a new profile directory".into());
            }
            #[cfg(windows)]
            {
                cxweb_platform::state::protected_directory(&profile)?;
                let mut browser = if offscreen {
                    cxweb_browser_adapter::ManagedBrowser::launch_offscreen(&browser, &profile)?
                } else {
                    cxweb_browser_adapter::ManagedBrowser::launch(&browser, &profile, open_login)?
                };
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
        Command::ProbeCatalog {
            client_build,
            coding,
            compaction,
        } => {
            let model = if let Some(build) = client_build {
                let codec = cxweb_codex_adapter::catalog_codec::CatalogCodec::for_build(&build)
                    .ok_or("E_CATALOG_CLIENT_BUILD")?;
                let route = cxweb_codex_adapter::catalog_codec::CatalogRoute {
                    id: "webbridge/diagnostic".into(),
                    observed_label: "Diagnostic".into(),
                    effort: "medium".into(),
                    reasoning: vec![],
                    coding,
                };
                if compaction {
                    codec.encode_with_context_budget(
                        &route,
                        cxweb_codex_adapter::context_budget::LocalContextBudget::DIAGNOSTIC,
                    )?
                } else {
                    codec.encode(&route)?
                }
            } else {
                if coding || compaction {
                    return Err("--coding and --compaction require --client-build".into());
                }
                cxweb_codex_adapter::catalog::synthetic_model()
            };
            print_json(&serde_json::json!({"models":[model]}));
        }
        Command::Probe {
            output,
            tools_output,
            identity_output,
            websocket_output,
        } => {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let mut state = cxweb_runtime::ProbeState::new(listener.local_addr()?.port());
            if let Some(path) = tools_output {
                state = state.with_registry_capture(path);
            }
            if let Some(path) = identity_output {
                state = state.with_identity_capture(path);
            }
            if let Some(path) = websocket_output {
                state = state.with_websocket_capture(path)?;
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
