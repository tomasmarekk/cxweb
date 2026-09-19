//! Durable orchestration: validate, admit, submit once, attribute, then deliver.
use crate::{
    ledger::{Admission, Ledger},
    scheduler::Scheduler,
};
use cxweb_browser_adapter::turn::{Baseline, Observation, Progress, TurnTracker};
use cxweb_codex_adapter::{envelope, request::CanonicalRequest, wire};
use cxweb_domain::{SessionKey, TurnState};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

pub type BrowserFuture<T> = Pin<Box<dyn Future<Output = Result<T, &'static str>> + Send>>;
pub struct Prepared {
    pub handle: String,
    pub verified_session: SessionKey,
    pub baseline: Baseline,
    pub verified_route: String,
    pub verified_effort: Option<String>,
}
/// Implementations must verify the account/workspace/session lease in prepare,
/// never fall back to another route, and never resend a submission on timeout.
pub trait BrowserDriver: Send + Sync {
    fn prepare(&self, session: SessionKey) -> BrowserFuture<Prepared>;
    fn submit(&self, handle: String, prompt: String, selected_model: String) -> BrowserFuture<()>;
    fn observe(&self, handle: String) -> BrowserFuture<Observation>;
    /// Recheck the account/workspace before any buffered output is delivered.
    fn verify_completion(&self, handle: String) -> BrowserFuture<()>;
    fn stop(&self, handle: String) -> BrowserFuture<bool>;
    fn release(&self, handle: String) -> BrowserFuture<()>;
}

pub struct TurnInput {
    pub request_id: String,
    pub session: SessionKey,
    pub bytes: Vec<u8>,
}
#[derive(Clone)]
pub struct Delivery {
    pub json: String,
    pub sse: String,
}
impl Delivery {
    fn bytes(&self) -> usize {
        self.json.len() + self.sse.len()
    }
}

#[derive(Clone)]
pub struct Coordinator {
    ledger: Ledger,
    scheduler: Scheduler,
    browser: Arc<dyn BrowserDriver>,
    replay: Arc<Mutex<VecDeque<(String, Delivery)>>>,
}
impl Coordinator {
    pub fn new(ledger: Ledger, browser: Arc<dyn BrowserDriver>) -> Self {
        Self {
            ledger,
            scheduler: Scheduler::default(),
            browser,
            replay: Arc::default(),
        }
    }
    pub async fn execute(
        &self,
        input: TurnInput,
        cancellation: CancellationToken,
    ) -> Result<Delivery, &'static str> {
        let cancel = cancellation.child_token();
        // Dropping the HTTP future signals the background coordinator, which
        // still owns its browser lease and can durably record cancellation.
        let _cancel_on_drop = cancel.clone().drop_guard();
        let coordinator = self.clone();
        tokio::spawn(async move { coordinator.run(input, cancel).await })
            .await
            .map_err(|_| "E_TURN_WORKER")?
    }
    async fn run(
        &self,
        input: TurnInput,
        cancel: CancellationToken,
    ) -> Result<Delivery, &'static str> {
        let request = CanonicalRequest::decode(&input.bytes)?;
        if request.model != input.session.route {
            return Err("E_MODEL_FIDELITY");
        }
        let _generation = self
            .scheduler
            .acquire(input.session.clone(), &cancel)
            .await?;
        let replay_key = format!("{:x}", Sha256::digest(input.request_id.as_bytes()));
        match self
            .ledger
            .admit(&input.request_id, &input.session, &input.bytes)
            .await?
        {
            Admission::New => (),
            Admission::Existing(TurnState::Completed) => {
                return self
                    .replay
                    .lock()
                    .map_err(|_| "E_REPLAY_STATE")?
                    .iter()
                    .find(|(key, _)| key == &replay_key)
                    .map(|(_, delivery)| delivery.clone())
                    .ok_or("E_REPLAY_UNAVAILABLE");
            }
            Admission::Existing(_) => return Err("E_REQUEST_ALREADY_ADMITTED"),
        }
        let prepared = match tokio::time::timeout(
            Duration::from_secs(90),
            self.browser.prepare(input.session.clone()),
        )
        .await
        {
            Ok(Ok(prepared)) => prepared,
            failed => {
                self.ledger
                    .transition(&input.request_id, TurnState::Failed)
                    .await?;
                return Err(match failed {
                    Ok(Err(code)) => code,
                    _ => "E_BROWSER_PREPARE",
                });
            }
        };
        let result = self
            .run_prepared(
                &input.request_id,
                &input.session,
                request,
                &prepared,
                &cancel,
            )
            .await;
        // Release occurs on success, validation failure, cancellation and uncertainty.
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            self.browser.release(prepared.handle),
        )
        .await;
        if let Ok(delivery) = &result {
            self.cache(replay_key, delivery.clone())?;
        }
        result
    }
    async fn run_prepared(
        &self,
        id: &str,
        expected_session: &SessionKey,
        request: CanonicalRequest,
        prepared: &Prepared,
        cancel: &CancellationToken,
    ) -> Result<Delivery, &'static str> {
        if &prepared.verified_session != expected_session {
            self.ledger.transition(id, TurnState::Failed).await?;
            return Err("E_SESSION_SCOPE");
        }
        let validation = request
            .verify_route(
                &prepared.verified_route,
                prepared.verified_effort.as_deref(),
            )
            .and_then(|_| {
                TurnTracker::new(prepared.baseline.clone(), &prepared.baseline.selected_model)
            });
        let mut tracker = match validation {
            Ok(tracker) => tracker,
            Err(error) => {
                self.ledger.transition(id, TurnState::Failed).await?;
                return Err(error);
            }
        };
        let nonce: String = rand::random::<[u8; 16]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let prompt = match request.browser_prompt(&nonce, 512 * 1024) {
            Ok(prompt) => prompt,
            Err(error) => {
                self.ledger.transition(id, TurnState::Failed).await?;
                return Err(error);
            }
        };
        self.ledger
            .transition(id, TurnState::ObservedBaseline)
            .await?;
        if cancel.is_cancelled() {
            self.ledger.transition(id, TurnState::Cancelled).await?;
            return Err("E_CANCELLED");
        }
        self.ledger.transition(id, TurnState::Submitting).await?;
        tracker.begin_submission()?;
        // This operation is deliberately not retried or raced against cancellation.
        // A timed-out send has an unknown upstream outcome.
        if !matches!(
            tokio::time::timeout(
                Duration::from_secs(60),
                self.browser.submit(
                    prepared.handle.clone(),
                    prompt,
                    prepared.baseline.selected_model.clone()
                )
            )
            .await,
            Ok(Ok(()))
        ) {
            self.ledger
                .transition(id, TurnState::SubmissionUncertain)
                .await?;
            self.stop(&prepared.handle).await;
            return Err("E_SUBMISSION_UNCERTAIN");
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1800);
        let mut progress_at = tokio::time::Instant::now();
        let mut previous_text_hash = String::new();
        let mut state = TurnState::Submitting;
        loop {
            if cancel.is_cancelled() {
                tracker.cancel()?;
                self.ledger.transition(id, tracker.state()).await?;
                let confirmed = self.stop(&prepared.handle).await;
                return Err(if confirmed {
                    "E_CANCELLED"
                } else {
                    "E_CANCELLED_STOP_UNCONFIRMED"
                });
            }
            if tokio::time::Instant::now() >= deadline
                || progress_at.elapsed() > Duration::from_secs(300)
            {
                let _: Result<(), _> = tracker.fail("E_GENERATION_TIMEOUT");
                self.ledger.transition(id, tracker.state()).await?;
                self.stop(&prepared.handle).await;
                return Err("E_GENERATION_TIMEOUT");
            }
            let observation = match tokio::time::timeout(
                Duration::from_secs(5),
                self.browser.observe(prepared.handle.clone()),
            )
            .await
            {
                Ok(Ok(observation)) => observation,
                _ => {
                    let _: Result<(), _> = tracker.fail("E_BROWSER_OBSERVATION");
                    self.ledger.transition(id, tracker.state()).await?;
                    self.stop(&prepared.handle).await;
                    return Err("E_BROWSER_OBSERVATION");
                }
            };
            let text_hash = format!("{:x}", Sha256::digest(observation.text.as_bytes()));
            if text_hash != previous_text_hash {
                previous_text_hash = text_hash;
                progress_at = tokio::time::Instant::now();
            }
            let progress = match tracker.observe(observation) {
                Ok(progress) => progress,
                Err(error) => {
                    // The tracker may have acknowledged the new user before
                    // discovering an invalid assistant identity in the same sample.
                    if state == TurnState::Submitting && tracker.state() == TurnState::Failed {
                        self.ledger.transition(id, TurnState::Submitted).await?;
                    }
                    self.ledger.transition(id, tracker.state()).await?;
                    self.stop(&prepared.handle).await;
                    return Err(error);
                }
            };
            if state == TurnState::Submitting && tracker.state() != TurnState::Submitting {
                self.ledger.transition(id, TurnState::Submitted).await?;
                state = TurnState::Submitted;
            }
            if state == TurnState::Submitted
                && matches!(
                    tracker.state(),
                    TurnState::Generating | TurnState::Completed
                )
            {
                self.ledger.transition(id, TurnState::Generating).await?;
                state = TurnState::Generating;
            }
            if let Progress::Completed(text) = progress {
                let verified = tokio::select! {
                    _ = cancel.cancelled() => Err("E_CANCELLED"),
                    result = tokio::time::timeout(Duration::from_secs(60), self.browser.verify_completion(prepared.handle.clone())) =>
                        result.unwrap_or(Err("E_SESSION_SCOPE")),
                };
                if let Err(code) = verified {
                    self.ledger
                        .transition(
                            id,
                            if code == "E_CANCELLED" {
                                TurnState::Cancelled
                            } else {
                                TurnState::Failed
                            },
                        )
                        .await?;
                    return Err(code);
                }
                let output = match envelope::validate(text.as_bytes(), &request.context(&nonce)) {
                    Ok(output) => output,
                    Err(_) => {
                        self.ledger.transition(id, TurnState::Failed).await?;
                        return Err("E_INVALID_TOOL_ENVELOPE");
                    }
                };
                let response_id = format!("resp_cxweb_{:x}", Sha256::digest(id.as_bytes()));
                let created_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| "E_CLOCK")?
                    .as_secs();
                let encoded = wire::encode(&output, &request.model, &response_id, created_at)?;
                self.ledger.transition(id, TurnState::Completed).await?;
                return Ok(Delivery {
                    json: encoded.response.to_string(),
                    sse: encoded.sse(),
                });
            }
            tokio::select! { _ = cancel.cancelled() => (), _ = tokio::time::sleep(Duration::from_millis(200)) => () }
        }
    }
    async fn stop(&self, handle: &str) -> bool {
        matches!(
            tokio::time::timeout(Duration::from_secs(2), self.browser.stop(handle.to_owned()))
                .await,
            Ok(Ok(true))
        )
    }
    fn cache(&self, key: String, delivery: Delivery) -> Result<(), &'static str> {
        const MAX_REPLAY_BYTES: usize = 8 * 1024 * 1024;
        if delivery.bytes() > MAX_REPLAY_BYTES {
            return Ok(());
        }
        let mut cache = self.replay.lock().map_err(|_| "E_REPLAY_STATE")?;
        while cache.len() >= 16
            || cache.iter().map(|(_, d)| d.bytes()).sum::<usize>() + delivery.bytes()
                > MAX_REPLAY_BYTES
        {
            cache.pop_front();
        }
        cache.push_back((key, delivery));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Notify;

    #[derive(Clone, Copy)]
    enum Mode {
        Final,
        Tool,
        Invalid,
        Uncertain,
        Waiting,
        WrongAssistant,
        ChangedScope,
    }
    struct MockBrowser {
        mode: Mode,
        sends: AtomicUsize,
        stops: AtomicUsize,
        releases: AtomicUsize,
        nonce: Mutex<String>,
        observing: Notify,
        released: Notify,
    }
    impl MockBrowser {
        fn new(mode: Mode) -> Arc<Self> {
            Arc::new(Self {
                mode,
                sends: AtomicUsize::new(0),
                stops: AtomicUsize::new(0),
                releases: AtomicUsize::new(0),
                nonce: Mutex::new(String::new()),
                observing: Notify::new(),
                released: Notify::new(),
            })
        }
    }
    impl BrowserDriver for MockBrowser {
        fn verify_completion(&self, _: String) -> BrowserFuture<()> {
            let changed = matches!(self.mode, Mode::ChangedScope);
            Box::pin(async move {
                if changed {
                    Err("E_SESSION_SCOPE")
                } else {
                    Ok(())
                }
            })
        }
        fn prepare(&self, session: SessionKey) -> BrowserFuture<Prepared> {
            Box::pin(async move {
                Ok(Prepared {
                    handle: "owned-target".into(),
                    verified_session: session.clone(),
                    baseline: Baseline {
                        ids: vec!["old".into()],
                        selected_model: "Observed text".into(),
                        composer_empty: true,
                        generating: false,
                    },
                    verified_route: session.route,
                    verified_effort: None,
                })
            })
        }
        fn submit(&self, _: String, prompt: String, _: String) -> BrowserFuture<()> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            let nonce = prompt
                .split("turn_nonce=")
                .nth(1)
                .unwrap()
                .split('.')
                .next()
                .unwrap();
            *self.nonce.lock().unwrap() = nonce.to_owned();
            let uncertain = matches!(self.mode, Mode::Uncertain);
            Box::pin(async move {
                if uncertain {
                    Err("E_PIPE_TIMEOUT")
                } else {
                    Ok(())
                }
            })
        }
        fn observe(&self, _: String) -> BrowserFuture<Observation> {
            self.observing.notify_one();
            let nonce = self.nonce.lock().unwrap().clone();
            let text = match self.mode {
                Mode::Tool => json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"tool_calls","calls":[{"tool_key":"tool_0001","input":{"path":"source.rs"}}]}).to_string(),
                Mode::Invalid => "```json\n{}\n```".into(),
                _ => json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"final","text":"verified mock answer"}).to_string(),
            };
            let generating = matches!(self.mode, Mode::Waiting);
            let assistant = if matches!(self.mode, Mode::WrongAssistant) {
                "old"
            } else {
                "assistant-new"
            };
            Box::pin(async move {
                Ok(Observation {
                    user_id: Some("user-new".into()),
                    user_matches: true,
                    assistant_id: Some(assistant.into()),
                    text,
                    generating,
                    completion_control: !generating,
                    fenced_output: false,
                    selected_model: "Observed text".into(),
                    ambiguous: false,
                })
            })
        }
        fn stop(&self, _: String) -> BrowserFuture<bool> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(true) })
        }
        fn release(&self, _: String) -> BrowserFuture<()> {
            self.releases.fetch_add(1, Ordering::SeqCst);
            self.released.notify_one();
            Box::pin(async { Ok(()) })
        }
    }
    fn input() -> TurnInput {
        TurnInput { request_id:"request-1".into(), session:SessionKey { installation:"i".into(),native_session:"s".into(),account_scope:"a".into(),workspace_scope:"w".into(),route:"webbridge/test".into(),epoch:0 }, bytes:json!({"model":"webbridge/test","input":"synthetic task","tools":[{"type":"function","name":"read_file","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}]}).to_string().into_bytes() }
    }
    #[tokio::test]
    async fn complete_response_is_replayed_without_a_second_browser_submission() {
        let browser = MockBrowser::new(Mode::Final);
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let first = coordinator
            .execute(input(), CancellationToken::new())
            .await
            .unwrap();
        let second = coordinator
            .execute(input(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(first.sse, second.sse);
        assert!(first.json.contains("verified mock answer"));
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn uncertain_send_and_invalid_output_are_not_repaired_by_resubmission() {
        for mode in [
            Mode::Uncertain,
            Mode::Invalid,
            Mode::WrongAssistant,
            Mode::ChangedScope,
        ] {
            let browser = MockBrowser::new(mode);
            let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
            assert!(
                coordinator
                    .execute(input(), CancellationToken::new())
                    .await
                    .is_err()
            );
            assert!(
                coordinator
                    .execute(input(), CancellationToken::new())
                    .await
                    .is_err()
            );
            assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
            assert_eq!(browser.releases.load(Ordering::SeqCst), 1);
        }
    }
    #[tokio::test]
    async fn completion_scope_failure_withholds_and_never_caches_the_answer() {
        let browser = MockBrowser::new(Mode::ChangedScope);
        let ledger = Ledger::in_memory();
        let coordinator = Coordinator::new(ledger.clone(), browser.clone());
        assert_eq!(
            coordinator
                .execute(input(), CancellationToken::new())
                .await
                .err(),
            Some("E_SESSION_SCOPE")
        );
        let input = input();
        assert_eq!(
            ledger
                .admit(&input.request_id, &input.session, &input.bytes)
                .await
                .unwrap(),
            Admission::Existing(TurnState::Failed)
        );
        assert!(coordinator.replay.lock().unwrap().is_empty());
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn validated_tool_call_is_delivered_without_local_execution() {
        let browser = MockBrowser::new(Mode::Tool);
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let delivery = coordinator
            .execute(input(), CancellationToken::new())
            .await
            .unwrap();
        let response: serde_json::Value = serde_json::from_str(&delivery.json).unwrap();
        assert_eq!(response["output"][0]["type"], "function_call");
        assert_eq!(response["output"][0]["name"], "read_file");
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn dropped_client_future_still_stops_browser_and_persists_terminal_state() {
        let browser = MockBrowser::new(Mode::Waiting);
        let ledger = Ledger::in_memory();
        let coordinator = Coordinator::new(ledger.clone(), browser.clone());
        let task =
            tokio::spawn(
                async move { coordinator.execute(input(), CancellationToken::new()).await },
            );
        tokio::time::timeout(Duration::from_secs(2), browser.observing.notified())
            .await
            .unwrap();
        task.abort();
        tokio::time::timeout(Duration::from_secs(2), browser.released.notified())
            .await
            .unwrap();
        assert_eq!(browser.stops.load(Ordering::SeqCst), 1);
        let input = input();
        assert_eq!(
            ledger
                .admit(&input.request_id, &input.session, &input.bytes)
                .await
                .unwrap(),
            Admission::Existing(TurnState::Cancelled)
        );
    }
}
