#![cfg_attr(windows, windows_subsystem = "windows")]
//! Recovery process entry point. No browser or configuration activation occurs.
//! Launch hidden through the supervisor; paths are trusted local launch inputs,
//! never private IPC arguments or destinations obtained from journal content.
use clap::Parser;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(version, about = "cxweb runtime recovery service")]
struct Args {
    #[arg(long)]
    journal: PathBuf,
    #[arg(long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> ExitCode {
    let Ok(args) = Args::try_parse() else {
        return ExitCode::from(2);
    };
    if !args.journal.is_absolute() || !args.config.is_absolute() {
        return ExitCode::from(2);
    }
    #[cfg(windows)]
    {
        let Ok(host) = cxweb_runtime::host::Host::recover(&args.journal, &args.config) else {
            return ExitCode::from(3);
        };
        // No stdout/stderr metadata: the supervisor observes process exit and
        // the authenticated private protocol, never parses capability-bearing errors.
        match host.serve().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(4),
        }
    }
    #[cfg(not(windows))]
    ExitCode::from(5)
}
