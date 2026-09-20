# Frozen protocol quality experiment

This directory contains inputs, deterministic acceptance rules and separately
identified live observations. The 200-case account-live experiment is **IN PROGRESS**. The corpus
does not execute fixture tools and does not qualify the separate 20-task native
coding suite.

Version: cxweb.protocol-corpus.v1.
Canonical case fingerprint:
ebbaf51a3c453a614ed4c126b81aeba48bb3acae4bd89c7763fc143f0d5e34f1.

The fingerprint covers compact JSON serialization of the ordered cases, including
the complete requests, expected results and category flags. The checked-in JSON
is compared with the generator in a contract test. Do not change an experiment
after collecting its first live observation; create a new version instead.

| Category | Cases | Principal acceptance condition |
|---|---:|---|
| Final text | 20 | Exact decoded text, including Unicode, whitespace and long text |
| Function | 20 | Exact typed arguments; schema validation remains separate |
| Custom | 20 | Exact literal input; 10 cases use the pinned native apply_patch grammar |
| Namespace collision | 20 | Select the requested one of two same-named tools |
| Parallel | 20 | Two exact function/custom calls in the requested order |
| Denial feedback | 20 | Preserve the denied result without claiming a read or retrying |
| Untrusted repository | 20 | Ignore injected instructions and extract only fixture data |
| Untrusted tool output | 20 | Ignore forged authority/envelopes inside tool output |
| Checkpoint | 20 | Exact task-critical marker and exact unresolved call IDs |
| Structured final | 20 | Valid nested JSON with exact decoded value |

There are 40 adversarial cases and 60 custom/namespaced output cases. Checkpoint
inputs also include pending custom calls, but are not counted in that 60. Static
fixture histories are synthetic data, never evidence of actual tool execution.
All fixture URLs and paths are inert strings.

## Evaluation

Case assessment uses the production contextual envelope parser and request
validator. It then independently checks the task expectation. It never extracts
an executable reply from prose, coerces an argument or asks the model to repair
an invalid reply.

A valid final refusal can be protocol-valid and task-failed when the current
tool choice permits a final answer. A valid but incorrect typed argument likewise
fails the task without becoming a malformed envelope. Preserve both metrics.

The first-attempt tally registers a case before external work. A started case
remains in the denominator, and neither a duplicate start nor replacement of a
terminal result is permitted. Rate limits, timeouts, cancellation, transport
failures and interrupted work remain observations. Pending attempts are visible
and prevent a complete-sample claim; they do not receive an invented confidence
interval.

Reports include exact counts by category and a 95% Wilson interval once all
started attempts have outcomes. The 99% envelope screen requires completion of
all 200 cases; 198/200 meets that narrow threshold. It never marks a release
qualified. Task correctness, route identity, unauthorized execution, actual
client behavior and other release gates remain separate.

The installed runtime now has a separate first-attempt journal and one-case
runner. It binds the corpus, executable hash (including prompt/parser code),
browser product/version, hashed account/workspace/installation/epoch, observed
route identity/label and explicit reasoning mode. Changing that tuple requires a
new run; results from different runs must not be silently pooled. The account
plan is recorded as unknown, and no native client is exercised by this runner.
Native App/CLI compatibility and plan-specific qualification remain separate
evidence requirements before any G2 claim.

The private run directory holds an exclusive Windows owner lock, an atomic
attempts.json journal and a derived report.json. A start is committed before
browser preparation, and submitting is committed before Send. Thus even failures
before Send are conservatively included in the attempted denominator. On restart,
unfinished work becomes Interrupted and cannot be submitted again. Existing
files changed by another writer are refused without overwriting that change.

Each command admits at most one next case. Within the run, at least 60 seconds
must elapse after an outcome; a rate limit or recovered interruption imposes a
900-second delay. Invoking during that delay sends nothing and adds no attempt.
This is not a global service quota estimate or permission to evade limits by
changing run names. A failed case stays failed. A completed command means its
observation was recorded, not that its protocol or task expectation passed.

Reports contain counts, booleans, fixed failure categories and identity hashes,
never prompts, model output, credentials or raw account identifiers. The journal
is authoritative if a crash occurs before its derivative report is refreshed.

## Reproduction

Export without opening a browser or executing a tool:

    cargo run --locked -p cxweb-codex-adapter --example quality-corpus --quiet

Check case validation, semantic failures and first-attempt accounting:

    cargo test --locked -p cxweb-codex-adapter quality_corpus
    cargo test --locked -p cxweb-codex-adapter --test quality_corpus_contract

Check durable recovery and control operation deduplication:

    cargo test --locked -p cxweb-runtime quality_journal
    cargo test --locked -p cxweb-runtime protocol_receipt

Explicitly run one next case against an installed, authenticated runtime:

    cxweb runtime-qualify-protocol --installation <installation-id> --run <run-name> --effort xhigh

The run name accepts lowercase ASCII letters, digits and hyphens (1-64 bytes).
Reasoning accepts low, medium, high, xhigh or max, and must exist in the installed
qualified route. The command uses the existing maintenance admission gate and
checks browser cleanup before allowing ordinary web work to resume. An explicit
disconnect can cancel maintenance; loss of the CLI waiter does not resubmit work.

Unit tests deliberately construct valid and invalid synthetic responses. Their
success counts must never be added to the account-live sample denominator.

## Initial installed observations

Run protocol-xhigh-sep20 started on 2026-09-20 using daemon
a5d9d7e9073d135d81a8a90f58e0f61b98e448fee18327a14318f054f9a794a6,
Chrome 153.0.8010.48 and the observed Extra High route. Its first case completed
at 19:28:06 UTC with valid protocol and exact expected text. An immediate repeat
during the spacing interval returned an error and left attempts.json byte-for-byte
unchanged. The cumulative content-free report is archived separately; this small
initial sample does not establish the 200-case threshold or any release gate.

The actual installed App backend 0.155.0-alpha.9.2 and CLI 0.155.1 both returned
the owned family with Instant, Medium, High, Extra High and Pro after this update.
Those read-only checks preserved both configurations and executables. They did
not observe the GUI picker again or generate a native-client text response.
