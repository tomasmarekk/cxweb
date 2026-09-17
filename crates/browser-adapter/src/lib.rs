//! Fixed-operation browser adapter. No model-supplied scripts or cookie APIs.
#[cfg(windows)]
mod pipe;
pub mod turn;
#[cfg(windows)]
pub use pipe::{
    LoginObservation, ManagedBrowser, ManagedPage, ModelCandidate, ModelSurface,
    ModelSurfaceDiagnostic,
};
