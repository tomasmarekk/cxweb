//! Audited OS-only boundary. The application crates forbid unsafe code.
#[cfg(windows)]
pub mod atomic_file;
#[cfg(windows)]
pub mod browser_process;
#[cfg(windows)]
mod browser_window;
#[cfg(windows)]
pub mod child_job;
#[cfg(windows)]
mod config_access;
#[cfg(windows)]
pub mod control_pipe;
#[cfg(windows)]
pub mod loopback;
#[cfg(windows)]
pub mod scheduled_runtime;
#[cfg(windows)]
pub mod secret;
#[cfg(windows)]
pub mod state;
#[cfg(windows)]
pub mod target_path;
