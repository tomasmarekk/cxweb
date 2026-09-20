//! Transport-independent identities and irreversible browser submission states.
pub mod health;
use serde::{Deserialize, Serialize};

pub const OWNED_MODEL_PREFIX: &str = "webbridge/";

/// Every dimension participates in session isolation. Account/workspace values
/// are opaque scope hashes; this structure must not be printed in diagnostics.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionKey {
    pub installation: String,
    pub native_session: String,
    pub account_scope: String,
    pub workspace_scope: String,
    pub route: String,
    pub epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Prepared,
    ObservedBaseline,
    Submitting,
    Submitted,
    Generating,
    Completed,
    Failed,
    Cancelled,
    SubmissionUncertain,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid turn state transition")]
pub struct InvalidTransition;

impl TurnState {
    pub fn transition(&mut self, next: Self) -> Result<(), InvalidTransition> {
        use TurnState::*;
        let valid = matches!(
            (*self, next),
            (Prepared, ObservedBaseline | Cancelled | Failed)
                | (ObservedBaseline, Submitting | Cancelled | Failed)
                // Failed is permitted only with positive evidence that Send
                // was not clicked. Timeouts still require SubmissionUncertain.
                | (Submitting, Submitted | Failed | SubmissionUncertain)
                | (Submitted, Generating | Completed | Failed | Cancelled)
                | (Generating, Completed | Failed | Cancelled)
        );
        if !valid {
            return Err(InvalidTransition);
        }
        *self = next;
        Ok(())
    }

    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::SubmissionUncertain
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncertain_submission_cannot_be_replayed_or_cancelled_as_unsent() {
        let mut state = TurnState::Submitting;
        assert_eq!(
            state.transition(TurnState::Cancelled),
            Err(InvalidTransition)
        );
        state.transition(TurnState::SubmissionUncertain).unwrap();
        assert!(state.terminal());
        assert_eq!(
            state.transition(TurnState::Submitting),
            Err(InvalidTransition)
        );
        assert_eq!(
            state.transition(TurnState::Completed),
            Err(InvalidTransition)
        );
    }

    #[test]
    fn cancellation_ignores_late_completion() {
        let mut state = TurnState::Generating;
        state.transition(TurnState::Cancelled).unwrap();
        assert_eq!(
            state.transition(TurnState::Completed),
            Err(InvalidTransition)
        );
    }
}
