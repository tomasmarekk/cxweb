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
