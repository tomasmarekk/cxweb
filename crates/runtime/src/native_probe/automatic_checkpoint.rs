//! Live native context-boundary diagnostic. Never supplies fake usage or history.
use super::*;

const GROW_PROMPT: &str = "Use no tools. This is a context-retention test. Invent a fresh random value of exactly 32 lowercase hexadecimal characters. Start your final answer with one line containing CXWEB_NATIVE_CHECKPOINT_ followed by that value. Then write a detailed English technical explanation of pure functions, immutable data, error handling and deterministic testing, with several short JavaScript examples. Aim for about 1000 words of normal useful prose; do not count characters or use repeated padding. Do not wrap the entire answer in a code fence. Remember only the first line as the latest value for later recall and preserve it verbatim in task checkpoints; it replaces any previous value. The technical explanation is disposable test history, not task state.";
const RECALL_PROMPT: &str = "Return exactly the latest complete CXWEB_NATIVE_CHECKPOINT_ line remembered from your most recent successful answer. Recover it from the task checkpoint. Use no tools, omit the disposable technical explanation and add no other text.";

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Report {
    #[serde(default)]
    history_fixture: String,
    completed: bool,
    completed_history_turns: usize,
    actual_answer_bytes: usize,
    context_refusals: usize,
    automatic_compactions: usize,
    exact_recall: bool,
    manual_compaction_requests: usize,
    #[serde(default)]
    last_native_error: Option<String>,
}
impl Report {
    fn save(&self, cwd: &Path) -> Result<(), &'static str> {
        std::fs::write(
            cwd.join("automatic-checkpoint-report.json"),
            serde_json::to_vec(self).map_err(|_| "E_NATIVE_PROBE_AUTOMATIC_REPORT")?,
        )
        .map_err(|_| "E_NATIVE_PROBE_AUTOMATIC_REPORT")
    }
}

fn done<'a>(events: &'a [Value], thread: &str, turn: &str) -> Option<&'a Value> {
    events.iter().find(|e| {
        e["method"] == "turn/completed"
            && e["params"]["threadId"] == thread
            && e["params"]["turn"]["id"] == turn
    })
}
fn native_error(events: &[Value], thread: &str, turn: &str) -> Option<String> {
    let error = &done(events, thread, turn)?["params"]["turn"]["error"]["codexErrorInfo"];
    if error.is_null() {
        return None;
    }
    let tag = error
        .as_str()
        .or_else(|| error.as_object()?.keys().next().map(String::as_str));
    Some(
        match tag {
            Some("contextWindowExceeded") => "contextWindowExceeded",
            Some("responseStreamDisconnected") => "responseStreamDisconnected",
            Some("responseTooManyFailedAttempts") => "responseTooManyFailedAttempts",
            Some("badRequest") => "badRequest",
            _ => "other",
        }
        .into(),
    )
}
fn context_refusal(events: &[Value], thread: &str, turn: &str) -> bool {
    done(events, thread, turn).is_some_and(|e| {
        e["params"]["turn"]["status"] == "failed"
            && e["params"]["turn"]["error"]["codexErrorInfo"] == "contextWindowExceeded"
    }) && !events.iter().any(|e| {
        e["method"] == "item/completed"
            && e["params"]["threadId"] == thread
            && e["params"]["turnId"] == turn
            && e["params"]["item"]["type"] == "agentMessage"
    })
}
fn seed(text: &str) -> Result<&str, &'static str> {
    let (marker, history) = text
        .split_once('\n')
        .ok_or("E_NATIVE_PROBE_AUTOMATIC_SEED")?;
    if !marker
        .strip_prefix("CXWEB_NATIVE_CHECKPOINT_")
        .is_some_and(|value| {
            value.len() == 32
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        || !(4096..=16384).contains(&history.len())
        || history.bytes().filter(u8::is_ascii_alphabetic).count() < 1024
        || history
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err("E_NATIVE_PROBE_AUTOMATIC_SEED");
    }
    Ok(marker)
}
fn automatic_compaction(events: &[Value], thread: &str, turn: &str) -> bool {
    let relevant: Vec<_> = events
        .iter()
        .filter(|e| {
            e["method"] == "item/completed"
                && e["params"]["threadId"] == thread
                && e["params"]["turnId"] == turn
        })
        .collect();
    let compact: Vec<_> = relevant
        .iter()
        .enumerate()
        .filter(|(_, e)| e["params"]["item"]["type"] == "contextCompaction")
        .collect();
    let answer = relevant
        .iter()
        .position(|e| e["params"]["item"]["type"] == "agentMessage");
    compact.len() == 1 && answer.is_some_and(|answer| compact[0].0 < answer)
}

impl Client {
    async fn checkpoint_turn(
        &mut self,
        thread: &str,
        id: u64,
        prompt: &str,
    ) -> Result<String, &'static str> {
        self.observations.clear();
        self.turn = None;
        let result = self.rpc(id, "turn/start", json!({"threadId":thread,"input":[{"type":"text","text":prompt,"text_elements":[]}]})).await?;
        let turn = result["turn"]["id"]
            .as_str()
            .ok_or("E_NATIVE_PROBE_RPC")?
            .to_owned();
        if self.turn.as_deref().is_some_and(|old| old != turn) {
            return Err("E_NATIVE_PROBE_ACTION");
        }
        self.turn = Some(turn.clone());
        while done(&self.observations, thread, &turn).is_none() {
            let message = self.next().await?;
            self.observe(message).await?;
        }
        Ok(turn)
    }

    pub(super) async fn verify_automatic_checkpoint(
        &mut self,
        cwd: &Path,
        thread: &str,
    ) -> Result<(), &'static str> {
        let mut report = Report {
            history_fixture: "technical-prose.v1".into(),
            ..Report::default()
        };
        report.save(cwd)?;
        let mut latest = None;
        let mut seen = std::collections::BTreeSet::new();
        for index in 0..8 {
            let turn = self
                .checkpoint_turn(thread, 20 + index, GROW_PROMPT)
                .await?;
            report.last_native_error = native_error(&self.observations, thread, &turn);
            report.save(cwd)?;
            if context_refusal(&self.observations, thread, &turn) {
                if latest.is_none() {
                    return Err("E_NATIVE_PROBE_AUTOMATIC_BASELINE");
                }
                report.context_refusals += 1;
                report.save(cwd)?;
                break;
            }
            let text = self
                .observations
                .iter()
                .find(|e| {
                    e["method"] == "item/completed"
                        && e["params"]["threadId"] == thread
                        && e["params"]["turnId"] == turn
                        && e["params"]["item"]["type"] == "agentMessage"
                })
                .and_then(|e| e["params"]["item"]["text"].as_str())
                .unwrap_or("");
            if !text_complete(&self.observations, thread, &turn, text)? {
                return Err("E_NATIVE_PROBE_AUTOMATIC_SEED");
            }
            report.actual_answer_bytes += text.len();
            report.save(cwd)?;
            let marker = seed(text)?.to_owned();
            if !seen.insert(marker.clone()) {
                return Err("E_NATIVE_PROBE_AUTOMATIC_SEED");
            }
            latest = Some(marker);
            report.completed_history_turns += 1;
            report.save(cwd)?;
        }
        if report.context_refusals != 1 {
            return Err("E_NATIVE_PROBE_AUTOMATIC_BOUNDARY");
        }
        // This is a new, explicit diagnostic user turn after a positively
        // observed pre-submission refusal, not a resend or compact/start RPC.
        self.compacting = true;
        let turn = self.checkpoint_turn(thread, 40, RECALL_PROMPT).await?;
        self.compacting = false;
        report.last_native_error = native_error(&self.observations, thread, &turn);
        report.save(cwd)?;
        if !text_complete(
            &self.observations,
            thread,
            &turn,
            latest.as_deref().ok_or("E_NATIVE_PROBE_AUTOMATIC_SEED")?,
        )? || !automatic_compaction(&self.observations, thread, &turn)
        {
            return Err("E_NATIVE_PROBE_AUTOMATIC_RECALL");
        }
        report.automatic_compactions = 1;
        report.exact_recall = true;
        report.completed = true;
        report.save(cwd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_is_real_bounded_text_and_never_abbreviated() {
        let marker = format!("CXWEB_NATIVE_CHECKPOINT_{}", "a".repeat(32));
        for count in [4096, 16384] {
            assert_eq!(
                seed(&format!("{marker}\n{}", "a".repeat(count))),
                Ok(marker.as_str())
            );
        }
        for history in [
            "a".repeat(4095),
            "a".repeat(16385),
            "*".repeat(4096),
            "a".repeat(4096) + "\0",
            "[explanation omitted]".into(),
        ] {
            assert!(seed(&format!("{marker}\n{history}")).is_err());
        }
        assert!(seed(&format!("{}\n{}", marker.to_uppercase(), "a".repeat(4096))).is_err());
    }
    #[test]
    fn refusal_and_automatic_compaction_require_attributed_native_events() {
        let failed = json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"r","status":"failed","error":{"codexErrorInfo":"contextWindowExceeded"}}}});
        let compact = json!({"method":"item/completed","params":{"threadId":"t","turnId":"r","item":{"type":"contextCompaction"}}});
        let answer = json!({"method":"item/completed","params":{"threadId":"t","turnId":"r","item":{"type":"agentMessage"}}});
        assert_eq!(
            native_error(std::slice::from_ref(&failed), "t", "r").as_deref(),
            Some("contextWindowExceeded")
        );
        let private_error = json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"r","error":{"codexErrorInfo":{"unexpectedPrivateValue":"must not be retained"}}}}});
        assert_eq!(
            native_error(&[private_error], "t", "r").as_deref(),
            Some("other")
        );
        assert!(context_refusal(std::slice::from_ref(&failed), "t", "r"));
        assert!(!context_refusal(
            &[failed.clone(), answer.clone()],
            "t",
            "r"
        ));
        assert!(!context_refusal(&[failed], "other", "r"));
        assert!(automatic_compaction(
            &[compact.clone(), answer.clone()],
            "t",
            "r"
        ));
        assert!(!automatic_compaction(
            &[answer.clone(), compact.clone()],
            "t",
            "r"
        ));
        assert!(!automatic_compaction(
            &[compact.clone(), compact.clone(), answer.clone()],
            "t",
            "r"
        ));
        assert!(!automatic_compaction(&[compact, answer], "other", "r"));
    }
}
