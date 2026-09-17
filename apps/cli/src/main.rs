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
    /// Prove private browser transport in a NEW dedicated test profile.
    BrowserProbe {
        #[arg(long)]
        browser: PathBuf,
        #[arg(long)]
        profile: PathBuf,
        #[arg(long, default_value_t = 0)]
        hold_seconds: u8,
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
    },
    /// Emit diagnostic model metadata for an isolated model_catalog_json.
    ProbeCatalog,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Command::BrowserProbe {
            browser,
            profile,
            hold_seconds,
        } => {
            std::fs::create_dir(&profile)?;
            #[cfg(windows)]
            {
                let mut browser =
                    cxweb_browser_adapter::ManagedBrowser::launch(&browser, &profile, false)?;
                let version = browser.version()?;
                print_json(
                    &serde_json::json!({"transport":"inherited_pipe", "pid":browser.pid(),"version":version["product"],"protocol":version["protocolVersion"],"login":"NOT RUN"}),
                );
                std::thread::sleep(std::time::Duration::from_secs(u64::from(hold_seconds)));
                browser.close()?;
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
        Command::Probe { output } => {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let state = cxweb_runtime::ProbeState::new(listener.local_addr()?.port());
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
