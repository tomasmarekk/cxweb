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

// A visible ChatGPT failure proves that this browser response cannot finish.
// Retrying stays inside the one admitted native request, on a fresh page.
const MAX_EXPLICIT_FAILURE_RETRIES: usize = 3;
// The browser adapter allows up to five seconds to confirm target closure,
// after its CDP operations. Never time out before it can report success.
const BROWSER_RELEASE_TIMEOUT: Duration = Duration::from_secs(60);

pub type BrowserFuture<T> = Pin<Box<dyn Future<Output = Result<T, &'static str>> + Send>>;
/// Bounded transport channel; only verified public status events use this path.
pub type PublicProgress = tokio::sync::mpsc::Sender<Vec<serde_json::Value>>;
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
    fn prepare_with_effort(
        &self,
        session: SessionKey,
        _effort: Option<String>,
    ) -> BrowserFuture<Prepared> {
        self.prepare(session)
    }
    fn submit(&self, handle: String, prompt: String, selected_model: String) -> BrowserFuture<()>;
    fn observe(&self, handle: String) -> BrowserFuture<Observation>;
    /// Recheck the account/workspace before public status or final output delivery.
    fn verify_completion(&self, handle: String) -> BrowserFuture<()>;
    fn stop(&self, handle: String) -> BrowserFuture<bool>;
    fn release(&self, handle: String) -> BrowserFuture<()>;
}

pub struct TurnInput {
    pub request_id: String,
    pub session: SessionKey,
    pub bytes: Vec<u8>,
}

struct BrowserRequest {
    request: CanonicalRequest,
    nonce: String,
    prompt: String,
}

struct ProgressState {
    summary: Vec<String>,
    published_events: usize,
    response_id: String,
    created_at: u64,
}
impl BrowserRequest {
    fn prepare(request: CanonicalRequest) -> Result<Self, &'static str> {
        let nonce: String = rand::random::<[u8; 16]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        // Count the complete escaped prompt, including instructions and tool
        // schemas, before admission or any browser operation. This is a local
        // byte ceiling, not an estimate of the provider's token capacity.
        let prompt = request.browser_prompt(&nonce, crate::context_budget::HARD_PROMPT_BYTES)?;
        Ok(Self {
            request,
            nonce,
            prompt,
        })
    }
}

/// Runtime-owned encryption bound to one qualified task and codec. The model
/// cannot select a key, scope or ciphertext, and sealing precedes durable completion.
pub(crate) trait CheckpointEncoder: Send + Sync {
    fn staged(&self) -> bool {
        false
    }
    fn seal(&self, summary: &str, pending: &[serde_json::Value]) -> Result<String, &'static str>;
    fn restore_summary(
        &self,
        _token: &str,
        _pending: &[serde_json::Value],
    ) -> Result<String, &'static str> {
        Err("E_CHECKPOINT_SUMMARY")
    }
}
#[derive(Clone)]
pub struct Delivery {
    pub json: String,
    pub sse: String,
    pub(crate) live_verified: bool,
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
    #[cfg(windows)]
    _consumer: Option<crate::generation_handoff::ConsumerLease>,
}
impl Coordinator {
    pub fn new(ledger: Ledger, browser: Arc<dyn BrowserDriver>) -> Self {
        Self {
            ledger,
            scheduler: Scheduler::default(),
            browser,
            replay: Arc::default(),
            #[cfg(windows)]
            _consumer: None,
        }
    }
    /// The detached coordinator retains exclusive session use through cancellation
    /// cleanup, even if its gateway or requesting UI has already disappeared.
    #[cfg(windows)]
    pub(crate) fn with_consumer(mut self, lease: crate::generation_handoff::ConsumerLease) -> Self {
        self._consumer = Some(lease);
        self
    }
    pub async fn execute(
        &self,
        input: TurnInput,
        cancellation: CancellationToken,
    ) -> Result<Delivery, &'static str> {
        self.execute_with_checkpoint(input, cancellation, None)
            .await
    }

    pub(crate) async fn execute_with_checkpoint(
        &self,
        input: TurnInput,
        cancellation: CancellationToken,
        checkpoint: Option<Arc<dyn CheckpointEncoder>>,
    ) -> Result<Delivery, &'static str> {
        self.execute_with_progress(input, cancellation, checkpoint, None)
            .await
    }

    #[cfg(windows)]
    pub(crate) async fn record_context_shape(&self, payload: &serde_json::Value) {
        let mut sizes = std::collections::BTreeMap::new();
        for field in ["instructions", "tools", "input"] {
            sizes.insert(field, payload[field].to_string().len());
        }
        for role in ["user", "developer", "system", "assistant"] {
            let bytes = payload["input"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter(|item| item["role"] == role)
                        .map(|item| item.to_string().len())
                        .sum()
                })
                .unwrap_or(0);
            sizes.insert(role, bytes);
        }
        let _ = self.ledger.record_context_shape(sizes).await;
    }

    pub(crate) async fn execute_with_progress(
        &self,
        input: TurnInput,
        cancellation: CancellationToken,
        checkpoint: Option<Arc<dyn CheckpointEncoder>>,
        progress: Option<PublicProgress>,
    ) -> Result<Delivery, &'static str> {
        let cancel = cancellation.child_token();
        // Dropping the HTTP future signals the background coordinator, which
        // still owns its browser lease and can durably record cancellation.
        let _cancel_on_drop = cancel.clone().drop_guard();
        let coordinator = self.clone();
        tokio::spawn(async move { coordinator.run(input, cancel, checkpoint, progress).await })
            .await
            .map_err(|_| "E_TURN_WORKER")?
    }
    async fn run(
        &self,
        input: TurnInput,
        cancel: CancellationToken,
        checkpoint: Option<Arc<dyn CheckpointEncoder>>,
        progress: Option<PublicProgress>,
    ) -> Result<Delivery, &'static str> {
        let request = if checkpoint.is_some() {
            CanonicalRequest::decode_compaction(&input.bytes)?
        } else {
            CanonicalRequest::decode(&input.bytes)?
        };
        let request = if checkpoint.as_ref().is_some_and(|encoder| encoder.staged()) {
            request.with_staged_compaction()?
        } else {
            request
        };
        if request.model != input.session.route {
            return Err("E_MODEL_FIDELITY");
        }
        let request = BrowserRequest::prepare(request)?;
        let _generation = self
            .scheduler
            .acquire(input.session.clone(), &cancel)
            .await?;
        let replay_key = format!("{:x}", Sha256::digest(input.request_id.as_bytes()));
        let mut publication = ProgressState {
            summary: Vec::new(),
            published_events: 0,
            response_id: format!(
                "resp_cxweb_{:x}",
                Sha256::digest(input.request_id.as_bytes())
            ),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "E_CLOCK")?
                .as_secs(),
        };
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
                    .map(|(_, delivery)| {
                        let mut replay = delivery.clone();
                        replay.live_verified = false;
                        replay
                    })
                    .ok_or("E_REPLAY_UNAVAILABLE");
            }
            Admission::Existing(_) => return Err("E_REQUEST_ALREADY_ADMITTED"),
        }
        let _ = self
            .ledger
            .record_shape(
                &input.request_id,
                input.bytes.len(),
                request.prompt.len(),
                checkpoint.is_some(),
            )
            .await;
        let result = async {
            let mut prepared = match tokio::time::timeout(
                Duration::from_secs(90),
                self.browser.prepare_with_effort(
                    input.session.clone(),
                    request.request.requested_effort.clone(),
                ),
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
            let mut retries = 0;
            let mut resumed = None;
            loop {
                let _ = self.ledger.record_attempt(&input.request_id).await;
                let result = self
                    .run_prepared(
                        &input.request_id,
                        &input.session,
                        &request,
                        &prepared,
                        &cancel,
                        checkpoint.as_deref(),
                        progress.clone(),
                        &mut publication,
                        resumed,
                        retries < MAX_EXPLICIT_FAILURE_RETRIES,
                    )
                    .await;
                // Never start another page until the failed page has been closed.
                let released = tokio::time::timeout(
                    BROWSER_RELEASE_TIMEOUT,
                    self.browser.release(prepared.handle),
                )
                .await;
                if result.as_ref().err().copied() == Some("E_CHATGPT_THINKING_FAILED")
                    && retries < MAX_EXPLICIT_FAILURE_RETRIES
                {
                    if !matches!(released, Ok(Ok(()))) {
                        self.ledger
                            .transition(&input.request_id, TurnState::Failed)
                            .await?;
                        return Err("E_WEB_CLEANUP_UNCONFIRMED");
                    }
                    resumed = match self
                        .ledger
                        .admit(&input.request_id, &input.session, &input.bytes)
                        .await?
                    {
                        Admission::Existing(
                            state @ (TurnState::Submitted | TurnState::Generating),
                        ) => Some(state),
                        _ => return Err("E_TURN_STATE"),
                    };
                    if cancel.is_cancelled() {
                        self.ledger
                            .transition(&input.request_id, TurnState::Cancelled)
                            .await?;
                        return Err("E_CANCELLED");
                    }
                    retries += 1;
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            self.ledger.transition(&input.request_id, TurnState::Cancelled).await?;
                            return Err("E_CANCELLED");
                        }
                        _ = tokio::time::sleep(Duration::from_millis(500 * retries as u64)) => (),
                    }
                    prepared = match tokio::time::timeout(
                        Duration::from_secs(90),
                        self.browser.prepare_with_effort(
                            input.session.clone(),
                            request.request.requested_effort.clone(),
                        ),
                    )
                    .await
                    {
                        Ok(Ok(prepared)) => prepared,
                        failure => {
                            self.ledger
                                .transition(&input.request_id, TurnState::Failed)
                                .await?;
                            return Err(match failure {
                                Ok(Err(code)) => code,
                                _ => "E_BROWSER_PREPARE",
                            });
                        }
                    };
                    continue;
                }
                if let Ok(delivery) = &result {
                    self.cache(replay_key, delivery.clone())?;
                }
                return result;
            }
        }
        .await;
        // Only the newly admitted owner records a terminal outcome. A duplicate
        // request must not overwrite the still-running owner's diagnostics.
        let _ = self
            .ledger
            .record_result(
                &input.request_id,
                result.as_ref().err().copied().unwrap_or("OK"),
            )
            .await;
        result
    }
    #[allow(clippy::too_many_arguments)]
    async fn run_prepared(
        &self,
        id: &str,
        expected_session: &SessionKey,
        request: &BrowserRequest,
        prepared: &Prepared,
        cancel: &CancellationToken,
        checkpoint: Option<&dyn CheckpointEncoder>,
        progress_sender: Option<PublicProgress>,
        publication: &mut ProgressState,
        resumed: Option<TurnState>,
        retry_thinking_failure: bool,
    ) -> Result<Delivery, &'static str> {
        let BrowserRequest {
            request,
            nonce,
            prompt,
        } = request;
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
        if resumed.is_none() {
            self.ledger
                .transition(id, TurnState::ObservedBaseline)
                .await?;
        }
        if cancel.is_cancelled() {
            self.ledger.transition(id, TurnState::Cancelled).await?;
            return Err("E_CANCELLED");
        }
        if resumed.is_none() {
            self.ledger.transition(id, TurnState::Submitting).await?;
        }
        tracker.begin_submission()?;
        // This operation is deliberately not retried or raced against cancellation.
        // A timed-out send has an unknown upstream outcome.
        let submitted = tokio::time::timeout(
            Duration::from_secs(60),
            self.browser.submit(
                prepared.handle.clone(),
                prompt.clone(),
                prepared.baseline.selected_model.clone(),
            ),
        )
        .await;
        if !matches!(submitted, Ok(Ok(()))) {
            self.ledger
                .transition(id, TurnState::SubmissionUncertain)
                .await?;
            self.stop(&prepared.handle).await;
            return Err(if matches!(submitted, Ok(Err("E_BROWSER_RATE_LIMITED"))) {
                "E_BROWSER_RATE_LIMITED"
            } else {
                "E_SUBMISSION_UNCERTAIN"
            });
        }
        // Generation has no elapsed-time or text-inactivity deadline. Pro can
        // think silently for a long time; completion, cancellation and actual
        // browser/protocol failures determine when this request ends.
        let mut state = resumed.unwrap_or(TurnState::Submitting);
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
            let observation = match tokio::time::timeout(
                Duration::from_secs(5),
                self.browser.observe(prepared.handle.clone()),
            )
            .await
            {
                Ok(Ok(observation)) => observation,
                failure => {
                    let _: Result<(), _> = tracker.fail("E_BROWSER_OBSERVATION");
                    self.ledger.transition(id, tracker.state()).await?;
                    self.stop(&prepared.handle).await;
                    return Err(if matches!(failure, Ok(Err("E_BROWSER_RATE_LIMITED"))) {
                        "E_BROWSER_RATE_LIMITED"
                    } else {
                        "E_BROWSER_OBSERVATION"
                    });
                }
            };
            let observed_summary = observation.summary.clone();
            let progress = match tracker.observe(observation) {
                Ok(progress) => progress,
                Err(error) => {
                    // The tracker may have acknowledged the new user before
                    // discovering an invalid assistant identity in the same sample.
                    if state == TurnState::Submitting && tracker.state() == TurnState::Failed {
                        self.ledger.transition(id, TurnState::Submitted).await?;
                    }
                    if error == "E_CHATGPT_THINKING_FAILED" && retry_thinking_failure {
                        return Err(error);
                    }
                    self.ledger.transition(id, tracker.state()).await?;
                    self.stop(&prepared.handle).await;
                    return Err(error);
                }
            };
            let previous_summary_len = publication.summary.len();
            if matches!(
                tracker.state(),
                TurnState::Generating | TurnState::Completed
            ) {
                for text in observed_summary {
                    if !text.is_empty()
                        && text.len() <= 8192
                        && publication.summary.len() < 64
                        && !publication.summary.contains(&text)
                    {
                        publication.summary.push(text);
                    }
                }
            }
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
            if tracker.state() == TurnState::Generating
                && request.public_summary
                && checkpoint.is_none()
                && publication.summary.len() > previous_summary_len
                && let Some(sender) = &progress_sender
            {
                let verified = tokio::select! {
                    _ = cancel.cancelled() => Err("E_CANCELLED"),
                    result = tokio::time::timeout(Duration::from_secs(60), self.browser.verify_completion(prepared.handle.clone())) =>
                        result.unwrap_or(Err("E_SESSION_SCOPE")),
                };
                let publication = verified.and_then(|()| {
                    let events = wire::public_summary_prefix(
                        &request.model,
                        &publication.response_id,
                        publication.created_at,
                        &publication.summary,
                    )?;
                    let count = events.len();
                    sender
                        .try_send(
                            events
                                .into_iter()
                                .skip(publication.published_events)
                                .collect(),
                        )
                        .map_err(|_| "E_PUBLIC_PROGRESS_DELIVERY")?;
                    publication.published_events = count;
                    Ok(())
                });
                if let Err(code) = publication {
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
                    self.stop(&prepared.handle).await;
                    return Err(code);
                }
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
                let output =
                    match envelope::validate_detailed(text.as_bytes(), &request.context(nonce)) {
                        Ok(output) => output,
                        Err(code) => {
                            self.ledger.transition(id, TurnState::Failed).await?;
                            return Err(code);
                        }
                    };
                if let Err(code) = request.validate_output(&output) {
                    self.ledger.transition(id, TurnState::Failed).await?;
                    return Err(code);
                }
                let encoded = match (&output, checkpoint, request.compaction_pending()) {
                    (
                        envelope::ValidatedOutput::Checkpoint(summary),
                        Some(codec),
                        Some(pending),
                    ) => codec.seal(summary, pending).and_then(|token| {
                        wire::encode_checkpoint(
                            &token,
                            &request.model,
                            &publication.response_id,
                            publication.created_at,
                        )
                    }),
                    (_, None, None) => wire::encode_with_summary(
                        &output,
                        &request.model,
                        &publication.response_id,
                        publication.created_at,
                        if request.public_summary {
                            &publication.summary
                        } else {
                            &[]
                        },
                    ),
                    _ => Err("E_COMPACTION_CODEC_REQUIRED"),
                };
                let encoded = match encoded {
                    Ok(encoded) => encoded,
                    Err(code) => {
                        self.ledger.transition(id, TurnState::Failed).await?;
                        return Err(code);
                    }
                };
                self.ledger.transition(id, TurnState::Completed).await?;
                return Ok(Delivery {
                    json: encoded.response.to_string(),
                    sse: encoded.sse(),
                    live_verified: true,
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::Notify;

    #[derive(Clone, Copy)]
    enum Mode {
        LimitedSubmit,
        LimitedObserve,
        Final,
        Tool,
        Invalid,
        Uncertain,
        Waiting,
        Stalled,
        WrongAssistant,
        ChangedScope,
        Checkpoint,
        BadCheckpoint,
        ThinkingThenTool,
        ThinkingAfterStatusThenTool,
        ThinkingThenUncertainSend,
        ThinkingAlways,
    }
    struct MockBrowser {
        mode: Mode,
        requested_efforts: Mutex<Vec<Option<String>>>,
        prepares: AtomicUsize,
        observations: AtomicUsize,
        sends: AtomicUsize,
        stops: AtomicUsize,
        releases: AtomicUsize,
        release_fails: AtomicBool,
        first_release_delay_ms: AtomicUsize,
        nonce: Mutex<String>,
        observing: Notify,
        released: Notify,
        release_gate: Option<Arc<Notify>>,
        hold_generation: AtomicBool,
        public_summary: Mutex<Vec<String>>,
    }
    impl MockBrowser {
        fn new(mode: Mode) -> Arc<Self> {
            Arc::new(Self {
                mode,
                requested_efforts: Mutex::default(),
                prepares: AtomicUsize::new(0),
                observations: AtomicUsize::new(0),
                sends: AtomicUsize::new(0),
                stops: AtomicUsize::new(0),
                releases: AtomicUsize::new(0),
                release_fails: AtomicBool::new(false),
                first_release_delay_ms: AtomicUsize::new(0),
                nonce: Mutex::new(String::new()),
                observing: Notify::new(),
                released: Notify::new(),
                release_gate: None,
                hold_generation: AtomicBool::new(false),
                public_summary: Mutex::new(Vec::new()),
            })
        }
    }
    impl BrowserDriver for MockBrowser {
        fn prepare_with_effort(
            &self,
            session: SessionKey,
            effort: Option<String>,
        ) -> BrowserFuture<Prepared> {
            self.prepares.fetch_add(1, Ordering::SeqCst);
            self.requested_efforts.lock().unwrap().push(effort.clone());
            let prepared = self.prepare(session);
            Box::pin(async move {
                let mut prepared = prepared.await?;
                prepared.verified_effort = effort;
                Ok(prepared)
            })
        }
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
            let uncertain = matches!(self.mode, Mode::Uncertain)
                || (matches!(self.mode, Mode::ThinkingThenUncertainSend)
                    && self.sends.load(Ordering::SeqCst) == 2);
            let limited = matches!(self.mode, Mode::LimitedSubmit);
            Box::pin(async move {
                if limited {
                    Err("E_BROWSER_RATE_LIMITED")
                } else if uncertain {
                    Err("E_PIPE_TIMEOUT")
                } else {
                    Ok(())
                }
            })
        }
        fn observe(&self, _: String) -> BrowserFuture<Observation> {
            self.observing.notify_one();
            let observation_number = self.observations.fetch_add(1, Ordering::SeqCst) + 1;
            if matches!(self.mode, Mode::LimitedObserve) {
                return Box::pin(async { Err("E_BROWSER_RATE_LIMITED") });
            }
            let nonce = self.nonce.lock().unwrap().clone();
            let thinking_failed = matches!(self.mode, Mode::ThinkingAlways)
                || (matches!(self.mode, Mode::ThinkingThenTool)
                    && self.sends.load(Ordering::SeqCst) == 1)
                || (matches!(self.mode, Mode::ThinkingThenUncertainSend)
                    && self.sends.load(Ordering::SeqCst) == 1)
                || (matches!(self.mode, Mode::ThinkingAfterStatusThenTool)
                    && self.sends.load(Ordering::SeqCst) == 1
                    && observation_number > 1);
            let text = match self.mode {
                Mode::Checkpoint => json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"checkpoint","summary":json!({"goal":"fixture","constraints":[],"changed_files":[],"decisions":[],"outstanding_work":[],"test_results":[],"unresolved_tool_ids":[]}).to_string()}).to_string(),
                Mode::BadCheckpoint => json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"checkpoint","summary":"unstructured summary"}).to_string(),
                Mode::Tool | Mode::ThinkingThenTool | Mode::ThinkingAfterStatusThenTool => json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"tool_calls","calls":[{"tool_key":"tool_0001","input":{"path":"source.rs"}}]}).to_string(),
                Mode::Invalid => "```json\n{}\n```".into(),
                _ => json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"final","text":"verified mock answer"}).to_string(),
            };
            let waiting_for_failure = matches!(self.mode, Mode::ThinkingAfterStatusThenTool)
                && self.sends.load(Ordering::SeqCst) == 1
                && observation_number == 1;
            let generating = matches!(self.mode, Mode::Waiting)
                || waiting_for_failure
                || self.hold_generation.load(Ordering::SeqCst);
            let summary = if waiting_for_failure {
                vec!["Thinking".into()]
            } else {
                self.public_summary.lock().unwrap().clone()
            };
            let completion_control = !generating && !matches!(self.mode, Mode::Stalled);
            let assistant = if matches!(self.mode, Mode::WrongAssistant) {
                "old"
            } else {
                "assistant-new"
            };
            Box::pin(async move {
                Ok(Observation {
                    user_id: Some("user-new".into()),
                    user_matches: true,
                    assistant_id: (!thinking_failed).then(|| assistant.into()),
                    text: if thinking_failed || waiting_for_failure {
                        String::new()
                    } else {
                        text
                    },
                    summary,
                    generating,
                    generation_failed: thinking_failed,
                    completion_control: completion_control && !thinking_failed,
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
            let first = self.releases.fetch_add(1, Ordering::SeqCst) == 0;
            self.released.notify_one();
            let gate = self.release_gate.clone();
            let fails = self.release_fails.load(Ordering::SeqCst);
            let delay = if first {
                self.first_release_delay_ms.load(Ordering::SeqCst)
            } else {
                0
            };
            Box::pin(async move {
                if let Some(gate) = gate {
                    gate.notified().await;
                }
                if delay > 0 {
                    tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                }
                if fails {
                    Err("E_BROWSER_RELEASE")
                } else {
                    Ok(())
                }
            })
        }
    }
    fn input() -> TurnInput {
        TurnInput { request_id:"request-1".into(), session:SessionKey { installation:"i".into(),native_session:"s".into(),account_scope:"a".into(),workspace_scope:"w".into(),route:"webbridge/test".into(),epoch:0 }, bytes:json!({"model":"webbridge/test","input":"synthetic task","tools":[{"type":"function","name":"read_file","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}]}).to_string().into_bytes() }
    }

    #[tokio::test]
    async fn explicit_browser_failure_retries_with_new_page_inside_one_admission() {
        let browser = MockBrowser::new(Mode::ThinkingThenTool);
        let ledger = Ledger::in_memory();
        let coordinator = Coordinator::new(ledger.clone(), browser.clone());
        let delivery = coordinator
            .execute(input(), CancellationToken::new())
            .await
            .unwrap();
        let response: serde_json::Value = serde_json::from_str(&delivery.json).unwrap();
        assert_eq!(response["output"].as_array().unwrap().len(), 1);
        assert_eq!(response["output"][0]["type"], "function_call");
        assert_eq!(browser.prepares.load(Ordering::SeqCst), 2);
        assert_eq!(browser.sends.load(Ordering::SeqCst), 2);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 2);
        assert_eq!(browser.stops.load(Ordering::SeqCst), 0);
        assert_eq!(
            ledger
                .admit(&input().request_id, &input().session, &input().bytes)
                .await
                .unwrap(),
            Admission::Existing(TurnState::Completed)
        );
        // A native reconnect replays the completed result without another Send.
        assert_eq!(
            coordinator
                .execute(input(), CancellationToken::new())
                .await
                .unwrap()
                .json,
            delivery.json
        );
        assert_eq!(browser.sends.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn confirmed_slow_page_closure_still_allows_recovery() {
        let browser = MockBrowser::new(Mode::ThinkingThenTool);
        // The browser adapter itself allows five seconds for confirmed target
        // destruction. The former two-second coordinator deadline rejected
        // this successful cleanup before the second page could be prepared.
        browser
            .first_release_delay_ms
            .store(2_100, Ordering::SeqCst);
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let delivery = coordinator
            .execute(input(), CancellationToken::new())
            .await
            .unwrap();
        let response: serde_json::Value = serde_json::from_str(&delivery.json).unwrap();
        assert_eq!(response["output"][0]["type"], "function_call");
        assert_eq!(browser.prepares.load(Ordering::SeqCst), 2);
        assert_eq!(browser.sends.load(Ordering::SeqCst), 2);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn explicit_browser_failure_has_bounded_attempts() {
        let browser = MockBrowser::new(Mode::ThinkingAlways);
        let ledger = Ledger::in_memory();
        let coordinator = Coordinator::new(ledger.clone(), browser.clone());
        assert_eq!(
            coordinator
                .execute(input(), CancellationToken::new())
                .await
                .err(),
            Some("E_CHATGPT_THINKING_FAILED")
        );
        assert_eq!(browser.prepares.load(Ordering::SeqCst), 4);
        assert_eq!(browser.sends.load(Ordering::SeqCst), 4);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 4);
        assert_eq!(
            ledger
                .admit(&input().request_id, &input().session, &input().bytes)
                .await
                .unwrap(),
            Admission::Existing(TurnState::Failed)
        );
        assert_eq!(
            coordinator
                .execute(input(), CancellationToken::new())
                .await
                .err(),
            Some("E_REQUEST_ALREADY_ADMITTED")
        );
    }

    #[tokio::test]
    async fn retry_preserves_already_streamed_public_status_as_one_response() {
        let browser = MockBrowser::new(Mode::ThinkingAfterStatusThenTool);
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
        let delivery = coordinator
            .execute_with_progress(input(), CancellationToken::new(), None, Some(sender))
            .await
            .unwrap();
        let mut emitted = Vec::new();
        while let Some(events) = receiver.recv().await {
            emitted.extend(events);
        }
        assert!(!emitted.is_empty());
        let completed: Vec<serde_json::Value> = delivery
            .sse
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(completed.starts_with(&emitted));
        assert_eq!(browser.sends.load(Ordering::SeqCst), 2);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_requires_confirmed_cleanup_of_failed_page() {
        let browser = MockBrowser::new(Mode::ThinkingAlways);
        browser.release_fails.store(true, Ordering::SeqCst);
        let ledger = Ledger::in_memory();
        let coordinator = Coordinator::new(ledger.clone(), browser.clone());
        assert_eq!(
            coordinator
                .execute(input(), CancellationToken::new())
                .await
                .err(),
            Some("E_WEB_CLEANUP_UNCONFIRMED")
        );
        assert_eq!(browser.prepares.load(Ordering::SeqCst), 1);
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        assert_eq!(
            ledger
                .admit(&input().request_id, &input().session, &input().bytes)
                .await
                .unwrap(),
            Admission::Existing(TurnState::Failed)
        );
    }

    #[tokio::test]
    async fn cancellation_between_explicit_failure_and_retry_never_resends() {
        let browser = MockBrowser::new(Mode::ThinkingAlways);
        let released = browser.released.notified();
        let ledger = Ledger::in_memory();
        let coordinator = Coordinator::new(ledger.clone(), browser.clone());
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(async move { coordinator.execute(input(), token).await });
        released.await;
        cancel.cancel();
        assert_eq!(task.await.unwrap().err(), Some("E_CANCELLED"));
        assert_eq!(browser.prepares.load(Ordering::SeqCst), 1);
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        assert_eq!(
            ledger
                .admit(&input().request_id, &input().session, &input().bytes)
                .await
                .unwrap(),
            Admission::Existing(TurnState::Cancelled)
        );
    }

    #[tokio::test]
    async fn uncertain_retry_send_is_terminal_and_never_gets_a_third_send() {
        let browser = MockBrowser::new(Mode::ThinkingThenUncertainSend);
        let ledger = Ledger::in_memory();
        let coordinator = Coordinator::new(ledger.clone(), browser.clone());
        assert_eq!(
            coordinator
                .execute(input(), CancellationToken::new())
                .await
                .err(),
            Some("E_SUBMISSION_UNCERTAIN")
        );
        assert_eq!(browser.sends.load(Ordering::SeqCst), 2);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 2);
        assert_eq!(
            ledger
                .admit(&input().request_id, &input().session, &input().bytes)
                .await
                .unwrap(),
            Admission::Existing(TurnState::SubmissionUncertain)
        );
    }

    #[tokio::test]
    async fn active_reasoning_completes_after_hours_without_new_answer_text() {
        let browser = MockBrowser::new(Mode::Final);
        browser.hold_generation.store(true, Ordering::SeqCst);
        *browser.public_summary.lock().unwrap() = vec!["Pro thinking".into()];
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let task =
            tokio::spawn(
                async move { coordinator.execute(input(), CancellationToken::new()).await },
            );
        browser.observing.notified().await;
        tokio::time::pause();
        for _ in 0..72 {
            tokio::time::advance(Duration::from_secs(300)).await;
            tokio::task::yield_now().await;
            assert!(
                !task.is_finished(),
                "active reasoning hit an elapsed-time limit"
            );
        }
        browser.hold_generation.store(false, Ordering::SeqCst);
        assert!(task.await.unwrap().is_ok());
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        assert_eq!(browser.stops.load(Ordering::SeqCst), 0);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn silent_generation_waits_for_user_cancellation_without_resubmission() {
        for mode in [Mode::Stalled, Mode::Waiting] {
            let browser = MockBrowser::new(mode);
            let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
            let cancel = CancellationToken::new();
            let worker_cancel = cancel.clone();
            let task =
                tokio::spawn(async move { coordinator.execute(input(), worker_cancel).await });
            browser.observing.notified().await;
            tokio::time::pause();
            for _ in 0..72 {
                tokio::time::advance(Duration::from_secs(300)).await;
                tokio::task::yield_now().await;
                assert!(!task.is_finished(), "silent generation was timed out");
            }
            cancel.cancel();
            assert_eq!(task.await.unwrap().err(), Some("E_CANCELLED"));
            assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
            assert_eq!(browser.stops.load(Ordering::SeqCst), 1);
            assert_eq!(browser.releases.load(Ordering::SeqCst), 1);
            tokio::time::resume();
        }
    }

    #[tokio::test]
    async fn public_status_arrives_before_completion_without_early_tools_and_replays_once() {
        let browser = MockBrowser::new(Mode::Tool);
        browser.hold_generation.store(true, Ordering::SeqCst);
        *browser.public_summary.lock().unwrap() = vec!["Thinking".into()];
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let worker = coordinator.clone();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
        let task = tokio::spawn(async move {
            worker
                .execute_with_progress(input(), CancellationToken::new(), None, Some(sender))
                .await
        });
        let mut prefix = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(!task.is_finished());
        assert_eq!(prefix.len(), 7);
        *browser.public_summary.lock().unwrap() = vec!["Checking cases".into()];
        prefix.extend(
            tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await
                .unwrap()
                .unwrap(),
        );
        assert!(!task.is_finished());
        assert_eq!(prefix.len(), 11);
        assert!(
            !serde_json::to_string(&prefix)
                .unwrap()
                .contains("function_call")
        );
        browser.hold_generation.store(false, Ordering::SeqCst);
        let delivery = task.await.unwrap().unwrap();
        let events: Vec<serde_json::Value> = delivery
            .sse
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|data| serde_json::from_str(data).unwrap())
            .collect();
        assert!(events.starts_with(&prefix));
        assert_eq!(events.last().unwrap()["type"], "response.completed");
        let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
        let replay = coordinator
            .execute_with_progress(input(), CancellationToken::new(), None, Some(sender))
            .await
            .unwrap();
        assert_eq!(replay.sse, delivery.sse);
        assert!(receiver.recv().await.is_none());
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn public_status_requires_scope_and_attribution_and_a_live_receiver() {
        for (mode, closed, expected) in [
            (Mode::ChangedScope, false, "E_SESSION_SCOPE"),
            (Mode::WrongAssistant, false, "E_TURN_ATTRIBUTION"),
            (Mode::Final, true, "E_PUBLIC_PROGRESS_DELIVERY"),
        ] {
            let browser = MockBrowser::new(mode);
            browser.hold_generation.store(true, Ordering::SeqCst);
            *browser.public_summary.lock().unwrap() = vec!["Thinking".into()];
            let ledger = Ledger::in_memory();
            let coordinator = Coordinator::new(ledger.clone(), browser.clone());
            let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
            if closed {
                receiver.close();
            }
            let result = coordinator
                .execute_with_progress(input(), CancellationToken::new(), None, Some(sender))
                .await;
            assert_eq!(result.err(), Some(expected));
            assert!(receiver.recv().await.is_none());
            assert_eq!(browser.stops.load(Ordering::SeqCst), 1);
            assert_eq!(browser.releases.load(Ordering::SeqCst), 1);
            assert_eq!(
                coordinator
                    .execute(input(), CancellationToken::new())
                    .await
                    .err(),
                Some("E_REQUEST_ALREADY_ADMITTED")
            );
            assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn cancelling_after_public_status_stops_once_without_completion() {
        let browser = MockBrowser::new(Mode::Waiting);
        *browser.public_summary.lock().unwrap() = vec!["Thinking".into()];
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
        let task = tokio::spawn(async move {
            coordinator
                .execute_with_progress(input(), token, None, Some(sender))
                .await
        });
        let prefix = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(prefix.len(), 7);
        cancel.cancel();
        assert_eq!(task.await.unwrap().err(), Some("E_CANCELLED"));
        assert!(receiver.recv().await.is_none());
        assert_eq!(browser.stops.load(Ordering::SeqCst), 1);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn summary_none_suppresses_public_status_during_generation() {
        let browser = MockBrowser::new(Mode::Waiting);
        *browser.public_summary.lock().unwrap() = vec!["Thinking".into()];
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let mut request = input();
        let mut payload: serde_json::Value = serde_json::from_slice(&request.bytes).unwrap();
        payload["reasoning"] = json!({"summary":"none"});
        request.bytes = serde_json::to_vec(&payload).unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
        let task = tokio::spawn(async move {
            coordinator
                .execute_with_progress(request, token, None, Some(sender))
                .await
        });
        browser.observing.notified().await;
        // A second observation proves the first status was handled, not merely queued.
        browser.observing.notified().await;
        cancel.cancel();
        assert_eq!(task.await.unwrap().err(), Some("E_CANCELLED"));
        assert!(receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn native_reasoning_choice_reaches_browser_preparation_for_every_turn() {
        for effort in ["low", "medium", "high", "xhigh", "max"] {
            let browser = MockBrowser::new(Mode::Final);
            let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
            let mut request = input();
            let mut payload: serde_json::Value = serde_json::from_slice(&request.bytes).unwrap();
            payload["reasoning"] = json!({"effort":effort});
            request.bytes = serde_json::to_vec(&payload).unwrap();
            coordinator
                .execute(request, CancellationToken::new())
                .await
                .unwrap();
            assert_eq!(
                *browser.requested_efforts.lock().unwrap(),
                vec![Some(effort.into())]
            );
            assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn invalid_summary_or_failed_encryption_never_completes_or_resubmits() {
        struct FailingEncoder;
        impl CheckpointEncoder for FailingEncoder {
            fn seal(&self, _: &str, _: &[serde_json::Value]) -> Result<String, &'static str> {
                Err("E_CHECKPOINT_KEY")
            }
        }
        for (mode, expected) in [
            (Mode::Checkpoint, "E_CHECKPOINT_KEY"),
            (Mode::BadCheckpoint, "E_CHECKPOINT_SUMMARY"),
            (Mode::Final, "E_TOOL_ENVELOPE_PURPOSE"),
        ] {
            let browser = MockBrowser::new(mode);
            let ledger = Ledger::in_memory();
            let coordinator = Coordinator::new(ledger.clone(), browser.clone());
            let make_input = || {
                let mut input = input();
                input.bytes = json!({"model":"webbridge/test","input":[{"role":"user","content":"fixture"},{"type":"compaction_trigger"}]}).to_string().into_bytes();
                input
            };
            assert_eq!(
                coordinator
                    .execute_with_checkpoint(
                        make_input(),
                        CancellationToken::new(),
                        Some(Arc::new(FailingEncoder))
                    )
                    .await
                    .err(),
                Some(expected)
            );
            assert_eq!(
                coordinator
                    .execute_with_checkpoint(
                        make_input(),
                        CancellationToken::new(),
                        Some(Arc::new(FailingEncoder))
                    )
                    .await
                    .err(),
                Some("E_REQUEST_ALREADY_ADMITTED")
            );
            let input = make_input();
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
    }
    #[tokio::test]
    async fn structured_output_violation_is_terminal_without_delivery_or_resubmission() {
        let browser = MockBrowser::new(Mode::Final);
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let mut request = input();
        let mut payload: serde_json::Value = serde_json::from_slice(&request.bytes).unwrap();
        payload["text"] = json!({"format":{"type":"json_schema","name":"fixture","strict":true,"schema":{"type":"object","required":["title"]}}});
        request.bytes = serde_json::to_vec(&payload).unwrap();
        let retry = TurnInput {
            request_id: request.request_id.clone(),
            session: request.session.clone(),
            bytes: request.bytes.clone(),
        };
        // The normal mock returns an attributed plain-text answer. It cannot be
        // delivered when this request requires JSON, even with a valid envelope.
        assert!(matches!(
            coordinator.execute(request, CancellationToken::new()).await,
            Err("E_OUTPUT_SCHEMA")
        ));
        assert!(matches!(
            coordinator.execute(retry, CancellationToken::new()).await,
            Err("E_REQUEST_ALREADY_ADMITTED")
        ));
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        assert_eq!(browser.releases.load(Ordering::SeqCst), 1);
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
        assert!(first.live_verified);
        assert!(!second.live_verified);
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
    async fn service_limit_is_terminal_preserves_uncertainty_and_never_resends() {
        for (mode, expected_state) in [
            (Mode::LimitedSubmit, TurnState::SubmissionUncertain),
            (Mode::LimitedObserve, TurnState::SubmissionUncertain),
        ] {
            let browser = MockBrowser::new(mode);
            let ledger = Ledger::in_memory();
            let coordinator = Coordinator::new(ledger.clone(), browser.clone());
            assert_eq!(
                coordinator
                    .execute(input(), CancellationToken::new())
                    .await
                    .err(),
                Some("E_BROWSER_RATE_LIMITED")
            );
            let request = input();
            assert_eq!(
                ledger
                    .admit(&request.request_id, &request.session, &request.bytes)
                    .await
                    .unwrap(),
                Admission::Existing(expected_state)
            );
            assert_eq!(
                coordinator
                    .execute(input(), CancellationToken::new())
                    .await
                    .err(),
                Some("E_REQUEST_ALREADY_ADMITTED")
            );
            assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
            assert_eq!(browser.releases.load(Ordering::SeqCst), 1);
            assert!(coordinator.replay.lock().unwrap().is_empty());
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
    #[cfg(windows)]
    #[tokio::test]
    async fn cancelled_request_retains_consumer_until_detached_browser_cleanup_finishes() {
        use crate::generation_handoff::ConsumerLease;
        let consumer = Arc::new(tokio::sync::Mutex::new(()));
        let gate = Arc::new(Notify::new());
        let mut browser = MockBrowser::new(Mode::Waiting);
        Arc::get_mut(&mut browser).unwrap().release_gate = Some(gate.clone());
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone())
            .with_consumer(ConsumerLease::acquire(consumer.clone()).unwrap());
        let task =
            tokio::spawn(
                async move { coordinator.execute(input(), CancellationToken::new()).await },
            );
        tokio::time::timeout(Duration::from_secs(2), browser.observing.notified())
            .await
            .unwrap();
        task.abort();
        assert!(task.await.err().unwrap().is_cancelled());
        tokio::time::timeout(Duration::from_secs(2), browser.released.notified())
            .await
            .unwrap();
        assert!(matches!(
            ConsumerLease::acquire(consumer.clone()),
            Err("E_BROWSER_IN_USE")
        ));
        gate.notify_one();
        let lease = tokio::time::timeout(Duration::from_secs(2), consumer.lock())
            .await
            .unwrap();
        drop(lease);
        assert!(ConsumerLease::acquire(consumer).is_ok());
        assert_eq!(browser.stops.load(Ordering::SeqCst), 1);
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
