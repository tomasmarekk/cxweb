# Frozen protocol quality experiment

This directory contains inputs and deterministic acceptance rules, not live
model results. The 200-case account-live experiment is **NOT RUN**. The corpus
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

The tally currently operates in memory. Before a live batch can count toward G2,
its runner must persist starts before submission, retain every terminal outcome,
recover interrupted attempts without resubmission, and bind the run to the
corpus hash, exact runtime/browser/client builds, prompt/parser implementation,
account plan category, observed route and reasoning mode. No live batch runner
or durable result journal is provided by the exporter.

## Reproduction

Export without opening a browser or executing a tool:

    cargo run --locked -p cxweb-codex-adapter --example quality-corpus --quiet

Check case validation, semantic failures and first-attempt accounting:

    cargo test --locked -p cxweb-codex-adapter quality_corpus
    cargo test --locked -p cxweb-codex-adapter --test quality_corpus_contract

Unit tests deliberately construct valid and invalid synthetic responses. Their
success counts must never be added to the account-live sample denominator.
