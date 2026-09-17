//! Fixed-operation browser adapter. No model-supplied scripts or cookie APIs.
#[cfg(windows)]
mod pipe;
#[cfg(windows)]
pub use pipe::ManagedBrowser;
