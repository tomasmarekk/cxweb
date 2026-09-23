//! Turn attribution is based on logical IDs and submission evidence, not DOM position.
use cxweb_domain::TurnState;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub ids: Vec<String>,
    pub selected_model: String,
    pub composer_empty: bool,
    pub generating: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub user_id: Option<String>,
    pub user_matches: bool,
    pub assistant_id: Option<String>,
    pub text: String,
    /// Public, rendered ChatGPT progress only. Never private model state.
    #[serde(default)]
    pub summary: Vec<String>,
    pub generating: bool,
    #[serde(default)]
    pub generation_failed: bool,
    pub completion_control: bool,
    pub fenced_output: bool,
    pub selected_model: String,
    pub ambiguous: bool,
}

pub enum Progress {
    AwaitingAcknowledgement,
    Generating,
    Completed(String),
}
pub struct TurnTracker {
    state: TurnState,
    baseline: HashSet<String>,
    model: String,
    user: Option<String>,
    assistant: Option<String>,
    committed_prefix: String,
}

impl TurnTracker {
    pub fn new(baseline: Baseline, expected_model: &str) -> Result<Self, &'static str> {
        if !baseline.composer_empty || baseline.generating {
            return Err("E_BROWSER_BUSY");
        }
        if baseline.selected_model != expected_model || expected_model.is_empty() {
            return Err("E_MODEL_FIDELITY");
        }
        let ids: HashSet<_> = baseline.ids.iter().cloned().collect();
        if ids.len() != baseline.ids.len() || ids.iter().any(String::is_empty) {
            return Err("E_TURN_ATTRIBUTION");
        }
        let mut state = TurnState::Prepared;
        state
            .transition(TurnState::ObservedBaseline)
            .map_err(|_| "E_TURN_STATE")?;
        Ok(Self {
            state,
            baseline: ids,
            model: expected_model.to_owned(),
            user: None,
            assistant: None,
            committed_prefix: String::new(),
        })
    }
    pub fn state(&self) -> TurnState {
        self.state
    }
    /// Call only after durably recording submission intent and before pressing Send.
    pub fn begin_submission(&mut self) -> Result<(), &'static str> {
        self.state
            .transition(TurnState::Submitting)
            .map_err(|_| "E_TURN_STATE")
    }
    pub fn observe(&mut self, observation: Observation) -> Result<Progress, &'static str> {
        if self.state.terminal() {
            return Err("E_TURN_TERMINAL");
        }
        if !matches!(
            self.state,
            TurnState::Submitting | TurnState::Submitted | TurnState::Generating
        ) {
            return Err("E_TURN_STATE");
        }
        if observation.ambiguous {
            return self.fail("E_TURN_AMBIGUOUS");
        }
        if observation.selected_model != self.model {
            return self.fail("E_MODEL_FIDELITY");
        }
        let Some(user) = observation.user_id else {
            return Ok(Progress::AwaitingAcknowledgement);
        };
        if !observation.user_matches {
            return self.fail("E_USER_MESSAGE_MISMATCH");
        }
        if user.is_empty()
            || self.baseline.contains(&user)
            || self.user.as_ref().is_some_and(|id| id != &user)
        {
            return self.fail("E_TURN_ATTRIBUTION");
        }
        self.user = Some(user);
        if self.state == TurnState::Submitting {
            self.state
                .transition(TurnState::Submitted)
                .map_err(|_| "E_TURN_STATE")?;
        }
        if observation.generation_failed {
            return self.fail("E_CHATGPT_THINKING_FAILED");
        }
        let Some(assistant) = observation.assistant_id else {
            return Ok(Progress::Generating);
        };
        if assistant.is_empty()
            || self.user.as_ref() == Some(&assistant)
            || self.baseline.contains(&assistant)
            || self.assistant.as_ref().is_some_and(|id| id != &assistant)
        {
            return self.fail("E_TURN_ATTRIBUTION");
        }
        self.assistant = Some(assistant);
        if self.state == TurnState::Submitted {
            self.state
                .transition(TurnState::Generating)
                .map_err(|_| "E_TURN_STATE")?;
        }
        if !observation.text.starts_with(&self.committed_prefix) {
            return self.fail("E_STREAM_REVISION");
        }
        if observation.completion_control && !observation.generating {
            // Intermediate rendering is not a completed protocol reply. Refuse
            // ambiguous fenced output without cancelling an in-progress answer.
            if observation.fenced_output {
                return self.fail("E_TOOL_ENVELOPE_FENCED");
            }
            self.state
                .transition(TurnState::Completed)
                .map_err(|_| "E_TURN_STATE")?;
            return Ok(Progress::Completed(observation.text));
        }
        Ok(Progress::Generating)
    }
    /// Used only by a separately qualified live streaming route.
    pub fn commit_prefix(&mut self, text: &str) -> Result<(), &'static str> {
        if self.state != TurnState::Generating || !text.starts_with(&self.committed_prefix) {
            return Err("E_STREAM_REVISION");
        }
        self.committed_prefix = text.to_owned();
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<(), &'static str> {
        let next = if self.state == TurnState::Submitting {
            TurnState::SubmissionUncertain
        } else {
            TurnState::Cancelled
        };
        self.state.transition(next).map_err(|_| "E_TURN_STATE")
    }
    pub fn fail<T>(&mut self, code: &'static str) -> Result<T, &'static str> {
        let next = if self.state == TurnState::Submitting {
            TurnState::SubmissionUncertain
        } else {
            TurnState::Failed
        };
        self.state.transition(next).map_err(|_| "E_TURN_STATE")?;
        Err(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tracker() -> TurnTracker {
        let mut tracker = TurnTracker::new(
            Baseline {
                ids: vec!["old".into()],
                selected_model: "observed".into(),
                composer_empty: true,
                generating: false,
            },
            "observed",
        )
        .unwrap();
        tracker.begin_submission().unwrap();
        tracker
    }
    fn observation() -> Observation {
        Observation {
            user_id: Some("user-new".into()),
            user_matches: true,
            assistant_id: Some("answer-new".into()),
            text: "answer".into(),
            summary: vec![],
            generating: false,
            generation_failed: false,
            completion_control: true,
            fenced_output: false,
            selected_model: "observed".into(),
            ambiguous: false,
        }
    }
    #[test]
    fn only_attributed_new_message_with_completion_evidence_completes() {
        let mut tracker = tracker();
        let mut o = observation();
        o.completion_control = false;
        assert!(matches!(tracker.observe(o).unwrap(), Progress::Generating));
        assert!(
            matches!(tracker.observe(observation()).unwrap(), Progress::Completed(text) if text == "answer")
        );
        assert!(tracker.observe(observation()).is_err());
    }
    #[test]
    fn explicit_thinking_failure_ends_only_an_attributed_submission() {
        let mut failed = observation();
        failed.assistant_id = None;
        failed.completion_control = false;
        failed.generation_failed = true;
        let mut current = tracker();
        assert_eq!(
            current.observe(failed.clone()).err(),
            Some("E_CHATGPT_THINKING_FAILED")
        );
        assert_eq!(current.state(), TurnState::Failed);
        failed.user_matches = false;
        assert_eq!(
            tracker().observe(failed).err(),
            Some("E_USER_MESSAGE_MISMATCH")
        );
    }
    #[test]
    fn historical_and_replaced_assistant_ids_are_rejected() {
        let mut historical = observation();
        historical.assistant_id = Some("old".into());
        assert!(tracker().observe(historical).is_err());
        let mut tracker = tracker();
        let mut o = observation();
        o.generating = true;
        tracker.observe(o.clone()).unwrap();
        o.assistant_id = Some("regenerated".into());
        assert!(tracker.observe(o).is_err());
    }
    #[test]
    fn attribution_failures_identify_the_failed_check_without_accepting_output() {
        for (code, observation) in [
            (
                "E_TURN_AMBIGUOUS",
                Observation {
                    ambiguous: true,
                    ..observation()
                },
            ),
            (
                "E_MODEL_FIDELITY",
                Observation {
                    selected_model: "changed".into(),
                    ..observation()
                },
            ),
            (
                "E_USER_MESSAGE_MISMATCH",
                Observation {
                    user_matches: false,
                    ..observation()
                },
            ),
            (
                "E_TOOL_ENVELOPE_FENCED",
                Observation {
                    fenced_output: true,
                    ..observation()
                },
            ),
        ] {
            let mut tracker = tracker();
            assert_eq!(tracker.observe(observation).err(), Some(code));
            assert!(tracker.state().terminal());
            assert!(tracker.begin_submission().is_err());
        }
    }

    #[test]
    fn cancelling_unknown_submission_does_not_claim_unsent() {
        let mut tracker = tracker();
        tracker.cancel().unwrap();
        assert_eq!(tracker.state(), TurnState::SubmissionUncertain);
        assert!(tracker.begin_submission().is_err());
        assert!(tracker.observe(observation()).is_err());
    }
    #[test]
    fn intermediate_rendering_is_not_validated_as_a_final_envelope() {
        let mut tracker = tracker();
        let mut intermediate = observation();
        intermediate.generating = true;
        intermediate.completion_control = false;
        intermediate.fenced_output = true;
        intermediate.text = "unfinished intermediate rendering".into();
        assert!(matches!(
            tracker.observe(intermediate),
            Ok(Progress::Generating)
        ));
        assert!(matches!(
            tracker.observe(observation()),
            Ok(Progress::Completed(_))
        ));
    }
    #[test]
    fn committed_text_cannot_be_revised() {
        let mut tracker = tracker();
        let mut o = observation();
        o.generating = true;
        tracker.observe(o.clone()).unwrap();
        tracker.commit_prefix("answer").unwrap();
        o.text = "rewritten".into();
        assert!(tracker.observe(o).is_err());
    }
}
