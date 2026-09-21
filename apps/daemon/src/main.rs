#![cfg_attr(windows, windows_subsystem = "windows")]
//! Recovery or login runtime entry point. Login mode opens the browser only
//! after an explicit private control request; recovery does not activate config.
//! Launch hidden through the supervisor; paths are trusted local launch inputs,
//! never private IPC arguments or destinations obtained from journal content.
use clap::Parser;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(version, about = "cxweb runtime service")]
struct Args {
    #[arg(long, conflicts_with_all = ["journal", "config", "web_tools"])]
    login_runtime: bool,
    #[arg(long, conflicts_with_all = ["journal", "config", "login_runtime"])]
    web_tools: bool,
    #[arg(long, required_unless_present_any = ["login_runtime", "web_tools"])]
    journal: Option<PathBuf>,
    #[arg(long, required_unless_present_any = ["login_runtime", "web_tools"])]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let Ok(args) = Args::try_parse() else {
        return ExitCode::from(2);
    };
    if args.web_tools {
        return match cxweb_runtime::web_tools::serve().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(3),
        };
    }
    #[cfg(windows)]
    {
        if args.login_runtime {
            return match cxweb_runtime::remote_control::serve_login().await {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::from(3),
            };
        }
        let (Some(journal), Some(config)) = (args.journal, args.config) else {
            return ExitCode::from(2);
        };
        if !journal.is_absolute() || !config.is_absolute() {
            return ExitCode::from(2);
        }
        let Ok(host) = cxweb_runtime::host::Host::recover(&journal, &config) else {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mcp_mode_cannot_be_combined_with_browser_or_host_startup() {
        assert!(
            Args::try_parse_from(["cxweb-daemon", "--web-tools"])
                .unwrap()
                .web_tools
        );
        assert!(Args::try_parse_from(["cxweb-daemon", "--web-tools", "--login-runtime"]).is_err());
        for option in ["--journal", "--config"] {
            assert!(
                Args::try_parse_from(["cxweb-daemon", "--web-tools", option, "fixture"]).is_err()
            );
        }
        assert!(Args::try_parse_from(["cxweb-daemon"]).is_err());
    }
}
