# Native arithmetic repair exercises

Status: **account-live execution IN PROGRESS; CLI and App sum passed**. These four exercises are
a subset of coding acceptance, not the complete 20-exercise G2 suite and not
additional protocol-corpus samples.

Version: cxweb.arithmetic-repairs.v1. Ordered specification SHA-256:
b073abc6cc040afaf32f3b72621c5aaf8ec73c07682a086ec774744e05291af5.

The fixed cases cover sum, inclusive interval count, absolute distance and ceiling
page count. Each starts with a function that fails its offline tests. Reference
solutions validate the fixtures themselves and are not sent to the model. The
model receives the problem and must read the actual function and cases through
Codex, run the failing tests, patch the return expression, rerun the tests and
acknowledge success only after the actual successful native result.

The fixture permits only a bounded ASCII expression over primitive numeric a/b
parameters and operators in an exact function wrapper. Strings, other names,
property access, arbitrary statements and additional function bodies are denied.
The trusted test runner revalidates source before evaluating it with primitive
inputs, disabled code generation and a deadline. This is not an arbitrary-code
sandbox. Different valid solutions can pass; approval does not compare against
the reference expression. Incorrect but admissible expressions fail the tests.

Only one attributed update to solve.cjs and the exact read/test commands can be
approved. The tests and test cases must remain unchanged. Ordered native item
results, actual exit codes, final file bytes and final-answer ordering determine
success. Local tests of this harness do not count as native client or account
execution. A live failure must be retained; rerunning a case is another attempt,
never a replacement for its first outcome.

Run local fixture and approval checks:

    node --test scripts/coding-fixture.test.mjs scripts/coding-fixture-approval.test.mjs scripts/probe-client-approval.test.mjs

Run one explicit installed-client exercise only when the browser account is idle:

    node scripts/probe-installed-client.mjs <reviewed-client-exe> <absolute-native-home> <owned-model> --coding=sum

Other IDs: inclusive-count, absolute-distance, ceiling-pages. Use each reviewed
native client separately. The existing probe binds reviewed executable hashes,
uses installed routing without config overrides and stores content-free results
in its unique local workspace. Native source/test artifacts are synthetic fixture
data. Broader sequence journaling, cancellation/concurrency exercises and the
remaining coding scenarios still require separate acceptance work.

Local validation: all 18 fixture/approval tests passed, including a real local
Node process exiting 1 before repair, 0 after repair and 2 for rejected source.
The standalone probe-arithmetic-patch.mjs also passed on actual CLI 0.155.1 and
App backend 0.155.0-alpha.9.2 using a synthetic loopback model and isolated native
homes. Both emitted and applied the exact attributed source patch. The later
--repair mode also passed read/fail/patch/pass on both clients with four scoped
approvals and five synthetic model responses. These are
native-client mock integration results, not account-live successes.

## Live observations and harness correction

- CLI sum at 20:23:04 UTC and inclusive-count at 20:24:30 UTC were refused before
  execution by the test client's overstrict environment guard. The latter
  diagnostic identified a non-null environment ID on an otherwise exact command.
- The reviewed [CLI environment registry](https://github.com/openai/codex/blob/rust-v0.155.1/codex-rs/exec-server/src/environment.rs)
  and [App environment registry](https://github.com/openai/codex/blob/rust-v0.155.0-alpha.9.2/codex-rs/exec-server/src/environment.rs)
  reserve local for the host environment. The guard now admits that exact ID,
  while rejecting other IDs, extra permissions and session-wide decisions.
  Its new regression failed before the correction and passed afterward.
- A separately recorded CLI sum attempt at 20:27:06 UTC successfully read the
  source/cases and ran the real failing test, then declined the proposed patch.
  Its raw proposal was not retained; its precise rejection cause is unknown.
- The first App sum attempt at 20:32:16 UTC likewise read the files and observed
  the real failure. Its captured synthetic source proposal used one indentation
  space instead of the original two. The exact wrapper guard refused it. It is
  not yet established whether the model or rendered-text extraction changed
  that whitespace. No test acceptance was weakened to claim success.

The first four failed live observations remain archived separately. They are excluded
from success claims, not erased or replaced. Neither unattempted remaining case
nor either native client is qualified by this small exploratory set. The native
schemas also use move_path for file moves; both spelling variants are explicitly
denied by repair approval, with regression coverage.

## DOM projection follow-up

A real-Chrome regression subsequently demonstrated that innerText folds repeated
spaces within JSON strings. Bounded DOM text projection now preserves those
spaces, and the regression passes. This establishes an adapter defect without
claiming the unavailable pre-rendered bytes of the earlier App attempt.

- CLI sum ended at 20:49:10 UTC on daemon 8c508b932b023172a1da40cc175fcf539ca5eee1f36898cc415286eb709b46b8.
  The actual patch preserved both indentation spaces and produced the correct
  function. Its native retest exited 1; a subsequent direct local test passed.
  Numeric retest results were not retained by that harness version. The cause
  remains unknown. The runner now reports VM failures separately from wrong
  arithmetic instead of swallowing them, without increasing its deadline.
- App sum ended at 20:55:51 UTC on daemon 0fc921ba774a4f5c53fb53f862bc04d8fa070efea3763830bd8077aee533d041.
  The read, initial failing test and admissible patch completed. The next command
  did not match the exact permitted test action and was declined. Its precise
  mismatch was not retained. The harness now classifies command payload and
  working-directory differences without retaining command contents or changing
  approval criteria.

These two failures are archived as each client's arithmetic-dom-text-first.json.
No failed observation is replaced by a subsequent attempt. Offline diagnostic
success does not qualify an account-live coding route.

CLI 0.155.1 subsequently passed sum at 21:02:06 UTC on daemon
0fc921ba774a4f5c53fb53f862bc04d8fa070efea3763830bd8077aee533d041
with runner SHA-256 6ec042e52764680f0bc881cfac4668ddc46435db00571f7c01667e30cb780478.
The actual client read the files, observed 3/4 failing cases, applied an admissible
patch, observed all four passing and only then acknowledged completion. Tests,
cases, native configuration and executable remained unchanged. The independent
attempt is archived as cli-0.155.1.arithmetic-sum-dom-text-pass.json. It does not
erase the earlier failures or qualify unattempted cases.

App backend 0.155.0-alpha.9.2 also passed sum at 21:05:19 UTC on the same daemon
and runner. The actual read, failing tests, source correction, passing retest and
final-answer order were verified, with tests, cases, config and executable
unchanged. Its separate report is
app-backend-0.155.0-alpha.9.2.arithmetic-sum-dom-text-pass.json.
Both reports include all five qualified reasoning choices; this particular coding
exercise used xhigh. These backend runs are not fresh GUI picker observations.
