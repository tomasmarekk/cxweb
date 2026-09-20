//! Passive subscription transport observations. Never retains request data.
use axum::http::StatusCode;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Received,
    Connected,
    AuthRequired,
    Forbidden,
    RateLimited,
    ServerError,
    RequestRejected,
    TransportError,
    Redirect,
    StreamError,
}

impl Outcome {
    pub(crate) fn status(status: StatusCode) -> Self {
        match status {
            StatusCode::UNAUTHORIZED => Self::AuthRequired,
            StatusCode::FORBIDDEN => Self::Forbidden,
            StatusCode::TOO_MANY_REQUESTS => Self::RateLimited,
            StatusCode::NOT_MODIFIED => Self::Received,
            value if value.is_success() => Self::Received,
            value if value.is_server_error() => Self::ServerError,
            value if value.is_redirection() => Self::Redirect,
            _ => Self::RequestRejected,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Observation {
    pub outcome: Outcome,
    pub observed_at: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct Tracker {
    next: Arc<AtomicU64>,
    latest: Arc<Mutex<Option<(u64, Observation)>>>,
}
impl Tracker {
    pub(crate) fn begin(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
    pub(crate) fn observe(&self, sequence: u64, outcome: Outcome) {
        if let Ok(mut latest) = self.latest.lock()
            && latest
                .as_ref()
                .is_none_or(|(previous, _)| sequence >= *previous)
        {
            *latest = Some((
                sequence,
                Observation {
                    outcome,
                    observed_at: cxweb_platform::clock::utc_timestamp(),
                },
            ));
        }
    }
    pub(crate) fn snapshot(&self) -> Option<Observation> {
        self.latest
            .lock()
            .ok()?
            .as_ref()
            .map(|(_, observation)| observation.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_response_cannot_erase_a_newer_authentication_failure() {
        let tracker = Tracker::default();
        assert_eq!(tracker.snapshot(), None);
        let older = tracker.begin();
        let newer = tracker.begin();
        tracker.observe(newer, Outcome::AuthRequired);
        let failure = tracker.snapshot();
        tracker.observe(older, Outcome::Received);
        assert_eq!(tracker.snapshot(), failure);
        tracker.observe(tracker.begin(), Outcome::Received);
        assert_eq!(tracker.snapshot().unwrap().outcome, Outcome::Received);
    }
}
