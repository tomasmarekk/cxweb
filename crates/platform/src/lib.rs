//! Audited OS-only boundary. The application crates forbid unsafe code.
#[cfg(windows)]
pub mod browser_process;
#[cfg(windows)]
pub mod state;
