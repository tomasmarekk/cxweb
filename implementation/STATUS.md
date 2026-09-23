# cxweb implementation status

## Public status streaming implementation (2026-09-21)

The post-update App read/apply_patch regression passed. The first CLI deferred
MCP regression completed discovery but its next browser preparation failed with
E_SESSION_SCOPE. The ledger records Failed at revision 1, before submission
intent; that failed attempt is preserved in the live report. Scope observation
now confirms a quick incomplete read once on the same page, after 200 ms. An
observed account/workspace reaches the existing mismatch check immediately;
restrictions, other errors, slow reads and persistent uncertainty are not retried.
No model submission is retried. The regression test and runtime clippy pass;
the follow-up daemon built and was installed with SHA-256
`3e9f166512423fac1df53bf037ba6abaf11b5e5a77e288c319ff91aa2e7de70e`.
The saved profile again recovered without login. Subsequent App read/apply_patch
and CLI deferred MCP discovery/search/exact-result recall passed on this build;
health returned Ready with both clients request-verified. Two earlier follow-up
MCP attempts remain recorded as failures: one omitted the MCP call, and another
completed the real call but failed exact result recall. Their provider error lists
are empty. A later passing attempt does not erase those failed tool-use/recall
checks or establish unrestricted tool reliability.
CI passed for both implementation commits: 4ab486c (run 35596438043) and
cefa7af (run 35597338564), including workspace tests, all-target clippy and the
configured JavaScript suites. The probe now retains private diagnostic details
on a missing MCP call or an exact-recall mismatch; published evidence remains
sanitized. Its updated JavaScript syntax check passed.

The WebSocket path now carries public ChatGPT status while the response is still
generating. A bounded channel leaves the existing request/drain owner in charge;
the final response and all tool inputs remain fully buffered and validated.
The coordinator checks turn attribution and account/workspace scope before each
new public-status batch. Public status remains optional (`summary: none` suppresses
it) and cannot originate in the model's JSON envelope. No hidden reasoning is read.

The relay requires the completed event sequence to extend the exact emitted
prefix, including IDs, text, timestamps and sequence numbers. It sends the suffix
once, preserves native quota events, and cancels/drains on client disconnect.
A later validation failure emits a terminal error, never a successful completion.
HTTP delivery is still buffered. This change transports the observed public
status; it does not establish detailed reasoning content or final-text streaming.

Validation: adapter 43 unit tests passed; runtime 234 passed / 10 opt-in tests
ignored; clippy for both packages/all targets passed; formatting and JavaScript
syntax checks passed. New tests cover early socket delivery, exact completion
suffixes, replay without a second generation, scope/attribution failures, revision
failures, cancellation/drain, a closed receiver and `summary: none`.
`probe-installed-client.mjs --reasoning-live` measures summary arrival before the
final message in native notifications. Both actual installed clients passed: App
0.155.0-alpha.9.2 received public status 24,584 ms before its final message; CLI
0.155.1 received it 21,271 ms earlier. Both returned the exact verified arithmetic
answer. Configuration and native executables were unchanged. The public content
was the rendered Thinking status; a detailed reasoning transcript was not observed.

The idle scheduled runtime was updated to source 4ab486c (daemon SHA-256
`9334f186d96942f03712ee8339853c0ffecd76354663f9dccc95ebfc2e74d915`). Its saved
session recovered in the background without another login. The stable daemon was
also updated; the running MCP process still owns the unchanged stable CLI binary.
All three release binaries built successfully. Evidence is recorded in
`integration-tests/quality/public-status-live-sep21.json`; the older report below
remains evidence for the prior buffered-status execution build.

## Latest installed verification (2026-09-21, 11:37 UTC)

The user completed the requested ChatGPT verification. The saved English session
now works in the background with no visible Chrome window required. Installed
health is Ready, and the actual cxweb panel shows Connected, ChatGPT Verified,
Codex App Request verified and Codex CLI Request verified.

Four live probes passed through the installed route and actual native binaries:
deferred MCP discovery/search on App backend 0.155.0-alpha.9.2 and CLI 0.155.1,
then native file read/apply_patch on each. Search returned three real public
results and the model recalled the exact first title and URL. The coding probes
read a random fixture marker absent from the prompt, applied the exact patch and
verified the resulting file. Codex executed the tools and handled approvals;
the harness did not execute them. Configuration and native executables remained
unchanged. The discovery probes used process-only tool-search feature flags;
neither routing nor the read/patch probes used overrides.

Both catalogs include ChatGPT Web Latest alongside native models and expose
Instant, Medium, High, Extra High and Pro. These backend probes complement the
previous signed GUI picker proof; they do not repeat a GUI coding test.
Public reasoning events carry the observed Thinking status, buffered until
completion. Detailed live reasoning, hosted OpenAI web_search and unrestricted
native feature parity are not established. Full PRD release gates remain open.

Evidence: `integration-tests/quality/client-tools-live-sep21.json`. Active execution
build is ad72c92 (daemon SHA-256
`57ace28b540e50b4124e54e843ec1146f6a87fc81ae4749c8b970d643c5ad309`). Stable binaries
and desktop shortcut are updated to 3c7e04e, whose additional change concerns
fresh-install MCP setup. The working active session was preserved. CI passed for
both commits (runs 35591303550 and 35592637567). Account-verification blocks in
the earlier chronology below are resolved by these observations.

## Acceptance and current gate

Immediate delivery priority, restated by the user on 2026-09-21: make ChatGPT Web
work in actual Codex CLI and Codex App with coding tools, MCP, web research, public
reasoning and all five effort choices. Preserve saved sign-in and background
operation. No subagents. This priority does not establish completion of the full
PRD or its release gates; those remain open wherever evidence below is incomplete.
Manual login and managed reuse of the saved session are user-confirmed. Scoped
background text generation and native read/apply_patch cycles now pass through
both actual native backends. The actual CLI picker passed isolated synthetic and
authenticated browser round trips, including its structured auxiliary request.
Broader native feature coexistence, coding/model qualification and release gates
remain incomplete. The setup activation path is
implemented and production routing is installed on this machine. Verification of
the installed catalogs and authenticated text requests passed both native builds.
The real installed CLI /model picker displays the owned row. A fresh signed App
instance displays the owned family and all five reasoning choices. Its full picker
and native/web switching passed, as did a GUI text round trip. Both native backends
pass web/native/web text generation in one process through the installed runtime.
Scheduled browser recovery now passes after removing an MSIX data-path ambiguity.
Earlier installed CLI and App-backend read/patch/final scenarios and a fixed GUI
message passed with the owned model and effort corroborated by native turn
context. Earlier live state (2026-09-21, 06:48 UTC): the user completed the new
ordinary-Chrome login and closed its window. Same-account, model, reasoning and
English background recovery passed. Both actual native backend catalogs include
all five choices alongside native models; a fresh xhigh text round trip passed
on each backend without config overrides. Health reached Ready with independent
request-success observations for App and CLI. That installation was subsequently
removed manually by the user during network troubleshooting, including its profile
and journal; the user also edited config.toml. At 07:42 UTC the native backend
preflight passed against the user's current configuration without modification,
and no cxweb/loopback override remained. The now-orphaned scheduled action was
disabled after checking its exact former runtime path. A new ordinary-Chrome
setup login was completed and closed by the user. Fresh English background text
and tool protocol checks passed, and a new installation was activated against the
user's edited configuration. Initial reasoning maintenance exposed a missing
controller in the fresh-activation path; the fix and regression test now pass.
The deployed runtime passed all eight additional reasoning checks and both native
catalogs expose all five choices. Both native backends also completed the actual
absolute-distance read/failing-test/patch/passing-retest exercise. Health is Ready.
A fresh signed GUI instance displays the owned family and all five positions;
an independent GUI task also returned the exact test marker. Resuming an existing
task delegated to the original App backend, which retained its old catalog and
used fallback metadata; that attempt failed before generation. Broader coding
qualification remains in progress; no release gate is certified.

## Automatic web-search installation (2026-09-21)

- Fresh production setup now includes its MCP search entry in the same atomic
  configuration journal as the Codex route. The entry has an installation-specific
  name, points to that installation's private daemon, and uses `--web-tools`.
  The daemon serves stdio without starting the host, browser or a console window.
  There is no extra executable or Node/Python dependency for this feature.
- Journal format v3 records the owned executable and validates its exact location
  on recovery. Existing v1/v2 journals remain supported without migration. Fresh
  installations require the new daemon before the v3 transaction is committed;
  setup already copies and verifies that exact executable before activation.
- Removal deletes only an unchanged owned MCP entry. Other servers and changed
  entries are preserved; a colliding entry is never overwritten. Repeated removal,
  a previously absent config, private journal recovery and a substituted executable
  path are covered by regression tests. Existing installed v2 connections retain
  their current configuration; the working manually installed search entry on this
  workstation is not rewritten by this change.
- Validation: adapter 42 unit tests plus all contract tests passed; runtime 227
  passed / 10 opt-in tests ignored; daemon argument and actual-stdio process tests
  passed; clippy for all three affected packages/all targets passed. CI for prior
  commit ad72c92 completed successfully (run 35591303550), including the complete
  workspace test suite and JavaScript checks. CI for this follow-up also passed
  (run 35592637567).
- The subsequent user verification and four installed live probes passed as
  recorded above. This closes the installer gap and the pending scoped retests;
  it does not certify the full release gates.
## Client tools, public status and cold startup (2026-09-21)

- Added native client-executed tool discovery, including definitions returned in
  `tool_search_output`. Loaded function/namespace schemas now enter the next
  request's callable registry; explicit current definitions take precedence.
  Discovery calls retain their native wire type, object arguments and correlated
  results. Structured MCP text results and public reasoning survive follow-up
  history and websocket reconstruction.
- Added `cxweb web-tools`, a standard stdio MCP server for public Bing search.
  It returns actual source titles, URLs and snippets without account credentials.
  Both installed native backends completed real MCP search and exact result recall.
  A separate scheduled-process test verified the executable/config outside the
  Codex MSIX container. The stable binary location is under `.cxweb-runtime/bin`.
  This is client MCP search, not OpenAI-hosted `web_search`. Automatic MCP setup
  for fresh installations was added in the follow-up recorded above.
- Public ChatGPT status is delivered as native reasoning-summary events. The live
  App-backend test returned the exact expected answer and the actual rendered
  `Thinking` status. It does not prove detailed reasoning or live streaming:
  delivery remains buffered until completion and scope revalidation.
- Actual deferred MCP discovery exposed a missing historical-definition merge;
  that failure is preserved in the development report. The fix passes the five
  adapter regression tests. The subsequent live deferred-discovery retest passed
  on both installed native backends after the user completed verification.
- A visible local diagnostic repeatedly saw a missing model control and later
  became ready without interaction. Background restoration now allows 90 seconds
  for composer/model hydration instead of 15. All account, language and model
  checks remain required. A failed hydration offers an explicit background retry.
- Validation: adapter unit/contract suites passed; runtime 225 passed / 10 ignored;
  runtime clippy passed; public-summary and installed-panel JavaScript tests 35
  passed; all three Windows release binaries built successfully. Latest daemon
  SHA-256: `57ace28b540e50b4124e54e843ec1146f6a87fc81ae4749c8b970d643c5ad309`.
  The updated panel and runtime are installed. The later live verification above
  supersedes the account-verification block observed at this development stage.
- Evidence: `integration-tests/quality/client-tools-development-sep21.json`.
  Full PRD release gates and unrestricted native feature parity remain unproven.
## Windows permissions and control panel (2026-09-21)

- CI exposed an inherited-DACL replacement difference absent on this Windows
  workstation. The targeted fixture captured an Administrators-owned legacy ACL
  receiving explicit copies of its inherited grants during `ReplaceFileW`.
  Merely marking the staging descriptor auto-inherited did not fix it; that
  change was removed. Staging remains private until publication checks pass.
  It then receives the destination policy. After replacement, the exact staged
  identity and candidate bytes must match. Only the observed, exact legacy
  duplication pattern permits restoring the original DACL through the held
  handle; arbitrary ACL changes still fail. Owner, order, rights, protection and
  inherited provenance remain checked. The new inherited regression now passes
  on the CI runner; the full job is still pending.
- The connected panel now shows three state rows and one primary action, with
  models/reasoning and removal in details. Closing it hides the window in the
  notification area. A single-instance handler restores the existing panel on
  relaunch and ignores all supplied arguments. Visibility actions do not stop
  the supervisor or change installed routing.
- Actual Windows UI verification: closed the panel while a live response was in
  progress, observed its window disappear while the process survived, then
  relaunched. The second process exited successfully and the original window
  and process returned. The background response completed successfully and the
  panel returned to Connected. Direct tray-menu interaction remains unverified.
- Validation: platform tests 45 passed / 3 explicitly ignored; platform and
  desktop clippy passed; desktop UI tests 75 passed; release desktop build passed.
  The exploratory account-live cohort stopped after case 20 timed out: 19/20
  protocol-valid and task-correct responses. The failure remains in
  `integration-tests/quality/protocol-restored-xhigh-sep21.json`. No retry or
  further cohort is scheduled after the user's narrowed delivery request.
- Full CI subsequently exposed a built-in Administrator SDDL alias and an
  ancestor test that depended on incidental host ACLs. Ownership now compares the
  actual SID and DACL protection bit; the negative fixture grants DELETE_CHILD
  explicitly. Local platform tests and clippy pass. Full CI remains separate
  from the verified local App/CLI handoff.

## Native loopback benchmark (2026-09-21)

- Added an explicit release-only benchmark using 32 KiB synthetic request and
  response data, fixed fake authorization, actual production TCP peer admission,
  HTTP classification/forwarding and exact response checks. No account, browser
  or installed configuration is accessed by the benchmark.
- After 100 warmup pairs, 1,000 alternating-order pairs measured incremental
  median 0.1971 ms and nearest-rank p95 0.3255 ms. The fixture met the 20 ms p95
  target on this workstation. The result records hardware, OS, existing power
  plan, concurrent activity and executable/source hashes. This is a shared-host
  HTTP keep-alive measurement, not G4 certification or an idle-resource result.
- Artifact: `integration-tests/benchmarks/loopback-sep21.json`; reproducible
  procedure and limitations: `integration-tests/benchmarks/LOOPBACK.md`.

## Native namespace and task isolation (2026-09-21)

- Added installed-client exercises for two dynamic tools with the same leaf name,
  one-response call batches and overlapping native tasks. Fixture outputs are
  generated independently and not included in the prompt. Exact namespace,
  Unicode arguments, unique call IDs, actual results and final-answer order are
  checked. No shell or model-provided code runs in the dynamic fixture handlers.
- Both actual native backends passed namespace/result isolation and a batch whose
  two call IDs identify the same production response. Both also passed two
  overlapping tasks in one process with distinct xhigh/medium selections and
  exact independent answers. Configurations and executables remained unchanged.
- Preserved one failed parallel-dispatch experiment. Its barrier incorrectly
  required concurrent execution of dynamic tools; the reviewed native executor
  intentionally serializes those tools. The replacement batch test proves a
  different claim: both calls originate in one response and native execution
  policy remains authoritative. No native capability was patched or overridden.
- Evidence: namespaces-sep21.json and concurrent-tasks-sep21.json. Four fixture
  regression tests pass, including foreign scope, altered arguments, duplicate
  execution and false batch attribution. Other fixture/approval tests passed.
  These exercises add G2 coverage without completing its full acceptance suite.
- A new frozen protocol cohort, restored-xhigh-sep21, runs on daemon
  6bbdf9054900f933fb29fb9582c8984a31b28d31aeffa9fa063564a24feb06a3.
  The first case passed. The serial runner preserves every attempt, respects the
  journal's spacing and stops on a transport failure or extended backoff. Earlier
  failed cohorts remain archived; the current 200-case result is incomplete.

## Fresh activation maintenance and protocol diagnostics (2026-09-21)

- Preserved the first bootstrap text failure (E_QUALIFICATION_PROTOCOL). Its old
  collector discarded the response; the cause is unproven. Added typed structural
  diagnostics for JSON object shape, protocol/nonce agreement and validation versus
  expected-output failure, without returning any response content. A new explicit
  text test and a separate tool test passed after rebuilding the collector.
- The user-completed login survives closing the ordinary window and restarting
  the setup owner. Background discovery observed all five English reasoning modes
  for Latest, Sol and GPT-5.5. Discovery is not qualification of those families.
- Activation preserves the user's current configuration as its reversible baseline.
  The first reasoning command failed before browser work because prepared hosts
  lacked the maintenance controller that restarted hosts already received.
- Fresh activation now attaches its existing browser/coordinator to the shared
  provider slot and installs the maintenance controller immediately. Catalog updates
  and re-login therefore use the same provider slot as normal requests. It neither
  creates a second browser owner nor starts background recovery at activation.
- The regression invokes prepared-host maintenance before any restart and checks
  that the attached provider receives it without applying configuration. Runtime
  tests: 220 passed, 9 opt-in ignored. Scoped all-target Clippy and formatting pass.
  Live restoration evidence is in setup-restoration-sep21.json. All eight additional
  reasoning checks passed, preserving the already qualified Extra High default.
  Both actual backend catalogs contain the owned family and all five choices.
- Actual CLI and App-backend absolute-distance repairs passed at 08:32:46 and
  08:35:58 UTC. Each observed two genuinely failing cases, patched the source via
  native apply_patch, ran the real tests successfully and answered only afterwards.
  Configuration/executable hashes and test inputs remained unchanged. The earlier
  scope failure is retained separately; this new installation's passes do not
  rewrite it. See arithmetic-distance-restored-sep21.json. This is one exercise
  per backend, not completion of the required G2 suite.
- Fresh signed GUI picker verification observed Latest alongside native models,
  and selected Light, Medium, High, Extra High and Max. These native labels map to
  Instant, Medium, High, Extra High and Pro on the web. The original running App
  still holds its previous native-only catalog. An existing-thread GUI attempt
  failed with E_UNSUPPORTED_REASONING_SUMMARY. Native logs confirm it was handled
  by the original backend using fallback metadata, not the fresh window's backend.
- A separate GUI task handled by the fresh backend returned the exact marker at
  08:57:43 UTC. Its native turn context selected the owned model and xhigh; the
  configured detailed summary was correctly omitted by native model capabilities.
  No catalog capability change or summary-request suppression was needed. The
  failed old-owner attempt remains in app-picker-restored-sep21.json. Added a
  terminal error explanation advising a complete Codex restart for stale catalogs;
  explicit summary requests remain rejected. Scoped provider tests: 12 passed,
  3 opt-in ignored; all-target runtime Clippy passed. The separate GUI instance was
  closed and its three configuration changes restored to the exact pre-test bytes.
- Fresh approval-denial exercises passed in both installed native backends at
  09:03:47 and 09:04:52 UTC. The exact command was declined, its file marker was
  never exposed, and neither an alternative tool nor a file change occurred.
  See denial-restored-sep21.json. The diagnostic-only runtime update was deployed
  while idle and its binary hash verified as
  6bbdf9054900f933fb29fb9582c8984a31b28d31aeffa9fa063564a24feb06a3.
  Background session recovery passed without reopening login.

## Interrupted coding observation and manual state removal (2026-09-21)

- The CLI absolute-distance exercise ended at 06:52:56 UTC with E_SESSION_SCOPE
  after an attributed file read and real failing test (2 of 4 passed). No patch
  was approved. The terminal report is preserved as arithmetic-distance-sep21.json;
  the absent process handle and completed evidence file were checked before doing
  further work. It was not silently restarted or counted as passed.
- On continuation the old runtime executable, profile and journal were absent.
  The user confirmed deleting .cxweb-runtime and editing Codex configuration.
  The exact relation between network trouble/removal and the earlier scope error
  is unknown. The current native subscription/catalog preflight passed with zero
  model requests, unchanged configuration and verified executable identity.
- Recreated setup state through the application's normal private control path.
  One ordinary English login Chrome process was observed without debugging flags.
  No stale journal was invented and the user's unrelated configuration remains
  the baseline for a new installation. Saved native Codex authentication is intact.

## Safe support diagnostics (2026-09-21)

- Added explicit Copy safe diagnostics and Export safe diagnostics actions under
  installed connection details. They use read-only private IPC with the currently
  selected installation and runtime instance; collecting never triggers login,
  recovery, generation or an upload. CLI exposes the same report through
  `runtime-diagnostics --installation <id>`.
- The report includes collector version, OS/architecture and typed health only.
  Dynamic schema text, unknown error strings and non-timestamp observation text
  are discarded or replaced by a fixed redaction code. It excludes installation
  and instance identities, paths, model labels, account data, credentials, logs and
  content. Collector version is not presented as the running daemon/client build.
- The native Save dialog supplies the destination directly to Rust. The webview
  cannot supply output content or a path. Cancellation writes nothing, existing
  files are preserved, and blocking dialog/file work stays off the async reactor.
  Only the two fixed app commands are granted; no generic dialog/filesystem IPC
  capability was added. Tauri dialog 2.7.3 is locked with its dependencies.
- Tests cover redaction of hostile values, new-file writes and preservation,
  explicit copy/export, duplicate clicks, cancellation and sanitized failures.
  328 workspace tests passed (22 opt-in ignored), all 75 UI tests passed, and
  all-target Clippy/format/diff checks passed. Actual collection from the installed
  daemon passed and reported E_LOGIN_WINDOW_OPEN without exposing local identity.
- Live desktop copy confirmation, native Save dialog and Escape cancellation
  passed. The user completed saving through the native dialog; the actual file
  was checked for the expected schema, exact top-level fields and absence of
  private content markers. Its backend new-file/preservation behavior passed
  the filesystem test. Release
  desktop/CLI builds passed. Evidence is in support-diagnostics-windows.json.
  Cargo audit reported zero vulnerabilities and seven existing warnings
  (six unmaintained dependencies and glib's unsoundness advisory); release
  security qualification remains incomplete. Full
  diagnostic build/capability metadata and collection when no host is reachable
  remain incomplete; this does not complete the diagnostics/release gates.

## Installed connection sign-in recovery (2026-09-21)

- Added instance-bound WebLogin open/finish commands through private IPC, the
  installed control API, diagnostic CLI and desktop. The desktop offers Open
  ChatGPT sign-in only for web authentication/scope requirements; native Codex
  auth errors, limits and busy/removal states cannot open it. Opening and finishing
  are separate explicit actions; Check status remains passive.
- The installed daemon retains the visible browser process and profile lock
  independently of desktop lifetime. Repeated open calls reuse the same owner.
  The user completes authentication/verification and closes the visible window;
  Verify sign-in then releases the exited browser and uses the existing background
  account/workspace/model revalidation. No credential entry, verification bypass,
  generation, new account binding or native configuration write is performed.
- Initial setup and installed re-login now launch ordinary English Chrome without
  a debugging/automation transport. The dedicated profile stays unchanged; only
  after the user closes all login windows can background validation attach.
  The login launch disables background mode so closing the last window can release
  the process. No identity spoofing, cookie export or challenge automation occurs.
- Live login processes prevent profile release rather than being killed.
  Disconnect also requires their safe release before restoring configuration.
  Normal native forwarding remains available while the web login is pending.
- The first actual desktop login click exposed missing Tauri command permissions;
  the build manifest and main-window capability now include installed_web_login.
  UI tests derive installed command names from the actual invoke calls, checking
  the registered handler, generated permission and capability together. The
  preceding CLI-driven managed login opened but encountered the user-reported
  challenge loop. After the old window was closed, the installed finish operation
  safely released its owner; background recovery still requested verification.
- Deployed the ordinary-login daemon, SHA-256
  371fa59efbf998ae7664cb9913d7297e4ca601b6a2acd29f3636d44a17b2545b,
  after checking the dedicated profile had no browser process and the runtime
  had no active web turn. Source/installed hashes match. Its native connection
  handshake was observed after restart. This daemon contains e12db8f's login
  changes; the diagnostics desktop/CLI are from 48fe243.
- The actual desktop Open ChatGPT sign-in button succeeded. The new dedicated
  Chrome process was checked for no remote-debugging/headless flags, English
  locale flags, the official URL and last-window process exit configuration.
  Health initially reported E_LOGIN_WINDOW_OPEN. The user then confirmed successful
  sign-in and closing the window. The finish operation verified the original
  account/workspace, English and all model variants in the background. Fresh
  non-generative catalog checks and one explicit xhigh text probe passed on each
  actual native backend. Configuration and executable hashes stayed unchanged.
- Validation: 326 workspace tests passed with 22 opt-in tests ignored, plus all
  72 desktop UI tests. Clippy with denied warnings, formatting and diff checks
  passed. Tests cover non-auth/cancelled refusal, instance binding, open/finish
  receipt separation, deduplication, ordinary-browser launch arguments and explicit
  UI sequencing. Live authentication, saved-session recovery and both backend
  text probes passed; fresh GUI-picker and full coding acceptance remain separate.

## Installed-client presence (2026-09-21)

- Added read-only presence observations for the supported Windows installations.
  Codex App uses current-user registration of its observed official package
  family through FindPackagesByPackageFamily; stale backend caches do not prove
  installation. CLI checks bounded runtime PATH and known npm executable paths.
  Missing environment sources, relative paths, identity/access failures and
  unresolved shims remain Unknown. No discovered executable or script runs.
- Presence is checked on the existing serialized blocking health worker and
  cached for at least 30 seconds, with its own observation timestamp. An installed
  but unobserved client shows Awaiting launch; a supported installation not found
  shows Not installed with the search scope in details. Actual client catalog or
  request evidence takes precedence, including failures and stale catalogs.
- Ready allows a verified client alongside an absent client, but never two absent
  clients or an unknown/pending client. Tests cover both single-client directions,
  uncertain scans, absent clients with actual traffic, cache timing, unresolved
  shims and preservation of failed catalog evidence. The current-user Windows API
  opt-in test passed for the real Codex registration and an absent fixture family.
- Validation: 323 workspace tests passed, 22 opt-in tests ignored; the package
  inventory test was additionally run explicitly and passed. All 70 desktop UI
  tests passed. All-target Clippy with denied warnings and formatting passed after
  the final package-query test change. The interrupted release build had no live
  handle/process and old outputs; a fresh release daemon/CLI/desktop build passed.
- Deployed daemon 6291ae9e9d4a6ff9afd5537930756565125ce73dc0af6bd57da072a75c086f98.
  Actual private health at 06:07:09 UTC found both installations and reported
  E_CLIENT_AWAITING_LAUNCH with independent local observation times. Desktop was
  reopened. Presence evidence is in installed-client-presence.json.
  Background session recovery now reports E_BROWSER_VERIFICATION_REQUIRED;
  connection readiness and fresh qualified catalogs are explicitly not verified
  on this build. No generation or automatic browser retry was requested.
- Next: implement the installed connection's missing OpenLogin action. Current
  installed UI offers Check status/removal but no path to fulfill the runtime's
  authentication action. Use the installed daemon's browser ownership and exact
  instance binding, preserve the native route and account/config, and leave any
  interactive verification to the user. Do not spawn a competing setup owner or
  bypass verification. This is an actionable lifecycle gap, not a reason to pause.
- Scope: CLI discovery describes the runtime environment, not every possible
  terminal override or portable installation. App discovery covers the observed
  official Windows package family, not unknown alternative distributions.
  Installation presence does not qualify backend versions or certify the picker.
  API source: [current-user package inventory](https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-findpackagesbypackagefamily).

## Native failure presentation (2026-09-20)

- Known native authentication, limit and transport failures now leave Preflight
  or Busy and surface an actionable overall state. A native auth failure directs
  the user to Codex details, never to the separate managed ChatGPT login flow.
  Concrete browser failures retain precedence; configuration conflicts, cleanup
  and removal states continue to override connection status.
- The installed panel identifies native Codex sign-in, service limits and
  connection failures separately from ChatGPT. Configuration observation and
  runtime cleanup errors also identify their actual component. Refresh remains
  passive and does not generate a message, retry a request or open a login window.
- Regression checks failed before the fix: native auth remained Preflight and
  the panel incorrectly requested ChatGPT sign-in. After the fix, 320 workspace
  tests passed with 21 opt-in tests ignored; all 69 desktop UI tests passed.
  All-target Clippy with denied warnings and formatting checks passed.
- Release daemon, CLI and desktop builds passed. Deployed idle daemon
  662981e53a81e25de68b06bc04422870a9068393ca4278a5a8348f02bf6d3529 and restarted
  the desktop. Saved-session recovery completed at 21:52:29 UTC. Actual CLI and
  App-backend catalog reads passed at 21:52:36 and 21:52:44 UTC, retaining all
  five reasoning choices and leaving native config/executables unchanged.
  Runtime health returned Ready. Evidence: installed-native-failure-recovery.json.
  Failure injection was confined to controlled fixtures; no account generation,
  authentication mutation or fresh GUI picker verification occurred.
- Single-client presence is still incomplete. PRD UX section 2 requires an
  absent client not to block an installed one, and an installed but unobserved
  client to show Awaiting launch. Current native_discovery is a candidate
  inventory, including stale desktop caches and bounded/permission diagnostics;
  an empty or incomplete inventory must not be treated as proof of absence.
  Integration still needs a trustworthy installed-client observation alongside
  the current catalog handshake before this readiness rule can be enabled.

## Installed catalog readiness (2026-09-20)

- Reviewed CLI/App catalog responses now produce independent client handshake
  evidence only after successful native-body validation and owned catalog merge.
  Locally generated conditional 304 responses retain this evidence; upstream
  rejection, malformed catalogs and unknown client codecs cannot manufacture it.
  Snapshot scope/generation changes invalidate earlier catalog observations.
  Reported codec metadata is compatibility evidence, not process authentication.
- Idle runtime health becomes Ready when configuration, browser, account, models,
  native transport and both client dimensions are healthy. A failed web request,
  stale/failed catalog, active turn or incomplete cleanup prevents this result.
  Desktop labels distinguish Catalog available from an actual verified request.
  Operational readiness is separate from coding qualification and release gates.
- Validation: 319 workspace tests passed, 21 opt-in tests ignored; all-target
  Clippy with warnings denied, formatting and diff checks passed. All 22 installed
  panel tests passed after the final label change. Release daemon, CLI and desktop
  builds passed; the desktop was explicitly rebuilt with the final embedded UI.
- Installed daemon 91f7d554fcadd04f927899fe70178289a04434c1fae5b9230f72afaf1c8cd4df
  recovered the saved session. Actual CLI 0.155.1 and App backend 0.155.0-alpha.9.2
  catalog reads passed with native models and all five reasoning choices intact.
  Private health reports Ready with all eight components healthy. Windows Computer
  Use verified the actual cxweb Connected heading and Catalog available labels;
  its accessibility tree contains Instant, Medium, High, Extra High and Pro with
  the native Low/Light and Max mapping. Native config and executables were unchanged.
  Reports: both catalog-readiness.json client files and installed-catalog-readiness.json.
- No model generation was requested in this interval; the earlier rate-limited
  arithmetic attempt remains a failure. No new native GUI picker claim is made.
  Both client observations are currently required; single-client discovery and
  broader lifecycle, coding, protocol and release qualification remain open.

## Passive native subscription health (2026-09-20)

- Native HTTP forwarding now records successful transport only after body EOF;
  a truncated body records a stream failure. Authentication rejection, forbidden
  access, rate limiting, server failure, redirects and rejected requests have
  fixed separate observations. Requests, bodies, headers and credentials never
  enter this tracker. An older response cannot overwrite a newer observation.
- Subscription WebSocket handshake evidence is distinguished from HTTP response
  evidence. Handshake rejection and observed stream errors remain failures;
  a plain HTTP 200 is not a successful WebSocket upgrade. Standalone Realtime
  sockets use another transport and cannot certify the subscription connection.
  Wire bytes, statuses, headers and redirect restrictions remain unchanged.
- Validation: 317 workspace tests passed, 21 opt-in tests ignored. All-target
  Clippy with denied warnings, formatting, diff checks and release build passed.
  After a fixture-only partial-read correction, its truncated-body test passed
  again. Existing socket fixtures now assert handshake/auth/redirect observations
  and Realtime isolation. The initial whole-suite abort was a test mismatch:
  the host fixture expected identical health timestamps after the previous real
  config-read change, then panicked again during cleanup. It now checks identical
  state/evidence with monotonic observation times and preserves primary failures.
- Installed daemon 615713fcdd238fa007532d383a253e72a0feb24110d566f92f6fb7afbd80175c
  recovered the saved browser session. Actual CLI 0.155.1 and App backend
  0.155.0-alpha.9.2 catalog reads passed at 21:29:56 and 21:30:07 UTC, retaining
  native models, the owned family and all five reasoning choices. Native config
  and executable hashes were unchanged. No model generation was requested.
- Private health now reports native_upstream healthy/local_probe after actual
  native traffic. Reports: both native-health-catalog.json client records and
  installed-native-transport-health.json. This is connection evidence, not
  fresh GUI verification, all-native-feature certification or whole-task success.
  Overall remains preflight because per-client readiness still lacks independent
  observations after restart. Continue client compatibility/readiness and lifecycle
  work without using web generations to manufacture status or retrying limited cases.

## Installed configuration observations (2026-09-20)

- Private Health exchanges now check the selected native configuration on a
  blocking worker after validating the request. The checked journal verifies its
  own integrity and compares effective routing through RoutePatch::can_resume;
  unrelated comments/model/theme edits remain valid. A changed route, provider,
  profile or catalog is reported as a conflict. Read/integrity failures have a
  fixed sanitized code. No config write, browser probe or generation occurs.
- The same route predicate fixes setup-owner routing status after unrelated
  user edits. Filesystem probes are serialized through result publication; an
  IPC timeout cannot accumulate detached workers. Configuration has its own
  observation timestamp, retained when only another health dimension changes.
- Validation: 313 workspace tests passed, 21 opt-in tests ignored. All-target
  Clippy with denied warnings, formatting and release build passed. After the
  final timestamp correction, its focused regression and Clippy passed again.
  Four new behavioral tests cover route edits, config health/revisions, actual
  controller observation and validated control dispatch.
- Deployed idle daemon fd9df26483ca95d0e2ff41675d9b18f293bb3af5a058644b939515390adbb281.
  The actual installed private Health exchange at 21:17:46 UTC changed config
  from unknown to healthy/local_probe without changing native config bytes.
  Evidence: integration-tests/compatibility/installed-configuration-health.json.
  Native upstream is still unknown and overall preflight is still intentional;
  this config check does not certify the other missing dimensions.
- CLI inclusive-count on the preceding daemon 0fc921ba774a4f5c53fb53f862bc04d8fa070efea3763830bd8077aee533d041
  failed at 21:07:54 UTC with E_BROWSER_RATE_LIMITED after its actual read, before
  tests or patch. That result remains archived. No further account generation
  was started in this interval. Continue independent native health/lifecycle
  work while respecting the web limit; do not retry a failed sample as a replacement.

## Actual arithmetic repair recovery (2026-09-20)

- CLI 0.155.1 and App backend 0.155.0-alpha.9.2 passed the account-live sum
  exercise at 21:02:06 and 21:05:19 UTC respectively on daemon
  0fc921ba774a4f5c53fb53f862bc04d8fa070efea3763830bd8077aee533d041.
  Each actual native process read the files, observed 3/4 failing cases, applied
  the correct two-space-indented patch, observed all four cases passing and only
  then returned its final acknowledgement. Tests, inputs, native config and
  executables remained unchanged. The reports are arithmetic-sum-dom-text-pass.json
  under each client prefix. This is a live coding success, not full G2 acceptance.
- The earlier App attempt at 20:55:51 UTC completed read/fail/patch but proposed
  a nonmatching retest action. It remains archived as arithmetic-dom-text-first.json;
  its precise command difference was not retained. Added bounded content-free
  payload/cwd diagnostics without relaxing approval. All 18 fixture/approval
  tests now pass; probe syntax and diff checks also pass.
- All five reasoning choices remain in both actual native catalogs. These sum
  exercises used xhigh and do not replace earlier all-effort or actual GUI tests.
- Follow-up: finish the remaining arithmetic and broader coding scenarios. The
  installed health tracker also needs independently verified config/native
  upstream observations: it currently leaves those dimensions unknown and keeps
  idle overall state at preflight, even after successful native web requests.
  Inspect lifecycle::routing_installed, ConfigJournal::recovery and RoutePatch::can_resume
  when implementing this; unrelated config edits must not become false conflicts.

## Visible DOM whitespace projection (2026-09-20)

- A controlled real-Chrome regression proved that innerText collapses two and
  four spaces inside JSON string values, including custom patch indentation.
  The DOM text nodes retained the exact bytes while the response observer lost
  spaces. The test failed before the projection change and passes afterward.
- Answer projection now preserves text-node whitespace and explicit paragraph/
  line-break boundaries, excludes hidden content and UI controls, and bounds
  length, node count and depth before joining. Numeric diagnostics distinguish
  projected from rendered text. This does not reconstruct pre-rendered Markdown
  punctuation; the existing Unicode-escape transport instructions remain needed.
- Validation: 309 workspace tests passed, 21 opt-in tests ignored; the whitespace
  and existing Unicode real-Chrome fixtures passed explicitly. All 61 Node DOM/
  fixture/approval tests passed before the later runner diagnostic addition.
  Clippy with denied warnings, formatting and release build passed. Final daemon
  0fc921ba774a4f5c53fb53f862bc04d8fa070efea3763830bd8077aee533d041
  also runs the bounded projection before collecting rendered-length diagnostics.
- The initial projection build 8c508b932b023172a1da40cc175fcf539ca5eee1f36898cc415286eb709b46b8
  let a live CLI sum exercise actually apply the correct patch with both original
  indentation spaces. That attempt still failed at 20:49:10 UTC: the native
  retest returned exit 1, then an unexpected further test request was declined.
  The final source was correct and direct local execution subsequently passed.
  Its original numeric retest counts/VM error were not retained; the cause is
  still unknown. The failure remains archived as arithmetic-dom-text-first.json.
- Extended the isolated actual-native fixture to run read/failing test/patch/
  passing retest. Both native builds passed that complete sequence against a
  synthetic local model, with five model responses and one exact patch each.
  These results do not count as account-live successes. The fixture runner now
  distinguishes VM timeout/evaluation errors from incorrect arithmetic instead
  of counting both as wrong answers; all 17 fixture/approval tests passed after
  this reporting correction. The arithmetic input corpus remains unchanged.

## Native follow-up and source fidelity investigation (2026-09-20)

- Actual installed CLI 0.155.1 and App backend 0.155.0-alpha.9.2 passed exact
  Unicode text checks on daemon
  0724e0be9e3c53ecbf316be74b8840ca5718e6a74260a3cb7bd6bb5c8c5889ee
  at 20:37:48 and 20:38:13 UTC. Reports include all five reasoning choices,
  unchanged configs/executables and no fresh GUI claim. Saved-session recovery
  and browser/auth/model checks work with the owned window hidden.
- Four arithmetic account-live observations remain failures. The initial CLI
  sum and inclusive-count attempts were refused before execution by an
  overstrict test-helper environment check. Source for both exact native tags
  reserves local for the host environment; that exact value now passes while
  unknown environments and additional permissions remain refused. See sources
  and the complete observation record in ARITHMETIC-REPAIRS.md.
- The later CLI sum attempt and first App sum attempt actually read their files
  and ran the failing tests. Both proposed patches were refused. The App's own
  synthetic source proposal was retained locally and showed one indentation
  space instead of the original two. Its target, identity and update shape were
  correct. The CLI proposal was not captured, so its exact cause is unknown.
  No acceptance criterion was loosened and no failed observation was replaced.
- A controlled synthetic model drove both actual native clients through the
  exact three-line arithmetic patch successfully, using isolated native homes
  and no account access. The new probe-arithmetic-patch.mjs reproduces that
  native approval shape. It also exposed native move_path spelling; repair
  guards now refuse non-null move_path and movePath. All 16 Node fixture and
  approval tests pass, including explicit local-environment and move refusal.
- Next investigation: distinguish model-generated whitespace changes from
  rendered DOM projection before selecting a source extraction change. Current
  answer observation reads innerText. The captured native diff alone does not
  prove the model's original bytes. Do not fix transport text by guessing spaces
  or relax exact fixture checks to turn these failed runs into passes. The bulk
  protocol experiment is stopped after its preserved case-015 failure; no new
  cohort or account generation is currently running.

## Interior Unicode rendering and arithmetic fixtures (2026-09-20)

- The protocol-unicode-xhigh-sep20 cohort stopped at case-015 at 20:16:23 UTC:
  14/15 protocol-valid, 12/15 task-correct and one E_BROWSER_UTF16 failure.
  Cleanup completed without destroying browser transport. All first attempts,
  including the two valid-but-wrong responses, are archived in the report.
- An extended real-Chrome regression failed before the change when an incomplete
  surrogate appeared before an already-rendered suffix. While generating, the
  observer now publishes only the prefix before the first unpaired code point
  and waits for a complete snapshot. Final malformed strings still fail; no
  characters are replaced and no second generation is requested. The regression
  covers high/low interior units, a valid non-BMP prefix, exact recovered suffix,
  completed invalid text, continued transport and cleanup. The live failed raw
  snapshot was not captured, so its precise rendering shape remains unproven.
- Validation: 309 workspace tests passed, 20 opt-in tests ignored; the expanded
  real-Chrome fixture explicitly passed; all 56 DOM/fixture/approval Node tests
  passed. Clippy with denied warnings, formatting and release build passed.
- Updated the idle installed daemon to
  0724e0be9e3c53ecbf316be74b8840ca5718e6a74260a3cb7bd6bb5c8c5889ee.
  Previous cohorts remain separate from qualification on this changed adapter.
- Added four deterministic arithmetic repair fixtures with actual offline Node
  tests and an installed-client --coding=<id> path. A native run must read the
  files, observe test failure, apply one bounded source patch, rerun passing
  tests and answer afterward. Reference solutions are not sent to the model.
  These are four small coding exercises, not the full required G2 suite.
  Local fixture/policy checks passed; account-live execution is still pending.

## Native fixture approval scope (2026-09-20)

- Generated experimental app-server schemas directly from the reviewed CLI
  0.155.1 and App backend 0.155.0-alpha.9.2 executables. Their command approval
  parameter schemas are identical, SHA-256
  c9728280b8f3204fd729d0fb3d1ca7bb05b1150de26b3653f7163e6a9bd941e7.
- The installed-client test helper now refuses additional permission overlays,
  non-default environments, subcommand/stdin approval IDs, and decision lists
  that do not offer ordinary one-command acceptance. Exact command, directory,
  network-context and shell checks remain required. Null/absent optional fields
  and display metadata remain compatible with reviewed native schemas.
- The fixture sends only accept or decline. Policy amendment proposals are not
  themselves permission grants and never cause a persistent acceptance response.
  This change affects test-client approval only; product approval remains owned
  by Codex. The running account-live protocol cohort uses no fixture executor
  and is unaffected.
- The new regression failed before the guard change. All nine tests in
  node --test scripts/probe-client-approval.test.mjs passed afterward; diff
  validation passed. No account generation or fresh native tool execution was
  performed to validate this helper-only change.

## Send readiness and Unicode transport recovery (2026-09-20)

- The original protocol-xhigh-sep20 run reached three attempted cases: two exact
  passes and one generic transport failure immediately after submission intent.
  Its precise original failure was not retained and cannot be reconstructed.
  The failed case remains in the denominator and was not resubmitted in that run.
- Added fixed per-attempt failure codes and content-free browser attribution
  diagnostics. Unknown error text is redacted and rejected on journal reload.
  A second, separately bound run using daemon
  f007ffe6afc11b00a6ac1afcd3eec2490bb73739ac0df2066443974c98787b09
  reached five cases: four protocol-valid, three task-correct and one transport
  failure. Case-004's valid-but-wrong result remains a task failure. Case-005
  failed cleanup while observing a generating Unicode answer; its Chrome process
  was still alive. Evidence: protocol-diagnostics-xhigh-sep20.json.
- A controlled real-Chrome test reproduced an immediate E_SEND_DISABLED when
  editor readiness lagged input by 400 ms. Send now polls a read-only guard for
  up to two seconds before one click. Cancellation, changed prompt/model, busy
  generation and permanent disablement do not click. A failed click is never
  repeated. The regression failed before the change and passes afterward.
- Another real-Chrome test reproduced a lone UTF-16 surrogate causing a JSON
  decode failure and terminating the reply reader, leaving Browser.getVersion
  unusable despite Chrome still running. Bundled DOM results now reject malformed
  strings/keys before CDP serialization. A generating answer may defer one final
  high surrogate until its low surrogate arrives; complete text is never repaired
  or replaced. A completed malformed string is an explicit E_BROWSER_UTF16 error.
  The live Unicode/cleanup failure is consistent with this defect, but its raw
  failed frame was not captured, so exact causal attribution remains unproven.
- A fully delimited malformed JSON reply still fails the waiting operation, but
  the reader retains frame alignment for cleanup. Oversized, incomplete and I/O
  framing failures remain terminal. Tests cover both boundaries.
- Final validation: 309 workspace tests passed, 20 opt-in tests ignored;
  41 DOM tests passed; the Send, Unicode and rate-limit dialog real-Chrome
  fixtures were explicitly run and passed separately. Clippy with warnings
  denied, formatting and release build passed. No synthetic test enters the
  account-live denominator.
- Installed daemon updated to
  f5a8d6ae34b5606df4fdf7b8b13a507f48052365dfb76705c50235821d302d08.
  Saved-session recovery restored browser/account/model checks. Actual installed
  CLI 0.155.1 and App backend 0.155.0-alpha.9.2 both passed exact Unicode text
  generation at 19:55:20 and 19:57:03 UTC, including a non-BMP character, accents,
  quotes and literal Windows backslashes. Both still return all five reasoning
  choices and preserved configuration/executable fingerprints. Reports:
  cli-0.155.1.unicode-text.json and
  app-backend-0.155.0-alpha.9.2.unicode-text.json. These were backend tests, not
  fresh GUI observations. Synthetic before/after evidence is recorded in
  browser-send-unicode-regressions.json.
- A new separately bound cohort, protocol-unicode-xhigh-sep20, passed its first
  case at 19:58:36 UTC on the fixed daemon. Its serial runner is configured to
  continue through case 200, stopping on an operational error and respecting
  the stored spacing. The archived report is a point-in-time snapshot; read
  the protected installation journal before continuing or reporting live counts.

## Durable installed protocol experiment runner (2026-09-20)

- Added runtime-qualify-protocol with an explicit run name and reasoning mode.
  Each command attempts one next frozen case through the installed browser owner,
  exact route/account checks and exclusive maintenance admission. It never
  executes fixture tools or publishes a coding/checkpoint qualification gate.
- Atomic private records persist the first attempt before preparation and the
  submission marker before Send. An exclusive OS lock excludes other owners.
  Interrupted work becomes a terminal failure after restart. Parameter changes,
  duplicate outcomes and external journal edits cannot replace prior evidence.
- Identity binds the frozen corpus, runtime executable, browser product/version,
  hashed installation/account/workspace/epoch and exact observed route/effort.
  The account plan remains unknown; native clients are not exercised by this
  protocol-only runner and must retain their separate compatibility evidence.
- Per-run spacing is 60 seconds after an outcome, or 900 seconds after a rate
  limit/recovered interruption. Reinvocation during backoff adds no new attempt.
  Reports separate protocol validity from task correctness and contain no prompt,
  response, credential or raw account data.
- Validation: 307 workspace tests passed, 18 opt-in tests ignored. Clippy with
  warnings denied, formatting and diff checks passed. Six new behavioral tests
  cover durable transitions, interrupted recovery, changed identity, preserved
  external edits, rate-limit accounting and private-control deduplication.
- Installed daemon updated while idle to
  a5d9d7e9073d135d81a8a90f58e0f61b98e448fee18327a14318f054f9a794a6.
  Run protocol-xhigh-sep20 has two first-attempt passes (case-001 and case-002),
  with both protocol validity and exact text correctness verified. They completed
  at 19:28:06 and 19:29:41 UTC. The cumulative report is
  integration-tests/quality/protocol-xhigh-sep20.json. The whole 200-case run is
  incomplete; no task/tool/client qualification follows from these two texts.
- A repeated live command within the spacing interval failed without modifying
  attempts.json or admitting another case. Read-only catalogs from installed
  App backend 0.155.0-alpha.9.2 and CLI 0.155.1 both passed at 19:28 UTC with
  all five reasoning choices, unchanged configs/executables, and no GUI claim.
  Reports: app-backend-0.155.0-alpha.9.2.protocol-runner-catalog.json and
  cli-0.155.1.protocol-runner-catalog.json in integration-tests/compatibility.

## Frozen protocol quality corpus and evaluator (2026-09-20)

- Added 200 deterministic generation cases in ten equal categories, including
  exact text, nested JSON, typed functions, custom literals/pinned apply_patch
  grammar, namespace collisions, parallel calls, denials, untrusted context and
  checkpoints. Forty cases are adversarial and sixty require custom/namespaced
  output. Checkpoint inputs do not inflate the latter count.
- Inputs and semantic expectations are frozen in
  integration-tests/quality/protocol-corpus-v1.json, hash
  ebbaf51a3c453a614ed4c126b81aeba48bb3acae4bd89c7763fc143f0d5e34f1.
  A contract test compares the complete generated manifest with that snapshot.
- Evaluation reuses the production envelope and contextual request validators,
  then separately measures exact task completion. Wrong but schema-valid answers
  and valid final refusals do not become false task passes or disappear.
- First-attempt accounting rejects duplicate starts and terminal replacements.
  Started/unfinished cases remain visible; failures stay in the denominator.
  Category counts and a 95% Wilson interval accompany complete observations.
  The 99% envelope screen cannot mark a release qualified.
- At the time this corpus was frozen, the live 200-case experiment was NOT RUN.
  The subsequent durable runner is described above; the whole experiment and
  independent native-client gates remain required before a G2 claim. See
  integration-tests/quality/PROTOCOL-CORPUS.md. Synthetic unit responses are
  not included in any live quality denominator.
- Validation: 301 workspace tests passed (18 opt-in tests ignored), Clippy with
  warnings denied, formatting and diff checks passed. After tightening pending
  interval behavior, the four corpus/tally unit tests and Clippy passed again.

## App tool checkpoint after generation backoff (2026-09-20)

- A new App-backend 0.155.0-alpha.9.2 run passed at 19:04:16 UTC after about
  fifteen minutes without ChatGPT generation since the last rate-limit failure.
  The installed daemon was
  11f8a96d75ecbaf5908e959d2e067a4d5dd8e7faef6d580190ed4d32bcb7a6d8.
  The native client performed one actual read, one manual compaction and exact
  final recall, with four WebSocket requests and confirmed cleanup.
- There was no runtime failure, native-upstream model frame or original
  assistant/tool plaintext in the continuation. Evidence:
  app-0.155.0-alpha.9.2.post-history-validation-tool-checkpoint.json.
  Earlier failures remain retained. This is one Extra High completed-tool
  scenario, not pending-process qualification or a reliability rate. Production
  checkpoint publication remains disabled.
- Ordinary installed text requests then passed for App at 19:07:37 UTC and CLI
  at 19:10:21 UTC. Both retained all five reasoning choices, native model rows,
  and unchanged configuration/executable hashes. The reports are
  app-backend-0.155.0-alpha.9.2.post-history-validation-text.json and
  cli-0.155.1.post-history-validation-text.json. Neither probe claims a new GUI
  interaction. The quality corpus work does not replace the running daemon or
  alter production routing; it is preparation for the larger quality gate.

## Full-history tool-result validation (2026-09-20)

- The ordinary request decoder had the same missing call/result relationship
  check: a standalone result with no preceding call passed validation. A new
  regression reproduced that failure before the fix.
- Complete request histories now require unique call IDs and exactly matched
  function/custom results in causal order. Unknown, duplicate, wrong-kind and
  prematurely ordered results fail with `E_TOOL_RESULT_HISTORY`. Compaction keeps
  its existing `E_CHECKPOINT_PENDING_TOOLS` error contract.
- Valid parallel calls may still complete in reverse order. Pending calls remain
  pending, historical tools need not appear in the current registry, and real
  denial/failure strings are preserved byte for byte in the serialized history.
- The isolated actual-native tool fixture now passes every native request through
  the production decoder, including read/patch, denial, test failure, repair and
  deliberate false-success rejection. This uses a synthetic local model server,
  not ChatGPT; it does not claim live model quality.
- Validation: 296 workspace tests passed (18 opt-in tests ignored); Clippy with
  warnings denied, formatting and the release CLI/daemon build passed.
  `actual_native_backend_executes_guarded_tools_and_denial_fixtures --ignored`
  also passed separately for reviewed CLI 0.155.1 and App backend
  0.155.0-alpha.9.2. Each ran six scenarios and 23 actual native requests through
  the decoder, with real file/tool execution and synthetic model responses.
  This does not contribute to the live model response-quality denominator.
- The release daemon SHA-256 is
  `11f8a96d75ecbaf5908e959d2e067a4d5dd8e7faef6d580190ed4d32bcb7a6d8`.
  After the second confirmed ChatGPT rate-limit failure, avoid another live
  generation before 19:03 UTC on 2026-09-20. Independent local implementation and
  testing can continue; this is a diagnostic backoff, not a paused goal or an
  automatic retry of a failed turn.

## Late tool-result validation after checkpoint restoration (2026-09-20)

- Found and reproduced a real gap in authenticated checkpoint continuation:
  restored pending calls were validated inside the token, but subsequent client
  results were not matched against the restored call sequence before generation.
  The integration regression failed before the fix because an unknown result ID
  was accepted and reached the synthetic browser.
- After authentication and expansion, the runtime now checks the complete call
  and result sequence. An unknown result, duplicate result, wrong function/custom
  result kind, repeated call ID or result appearing before its call is rejected
  with `E_CHECKPOINT_PENDING_TOOLS` before any browser send.
- The same integration regression passes all five rejection cases and the
  existing valid custom-call denial, exact restored arguments, replay and second
  compaction checks. Normal requests without checkpoints retain their existing
  behavior. This is deterministic runtime evidence, not qualification of an
  actual native client's still-running command across compaction.
- Validation: all 295 workspace tests passed (18 opt-in tests ignored), Clippy
  with warnings denied, formatting, diff checks and the Windows release build
  passed. The installed daemon is
  `f76a831482b32b76ed662a113d58fc13be0de2a15dc50470b966b30575c59d01`.
- After more than six minutes without a generation since the previous browser
  rate-limit failure, one new App tool-result exercise completed the actual read
  and compaction but failed at continuation with `E_BROWSER_RATE_LIMITED` at
  18:47:54 UTC. A private screenshot confirms the same `Too many requests`
  dialog. Four WebSocket requests, one compaction, one continuation, no original
  assistant/tool plaintext and confirmed cleanup were recorded, but exact final
  recall was not verified. The separate report is
  `app-0.155.0-alpha.9.2.tool-result-checkpoint-continuation-rate-limited.json`.
  No further generation was attempted in this batch. This is not a new App pass.

## Native tool-result checkpoint qualification (2026-09-20)

- Added opt-in `runtime-verify-compaction --client <reviewed-executable>
  --tool-result`, optionally over WebSocket. It is mutually exclusive with the
  automatic-history exercise and uses the existing exclusive installed-browser
  lease, disposable signed-out native home and unpublished checkpoint codec.
- The diagnostic creates one random line in its own protected fixture directory.
  Actual Codex must execute exactly the permitted file read and return a fixed
  acknowledgement that excludes the line. The client then manually compacts and
  must recall the exact line from the tool result. No user message supplies it.
- A buffered delivery policy rejects every model response except the exact read,
  acknowledgement, authenticated checkpoint item and exact final recall, in that
  order. Replays require an identical response ID/body. No patch, alternate
  command, second read or network access is admitted to the native client.
- Native evidence must show one attributed completed command, exit code zero,
  exact actual output, the fixed acknowledgement and unchanged fixture bytes.
  Missing/duplicate/foreign tool events, fabricated success, incorrect output and
  an answer that leaks the line fail regressions. The usual one-compaction,
  no-plaintext-continuation and confirmed-cleanup checks remain mandatory.
- App backend 0.155.0-alpha.9.2 passed at 18:27:32 UTC using the installed
  authenticated background browser: one actual read, one manual compaction and
  exact tool-result recall. Four WebSocket requests completed with no runtime
  failure, native-upstream model frame or original assistant/tool plaintext in
  continuation. Cleanup was confirmed. Report:
  `app-0.155.0-alpha.9.2.installed-tool-result-checkpoint-websocket.json`.
- Two CLI attempts at 18:22:52 and 18:31:52 UTC failed with
  `E_CHECKPOINT_SUMMARY`; no continuation was submitted and cleanup was confirmed.
  Both public reports are retained. The second confirms the actual read and
  shows valid outer JSON containing an invalid inner summary JSON string.
- Reports now expose the existing content-free output-shape diagnostics and
  preserve completed-read evidence separately from recall success. A typed
  progress file in the protected fixture directory records these facts and
  contains no marker or tool output.
- The compaction prompt now explicitly separates the two JSON encoding stages.
  Its executable example preserves quotes within values, a Windows path and a
  newline. A regression decodes both layers and rejects missing inner escapes.
  Strict parsing is unchanged; this does not reconstruct malformed model output.
- The revised prompt passed the CLI 0.155.1 live exercise at 18:37:43 UTC:
  one actual native read, one compaction, exact recall, four WebSocket requests,
  no original assistant/tool plaintext in continuation, no runtime failure and
  confirmed cleanup. Report:
  `cli-0.155.1.installed-tool-result-checkpoint-websocket.json`.
  Daemon SHA-256:
  `2dd71c983519632697bbd3c8dfce3141c85bfbba3d8e22c868d8fe6c7a5cdd40`.
  This controlled pass does not establish a first-pass reliability rate or prove
  the exact escaping defect in either earlier failure without its raw response.
- App's revised-prompt exercise at 18:39:15 UTC reached the confirmed native read
  but stopped before summary submission on `E_BROWSER_RATE_LIMITED`. A private
  screenshot confirms ChatGPT's `Too many requests` dialog instructing a wait of
  a few minutes. There was no automatic retry, route substitution or continuation;
  cleanup was confirmed. Report:
  `app-0.155.0-alpha.9.2.tool-result-checkpoint-rate-limited.json`.
  App's earlier pass remains tied to the preceding daemon hash. Revised-prompt
  App completion and ordinary installed text checks remain unverified for this
  build; catalog-only checks do not consume additional ChatGPT generations.
- The installed CLI and App-backend catalog checks passed at 18:40:42 and
  18:40:55 UTC with all five choices: Instant, Medium, High, Extra High and Pro.
  Both retained native model entries and unchanged configuration/executable
  hashes. See `*.post-tool-checkpoint-catalog.json`. These checks do not claim a
  new GUI observation or a successful generation after the rate-limit dialog.
- This exercise concerns completed tool results. It does not claim an unresolved
  call or a late result crossing a checkpoint works, and does not publish any
  production capability. Only Extra High is exercised. The first App pass used
  diagnostic daemon SHA-256
  `218a7527d4f50d002897240c2907c995d854fd711be5e41b2dde7cd49206055c`.
- Validation: 295 workspace tests passed (18 opt-in tests ignored), Clippy with
  warnings denied and formatting passed. The preexisting fixture tool policies
  remain covered by the workspace suite.

## Live native automatic checkpoint result (2026-09-20)

- CLI 0.155.1 passed at 18:04:46 UTC using the installed authenticated background
  browser. Eight actual history turns produced 71,354 answer bytes. The next
  request hit the unchanged local byte guard before browser submission and
  produced an attributed native `contextWindowExceeded` failure.
- On the next explicit user turn, Codex automatically compacted once and returned
  the exact last model-generated marker. The marker appeared in no user message;
  the test called no manual compaction RPC. Runtime counters independently showed
  eleven WebSocket requests (eight history, one refusal, one compaction and one
  continuation), no runtime failure, no native-upstream model frame and no original
  assistant/tool plaintext in the checkpoint continuation. Cleanup was confirmed.
- Report: `cli-0.155.1.installed-automatic-checkpoint-websocket.json`. Installed
  diagnostic daemon SHA-256:
  `4a14a6506b1ed8e7d3cddb63af1144c3cfc29ac3d07f71ad1b28058c47c4686f`.
  This is Extra High text-only next-turn recovery in an isolated native home,
  borrowing the installed browser. It does not qualify in-place retry, outstanding
  tool state or every reasoning level, and it does not publish production
  checkpoint capabilities.
- App backend 0.155.0-alpha.9.2 passed at 18:11:42 UTC: seven real history turns
  and 61,472 answer bytes, one local refusal, one native automatic compaction and
  exact recall. Ten WebSocket requests completed the scenario with no runtime
  failure, no native-upstream model frames, no original assistant/tool plaintext
  in continuation and confirmed cleanup. See
  `app-0.155.0-alpha.9.2.installed-automatic-checkpoint-websocket.json`.
- The installed diagnostic build passed both updated checkpoint unit tests,
  Clippy with warnings denied, formatting and the release CLI/daemon build.
  The full workspace run for the preceding progress change passed 292 tests
  (18 opt-in tests ignored); all 38 browser JavaScript regressions also passed.
- Ordinary requests through the real installed Codex home then passed for App
  at 18:12:31 UTC and CLI at 18:13:17 UTC. Both retained all five reasoning choices
  and native/owned catalog entries, and verified unchanged configuration and
  executable hashes. Reports are `*.post-automatic-checkpoint-verification.json`.
  Runtime is idle; browser/auth/models and both client request components are
  healthy. Overall health remains preflight because configuration/native-upstream
  health evidence is unknown in this restarted runtime; it is not a full release
  qualification. Pending tool state and other reasoning modes still need separate
  checkpoint qualification before production enablement.

## Automatic checkpoint history bound (2026-09-20)

- The first prose run completed all eight history turns with 70,866 actual answer
  bytes, eight WebSocket requests, no runtime/native error and confirmed cleanup.
  It correctly failed `E_NATIVE_PROBE_AUTOMATIC_BOUNDARY`: the history still fit
  the 96 KiB diagnostic guard, so it exercised no compaction. The full incomplete
  report is retained as
  `cli-0.155.1.automatic-checkpoint-eight-turn-boundary-incomplete.json`.
- Raised the diagnostic maximum to 32 history turns. At the accepted 4 KiB minimum
  this is sufficient to reach the unchanged 96 KiB guard. The normal/summary
  budgets, actual native refusal, automatic compaction, exact recall and cleanup
  requirements are unchanged. The report includes the maximum; the fixed recall
  RPC uses a separate ID range. The original eight-turn attempt remains a failure
  for compaction qualification.

## Automatic checkpoint fixture and progress diagnostics (2026-09-20)

- The post-keepalive repetitive-padding run stayed on one WebSocket request for
  more than fifteen minutes without finishing its first model answer. The isolated
  native diagnostic child was deliberately stopped; this attempt is incomplete,
  not a successful checkpoint run or a diagnosed browser failure. Browser stop
  acknowledgement was unconfirmed, but final idle cleanup was confirmed. Retained
  report: `cli-0.155.1.automatic-checkpoint-repetitive-fixture-incomplete.json`.
- The transport asks the model to encode each literal asterisk as six characters
  (`\u002a`) to preserve it through Markdown. The old fixture therefore requested
  at least 24,576 repetitive wire characters before counting the envelope. This
  explains its unusual workload, but does not establish the cause of its runtime.
- Replaced the repeated-asterisk fixture with frozen `technical-prose.v1`: a fresh
  model-generated marker followed by about 1,000 words of technical explanation.
  Admission requires 4–16 KiB of actual returned history, at least 1,024 alphabetic
  bytes, and no unexpected control characters. It still requires the same real
  local refusal, attributed native automatic compaction, exact marker recall,
  absence of plaintext checkpoint history and confirmed cleanup. No failure is
  removed from the record and no production capability is enabled.
- The isolated live gateway writes an optional count-only progress snapshot every
  15 seconds. It reports observed answer length, generation state and renderer
  counts, with no answer text, prompt, identity or account fields. Unknown fields
  and nonnumeric values are excluded by a regression test. The monitor uses the
  driver's cached diagnostics, does not drive the page and never extends model
  deadlines. Reporting errors do not cancel work; cancellation drops the monitor.
- Windows Computer Use inventory worked, but the offscreen managed Chrome window
  was not targetable. No additional browser instance or login was started.
- Validation: 292 Rust tests passed (18 opt-in tests ignored), both updated
  automatic-checkpoint unit tests passed, all 38 browser adapter JavaScript tests
  passed, and Clippy, formatting and the release build passed. The diagnostic
  build installed for the prose attempt has SHA-256
  `988d760fe66d5e2d4b5fdd6c96be9203f0d9d95282045cd47a58fa94bc5c47be`.

## Buffered WebSocket idle recovery (2026-09-20)

- A live automatic-checkpoint attempt was cancelled before its first long response
  completed. Its retained report does not establish the cancellation cause.
  A separate controlled regression reproduced a native WebSocket idle failure:
  with a 20-second watchdog and a 35-second synthetic provider delay, CLI 0.155.1
  abandoned WebSocket and retried over HTTP. The final turn completed, but the
  transport assertion correctly failed (one request on each transport).
- During owned buffered work, the gateway now emits a local `cxweb.keepalive`
  text event every 15 seconds. Native clients consume and ignore the unknown
  event; ping/pong alone is filtered before their application idle watchdog.
  This event contains no output, usage, progress, response identity or completion.
  It never reaches the native upstream and does not extend browser generation
  or no-progress deadlines. The outer transport ceiling now allows the existing
  30-minute generation bound plus preparation and cleanup.
- The same delayed-response test passed with both reviewed actual backends:
  CLI 0.155.1 and App 0.155.0-alpha.9.2 each completed with exactly one WebSocket
  request, zero HTTP retries, one browser-stub submission, no runtime errors and
  unchanged executable hashes. Only the isolated synthetic test home uses a
  custom provider to shorten the watchdog; installed configuration is untouched.
  Reports: `*.buffered-websocket.json`, with the CLI negative control retained
  as `cli-0.155.1.buffered-websocket-before.json`.
- A socket regression independently verifies the exact claim-free keepalive and
  cancellation/cleanup behavior after disconnect. The live checkpoint diagnostic
  now retains only allowlisted native error categories on failure.
- Validation: 291 workspace Rust tests passed (18 opt-in tests ignored), both
  explicit native delayed-response probes passed, Clippy with warnings denied
  and formatting passed, and the release CLI/daemon build succeeded. Installed
  daemon SHA-256: `72064c91b9d79a5351083bb6fcaae20453fae21b98335d595fe7c081a146a112`.

## Native automatic context-boundary verification (2026-09-20)

- Added opt-in `runtime-verify-compaction --client <reviewed-executable> --automatic`,
  optionally with `--websocket`, under the same exclusive installed-browser lease.
  It uses an isolated native home and a fixed diagnostic 96 KiB normal encoded
  prompt ceiling with 256 KiB summary headroom. Both the diagnostic catalog and
  execution use that budget; no production capacity or usage claim is published.
- The model supplies real history: a fresh fixed-shape marker and a bounded line
  of literal Markdown-significant padding. Native history is never injected or
  edited. Up to eight distinct user turns may grow that history; an invalid,
  abbreviated or reused fixture stops the diagnostic. No tools are authorized.
- Success requires an actual runtime byte-budget refusal and attributed native
  `contextWindowExceeded` failure, then one native automatic compaction before
  the next explicit diagnostic user turn and exact recall of the last successful
  model marker. There is no `thread/compact/start` call in this mode. This is
  next-turn recovery, not an automatic resend of the rejected request. The marker
  is absent from every user message. Transport counters independently require one
  refusal, one checkpoint continuation without original assistant/tool plaintext,
  and confirmed cleanup. No token-usage data is manufactured.
- Added deterministic regressions for strict fixture bounds, refusal attribution
  and compaction-before-answer ordering. All 290 workspace tests passed with 17
  opt-in tests ignored; Clippy with warnings denied, formatting and diff checks
  passed before the transport follow-up. The first live attempt stopped before
  the context boundary with `E_CANCELLED_STOP_UNCONFIRMED` and a rejected retry
  (`E_REQUEST_ALREADY_ADMITTED`); cleanup was confirmed. See the retained
  `cli-0.155.1.installed-automatic-checkpoint-incomplete.json`. Live automatic
  recovery remains unqualified until a complete run succeeds.

## Native checkpoint transport using the installed browser (2026-09-20)

- Extended the explicit checkpoint diagnostic with `--client <reviewed-executable>`
  and optional `--websocket`. It holds the installed maintenance lease while a
  disposable signed-out native home uses a separate capability-bound local gateway.
  The existing authenticated browser owner is borrowed for the complete test;
  native forwarding and installed client configuration are not replaced.
- The native client receives a model-generated random fixture, calls its actual
  `thread/compact/start`, waits for exactly one attributed compaction item and a
  successful turn, then requests exact recall. Neither user message includes the
  fixture. Runtime counters require one checkpoint continuation with no plaintext
  assistant/tool history. All native tool requests are rejected in this exercise.
- The first CLI attempt stopped before generation with `E_NATIVE_PROBE_CONFIG`.
  Native `config/read` exposed a Windows verbatim-path issue: converting the
  `\\?\` prefix to forward slashes produced an inconsistent round trip. Preserve
  backslashes when writing TOML and compare guarded canonical file identities
  when reading the native normalized path. The initial failed report is retained
  in `cli-0.155.1.installed-checkpoint-config-failure.json`.
- This diagnostic does not publish production compaction or context capacity.
  CLI 0.155.1 passed the complete HTTP/SSE cycle at 16:04:43 UTC: one native
  compaction, one exact continuation without plaintext assistant/tool history,
  no generation failures, and confirmed cleanup. See
  `cli-0.155.1.installed-browser-checkpoint-http.json`.
  App backend 0.155.0-alpha.9.2 passed the same cycle at 16:06:01 UTC.
  The first CLI WebSocket attempt accepted compaction but failed the browser
  scope check on continuation (`E_SESSION_SCOPE`, 16:07:39 UTC); its incomplete
  report is retained. Added structural browser-scope diagnostics before further
  investigation. The App WebSocket attempt then identified `E_MODEL_CLOSE` in
  account inspection, before generation. A fresh offscreen Chrome regression
  reproduced the same failure with stacked menus: one Escape closes only the
  inner portal. Closing now leaves the hover target, checks visibility between
  dismissals and allows at most three Escapes within the existing deadline.
  That browser regression failed before the fix and passed afterward, preserving
  the draft. A later CLI run isolated an additional settings dialog that outlived
  the menus (`E_ACCOUNT_SETTINGS_CLOSE`). Account inspection now confirms both
  menus and settings are closed. The expanded real-Chrome regression covers two
  menu layers with and without an underlying dialog and preserves the draft.
  A scoped Close-control fallback was added for settings dialogs that ignore
  Escape, with ambiguity/hit-target checks and a real-Chrome fixture. This was
  a tested local robustness change, not proof of the live failure's cause.
  Further structural diagnostics showed an unrelated dialog; a private opt-in
  failure screenshot at 16:39:39 UTC identified the actual service message:
  **Too many requests**, with temporary conversation-access restrictions and an
  instruction to wait a few minutes. The dialog covered the account control.
  The failed report is `app-0.155.0-alpha.9.2.installed-browser-checkpoint-websocket-rate-limit.json`;
  its screenshot stays private and is excluded from repository evidence. Earlier
  failed attempts are retained as failures; their exact causes are not inferred
  retroactively from the final screenshot.
- Added `--capture-failure` (requires `--client`) for explicit private diagnostics.
  The exclusive guard is disabled on scope exit and never used by regular work.
  The exact visible service restriction is now detected without reading model
  responses, dismissing the dialog or clicking Send. A fixed
  `E_BROWSER_RATE_LIMITED` survives account/turn processing and reaches terminal
  HTTP 400 (reviewed clients retry 429). The durable ledger preserves uncertain
  submissions, refuses replay and retains cleanup. Health displays `rate_limited`
  with a wait instruction, not a sign-in or browser-restart action. No quota
  reset time is invented. New live probes were stopped after identifying the limit.
- After the service-request cooldown and deployment of the final fix, the App
  backend passed the complete WebSocket scenario at 16:53:20 UTC: three owned
  WebSocket requests, one native compaction, one exact checkpoint continuation,
  zero plaintext assistant/tool history in that continuation, no transport
  failures, no native WebSocket frames and confirmed cleanup. The report is
  `app-0.155.0-alpha.9.2.installed-browser-checkpoint-websocket.json`.
  CLI 0.155.1 passed the same three-request WebSocket cycle at 16:55:13 UTC;
  see `cli-0.155.1.installed-browser-checkpoint-websocket.json`. Both native
  builds now pass explicit text compaction/recall over HTTP and WebSocket.
  Full long-history/tool-state qualification remains pending; these scenarios
  alone do not complete the production capability gates.
- Validation: 288 Rust workspace tests and all 103 browser/desktop JavaScript
  tests passed; 17 opt-in Rust tests were ignored by the normal workspace run.
  The real offscreen Chrome menu-closure and service-limit regressions each passed
  explicitly. Clippy with warnings denied, formatting and diff checks passed.
  Release CLI, daemon and desktop builds passed. The owned runtime was updated
  while idle; passive saved-session recovery passed without a login window and
  the desktop was reopened. Diagnostic runs do not mark installed App/CLI client
  health as request-verified.
- Ordinary requests through the unchanged installed configuration then passed
  the App backend at 16:56:17 UTC and CLI at 16:57:32 UTC, with all five reasoning
  choices plus native models present. Config and native executable hashes stayed
  unchanged. These reports are `*.post-native-checkpoint-verification.json`.

## Installed live checkpoint diagnostic (2026-09-20)

- Added explicit `runtime-verify-compaction --installation <id>` through private,
  instance-bound operation receipts. Duplicate delivery cannot generate twice.
  It obtains the installed gateway's exclusive web lease; native forwarding
  remains independent and explicit disconnect may cancel/drain the diagnostic.
  No second browser owner, native configuration override or re-login is needed.
- The diagnostic clones the installed coordinator with a private checkpoint key
  and the reviewed v2 codec. Synthetic fixed history contains a random completed
  tool result. The live web model produces the structured summary; the runtime
  encrypts it, verifies byte-identical completed-request replay, then requests
  recall in a new context containing only the checkpoint and a fixed question.
  The diagnostic codec is never assigned to the published production provider.
- Managed browser idle verification checks both owned leases/orphans and browser
  targets before and after the operation. Unconfirmed cleanup blocks normal web
  admission. The report records only stages, booleans, fixed codes and sanitized
  output shape; it exports no prompt, marker, checkpoint ciphertext or account ID.
- The complete live diagnostic passed at 15:40:15 UTC using Extra High. Exact
  recall, encrypted checkpoint, identical replay and target cleanup all passed;
  the installed runtime returned to idle. See
  `integration-tests/compatibility/installed-compaction-checkpoint.json`.
  Repeated against the final release build at 15:50:09 UTC after adding the
  missing-worker cleanup guard; all checks passed again. The report records
  that final deployed executable hash and observation time.
  Earlier failed standalone probes remain historical failures, not overwritten.
- After releasing diagnostic maintenance, actual CLI 0.155.1 passed web/native/
  web generation in one process at 15:43:04 UTC. All five reasoning choices and
  native models remained present, with unchanged config and executable hashes.
  App backend 0.155.0-alpha.9.2 then passed an ordinary web response at 15:44:01
  UTC with the same catalog and integrity checks. Both sanitized reports are
  stored as `*.post-compaction-maintenance.json` in the compatibility directory.
- This is account-live evidence with synthetic input history. Actual native
  compaction transport and automatic long-history recovery still need live
  qualification before publishing the capability. No broader context capacity or
  reliability claim is made from this two-generation exercise.
- Validation: 283 Rust workspace tests passed, 15 opt-in tests ignored. Clippy
  with warnings denied, formatting, diff checks and explicit Windows CLI/daemon
  release builds passed. New tests cover exact response shapes and instance-bound,
  deduplicated private control, including explicit concurrent disconnect and a
  missing browser worker being treated as unconfirmed cleanup.

## Installed failed-test repair exercise (2026-09-20)

- Added `--repair` to the installed-client probe. The disposable workspace starts
  with a random input line and deliberately incorrect output. Actual Codex must
  read the input, execute the exact equality test and observe exit code 1, apply
  the exact single-file correction, execute the test again and observe exit code
  0 before reporting success. The random input is not supplied in the prompt.
- Approval admission follows the completed, attributed native event prefix.
  A patch cannot be approved before a real failing test, a retest cannot run
  before the completed patch, and duplicate approvals or different commands are
  refused. A moved file, expanded grant root, extra tool or mismatched turn cannot
  qualify the exercise. The product still executes no model-generated tools.
- CLI 0.155.1 passed the complete authenticated scenario at 15:27:57 UTC. All
  four exact approvals were observed, both files were checked independently,
  the final answer followed the successful retest, and native configuration and
  executable hashes remained unchanged. See `cli-0.155.1.installed-repair.json`.
  App backend 0.155.0-alpha.9.2 passed the same complete exercise at 15:30:23 UTC,
  including all four approvals and unchanged native hashes; see
  `app-backend-0.155.0-alpha.9.2.installed-repair.json`. No browser window was
  required. The installed runtime returned to zero active turns with verified
  browser scope and request evidence for both clients.
- Validation: all seven approval harness tests passed, including wrong order,
  fabricated success, incorrect exit codes, extra calls, foreign-turn evidence,
  file moves and altered test commands. JavaScript syntax and diff checks passed.
- Next functional gap: the installed recovery provider does not yet enable its
  checkpoint codec/context budget. Existing synthetic context tests and earlier
  incomplete live compaction reports do not establish long-task support. Finish
  authenticated compaction and continuation qualification before enabling that
  path; retain all pending tool results and native compatibility boundaries.

## Installed approval-denial exercise (2026-09-20)

- The installed-client probe now has a `--denial` exercise. It starts an
  ephemeral read-only, untrusted-approval thread in a disposable workspace and
  declines exactly one attributed request for the fixture read. No approval
  policy is changed on disk and the bridge does not execute tools. A different
  command, patch, repeat request or other action fails the exercise.
- Success requires one native `declined` command item, no process/exit/output
  result, no unread random marker in any client event, unchanged fixture bytes,
  no added files and the exact final acknowledgement after the declined item.
  Unknown, duplicate, foreign-turn and execution-shaped records fail local tests.
- The first CLI and App runs completed their native turns but failed the harness
  because it incorrectly required a string output for the declined item. The App
  diagnostic showed `declined`, null exit status and no output. The reviewed
  [native completion implementation](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/app-server/src/bespoke_event_handling.rs#L1495)
  explicitly emits null output/exit status for this path. The harness now checks
  that contract; original failures remain in `*.denial-harness-incomplete.json`.
- A fresh CLI run passed the complete exercise at 15:19:38 UTC with unchanged
  native configuration/executable hashes. Its report is
  `cli-0.155.1.installed-denial.json`. The App backend passed the same complete
  exercise at 15:20:48 UTC, also preserving both hashes; see
  `app-backend-0.155.0-alpha.9.2.installed-denial.json`. Final runtime health is
  idle with verified browser scope and successful request evidence for both
  builds. This is one actual coding-denial scenario, not the complete G2 corpus.
- `node --check scripts/probe-installed-client.mjs` passed and all five
  `scripts/probe-client-approval.test.mjs` tests passed, including rejection of
  successful execution, missing fields and process IDs in denial evidence.

## Installed client request evidence (2026-09-20)

- Completed web requests now update the App and CLI health dimensions separately
  for the two reviewed native builds. HTTP and WebSocket use the same evidence
  path. Only the build classification, result and timestamp are retained; raw
  User-Agent headers and native authentication never enter browser work.
- A successful status requires the provider's in-process browser verification
  marker and a successful response. Catalog reads, warmups, unknown or duplicate
  build headers, plain HTTP 200 responses and cancelled work do not qualify it.
  Failures remain distinct from success. Disconnect clears the active claims;
  daemon restart starts with unknown client status until another actual request.
- The desktop labels this evidence `Request verified` and explains that picker
  verification is separate. It does not promote native upstream/config health or
  overall connection readiness on the strength of a web response alone.
- After updating the idle installed daemon, saved-session recovery restored all
  five English choices. Actual CLI 0.155.1 completed a background request at
  15:11:39 UTC: only CLI became healthy and App remained unknown. Actual App
  backend 0.155.0-alpha.9.2 completed its request at 15:12:20 UTC: App then became
  healthy and the CLI observation remained unchanged. Both probes preserved
  native configuration and executable hashes. See `*.installed-health.json` in
  `integration-tests/compatibility`. Computer Use confirmed both `Request
  verified` rows in the rebuilt desktop; no visible browser window was needed.
- Validation: `cargo test --workspace --locked --quiet` passed 280 tests, with
  15 opt-in tests ignored; `node --test apps/desktop/tests/*.test.mjs` passed all
  64 tests. Clippy with warnings denied, formatting, diff checks and explicit
  Windows release builds of CLI, daemon and desktop passed.

## Reasoning family publication (2026-09-20)

- The desktop's installed view now reads the actually published family through
  a passive private-control command and lists its English choices. A newly
  activated single-choice installation offers explicit background qualification;
  its eight-message allowance cost is disclosed. Already complete families do
  not offer redundant qualification. Successful activation opens this view
  immediately. Duplicate clicks, active work, missing metadata, sign-out and
  stale runtime instances cannot start another test. Failure clears stale UI
  claims and never retries generation.
- Health and reasoning responses must belong to the same runtime instance.
  Older hosts without the metadata command remain readable with no reasoning
  action. Only public catalog names/efforts cross this command, with no browser
  identities, account scope, credentials or configuration contents.
- Actual desktop inspection confirmed Instant, Medium, High, Extra High and Pro
  with the Low/Light and Max aliases explained, and no redundant test button.
  The installed daemon recovered all five choices in the background. The current
  health tracker at that stage still left App/CLI verification unknown; automatic
  current-runtime request evidence is implemented in the follow-up above.
- Desktop follow-up checks: Rust workspace 278 passed / 15 ignored; all 63
  desktop tests passed, including five new installed-flow cases. Clippy with
  warnings denied, formatting, diff checks, and Windows release builds passed.

- An installed route can now expose multiple independently qualified reasoning
  choices under one model family. The native request's explicit effort selects
  its exact observed browser position; unsupported efforts fail before opening a
  target. The selected identity is rechecked immediately before Send. Legacy
  single-effort receipts remain readable.
- The live English Latest slider exposed Instant, Medium, High, Extra High and
  Pro at positions 0 through 4. The published native keys are `low`, `medium`,
  `high`, `xhigh` and `max`. The reviewed App filters `none` out of its picker:
  real UI inspection showed only four choices with the first implementation.
  Low is therefore an explicit Instant alias, and Max selects Pro; the catalog
  description explains both mappings. Legacy Instant receipts and requests using
  `none` still select the same observed Instant identity. The web label is Pro
  on this account; the adapter also accepts 6 PRO. Labels are never inferred from
  slider positions, and native client code/preferences are not patched.
- `runtime-qualify-reasoning --installation <id>` owns an exclusive web admission
  lease and runs fixed text plus typed function/custom-tool diagnostics for each
  additional choice. It never retries generation automatically. Explicit
  disconnect cancels and drains it; unconfirmed cleanup prevents new web work.
  Native forwarding remains available. A compare-and-store journal extension
  precedes catalog publication and preserves the account, default route, native
  configuration and existing completed-response replay cache.
- All eight added browser checks passed at 14:24:22 UTC; the existing Extra High
  route retains its prior text and actual native read/patch evidence. The running
  installation now publishes all five choices. Evidence is in
  `integration-tests/compatibility/browser-latest-reasoning.json`. This is basic
  live protocol evidence, not full G2 coding qualification.
- An earlier batch completed six checks and failed the account guard before the
  first Pro Send. New Temporary Chat readiness waits for a visible profile entry
  point as well as the composer, allowing responsive duplicate entry points;
  identity verification remains separate. Scope failures now expose fixed causes
  instead of retaining a previous successful diagnostic. The initial failing
  subcheck was not recorded, so the successful later batch is not proof of that
  first failure's precise cause.
- Both actual native executables accepted an isolated synthetic five-choice
  catalog and passed text/cancellation tests. A live installed CLI run then
  completed all five efforts, but its overall result failed because the native
  config hash changed while an independent verification App was starting.
  The later CLI run with the published Low alias passed all five actual browser
  turns at 14:40:21 UTC with configuration and executable hashes unchanged.
  Its complete report is `cli-0.155.1.installed-reasoning.json`. The App backend
  passed the same five Low-alias turns at 14:42:18 UTC, also with unchanged
  configuration and executable hashes; see
  `app-backend-0.155.0-alpha.9.2.installed-reasoning.json`.
- Computer Use verified all five positions in the actual signed App
  26.915.4065.0. Its visible labels are Light, Medium, High, Extra High and Max;
  accessibility calls the middle two Standard and Extended. These are the native
  client's presentation of low/medium/high/xhigh/max. The complete model menu
  includes ChatGPT Web alongside native models and retains it after selecting
  GPT-6 Astra. Switching back to the web family and restoring Extra High passed.
  A fixed no-tools message was sent once from the existing GUI test task and
  rendered the expected answer at 14:47:43 UTC. The native turn context records
  the owned model ID and `xhigh`; its response has the runtime's owned response
  ID and no tool calls. Background health returned to idle with verified scope.
  See `app-26.915.4065.0.installed-picker.json`. This live check used the existing
  native home, not the isolated synthetic home. The original running App retains
  its older catalog until a full restart.
- Validation so far: Rust workspace 277 passed / 15 ignored, browser DOM 35
  passed, desktop/approval tests 62 passed; Clippy, formatting, diff checks and
  explicit Windows CLI/daemon release builds passed. The real GUI mouse path now
  opens the picker after refreshing its window/screenshot selection.

## Saved-session hydration and installed tool verification (2026-09-20)

- After the user completed sign-in, the managed background transition retained
  the authenticated English session. Startup still failed with the previously
  generic `E_BROWSER_OBSERVATION`. Fixed structural diagnostics isolated
  `E_BROWSER_BASELINE_MODEL`: the composer/account loaded before its model control.
- Startup now waits up to 15 seconds for that same page's composer/model baseline,
  without navigation, submission or generation retry. Cancellation and unrelated
  errors terminate immediately. Session observation also tolerates a transient
  document-read failure within its existing deadline. The latter alone did not
  resolve the observed failure; waiting for the model baseline did.
- Installed scheduled recovery passed at 10:55:33 UTC. It rechecked the saved
  account/workspace binding, English language, route and Temporary Chat. The
  current installation contains the new startup logic and recovery catalog fix.
- Added `runtime-retry-web --installation <id>` as a CLI entry point to the
  existing instance-bound recovery operation. Eligibility and admission checks
  remain owned by the installed host; this is not a message retry or re-login.
- Actual installed CLI 0.155.1 passed its full native read, native `apply_patch`,
  exact output-file and final-answer checks at 10:57:23 UTC. Its existing native
  subscription, configuration and executable were preserved. No harness code
  executed model-generated tools. This is one scenario, not G2 certification.
- The same scenario passed App backend 0.155.0-alpha.9.2 at 10:59:15 UTC. Both
  clients listed native and owned models together. Sanitized, complete reports:
  `integration-tests/compatibility/cli-0.155.1.installed-tools.json` and
  `integration-tests/compatibility/app-backend-0.155.0-alpha.9.2.installed-tools.json`.
- Validation: `cargo test --workspace --quiet` passed 268 tests / 15 ignored;
  browser DOM tests passed 34; Clippy with warnings denied, formatting and diff
  checks passed. Release CLI and daemon builds passed for the explicit Windows
  target. Computer Use read the actual Codex task UI but mouse input failed with
  `SendInput sent 0 of 1 events; GetLastError=87`, including one attempt after
  refreshing the selected window and focus. No GUI picker pass is inferred.

## Recovery catalog and login readiness (2026-09-20)

- Reviewed-client model requests during browser recovery now return HTTP 503,
  `E_WEB_RECOVERING`, `Cache-Control: no-store` and `Retry-After: 1`. They no longer
  return a successful native-only snapshot that can replace the client's merged
  catalog for its five-minute cache TTL. No native cache file is edited. Native
  generation, unknown-client catalogs, confirmed sign-out and disconnected
  passthrough retain their previous behavior. A gateway regression verifies
  conditional requests, successful augmentation after recovery and unchanged
  native response bytes/metadata. Exact-client refresh timing and GUI behavior
  still need live verification; a 503 is not claimed to refresh the GUI itself.
- Login detection requires visible composer, profile and login controls. Hidden
  markup cannot establish or invalidate authentication. Tests cover a hidden
  login link followed by a visible one and a hidden authenticated surface. The
  live login requirement persisted after this fix, so it is not counted as the
  cause of the current sign-in failure.
- Explicit login windows now request primary-screen bounds instead of accepting
  the browser profile's previous placement. The window was discovered through
  Computer Use, but screenshot verification was stopped because the tool could
  not determine the browser URL sufficiently to enforce policy. Visual placement
  and the actual user page were not verified by that tool. A separate real-Chrome
  regression with a fresh, credential-free profile now passes: it records an
  off-screen window, closes that owner, reopens the profile through the actual
  login path, and verifies the requested on-screen bounds and normal window state.
  No automated login or interaction with the user's login window was attempted.
- Preparation diagnostics retain fixed causes through startup and native turns;
  health reports preserve reviewed preparation codes and sanitize unknown text.
- Validation: workspace 267 passed / 14 ignored; browser DOM 33 passed; Clippy,
  formatting and diff checks passed. The latest catalog change is not yet in the
  running installation. The live login owner is preserved while the user signs in.
  The added browser placement test passed separately with
  `cargo test -p cxweb-browser-adapter --lib login_window_returns_to_desktop_after_background_profile_use -- --ignored`.
  Catalog recovery regression now covers both reviewed CLI and App request
  versions, with and without validators. Full release CLI/daemon/desktop builds
  passed for the explicit `x86_64-pc-windows-msvc` target in a separate output
  directory so the active login service did not need to be terminated.

## Installed tool string rendering and remaining preparation failure (2026-09-20)

- A subsequent scheduled restart exposed an existing read/execute grant for the
  native `CodexSandboxUsers` group on the installation index. Exact-private
  validation incorrectly rejected that ancestor. Only this protected,
  current-user-owned index now permits readers through the reviewed path policy;
  foreign writes, deletion, ownership/ACL changes and inherited protection are
  still rejected. Every state/profile/journal child retains its private policy.
  No host ACL was changed. An actual NTFS fixture covers read grants, forbidden
  write masks and unchanged child privacy. Scheduled recovery passed this check
  and subsequently reported `E_LOGIN_REQUIRED`; authenticated recovery has not
  passed this build yet.
- Temporary Chat preparation now distinguishes fixed structural failure states
  and preserves target/window/navigation/attachment failures. Diagnostics contain
  no page text, URLs or account identifiers. URL and unique visible composer
  requirements remain unchanged. Workspace checks: 266 passed / 14 ignored;
  browser DOM tests: 32 passed; Clippy and release builds passed.

- Added an installed `--tools` exercise alongside the existing text/coexistence
  probe. It uses the real native home, subscription and gateway, creates a fresh
  random input fixture, and asks Codex to read it and apply one literal patch.
  The marker is absent from the prompt. Approval binds to the exact native turn,
  command, directory, patch and observed read result. The harness never executes
  model tools. Final verification checks event order, file bytes and exact text.
- The installed exercise exposed malformed response JSON before any native tool
  execution. Fixed diagnostics distinguished JSON, envelope shape/identity,
  purpose, unknown tools, input schema, tool choice and rendered code fences.
  Invalid escape sequences and raw controls have separate fixed codes. Parser
  messages, response content, arguments and account data are never exported.
  Existing validation acceptance remains unchanged, including duplicate-key,
  unknown-field, schema and nonce rejection. The original coarse error API is
  preserved for callers that do not need detailed diagnostics.
- The App backend reproduced `E_TOOL_ENVELOPE_JSON_ESCAPE`. Normal tool replies
  lacked the Markdown-safe string guidance already used by structured finals
  and compaction. All normal string values now use Unicode escapes for literal
  backslashes, inner quotes and Markdown punctuation, including nested function
  arguments and custom patches. The receiver does not repair or coerce output.
  A regression preserves Windows paths, quoted commands, literal patch markers,
  backticks and newlines through decoding and native wire serialization.
- After that change, the installed CLI produced a byte-identical output fixture
  through the native read/patch flow. The full task still failed with
  `E_TEMPORARY_CHAT` while preparing the final model request. An independent App
  backend exercise then failed at the same preparation boundary before tools.
  These are FAILED scenarios, retained in `installed-tools-investigation.json`.
  Temporary Chat preparation is the next active investigation, not a passed gate.
- A probe launched before browser recovery completed also observed a native-only
  catalog. The native client can cache that response for five minutes. Startup
  catalog visibility needs a product-level regression; no native cache was edited
  to conceal the observation. Subsequent checks waited for actual browser readiness.
- Validation: workspace 264 passed / 14 ignored; desktop plus fixture approval
  tests 62 passed; Clippy, formatting and release CLI/daemon/desktop builds passed.
  The scheduled runtime runs the new encoding/diagnostic build. Earlier live
  results do not certify the changed prompt; full installed tool completion,
  actual App picker switching and the wider qualification corpus remain open.

## Installed native subscription coexistence (2026-09-20)

- Added `--coexistence` to `scripts/probe-installed-client.mjs`. It requests a web
  answer, a native subscription answer, then another web answer in the same native
  process. Each uses an ephemeral read-only thread, verifies the exact selected
  model and built-in provider, and checks the attributed answer against a fresh
  random marker. It approves no client actions and checks for unexpected tools.
- Both CLI 0.155.1 and App backend 0.155.0-alpha.9.2 passed all three turns through
  the scheduled installation. The native model was GPT-6 Astra with supported low
  effort; the owned model retained its qualified Extra High effort. Existing
  subscription authentication, native config bytes and executable bytes stayed
  unchanged. No route, catalog or authentication overrides were used.
- Sanitized reports are the two `*.installed-coexistence.json` files under
  `integration-tests/compatibility`. The command is
  `node scripts/probe-installed-client.mjs <reviewed-client> <native-home> <owned-model> --coexistence`.
  This proves the tested text requests and model selections, not every native
  tool, transport or GUI workflow.
- A separate fresh instance of the unmodified signed Codex App, using isolated
  GUI data and the existing native home, displayed `ChatGPT Web · Latest · Extra
  High` in its composer. No native auth store was copied. The previous running
  instance showed only native choices. The actual source also has conditional
  renderer catalog filtering; which conditions explain all switching behavior
  remains unverified. No package, feature flag or catalog workaround was applied.
- Computer Use was interrupted before the fresh instance's expanded picker and
  round trip could be verified. This partial observation does not pass G0.

## Shared browser state across Windows launch contexts (2026-09-20)

- Identified the recovery failure using OS file-handle paths, without reading
  browser credential values. A browser launched under the Codex MSIX context
  opened its data in the package's LocalCache overlay. Task Scheduler opened
  the unvirtualized AppData files. Chrome reported the same profile path in
  both cases, so string/path comparisons had concealed the different files.
- User, logon session, elevation, profile environment, synthetic DPAPI access,
  working directory, process priority and an additional job restriction did
  not explain the difference. The earlier elevation hypothesis was incorrect.
  Neither task privileges nor the native app package/manifest were changed.
- Active browser and application state now live under the protected
  OS-selected user profile at .cxweb-runtime/data. Registration inventory still
  checks the old state directory without importing its ambiguous profile view.
  Fresh installations do not borrow data from another application's package.
- Migrated this development installation's dedicated cxweb profile and local
  qualification database into the private location while its browser was
  stopped. No personal browser profile or native authentication store was used.
  The old dedicated data remains protected and unused for rollback; explicit
  legacy-data cleanup remains part of the lifecycle acceptance work.
- Private ACL validation accepts the Windows auto-inherited bookkeeping flag
  only when the DACL remains protected and its owner and exact grants match.
  Missing protection, inherited ACEs, changed ownership and foreign access
  remain rejected. No system ACL was changed.
- Added an opt-in saved-session regression test. The same test executable passed
  directly and under an interactive-token scheduled task, without a prompt,
  login action, startup delay workaround or page reload. The disposable test
  registration was removed. Actual installed daemon recovery then passed from
  its original scheduled registration and retained the previously verified
  account, model binding and English browser UI.
- Both actual native builds again passed catalog and text generation through
  the scheduled runtime, with existing subscription authentication and unchanged
  native configuration/executables. This is backend evidence, not a GUI picker
  pass. The actual Codex App picker/restart remains under verification.
- Validation: cargo test --workspace --locked --quiet (261 passed / 14 ignored),
  cargo clippy --workspace --all-targets -- -D warnings, desktop Node tests
  (58 passed), and release builds for CLI/daemon/desktop passed.
- Microsoft documents this AppData redirection for packaged desktop apps:
  https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes

Earlier manual check: the user reports `ChatGPT: Session detected` and
`Codex connection: Awaiting verification` after checking the desktop status.
This confirms the login controller recognized the saved session in this run;
it does not establish the selected account/workspace, model inventory, Temporary
Chat behavior or a successful generation. After restarting Codex App, its real
picker still contained native entries only: Default, GPT-6 Astra, GPT-5.6 Sol,
GPT-5.6 Terra, GPT-5.6 Luna and GPT-5.5, with GPT-5.6 Sol selected. No owned cxweb
entry was visible, which is the expected evidence while activation remains absent.

## Windows native configuration and production activation (2026-09-20)

- Native config and regenerable model-cache snapshots now allow existing foreign
  read/execute grants, while rejecting foreign writes and replacement rights.
  Private journals, staging and browser data retain their stricter policy.
  Replacement preserves the native file's ACL; no user/system ACL was changed.
- Production HTTP/WebSocket accepts verify the OS-observed peer process user
  before HTTP parsing. Connection creation time and a second owner lookup guard
  against PID reuse. Same-process and separate-process loopback tests passed;
  a mismatched expected SID is rejected. No actual second-account test was run.
- Pinned, nonempty NTFS ancestors permit sibling creation and attribute writes,
  but still reject deletion/replacement, ownership and DACL rights. Native
  executable bytes remain reviewed, hashed and held against writes/deletion;
  their parent is not treated as a persistent data-storage destination.
- Persistent installs use a protected .cxweb-runtime directory under the Windows
  user profile, avoiding the broad replacement grants on this machine's AppData
  ancestors. Browser profile location and stored login were preserved. Legacy
  installations remain discoverable without moving active journals or tasks.
- Actual preflight passed both reviewed native backends with subscription auth,
  all path/access checks and no conflicts. Production activation initially found
  a remaining private-config check in TaskPlan; its native-config policy and a
  real ACL regression fixture now cover the previously failing path.
- Production connect-codex successfully registered supervision and applied the
  selected home's route. The background browser passed live text and tool
  protocol tests with English UI and 15 observed family/effort candidates. Only
  the tested Latest / Extra High route is published by this installation.
- The first installed CLI catalog check exposed an actual User-Agent parser bug:
  Codex Desktop contains a space. Codec selection now splits at the slash and
  still requires the exact reviewed full build and query version. Regression
  tests cover both builds and refuse an adjacent, unreviewed alpha.
- An automatic scheduled restart exposed early signed-out-shell handling.
  Recovery now observes the same page until its bounded startup deadline before
  declaring login required, without login clicks, extra tabs or submissions.
  Session hydration, persistent logout, cancellation and immediate challenge
  handling are covered by the updated recovery regression test.
- Added scripts/probe-installed-client.mjs for the actual selected native home,
  subscription, combined catalog and optional ephemeral text-only turn. It uses
  no static catalog or routing/auth override, approves no tools, exports only
  sanitized evidence, and verifies config/executable bytes remain unchanged.
- Validation so far: workspace 260 passed / 13 ignored (including the peer child
  helper exercised by its parent); desktop 58 passed; Clippy, formatting and diff
  checks passed. Actual Task Scheduler activation/removal-refusal fixture passed.
  Builds succeeded; installed catalog and restart evidence follows below.
- Live production verification passed both native builds with their existing
  subscription and actual selected home, combined native/owned model catalogs,
  and exact responses through the saved background ChatGPT session. Tests had
  initially inherited the native home's effort for another model and correctly
  failed E_MODEL_FIDELITY before submission; they now select the owned model's
  reported default xhigh effort per turn, without changing the user's defaults.
  Evidence: cli-0.155.1.installed-live.json and
  app-backend-0.155.0-alpha.9.2.installed-live.json.
- The actual CLI 0.155.1 /model picker displayed the six native choices and the
  owned route as choice 7. Exited without selecting a new default or generating
  another turn. Its unrelated configured MCP startup warning was not repaired.
- Computer Use verified the existing OpenAI.Codex package window is the Codex
  workspace, despite its ChatGPT.exe/title metadata. Its currently running picker
  still displays only native entries. The competitor's troubleshooting confirms
  a full native process restart is required. Other user tasks are running in this
  app, so they were not interrupted to force this test. No actual App picker pass
  is inferred from the passing embedded-backend check.
- Initial scheduled recovery failed with E_LOGIN_REQUIRED despite successful
  direct launches. The cause was subsequently traced to MSIX AppData redirection,
  not elevation; see the shared-state correction and passing live evidence above.
- Next: verify full App restart/picker and native subscription response
  coexistence. Broader PRD/release gates remain open.

## Connect models to both native clients (2026-09-20)

- The user explicitly removed the GUI-title ambiguity as a reason to stop. The
  acceptance condition is usable cxweb models in both real clients. An App
  launcher's filename/window title is not an installation prerequisite.
- Reviewed the competitor's setup, integration journal, catalog handler and
  troubleshooting locally; no implementation was copied. It starts its proxy,
  patches the selected Codex openai_base_url, appends owned catalog entries to
  the native catalog, invalidates the native model cache and asks clients to
  restart. Actual picker verification necessarily follows installation.
- Connected the existing cxweb reservation, live background session, both
  reviewed catalog codecs, Host, scheduler and configuration transaction through
  SetupOwner. Added Connect to Codex in the desktop and connect-codex in the CLI.
  Private IPC deduplicates the operation with a target-bound receipt and retains
  its worker when the requesting window closes. The native model IDs are read
  through the reviewed client's model/list RPC, not guessed.
- The installed daemon is an independent copy in the private installation
  directory. Config is applied only after the host serves and the supervisor is
  registered. Started but unapplied installations remain discoverable for
  recovery. The setup UI separates installed routing from client verification;
  live route status rechecks the journal and respects disconnect/user edits.
- Activation invalidates only the selected home's regenerable models_cache.json
  through the existing checked-handle deletion. A linked/replaced/inaccessible
  cache fails before applying config. Native auth and history remain untouched.
- Live preflight on the actual CLI and C:\Users\Palo\.codex returned
  E_PREFLIGHT_CONFIG_PERMISSIONS. Read-only ACL inspection identifies the
  CodexSandboxUsers read/execute grant on config.toml and its parent; current
  private-file policy rejects that grant. A separate ancestor policy also
  rejects a sandbox write grant at the volume root. No ACL was changed and no
  real Codex configuration was written. Next work must reconcile normal native
  configuration readability with protection of the gateway capability, rather
  than simply permitting arbitrary readers or changing the user's permissions.
- The old manual-login probe process is no longer running. No cxweb runtime or
  desktop process was present in this audit; no browser or ChatGPT desktop was
  launched. Authenticated activation and both actual picker round trips remain
  unverified. The earlier title question and close-probe request are obsolete.
- Validation: workspace 256 passed / 12 opt-in ignored; desktop UI 58 passed;
  Clippy with warnings denied, Rust formatting and diff checks passed. Release
  CLI/daemon/desktop builds passed. No actual client picker pass or
  authenticated production activation is claimed by these tests.

## Native failed-test repair exercise (2026-09-20)

- Added the explicit ReadTestRepair diagnostic and Test failure and repair UI
  action. An isolated workspace starts with an incorrect output file. Codex
  reads the input, runs a genuinely failing comparison, updates the exact output
  line, runs the same test again and returns the verified final result. The
  bridge validates tool requests; the native backend executes them.
- Repair approval requires the expected completed failure attributed to the
  same thread/turn. Only the fixed update diff and target file are accepted;
  moves, input edits, extra commands, skipped/reordered tests and early final
  claims are refused. Successful verification requires real files plus ordered
  native completion events, exit codes and outputs. Delivery replay keeps the
  original receipt without advancing the exercise twice.
- Both reviewed native backends (CLI 0.155.1 and App backend
  0.155.0-alpha.9.2) passed actual execution with synthetic model responses. A
  second case corrupts only the disposable output after the repair: the actual
  retest then fails, its output returns to the model request, and a fabricated
  successful final response is rejected. Existing read/patch/test and denial
  scenarios passed in the same runs. No graphical application was launched.
- The UI serializes the new action with other tests, exposes the existing
  cancellation flow and does not label incomplete repair evidence as text or
  coding success. A passing exercise still leaves integration unqualified.
- Validation: workspace 252 passed / 12 opt-in ignored; desktop UI 56 passed;
  both actual-backend opt-in runs passed; Clippy with warnings denied, Rust
  formatting, diff checks and release CLI/daemon/desktop builds passed. Evidence:
  integration-tests/compatibility/runtime-native-repair-windows.json.
- NOT RUN: this exercise through authenticated ChatGPT, actual App picker, the
  complete coding/reliability corpus, production activation and native account
  coexistence. Synthetic response selection does not establish live model
  reasoning or coding reliability. The saved-profile manual-login probe remains
  separate from these disposable native tests.

## Explicit background startup verification retry (2026-09-20)

- An installed host can explicitly repeat a completed transient browser startup
  failure using its original saved account/workspace, route, profile and browser
  binding. The desktop offers Retry background verification for the fixed
  transient error set. Passive health checks never trigger recovery, sign-in or
  generation. Successful verification does not certify either Codex client.
- The private control operation is bound to the observed runtime instance and
  a deduplicated operation receipt. Losing an acknowledgement polls that receipt;
  it does not resubmit or target a replacement runtime. The worker survives the
  requesting window closing. Recovery still owns a cancellable gateway lease;
  disconnect drains cleanup before releasing ownership. A disconnect that wins
  admission leaves a terminal unavailable state without starting a browser.
- Only failed startup verification can be retried. In-flight/ready providers,
  authentication challenges, changed scope/build/language/model, cleanup failure
  and cancelled recovery remain ineligible. This is not a prompt retry or a
  reauthentication implementation. Hosts without a saved recovery controller
  refuse the operation; the UI then clears stale state and asks for a status check.
- Validation: cargo test --workspace --locked --quiet: 250 passed, 12 opt-in
  ignored; node --test apps/desktop/tests/*.test.mjs: 54 passed. Clippy with
  warnings denied, Rust formatting, diff checks and release CLI/daemon/desktop
  builds passed. Regression coverage includes retry eligibility, duplicate
  requests, instance changes, recovery cancellation/drain and admission races.
- Computer-use inspection used the actual local desktop HTML/CSS/scripts with
  synthetic IPC at 440 x 540. The retry transition hid its button, retained
  Unverified for both Codex clients, had no horizontal overflow and no browser
  console warnings/errors. The isolated preview tab/server were closed afterward.
- NOT RUN: authenticated installed-host retry or actual Codex App picker. The
  earlier manual-login probe still owns the saved profile. No competing browser,
  forced process termination or ChatGPT desktop launch was used for this work.

## Activation-time configuration access requirements (2026-09-20)

- The production ActivationHandle path now calls a qualified journal transaction.
  Before consuming preparation or staging any candidate, it checks unchanged
  inputs and requires current ancestor owner/DACL evidence for both selected
  configuration and recovery journal. A failed permission qualification returns
  E_ACTIVATION_TARGET_PERMISSIONS and retains the prepared state. A changed user
  config remains a configuration conflict rather than a permission diagnosis.
- Qualified atomic snapshots retain opaque access evidence in memory. Stage,
  replace and remove recheck the same ancestor descriptors before their OS
  mutation and keep identity handles through the operation; stage/replacement
  also verify afterward. Calling qualification again cannot silently accept
  changed permissions. No descriptor, user SID or new policy override is stored
  in a recovery record. This does not claim a filesystem compare-and-swap.
- Existing low-level journal/recovery fixtures retain their explicit unqualified
  transaction path. Only the cfg(test) fixture activation variant uses it through
  lifecycle orchestration; the registered production activation variant always
  uses apply_qualified. Native forwarding/recovery/removal behavior is unchanged.
- New regressions prove an exposed owned test ancestor is refused without
  staging, config changes or ACL repair, and an edited config does not consume
  prepared state or change the durable receipt. The complete qualified mutation
  success/changed-ancestor exercise requires CXWEB_QUALIFIED_FIXTURE_ROOT; the
  existing supervised activation test now requires that same qualified root.
  Both validate the root before creating fixtures or registering a task.
- Validation: workspace 247 passed / 12 opt-in ignored; desktop UI 51 passed;
  Clippy with warnings denied, formatting, diff checks and release CLI/daemon/
  desktop builds passed.
- NOT RUN: positive qualified mutation/supervised activation on this host. The
  available C: ancestor permissions already failed qualification and D: exposes
  an unsupported owner/access layout. No host ACL, ownership, volume or startup
  entry was changed to manufacture a pass. Full activation still also requires
  client/picker, native coexistence, browser/coding and release qualification.

## Selected target owner and ancestor access qualification (2026-09-20)

- Read-only preflight now captures and rechecks owner/DACL evidence for every
  component of the selected executable and Codex home. Evidence stays in memory;
  reports expose only selected_target_access_verified and a fixed
  target_permissions conflict. Unsupported or changed permissions make the
  assessment incompatible. Repository cwd identity remains checked separately.
- Path policy allows public read/execute and directory creation, but refuses
  foreign file writes/appends, reparse-capable file creation/write attributes,
  deletion, child deletion, ownership and DACL changes. Generic masks are mapped
  before evaluation; inherit-only grants are evaluated on their descendants.
  Unknown ACE layouts and NULL DACLs fail closed. Broad allows are not qualified
  on the assumption that a deny or current group membership neutralizes them.
- Current user, SYSTEM and Administrators remain trusted. The exact Windows
  Modules Installer service SID is additionally accepted for system path
  components; it is not accepted for private configuration ownership/grants.
  The SID was checked against the local Windows account resolver. No sandbox
  account or broad group is automatically trusted, and no user ACL is modified.
- New tests cover effective/inherit-only and generic rights, directory-create
  versus file-append semantics, NULL DACL, dangerous masks, changed trusted ACLs,
  unchanged private-config restrictions and the desktop conflict explanation.
  All 245 workspace tests passed / 11 opt-in ignored, all 51 desktop UI tests
  passed, and Clippy, formatting, diff checks and release builds passed.
- Actual CLI 0.155.1 and cached App backend 0.155.0-alpha.9.2 preflight passed in
  an existing isolated signed-out fixture: both reported the host's unsupported
  target ancestor permissions, zero model requests, unchanged configuration and
  no activation eligibility. Existing fixture routing conflicts were retained.
  Evidence: integration-tests/compatibility/native-ancestor-access-windows.json.
  An initial command to create/ACL/clean a new fixture was rejected by automatic
  policy before execution; using the existing fixture required no ACL changes.
- This is preflight evidence, not a future activation permission receipt. Binding
  qualification to the eventual activation transaction and deployment layout,
  actual App picker/native coexistence, browser qualification and release gates
  remain open. No production activation or authenticated browser test ran here.
- Sources: [Windows rights and inheritance](https://learn.microsoft.com/en-us/windows/win32/fileio/file-security-and-access-rights),
  [file/directory rights](https://learn.microsoft.com/en-us/windows/win32/fileio/file-access-rights-constants),
  [reparse-point access requirements](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fsa/4aeefef8-92c3-4abc-af7a-a610caf8a165).

## Configuration ancestor identity during mutations (2026-09-20)

- Configuration snapshots now inspect every component of the original selected
  parent before canonicalization. Junctions in the parent or an earlier ancestor
  are refused for existing and absent configuration files. Revalidation retains
  the original selected path instead of trusting only its canonical destination.
- Stage, commit and removal retain ancestor handles across their operation and
  recheck identity before the replacement/deletion boundary. Handles are released
  after the operation, rather than keeping user directories pinned for the whole
  lifetime of an installed journal. Existing content/identity/ACL checks remain.
- A new empty-directory regression exposed that the former attribute-only
  directory handles did not prevent rename on this Windows installation. The
  path guard now requests directory read access, without enumerating contents,
  to establish the intended sharing restriction. The regression proves both
  immediate-parent and earlier-ancestor renames fail while the guard is held and
  succeed after release. The older executable test alone did not catch this.
- Added real Windows junction regressions for initial capture and redirection
  between capture and mutation. Redirected stage/commit/removal leave the original
  config and staging bytes unchanged. Test directory names now include a process
  counter: timestamp-only names collided during parallel fixture creation.
- Validation: workspace 241 passed / 11 opt-in ignored; desktop UI 50 passed;
  Clippy with warnings denied, formatting, diff checks and release CLI/daemon/
  desktop builds passed. These tests use disposable local directories, with no
  login, user config writes or app launch. The old manual-login process remains
  live; no competing saved-profile browser or ChatGPT desktop was launched.
- Remaining: ancestor ACL qualification and in-place reparse mutation races.
  This change does not claim filesystem CAS or authorize production activation.
  The selected config/immediate-parent ACL checks remain separate from that work.

## Native patch-and-test feedback exercise (2026-09-20)

- Extended the selected-client tool exercise with an actual test command after
  the read and custom patch. The command compares the two disposable fixture
  files and returns a real success/failure exit code. The existing desktop
  button is now Test coding tools; its result identifies all three operations.
  The earlier read/patch and denial modes remain independently covered.
- Pre-delivery validation permits only read, exact patch, exact test and final
  result, in that order, retaining response replay identity. The test command
  cannot request a different directory, network approval or altered script.
  Native approvals remain individually attributed to the current thread/turn;
  test approval requires the observed completed patch. The bridge executes no
  tools. Final success requires native completion, exit code 0, exact test
  output and unchanged input/correct output files.
- Both reviewed native backends passed isolated read/patch, denial, successful
  patch/test and injected failing-test scenarios. The fault case corrupts only
  its disposable output file before the native test. Real failure feedback
  reaches the subsequent synthetic model request, and a fabricated successful
  final response is rejected as E_NATIVE_PROBE_TEST. No account is used.
- Validation: workspace 238 passed / 11 opt-in ignored; desktop UI 50 passed;
  the actual-native fixture suite passed separately on CLI 0.155.1 and cached
  App backend 0.155.0-alpha.9.2. Clippy, formatting, included-file formatting,
  diff checks and release CLI/daemon/desktop builds passed. Evidence:
  integration-tests/compatibility/runtime-native-test-feedback-windows.json.
- NOT RUN: this new exercise through the authenticated web model or actual App
  renderer/picker. The manual-login process was revalidated live and still owns
  the saved browser profile. No competing browser was started. This fixture is
  one additional coding scenario, not the full G2 corpus or route reliability
  sample, and it does not enable production activation.

## Desktop attachment and guarded removal (2026-09-20)

- Desktop startup now inventories owned applied/restoring/restored integration
  journals before attaching to the login controller. Prepared-only journals do
  not imply installation. A single connection opens its private health view;
  multiple homes require local selection. Missing, corrupt, duplicate or stopped
  installed connections never launch a replacement runtime or login browser.
- Inventory uses bounded enumeration, protected directories, path identity
  guards and atomic journal snapshots without taking the live writer's lock.
  It returns only installation identity and the local home label, never original
  config, account credentials, route capability or proxy URL. Every check/removal
  revalidates membership. This inventory is not a diagnostic export.
- The new local Tauri commands expose passive health and explicit removal. The
  UI shows separate ChatGPT/App/CLI states and component observations in Details.
  Idle checks run at 30-second intervals and stop while hidden. Failed checks
  clear stale verified rows. Existing login/qualification controls remain for
  the no-installation case; activation eligibility is unchanged.
- Idle removal atomically closes web admission only with no active generation.
  A turn that starts after the displayed snapshot returns ActiveWork and opens
  the same confirmation dialog used for an already busy snapshot. Escape keeps
  the connection; focus returns to the removal button. Confirmed cancellation
  remains bound to the exact displayed runtime instance. An idle operation ID
  cannot be upgraded into a force-removal receipt. Ambiguous acknowledgements
  are inspected, never resubmitted automatically.
- An accepted removal survives the UI caller. Cleanup and configuration undo
  still use the existing lifecycle owner; the retained native listener is not
  stopped. The UI preserves the requirement to restart Codex after restoration.
- Validation: workspace 236 passed / 11 opt-in ignored; all desktop UI tests
  50 passed; Clippy, formatting, included-file formatting and diff checks passed.
  CLI, daemon and desktop release builds passed. A local synthetic UI fixture
  was inspected in the in-app browser, including the 440x540 dialog layout,
  focus, Escape and completed-removal feedback; no console warnings/errors were
  observed. The temporary preview tab and server were closed afterward.
- NOT RUN: actual installed Tauri-to-authenticated-browser lifecycle, App picker,
  native subscription coexistence and production activation. Browser fixture
  rendering is not evidence for those paths. ChatGPT desktop was not used.
  Evidence: integration-tests/compatibility/desktop-installed-control-windows.json.

## Passive installed-runtime health (2026-09-20)

- Added the PRD health contract to domain types and private runtime IPC. The
  `cxweb runtime-health --installation <id>` command reads this snapshot without
  starting a runtime, opening a browser, navigating or submitting model text.
  Output excludes the installation/instance IDs, route capability, private URL,
  account data and filesystem paths. Unknown backend codes are sanitized.
- The reducer distinguishes recovery, observed browser/auth/model health,
  unavailable sessions, active turns, cleanup failure and disconnect/config
  restoration states. Repeated unchanged observations keep their revision and
  timestamps. Recovery/catalog leases do not count as active model turns.
- Initial activation and recovered providers retain their actual observation
  time and owner-channel liveness. Fresh validated responses refresh browser
  evidence; observed scope/auth/model failures invalidate it. Cached deliveries
  and request-validation errors cannot erase a known session failure or claim
  a new browser verification. Evidence travels only in process, not on the wire.
- Browser evidence alone never marks native subscription forwarding, actual
  App/CLI picker or configuration health as verified. No Ready state is inferred
  from an open port. Unknown health schema versions are rejected by the client.
- Validation: workspace 234 passed / 11 opt-in ignored; desktop UI 40 passed;
  actual built CLI against a synthetic protected runtime 1 passed. Its output
  also passed the published JSON Schema with date-time format checking. Clippy,
  formatting, included-file formatting, diff checks and release builds of CLI,
  daemon and desktop passed. Evidence:
  integration-tests/compatibility/runtime-health-windows.json.
- Remaining: desktop attachment to installed-runtime health and independent
  native/config/client evidence. No authenticated browser or actual Codex App
  picker exercise ran in this change; ChatGPT desktop was not launched or used.
  Production activation and complete qualification remain disabled/incomplete.

## Installed web-session recovery after runtime restart (2026-09-20)

- Connected the activation owner to durable web recovery metadata in the private
  integration journal: installation-bound account/workspace hashes, one exact
  observed route, browser build, protocol evidence and reviewed client codecs.
  These are recorded before configuration apply. This records the caller's
  existing qualification decision; it does not certify new clients or routes.
- Host recovery retains its original loopback route and starts native forwarding
  immediately. An applied installation with valid metadata and a still-owned
  config route receives a pending web provider. Its one background recovery
  attempt rechecks browser build, English authenticated UI, account/workspace,
  route identity/label, empty composer and Temporary Chat without sending text.
  The original scope and turn ledger are reused, preserving replay/uncertainty.
- A slow initial navigation is reobserved on the same page for up to 45 seconds.
  Login/verification requirements, observation errors and cancellation terminate
  the attempt. No visible login window, second browser or model request is used
  as an automatic retry. A busy profile is refused without disturbing its owner.
- Native forwarding continues while web recovery is pending or failed. Owned
  catalogs become available only after recovery succeeds. Prepared installs,
  changed owned routing, new profile/provider/catalog conflicts, old journals
  without web evidence and malformed/obsolete web receipts remain native-only.
  Unrelated configuration edits survive; recovery never rewrites configuration.
- Recovery holds a gateway drain lease. Disconnect cancels it and waits for its
  cleanup; a late successful result cannot republish after cancellation. Failed
  browser cleanup prevents a successful disconnect result. Tests use temporary
  protected journals, local synthetic upstreams and synthetic readiness results.
- Validation: workspace 229 passed / 10 opt-in ignored; desktop UI 40 passed;
  Clippy, formatting, explicit included-file formatting and diff checks passed.
  Release CLI, daemon and desktop builds passed.
  Evidence: integration-tests/compatibility/installed-web-recovery-windows.json.
- NOT RUN: actual authenticated recovery, real browser revalidation and the
  production desktop activation path. The previous manual-login cxweb process
  was rechecked and still holds the saved profile. This adds the restart path
  needed by activation; it does not enable activation or complete G0/G2/G3.

## Native command denial exercise (2026-09-20)

- Added an explicit Test command denial action to selected-client diagnostics.
  The typed exercise field travels through Tauri, the private control protocol,
  the operation receipt and the persistent setup owner. Omitted exercise means
  text; receipt reuse with either different tool exercise is refused.
- The runner declines exactly one fixture-bound read approval. It then requires
  an attributed native completed command with status declined, no file-change
  event, no input marker in command output, unchanged input and absent output
  file, and the exact final denial acknowledgement. It never approves the read
  or a patch in this exercise. The existing cancellation and process-tree cleanup
  apply. Unexpected approval requests still fail the diagnostic.
- The buffered response policy permits only the exact read followed by the
  denial acknowledgement. Early final output, a second read, patches, claims of
  successful reading and additional responses fail before reaching the native
  client. Exact ledger replay does not advance the sequence twice.
- Actual CLI 0.155.1 and App backend 0.155.0-alpha.9.2 passed text/cancellation,
  read/patch and denial exercises using synthetic local responses. The denial
  check observed the native rejection in the next model request and zero file
  content leakage. No ChatGPT desktop application was launched or automated.
- Workspace: 221 passed, 10 opt-in ignored. Desktop UI: 40 passed. Clippy,
  formatting, explicit native-context include formatting and diff checks passed.
  Release CLI, daemon and desktop builds passed.
  Evidence: integration-tests/compatibility/runtime-native-denial-windows.json.
- These checks do not certify live model compliance, the required coding corpus,
  the actual Codex App picker or subscription coexistence. The previous manual
  login still owns the browser profile, so the authenticated desktop action is
  not yet run. Production activation remains disabled.

## Runtime-owned native read and patch exercise (2026-09-20)

- Added Test read and patch to the desktop's selected-client diagnostics. It uses
  the existing persistent setup owner, operation receipt, cancellation and reset
  paths. The receipt binds the tool-test option as well as every selected path
  and route; omitted options remain text-only. Results explicitly distinguish
  this single exercise from the full coding corpus, actual picker and activation.
- The native runner now seeds one random input line in a fresh private workspace,
  asks the reviewed backend to read it once with exec_command and add one file
  with the literal apply_patch tool, and verifies the attributed completed native
  items, exit status, exact final answer and both file contents. The random line
  is not supplied in the task prompt. The bridge writes only the input fixture;
  Codex executes the read and writes the output. No runtime Node dependency.
- The isolated test provider validates the complete buffered native response
  before returning any JSON/SSE bytes. It permits exactly the fixed read with
  login=false, the exact single-file patch, and the expected final answer, in
  that order. Changed commands/paths/arguments/namespaces, extra calls, elevated
  permissions and out-of-order output fail. Replayed ledger deliveries are
  checked again and do not advance the exercise twice. This fixture restriction
  is absent from production providers and ordinary text diagnostics.
- The isolated app-server runs with explicit untrusted/read-only settings and
  accepts only the fixture's individual native approval requests, bound to its
  thread/turn and patch item. Native credentials remain absent. Before protocol
  initialization, the reviewed backend is attached to a Windows kill-on-close
  job. Completion/cancellation terminates and drains its process tree, then
  reaps the direct child. This is attachment before tool execution, not a claim
  that arbitrary executables are suspended before their startup code.
- Verification: cargo test --workspace --locked --quiet passed (219 tests,
  10 ignored); node --test apps/desktop/tests/app.test.mjs passed (39 tests).
  Workspace Clippy with warnings denied, formatting, diff checks and release
  CLI/daemon/desktop builds passed. Both opt-in native tests passed separately
  on CLI 0.155.1 and App backend 0.155.0-alpha.9.2: text/cancellation and the
  read/patch exercise, using local synthetic responses. The process-tree test
  verified that attached descendants are terminated and drained.
- Limits: this is one fixed exercise, not G2 certification. New authenticated
  desktop-to-browser-to-native tool execution has not run because the manual
  login process still owns the saved profile. No existing blocked browser
  surface or ChatGPT desktop was accessed. Actual App picker, native coexistence,
  full coding/model qualification and production activation remain incomplete.
- Evidence: integration-tests/compatibility/runtime-native-tools-windows.json.

## Idle native test recovery (2026-09-20)

- Added an explicit Reset client test session action through the scoped desktop
  command and receipt-based private control protocol. It releases an idle
  generation owner, reacquires the profile lock for login control, clears the
  prepared test target/results, and returns to disconnected without opening a
  browser or sending another test. Reset before handoff drops only the unused
  preparation, preserving the login controller's existing browser state.
- Retirement holds the exclusive generation consumer lease. Running probes,
  request cleanup and installed hosts holding that lease prevent reset. The
  driver also refuses live/orphaned page leases and checks the browser target
  inventory before closing. Unknown inventory or additional tabs preserve the
  owner. A refused reset keeps the cached qualification and returns a fixed
  error; operation replay never repeats the reset. Lost ownership or failure to
  reacquire the profile instead invalidates the previous qualification.
- The accepted worker operation waits for actual browser exit and profile lock
  release. Stored sign-in data is not deleted. Test reset is not a substitute for
  disconnecting an installed route or signing out. Production activation remains
  disabled and no installed routing is modified by this action.
- Hidden desktop windows stop their cached native-test status polling. Showing
  the window resumes a cached read, without re-observing ChatGPT or resubmitting
  a model request. Scoped command-registration tests now also check native text,
  cancellation and reset permissions.
- Verification: cargo test --workspace --locked --quiet passed (213 tests,
  9 ignored); node --test apps/desktop/tests/app.test.mjs passed (38 tests).
  Clippy with warnings denied, formatting, diff checks and release CLI/daemon/
  desktop builds passed. The opt-in generation receipt test passed with actual
  Chrome on fresh unsigned-in profiles, covering ordinary shutdown, idle reset,
  consumer refusal, profile lock release, retained profile data and idempotent
  retirement. It did not access the user's existing browser surface.
- Live limits: the previous manual-login process was revalidated as running and
  still owns the saved profile. The new reset action has not been visually tested
  against authenticated ChatGPT. Actual App picker, native coexistence, broader
  coding qualification and production activation remain open.
- Evidence: integration-tests/compatibility/idle-native-test-recovery-windows.json.

## Native test cancellation and reopened controls (2026-09-20)

- Cached runtime status now identifies the active native test and whether its
  cancellation was requested. An explicit CancelNative command is bound to the
  exact runtime instance and operation receipt. Unknown, stale-instance and
  non-native receipts are rejected; replaying a completed cancellation cannot
  stop a later test. Receipt admission remains busy until cleanup finishes.
- The setup owner passes cancellation through to the isolated native runner,
  checks it before preflight/handoff work, and retains the browser handoff receipt
  even if cancellation arrives during transfer. Already-cancelled setup only
  reads its cached browser receipt and does not launch a client. Cancellation
  does not drop an in-progress ownership transfer or native cleanup future.
- The desktop can observe and cancel a test while its original request is waiting
  or after reopening. Cached reads no longer wait for the mutation mutex. The UI
  keeps conflicting controls disabled until cleanup, ignores stale polling
  results after completion, and retries only cached reads after a transient IPC
  failure. It never automatically submits another model test. Fixed an invalid
  UTF-8 byte in the previous native test waiting label.
- Verification: workspace tests passed (211 passed, 9 ignored); desktop UI tests
  passed (35); Clippy with warnings denied, formatting and diff checks passed.
  Release CLI, daemon and desktop binaries built. The opt-in isolated native
  response/cancellation test also passed separately for CLI 0.155.1 and App
  backend 0.155.0-alpha.9.2, using synthetic local responses and disposable homes.
  Those tests launch backend executables only, never the ChatGPT desktop app.
- Live limits remain: the manual-login process still owns the saved profile;
  no blocked browser surface was accessed through an alternative. The new
  desktop cancellation flow has not been tested visually against authenticated
  ChatGPT. Actual Codex App picker and production activation remain unverified.

## Desktop-to-runtime native qualification control (2026-09-20)

- Added a daemon-owned SetupOwner. It reinspects the selected executable/home/cwd,
  holds the original home path identity, reserves a reversible installation plan
  without applying it, and hands the background-qualified route to that persistent
  installation scope. It retains the prepared listener/journal and generation
  receipt across both successful and failed native text tests. A different home
  or route cannot silently reuse an existing preparation.
- Added a typed private NativeText command. Operation receipts include every
  selected target field, reject conflicting reuse, and finish independently of
  the requesting desktop future. Cached status carries a sanitized native text
  result/error. The new non-observing Control snapshot preserves qualification
  while the setup owner checks eligibility or reports a failed test.
- Desktop Details now exposes Test selected client after background protocol
  qualification. It requires the selected paths, disables conflicting controls,
  sends one explicit request, and distinguishes text transport from coding,
  actual picker qualification and production activation. The Tauri command is
  scoped to the existing local main window; its permission file was generated
  from build.rs. The report uses owned strings for private protocol decoding.
- Tests cover native request deduplication and conflicts across all four target
  fields, preservation of qualification on early refusal, fixed error redaction,
  the actual Windows private pipe with a dropped desktop waiter and a reopened
  status client, and frontend selection/gating/single submission/error display.
- Verification: cargo test --workspace --quiet (209 passed, 9 ignored), node --test
  apps/desktop/tests/app.test.mjs (31 passed), workspace Clippy with warnings
  denied, cargo fmt check, explicit native_context_probe.rs rustfmt, diff check
  and release CLI/daemon/desktop builds passed.
- Live state: the previous manual-login probe was authoritatively polled and is
  still running. It retains the saved browser profile. Computer Use again stopped
  when attempting to observe the existing Chrome login window because it could
  not determine the URL confidently enough to enforce policy. No further Windows
  UI input or alternate access to that blocked surface was attempted. The user
  was asked to close that manual test window. The new desktop binary was not
  launched and the new authenticated end-to-end action has not yet run.
- Evidence: integration-tests/compatibility/desktop-native-text-control-windows.json.
  Remaining: finish the live browser/native test after the profile is released,
  expose cancellation/recovery and complete coding/picker/native coexistence and
  production activation. G0 and the original acceptance goal remain incomplete.

## Runtime-owned native text qualification (2026-09-20)

- Added a Rust native client runner and an in-process qualification entry point
  for a handed-off GenerationSession. It starts the existing-session diagnostic
  endpoint, creates a fresh protected native home/workspace, verifies the selected
  executable against reviewed hashes while holding its original path identity,
  checks effective local routing and signed-out account status, then asks the
  actual app-server to list/select the owned model and complete one exact text
  turn. No Node runtime is required by this path.
- The accepted operation survives a dropped UI waiter. Explicit cancellation and
  the generation deadline stop and reap only its own child before draining the
  diagnostic endpoint. The shared browser owner remains alive. Client RPC input
  is bounded, duplicate JSON keys are rejected, server actions are refused, and
  text results must match one completed message from the requested thread/turn.
  Errors and public reports do not export prompts, headers, account IDs, native
  error text or capability URLs.
- Actual backend tests passed for CLI 0.155.1 and App backend 0.155.0-alpha.9.2
  against a local synthetic server. Both list/select the owned route, return the
  exact fixture answer, and stop during a deliberately held HTTP response.
  Request inspection confirms no Authorization, account ID or Cookie header.
  The test does not launch either desktop renderer or use the user's native home.
- Observed contract detail: these app-server builds report account:null and send
  no Authorization header when merely given OPENAI_API_KEY in their environment.
  The new diagnostic no longer supplies a synthetic key at all. It requires a
  fresh signed-out home and uses the private local capability endpoint. This is
  isolated diagnostic evidence, not proof of subscription-auth coexistence.
- Added Windows cfg guards around the generation-consumer lease in the portable
  coordinator. No non-Windows compilation was run in this environment.
- Verification: cargo test --workspace --quiet (205 passed, 9 ignored); the new
  ignored actual-native-backend test was separately run successfully with each
  reviewed executable. Workspace Clippy with warnings denied, cargo fmt check,
  explicit native_context_probe.rs rustfmt and git diff --check passed. Release
  builds for CLI, daemon and desktop passed.
  Evidence: integration-tests/compatibility/native-text-runner-windows.json.
- Remaining: connect the new entry point to the daemon/control progress and
  selected desktop target, then run it against the authenticated generation
  session. This turn verifies the native client component using synthetic text;
  it does not certify live browser output, coding capability, actual App picker
  or native subscription coexistence. Production integration remains disabled.

## Reuse the generation session for isolated qualification (2026-09-20)

- The live probe now accepts a handed-off GenerationSession. Its endpoint keeps
  the selected route ID, observed English label, effort and account/workspace
  scope. It neither launches another browser nor closes the existing owner when
  the diagnostic finishes. The original standalone probe still closes its own
  browser on normal completion and reported errors.
- Diagnostic native traffic stays at a local rejection server for HTTP and
  WebSocket. HTTP POST now explicitly returns 503 instead of the router's default
  405. The diagnostic catalog remains separate from production qualification;
  this API does not modify native configuration or certify coding support.
- A session consumer lease excludes a second probe or installed coordinator.
  The coordinator retains that lease through its detached cancellation cleanup,
  even after the caller and gateway disappear. PreparedInstallation acquires
  the same lease before opening its generation ledger.
- The actual Chrome integration test reuses one fresh headless profile for both
  diagnostic transports and the prepared host. It checks route preservation,
  local native rejection, competing-consumer refusal, preserved existing output,
  continued driver/profile ownership after tests, and subsequent host binding.
  A deterministic unit test holds cleanup open and verifies a cancelled request
  cannot release exclusive ownership prematurely.
- Verification: cargo test --workspace --quiet (203 passed, 8 ignored);
  cargo test -p cxweb-runtime generation_session_binds_to_reserved_installation_without_another_browser -- --ignored --nocapture
  (passed); cargo clippy --workspace --all-targets -- -D warnings, cargo fmt
  --all --check, explicit native_context_probe.rs rustfmt and git diff --check
  passed. Release CLI, daemon and desktop builds also passed. Evidence:
  integration-tests/compatibility/reused-generation-probe-windows.json.
- Remaining: an activation owner must run the selected native client against this
  isolated endpoint and connect the result to desktop progress. This change does
  not establish authenticated browser generation, actual App picker behavior or
  native subscription coexistence. Production integration remains disabled.

## Mandatory supervisor receipt for public activation (2026-09-20)

- ActivationHandle now registers the independently qualified installed daemon
  through the same serialized installation owner as apply/disconnect. It checks
  the original executable path identity, persists the Task Scheduler plan first,
  creates a new task without replacing an existing task, and persists the exact
  registration receipt. An accepted operation outlives its calling UI future.
- Public apply requires a recorded task whose current OS definition still matches
  the receipt. Missing/pending registration and a removed/changed registration have
  distinct fixed errors. Invalid executable paths do not create a pending plan.
  Read-only registration status exposes no task XML, local user ID or capability.
- Previous file/ordering tests retain an explicit test-only supervisor substitute;
  that variant is absent from production builds. Those tests now also assert that
  the public operation refuses their missing registration. Real registration and
  public apply are covered separately through the actual Windows scheduler.
- The live test creates two disposable installations using independent copies of
  the built windowless daemon. It verifies refusal before host startup and before
  registration, create-only behavior, successful public apply with a receipt, and
  refusal after removing the exact recorded task. Scheduler status confirmed an
  actual daemon run with exit code 3 while the active host held the journal.
  Both test tasks were removed after confirmed stop; no arbitrary PID was killed.
- The first fixture used Cargo's hardlinked top-level build output and correctly
  failed executable identity checks. The fixture now uses an installed copy;
  hardlink protection remains intact. Test cleanup also preserves diagnostics
  during unwinding instead of causing a second panic on a still-open journal.
- Verification: 202 workspace tests passed (8 opt-in tests excluded by the default
  run); the new real-scheduler test passed against the newly built daemon. Clippy
  with warnings denied, formatting, diff checks and release CLI/daemon/desktop
  builds passed. Evidence: `integration-tests/compatibility/supervised-activation-windows.json`.
- This confirms registration and guarded duplicate startup, not full crash recovery
  of an authenticated generation session. Desktop/daemon activation orchestration,
  client/browser qualification, installed-binary trust and the remaining release
  requirements still prevent production activation.

## Reserved installation and running activation host (2026-09-20)

- Added a new-installation runtime path alongside native-only recovery. It binds
  an exclusive loopback port and persists the reversible configuration plan before
  exposing a persistent installation ID to the browser handoff owner. Reserving or
  dropping a reservation never changes the selected Codex configuration.
- The prepared installation binds GenerationSession to its durable coordinator,
  ledger and catalog. Installation identity, selected route ID, observed label and
  effort must agree. Only explicitly supplied coding-qualified client catalog rows
  are accepted; protocol evidence is not automatically promoted to coding evidence.
- Host construction checks the exact IPv4 loopback endpoint, journal capability
  and catalog receipt, then exposes the existing private disconnect service. A
  separate activation handle applies the captured configuration only while the
  serve future is running. Apply and disconnect share serialization; an accepted
  write outlives its caller. A write refusal preserves the running native listener.
- Tests use disposable configuration files and local synthetic native/web peers.
  They prove both routes answer before apply, stopped/unstarted hosts cannot apply,
  edits after preparation are preserved, repeated apply is refused, and dropping
  an accepted waiter does not interrupt the write or its later reversible restore.
  An additional real-Chrome test binds a synthetic session to this installation,
  starts Host, applies only its disposable config, and verifies profile ownership
  until driver shutdown. It submits no account messages or native requests.
- Verification: 202 workspace tests passed (7 opt-in tests excluded by the default
  run); the new browser/installation test passed separately. Clippy with warnings
  denied, formatting and diff checks passed. Release CLI, daemon and desktop builds
  succeeded. Portable evidence is recorded in
  `integration-tests/compatibility/activation-host-windows.json`.
- This is the in-process activation layer, not a new desktop/private-IPC command.
  The daemon entry point and desktop still need an activation owner that completes
  selected-target preflight, actual coding/picker/native-coexistence qualification
  and supervised restart registration before calling apply. The new path does not
  yet configure checkpoint recovery. These prerequisites and authenticated full-
  flow verification remain release blockers; production activation stays disabled.

## Internal login-to-generation ownership handoff (2026-09-20)

- Added an in-process Control operation that transfers the existing managed browser
  and its exclusive profile lock into ManagedDriver. No second browser is launched.
  The operation waits for its worker receipt; a completed receipt remains owned by
  Control if the requester disappears. The same installation/route request returns
  the same session, while a conflicting request is refused.
- Before transfer, require current tool-protocol evidence for the exact selected
  route, Temporary Chat evidence and a background page. Reobserve authentication,
  English locale, idle composer, model identity and account/workspace; derive the
  persistent installation's scope from that fresh surface. Reject other tabs and
  recheck the composer after account inspection before releasing the setup page.
  Observation failures retain the browser and profile owner in Control.
- While the generation owner is live, login control returns status but refuses
  reconnect, background and qualification mutations. After driver shutdown it
  reacquires the profile lock before accepting browser work again. The desktop
  renders `generation_ready` as awaiting integration, with only status available.
- This API is intentionally not a new private-IPC/desktop activation command.
  Native coding/picker qualification, production activation and configuration
  application are not granted by a tool-protocol result. Connecting the activation
  owner to this operation and verifying the full authenticated UI-to-gateway flow
  remain open.
- Verification: 199 workspace tests passed (6 opt-in tests excluded by the default
  run); all 28 desktop UI tests passed; Clippy with warnings denied, formatting and
  diff checks passed. Release CLI, daemon and desktop builds succeeded. The new
  real-Chrome ownership test passed separately with a
  fresh profile: the receipt replays the same owner, rejects changed identity,
  retains the exclusive lock across requester/receipt drops and releases it after
  driver shutdown. It did not log in or submit account messages.
  Portable evidence: `integration-tests/compatibility/generation-handoff-windows.json`.

## Browser replacement preserves retained pages (2026-09-20)

- Background/login transitions now check the target inventory before releasing
  the browser. Other tabs, including retained uncertain qualification pages,
  prevent replacement. The known login keeper is exempt only while its exact
  target still reports `about:blank`. Malformed inventories and duplicate target
  identities cannot authorize closure. The inventory is checked again after the
  selected target closes; this is not an atomic lock against external navigation.
- A background page must be idle before explicit replacement. Drafts and active
  responses remain open. Recognized login/verification surfaces without a composer
  can transition to visible sign-in; unknown markup and credential forms cannot.
  This check reads no credential values and does not complete any verification.
- The process owner checks its actual Windows process handle. A transport failure
  no longer discards a live browser. Reconnect reobserves the same tracked page;
  a positively exited process can be released without a successful CDP observation.
  Launch/transition failures retain the owner instead of killing its other tabs.
- The desktop explains the extra-tab refusal in English and does not retry the
  action or claim background success. A new test covers this error receipt.
- The opt-in regression ran against installed Chrome 153.0.8010.48 using fresh
  headless and offscreen profiles. It exercised retained tabs, a closed login
  target, the login keeper, drafts, active responses, unknown markup, empty
  composers, login/challenge fixtures and confirmed process exit. No account,
  saved production profile or ChatGPT desktop application was used. Portable
  evidence: `integration-tests/compatibility/browser-replacement-windows.json`.
- Verification: 197 workspace tests passed (5 opt-in tests ignored by the default
  run); the new real-browser test passed separately; all 27 desktop UI tests,
  workspace Clippy with warnings denied, formatting and diff checks passed.
  Release CLI, daemon and desktop builds succeeded. These results do not certify
  live authentication, the actual Codex App picker
  or production browser handoff/activation.

## Desktop target selection and actual preflight UI (2026-09-20)

- Desktop details now include explicit executable, CODEX_HOME and working-directory
  selection. Discovered reviewed binaries populate the selector; unknown builds
  cannot be selected from it. Manual paths still pass the same pinned executable,
  path identity, configuration access and native RPC checks as the CLI. No guessed
  home or configuration path is silently applied.
- The Tauri command returns the existing sanitized report. The UI displays native
  authentication mode, configuration conflicts and remaining qualification checks.
  Compatible configuration still does not imply active integration or a verified
  picker. An edited path invalidates the previous report; in-flight results for a
  changed target are discarded. Duplicate submission and concurrent discovery are
  excluded while inspection is pending. Failures never expose raw backend text.
- Actual release-window testing found a native discovery crash (Windows Application
  event 1000, exception 0xc00000fd). The fingerprint future held a 64 KiB inline
  buffer, which amplified stack usage through GUI command dispatch. The chunk now
  resides on the heap. A regression bounds the fingerprint, discovery and preflight
  futures so the original allocation fails the test. Chunk size and read limits
  are unchanged. The exception means stack exhaustion, documented by
  [Microsoft](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/debugging-a-stack-overflow).
- After rebuilding, Windows Computer Use successfully ran native discovery in the
  actual cxweb Tauri window, entered a protected disposable signed-out home/workspace,
  inspected CLI 0.155.1, selected App backend 0.155.0-alpha.9.2 from the dropdown and
  inspected it. Both reported Signed out and the subscription requirement without
  activation. The first result disappeared on selection change. Result layout was
  visually inspected, the fixture configuration remained absent and the test window
  was closed afterward. Portable evidence: `integration-tests/compatibility/desktop-native-preflight-ui.json`.
- The existing manual-login probe remained live and held the browser profile, so
  the separate browser runtime status correctly stayed unavailable during this
  independent native-target test. No blocked Chrome surface was accessed, no
  ChatGPT desktop was launched and no real native login/configuration was changed.
- Validation: 26 Node desktop UI tests passed; all 195 workspace Rust tests passed
  with four opt-in tests ignored. Workspace Clippy with warnings denied, formatting
  and diff checks passed. Release desktop and CLI builds succeeded. Full browser,
  actual Codex App picker, native subscription coexistence and production activation
  remain unqualified.

## Read-only native executable discovery (2026-09-20)

- Added `cxweb native-discover` and Find Codex installations in desktop details.
  Both use the same read-only inventory: direct PATH executables, observed Windows
  x64 npm loader layouts and the desktop backend cache. Script wrappers and package
  hooks are never executed. Discovery does not open native credentials, inspect
  conversation history, launch a client or change its configuration.
- Candidate paths pass the existing original-path guard before filesystem reads
  can follow an ancestor reparse point. Executables stay protected against writers
  during bounded fingerprinting. Canonical duplicates merge their source labels;
  different installations remain distinct. Only pinned binary hashes receive a
  reviewed build/codec; unknown bytes are explicitly unreviewed. Relative PATH
  entries, inaccessible/reparse targets and enumeration limits produce fixed
  diagnostics without exporting raw environment values or OS errors.
- The installed desktop cache contained four version directories but only one
  remaining codex.exe. The live inventory found exactly the two expected reviewed
  binaries: CLI 0.155.1 and App backend 0.155.0-alpha.9.2. An independent harness
  reread each returned executable and confirmed its reported SHA-256. One target
  identity diagnostic remained for an unsupported search location; discovery does
  not claim every possible installation was inspected successfully.
- Evidence: `integration-tests/compatibility/native-discovery-windows.json`,
  reproduced by `node scripts/probe-native-discovery.mjs`. Portable evidence omits
  user paths and environment values. The normal local UI/CLI output includes the
  executable paths so a later target-selection flow can use them.
- A cached backend is not proof of the running GUI's backend or effective home.
  Target selection, configuration preflight and actual picker qualification remain
  required. Custom package layouts and executable links need separate supported
  discovery; this does not guess their targets or certify unknown versions.
- Validation: all 194 workspace Rust tests passed (four opt-in tests ignored),
  and all 21 Node desktop UI tests passed. Tests cover multiple sources, duplicate
  locations, unknown executables, empty/relative PATH entries, bounded discovery,
  explicit UI action and preserving browser state on discovery failure. Workspace
  Clippy with warnings denied, formatting and diff checks passed. Release CLI,
  daemon and desktop builds succeeded. Native App UI and browser qualification
  were not exercised by these tests.

## Desktop background and tool-test controls (2026-09-20)

- The desktop now exposes the existing private runtime actions for background
  transition and tool-protocol qualification. Tauri handlers, generated command
  permissions and the local main-window capability are connected; no remote UI
  receives these commands. No arbitrary command, path or browser script is accepted.
- A detected foreground session offers Continue in background. The existing
  controller checks the official signed-in page and refuses a draft or active
  response before closing it. Loading or verification challenges are displayed
  separately from a recognized background session. Closing the control window
  continues to leave the separate runtime alive.
- A successful text receipt with verified Temporary Chat offers one explicit tool
  protocol test. The disclosure describes allowance use and fixture-only tool calls;
  the result still states that execution through Codex needs verification. The UI
  does not install routing or treat either diagnostic as production qualification.
- All action buttons share pending-state exclusion. Duplicate and cross-action
  clicks cannot enqueue another test or close the browser during a pending test.
  After an error, the UI retrieves the cached runtime state with refresh:false,
  without repeating browser work. An unavailable receipt suppresses stale success
  and test controls while keeping the original failure visible.
- Validation: all 19 Node desktop UI tests passed, covering action sequencing,
  qualification prerequisites, uncertain submissions, background loading/failure,
  stale-state recovery and command permissions. All four real Windows-pipe
  RemoteControl tests passed, including runtime survival when the UI waiter closes.
  Desktop Clippy with warnings denied, formatting and diff checks passed.
  The release desktop build succeeded.
- These checks use synthetic browser results and do not establish a new live
  background or tool-generation pass. Windows Computer Use refused observation
  of the existing Chrome window because its URL could not be established reliably.
  The separate local static UI preview was also unavailable in the in-app browser
  (net::ERR_BLOCKED_BY_CLIENT); its temporary server was stopped. Neither blocked
  surface was accessed through another method. No ChatGPT desktop was launched.

## Original target path identity in native preflight (2026-09-20)

- Native inspection now checks the original executable, home and workspace paths
  before canonicalization can erase a junction or symbolic-link ancestor. Each
  component is opened with reparse processing disabled and its object type and
  volume/file identity are checked. Only drive-absolute paths are supported;
  parent traversal, alternate streams, trailing-dot/space aliases and network or
  device namespaces are refused. The executable also rejects hard links.
- Handles remain alive through inspection, excluding ancestor rename/delete and
  executable writers. Both held objects and path resolutions are rechecked before
  child launch and after shutdown. A regression caught that metadata-only file
  access did not exclude writers; the executable guard now requests read access,
  and the test verifies that an actual overwrite fails while the guard is held.
- This verifies identity during read-only inspection, not ancestor ACL trust,
  future configuration transaction atomicity or another client's effective target.
  In-place directory changes are checked at the stated boundaries; no filesystem
  CAS guarantee is made. Production activation remains disabled, and remaining
  checks explicitly retain ancestor permissions and client target qualification.
- Both actual backend versions passed fourteen isolated preflight cases each,
  including home, workspace and executable-parent junctions refused before backend
  launch. The fresh home/workspace remained empty in all three rejection cases.
  Selected configuration bytes and executable fingerprints stayed unchanged in
  accepted cases. No real account, user configuration or desktop UI was used.
- Reports: `integration-tests/compatibility/cli-0.155.1.preflight-targets.json`
  and `app-backend-0.155.0-alpha.9.2.preflight-targets.json`. Reproduce with
  `node scripts/probe-preflight.mjs <reviewed backend> target/release/cxweb.exe`.
- Validation: workspace tests passed 192 tests with four opt-in tests ignored;
  the final preflight tests also passed after adding the immediate pre-launch
  recheck. Workspace Clippy with warnings denied, formatting, JavaScript syntax
  and diff checks passed. Release CLI, daemon and desktop builds succeeded.
- Windows contract: [CreateFile access, sharing and reparse flags](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew).

## Terminal web failures without native retry loops (2026-09-20)

- Actual native tests confirmed that the prior HTTP 422/409 mapping retried a
  refused turn and replaced its useful cause with `E_REQUEST_ALREADY_ADMITTED`.
  Web generation failures now use the reviewed HTTP 400 contract while preserving
  the exact local error code and message. The WebSocket adapter carries that status
  through its existing error frame. Native upstream responses remain byte-preserved.
- Only `E_QUEUE_FULL` and `E_QUEUE_TIMEOUT` return a retryable 503: both originate
  in the scheduler before ledger admission and browser preparation. All other web
  failures handled by this mapper, including unknown failures that may follow
  submission, are terminal.
  Recoverable context exhaustion retains its separate reviewed `response.failed`
  event and automatic compaction behavior. No quota or policy error is fabricated.
- Unqualified/disconnected web routes, cancellation before dispatch, worker panic,
  unsupported owned capabilities and nonportable owned context use the same terminal
  mapping. Native transport outage handling and native service error bodies are
  unchanged. Cleanup uncertainty still prevents a successful disconnect report.
- Expanded the opt-in actual-backend test with a pre-submit model/effort mismatch
  and a submit operation whose outcome is uncertain. Both CLI 0.155.1 and App
  backend 0.155.0-alpha.9.2 passed both cases over HTTP and WebSocket: exactly one
  failed request, its original local error visible to Codex, then one successful
  fresh user turn. There were no automatic repetitions or native upstream frames.
  The uncertain case counts its original attempted submission plus the explicitly
  new turn; it does not claim the first attempted operation never happened.
- Reports: `integration-tests/compatibility/*.terminal-model-*.json` and
  `*.terminal-uncertain-*.json`. Reproduction uses `CXWEB_CONTEXT_PROBE_BACKEND`
  and `cargo test -p cxweb-runtime --lib web_provider::tests::native_context_probe::actual_backend_stops_retrying_terminal_refusals -- --ignored --exact --nocapture`.
  Both native context-recovery runs also passed again and their four reports were
  refreshed. Every native run uses isolated homes, a synthetic browser and no tools.
- Validation: `cargo test --workspace` passed 190 tests with four opt-in tests
  ignored; all actual-backend opt-in tests above were run separately. Workspace
  Clippy with warnings denied, formatting and diff checks passed. Release CLI,
  daemon and desktop builds succeeded. Live browser qualification and actual App
  picker integration remain open; this does not activate production routing.

## Actual native context recovery through the runtime (2026-09-20)

- Added an opt-in integration test that runs each fingerprint-pinned native
  backend through the real gateway, WebSocket history reconstruction, coordinator,
  context guardrails, DPAPI-backed checkpoint key and authenticated expansion.
  Only the browser driver is a fixed synthetic fixture; there is no browser or
  native account access. Each native client uses a new disposable home/workspace.
- CLI 0.155.1 and App backend 0.155.0-alpha.9.2 both passed separately over HTTP
  and WebSocket. In all four cases, two turns completed, the third exceeded the
  local byte ceiling without a browser submission, and the fourth triggered native
  automatic compaction followed by successful continuation. Exactly three normal
  fixture submissions and one summary submission occurred, with no duplicates.
- Every case recorded five requests on the intended transport, one compaction,
  one authenticated checkpoint continuation, restored summary content and zero
  native upstream frames. The WebSocket cases completed their handshake and
  recorded zero HTTP generation requests, so no HTTP fallback occurred. Reports
  are under `integration-tests/compatibility/*.context-runtime-*.json`.
- The fixture uses a 96 KiB ordinary prompt ceiling, a 256 KiB summary reserve
  ceiling and fixed 64 KiB responses to reach the boundary deterministically.
  The same explicit `LocalContextBudget` configures runtime and catalog. Reports
  contain counts and fixed diagnostic fields, never raw task IDs or capability URLs.
  `runtime_verified` additionally checks Rust-side invariants before reporting pass.
- Reproduction: set `CXWEB_CONTEXT_PROBE_BACKEND` to one reviewed absolute backend
  path, then run `cargo test -p cxweb-runtime --lib web_provider::tests::native_context_probe::actual_backend_compacts_through_runtime_http_and_websocket -- --ignored --exact --nocapture`.
  The test invokes `scripts/probe-context-runtime-client.mjs`, verifies the binary
  fingerprint before and after execution and declines native tool approvals.
- An initial invalid fixture omitted the client's requested `medium` effort.
  Fidelity validation correctly refused it before any submission. That run also
  showed CLI retries of a generic HTTP 422/409 rejection; HTTP status selection
  alone does not establish native terminal behavior. The subsequent mapping fix
  and actual-client verification are recorded above. This failed fixture is not
  counted as a pass.
- Validation: both opt-in native runs passed; final `cargo test --workspace`
  passed 189 tests with three opt-in tests ignored. Clippy with warnings denied,
  formatting and diff checks passed. Production code and release binaries are
  unchanged in this step. Authenticated long-history generation, summary quality,
  native account coexistence, actual App picker and full route qualification remain
  open. The existing manual browser verification process is still running; no
  additional browser or ChatGPT desktop application was launched.

## Native context accounting and bounded recovery (2026-09-20)

- Ran six isolated accounting cases against both fingerprint-pinned actual
  backends: CLI 0.155.1 and App backend 0.155.0-alpha.9.2. Without usage, history
  exceeded 100 kB without native automatic compaction. A synthetic usage control
  caused three compactions. An HTTP context error did not mark native context
  full; a Responses `response.failed` event with `context_length_exceeded` did.
  With explicit catalog limits, the latter failed the current turn and caused
  one compaction before the next turn, followed by checkpoint continuation.
  It is not an in-place automatic retry. An additional negative control found
  that omitting catalog limits prevents automatic compaction even when the error
  is recognized. Both clients now pass the positive case with the unmodified
  diagnostic compaction catalog emitted by cxweb.
- Reproducible harness: `scripts/probe-context-budget.mjs`; sanitized reports:
  `integration-tests/compatibility/*.context-accounting-synthetic.json`. All six
  cases passed on each backend. These use disposable homes, loopback fixtures,
  synthetic content and no browser. The actual clients omitted Authorization;
  the fixture records that fact and rejects any unexpected credential. Synthetic
  control token counts and opaque fixture checkpoints are never product values.
- Providers with an explicitly enabled checkpoint codec and local budget reserve
  128 KiB in the diagnostic configuration between the
  ordinary 384 KiB encoded prompt guardrail and the 512 KiB summary ceiling.
  These are initial local engineering limits, not verified web token capacity.
  Every byte of instructions, tool definitions and escaped history counts. Before
  signalling recovery, the complete tool-disabled summary request must fit and
  native-retained messages plus unresolved calls must fit the ordinary limit.
  The latter is only a lower bound: the eventual summary still undergoes the
  complete normal guardrail. No sizing projection is submitted or stored as history.
- The immutable catalog and runtime share one `LocalContextBudget`. Its native
  context metadata is explicitly labelled as a four-bytes-per-token local sizing
  estimate (98,304 estimated tokens for the diagnostic configuration), with a
  90% compaction threshold. No usage/billing data is manufactured. Budgets require
  the checkpoint codec, cannot be changed after publication and must stay within
  the global byte ceiling. Only opt-in compaction diagnostics enable this path;
  default unqualified catalogs still omit capacity metadata.
- A recoverable oversized SSE request returns the reviewed native failed event
  without browser preparation, submission or a fake usage field. JSON callers,
  unqualified providers and inputs that cannot fit a safe summary keep explicit
  local errors. Compaction requests never recurse into recovery. Production
  activation remains disabled; route-specific capacity qualification is still open.
- WebSocket delivery accepts only the exact local empty-output context failure,
  clears its delta-history cache, and keeps the socket available for a fresh
  transcript. A real local-socket regression verifies subsequent web completion
  and byte-preserving native passthrough. Altered failure payloads are refused.
- Validation: the final workspace test run passed 189 tests with two opt-in tests
  ignored, including all 116 runtime tests. Workspace Clippy with warnings denied,
  formatting/diff checks, and release CLI/daemon/desktop builds passed.
  The new schema-size case initially
  hit the independent per-tool schema limit; its corrected fixture now verifies
  the encoded context guardrail without weakening either check. Native accounting
  tests establish HTTP behavior. Subsequent real-runtime WebSocket verification is
  recorded above; authenticated long-history recovery remains unqualified.
  No ChatGPT desktop was launched.

## Context rejection before browser preparation (2026-09-20)

- The coordinator now encodes and checks the complete prompt before scheduler
  admission, durable turn admission or browser preparation. The same prepared
  prompt and nonce are used for the eventual submission; there is no truncation
  or second encoding with potentially different content.
- `E_CONTEXT_BUDGET` is a local rejection returned as HTTP 422 instead of 502.
  Later native experiments showed that a generic 422 can still trigger transport
  retries. The current mapping uses terminal HTTP 400 as recorded above; no
  repeated browser submission is admitted.
  Regression coverage includes oversized history, current instructions, tool
  schemas and compaction input, including inputs that exceed the ceiling only
  after Markdown-safe Unicode escaping. All cases reject without preparing a
  browser page, including a repeated identical request.
- Validation: 111 runtime library tests passed, workspace Clippy with warnings
  denied and formatting/diff checks passed. Release CLI, daemon and desktop
  builds succeeded. The earlier full workspace run passed 182 tests with two ignored;
  this adds one regression case. This fixes the local guardrail path; automatic
  route-specific native context budgets remain a separate qualification task.

## Compaction execution and continuation (2026-09-20)

- Connected the reviewed Responses v2 trigger to a separate tool-disabled browser
  turn. Its checkpoint is a strict JSON summary containing goal, constraints,
  reported changed files, decisions, outstanding work, known test results and
  exactly the unresolved tool IDs measured from canonical history. Function and
  custom calls that remain unresolved are preserved verbatim in the encrypted
  payload. Unknown/duplicate/mismatched results and invalid summaries fail closed.
- Sealing occurs before durable completion and replay caching. Retried requests
  return the same encrypted response without another browser submission. A failed
  summary or key operation is terminal, releases the browser lease and cannot
  manufacture a successful compaction. Native wire output contains exactly one
  local `wbr1:` compaction item; no fabricated token usage is added.
- Checkpoints are authenticated against installation, stable native task identity,
  account, workspace, route, epoch and client/schema codec. Changing the native
  context-window ID does not change the task binding. Current instructions and
  native-retained messages remain intact. Expansion restores the historical
  summary and pending calls; a subsequent result resolves those calls before the
  next compaction. Foreign/tampered tokens fail before browser preparation.
- WebSocket compaction clears the connection's previous-response delta cache so
  the following native context starts from a complete transcript. HTTP/WS native
  model-switch guards still refuse owned checkpoints before upstream forwarding.
  Codec enablement is explicit; legacy compact and unqualified providers remain
  refused. Production activation has not been enabled.
- Actual CLI 0.155.1 passed one authenticated HTTP read/compact/continue example:
  a random marker introduced only by a native file-read result survived the
  encrypted checkpoint. Evidence: `cli-0.155.1.compaction-v2-live.json`. This run
  predates the additional prompt encoding example and stronger continuation-shape
  assertions; it is not broad reliability qualification.
- App backend attempts remain incomplete and are recorded, not counted as passes:
  one summary failed its inner schema; a later WebSocket attempt completed
  compaction but continuation stopped on an incomplete account settings panel;
  subsequent startup attempts encountered browser verification. See
  `app-backend-0.155.0-alpha.9.2.compaction-live-incomplete.json`.
- Added content-free summary-shape diagnostics, an explicit encoding example,
  bounded 15-second settings hydration observation, and passive startup waiting
  that distinguishes browser verification from login expiry. No challenge is
  clicked automatically. The ordinary cxweb Chrome profile is currently open for
  the already-requested user verification; no ChatGPT desktop was launched.
  Computer Use stopped its own inspection because it could not verify the URL.
- Verification: the final `cargo test --workspace` passed 182 tests with two
  opt-in tests ignored. Clippy with warnings denied, formatting/diff checks and
  release CLI/daemon/desktop builds passed. JavaScript DOM/approval tests passed
  33 cases. Pending: completed App continuation, final-prompt live requalification,
  automatic context-budget integration and the full long-conversation corpus.

## Checkpoint encryption and current-user key protection (2026-09-19)

- Added a bounded `wbr1:` checkpoint codec using AES-256-GCM, a fresh random
  96-bit nonce and authenticated installation/task/account/workspace/route/epoch/
  codec context. Invalid prefixes, malformed encodings, altered ciphertext,
  different keys and any changed scope dimension are refused before plaintext
  is returned. The plaintext limit is 2 MiB; no compression is involved.
- Persistent 256-bit keys are wrapped with current-user Windows DPAPI and stored
  through private atomic file creation. DPAPI is non-interactive and never uses
  machine-wide protection or a plaintext fallback. The installation identity is
  included in key-wrapping context. Existing key files are checked for identity
  and permission changes; unreadable, corrupt or mismatched state is never
  regenerated or repaired. Raw key and DPAPI output buffers use zeroization.
- Added `aes-gcm` 0.10.3, with ten newly locked packages in total, plus direct
  use of the existing `zeroize` dependency. Unrelated locked versions are unchanged.
  This is a checkpoint storage primitive: the model-generated checkpoint schema,
  coordinator purpose, native v2 output and restored-history expansion still need
  to be connected and qualified. Production compaction remains explicitly refused.
- Validation: actual local DPAPI wrapping/unwrapping passed; tests cover reopening
  the stored key, tampering, wrong binding/key, size bounds and preservation of
  corrupted key state. `cargo test --workspace` passed 177 tests with two opt-in
  tests ignored. After adding the final key-file recheck, both checkpoint tests
  and workspace Clippy with warnings denied passed again. Formatting and diff
  checks passed. Release CLI, daemon and desktop builds all succeeded. No account
  credentials or native authentication files were read.
- References: [Windows current-user DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
  and [reviewed RustCrypto AES-GCM implementation](https://docs.rs/aes-gcm/0.10.3/aes_gcm/).

## Actual compaction transport and context boundaries (2026-09-19)

- Both reviewed native backends use remote compaction v2 by default: the ordinary
  Responses request ends with `compaction_trigger`, and successful SSE must
  contain exactly one `compaction` output item. An initial diagnostic returning
  ordinary assistant text failed in both builds. The legacy `/responses/compact`
  plaintext-output hypothesis is not sufficient for these default clients.
- The extended isolated harness invokes `thread/compact/start`, waits for native
  completion, then starts another turn. Both native builds accepted a synthetic
  opaque item and sent its exact bytes in the following request. This proves the
  v2 transport contract, not model summarization, encryption, GUI picker behavior
  or long-running coding acceptance. The fixture has an explicit synthetic-only
  prefix; it is not represented as an encrypted production checkpoint.
- Evidence: `cli-0.155.1.compaction-v2-synthetic.json` and
  `app-backend-0.155.0-alpha.9.2.compaction-v2-synthetic.json`. Reproduce with
  `node scripts/probe-client.mjs <reviewed backend> --compact`. No browser,
  desktop window or real native credentials were used.
- The production request adapter now identifies unqualified v2 compaction
  explicitly and returns a terminal 422 error before browser submission, matching
  the legacy compaction refusal. Encrypted, scope-bound checkpoint generation and
  expansion remain to implement; no native feature flag was disabled for the test.
- Fixed a model-switch boundary gap: HTTP Responses/compact and Responses WebSocket
  requests cannot forward owned response references or `wbr1:` compaction data
  to native inference. This also applies after web disconnect and to compressed
  HTTP input. Ordinary text mentioning these prefixes, portable tool results and
  native opaque references retain their existing behavior.
- Validation: all 105 runtime tests passed, the new adapter compaction refusal
  regression passed, and both actual backend transport probes passed. Clippy with
  warnings denied, formatting, JavaScript syntax and diff checks passed.
- Reviewed source: [v2 request construction](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/core/src/compact_remote_v2_attempt.rs),
  [v2 output/history handling](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/core/src/compact_remote_v2.rs).

## Windows configuration access validation (2026-09-19)

- Configuration snapshots now validate the selected file and immediate parent
  through live Windows handles. Null DACLs, unsupported ACE forms, foreign grants,
  untrusted ownership, read-only files and insufficient effective token access
  fail before staging. Current-user, SYSTEM and Administrators ownership is
  accepted only with the required effective rights; no privileges are enabled.
- Access snapshots are rechecked before staging, replacement and removal.
  A changed trusted ACL also conflicts. Replacement preserves the original owner
  and DACL; newly created staging files have protected private ACLs. No selected
  configuration's permissions are repaired or widened to make installation work.
- Windows can normalize file ACE child-propagation flags and DACL auto-inherited
  bookkeeping during ReplaceFile. Post-commit comparison accounts for those two
  cases while preserving effective rights, inherit-only behavior, protection,
  owner, ACE order/type/mask and trustees. Pre-write comparisons remain exact.
  New files omit meaningless directory inheritance flags; old private files are
  covered by replacement regression tests.
- Native preflight reports selected-file/immediate-parent access verification,
  and permission refusal has a fixed diagnostic code. It does not certify all
  ancestor paths, other clients' effective configuration or activation. The
  known external rename race in path-based replacement remains documented.
- Both actual native backends passed eleven isolated preflight cases each with
  the release CLI, including refusal of an exposed home before backend launch.
  The exposed fixture remained empty. Existing config bytes remained unchanged.
  Fixture ACL setup is limited to newly created empty probe directories; no real
  user configuration, native credentials or system policy was modified.
- Evidence: `cli-0.155.1.preflight-access.json` and
  `app-backend-0.155.0-alpha.9.2.preflight-access.json` in
  `integration-tests/compatibility`. Reproduce with `node scripts/probe-preflight.mjs
  <reviewed backend> target/release/cxweb.exe` after building the release CLI.
- Validation: `cargo test --workspace` passed 170 tests with two opt-in tests
  ignored. `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`, JavaScript syntax and diff checks passed.
  `cargo build --release -p cxweb` succeeded. No desktop application was launched.
- Windows contracts: [handle security inspection](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo),
  [effective access evaluation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-accesscheck),
  [replacement behavior](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew),
  [ACE inheritance semantics](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-ace_header).

## Native configuration preflight (2026-09-19)

- Added `cxweb native-preflight --client <absolute executable> --home <absolute
  selected home> --cwd <absolute project directory>`. It queries the reviewed
  backend's `config/read`, `configRequirements/read` and `account/read` APIs
  (`refreshToken:false`), rather than duplicating native configuration precedence
  or opening native credential files. No model, login, config mutation or tool
  operation is requested. The child is hidden and owned by this inspection.
- Only the two reviewed executable SHA-256 fingerprints may start. The selected
  user configuration's canonical file identity and original bytes must agree
  with the native response and remain unchanged afterward. The executable is
  fingerprinted again after inspection. Unknown binaries fail before execution.
  Shutdown affects only the child; RPC deadlines, frame size and message count
  are bounded. Server-originated actions are denied, and arbitrary native errors
  are replaced with fixed error codes.
- Results contain sanitized auth mode, fixed layer categories and conflict codes,
  not account metadata, URLs, instructions, configuration contents or native
  diagnostics. Existing route/catalog declarations, different providers, selected
  profiles, unknown layers, relevant managed constraints and environment routing/
  auth overrides block compatibility. Disabled project layers retain native
  behavior. Unrelated native approval/sandbox requirements are not modified.
- Native `config/read` includes serialized defaults without provenance. Tests
  caught an initial false conflict from its default ChatGPT URL and empty profiles
  table; only provenance-backed declarations now count as configured values.
  Actual API-key fixture login and a bare OPENAI_API_KEY environment variable also
  differ in `account/read`; environment overrides are therefore checked separately.
- Both installed native backends passed ten isolated cases each: signed out,
  synthetic API-key login, environment auth, user route, environment route,
  custom provider, deprecated home policy file, static catalog, disabled project,
  and an unreviewed executable. Input config/catalog/policy bytes were unchanged;
  no user config was created when absent. The synthetic key was installed only by
  the native login command inside disposable fixture homes. No real native account,
  system policy, desktop UI or browser was used.
- Windows source and both actual builds confirm CODEX_HOME/managed_config.toml is
  ignored. Earlier documentation describing it as active is stale for these builds.
  Tests now explicitly verify that behavior. Managed routing rejection is covered
  by unit RPC fixtures; no actual system/cloud policy was changed for testing.
- Evidence: `cli-0.155.1.preflight.json` and
  `app-backend-0.155.0-alpha.9.2.preflight.json` under
  `integration-tests/compatibility`. `scripts/probe-preflight.mjs` reproduces the
  isolated checks. The five new assessment/RPC tests passed, alongside two existing
  journal preflight regressions. Clippy with warnings denied and debug CLI build
  passed. The final provenance scan was narrowed to fixed routing keys to avoid
  quadratic work on unrelated configuration; its regression tests and Clippy passed.
- Release CLI, daemon and desktop builds succeeded. A release preflight smoke
  test against the actual CLI and the signed-out fixture passed in 1,538 ms,
  including both executable hashes and configuration identity checks. This is one
  local timing observation, not a general performance guarantee. Evidence:
  `cli-0.155.1.preflight-release.json`.
- This report covers exactly the selected backend/home/cwd/inherited environment,
  without CLI overrides or an explicit selected profile. It does not discover all
  existing terminals or certify another GUI process's environment. Native startup
  may maintain its own logs/cache. Filesystem ownership/permission qualification,
  actual App picker, native subscription coexistence and browser/coding gates still
  prevent activation; a clean config assessment cannot enable production routing.
- Reviewed official sources:
  [configuration RPC contract](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/app-server-protocol/src/protocol/v2/config.rs),
  [native configuration loader](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/config/src/loader/mod.rs),
  [Windows legacy policy behavior](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/config/src/loader/layer_io.rs).

## Client-specific catalogs and desktop target correction (2026-09-19)

- Added independent catalog encoders for CLI 0.155.1 and App backend
  0.155.0-alpha.9.2. Rows are built from observed English labels, route IDs,
  explicit effort and separately qualified tool support. They do not clone native
  instructions, context limits, image support, service tiers or compaction data.
- Dynamic catalog selection requires one `client_version` query value and one
  matching full-build User-Agent. Native queries omit prerelease suffixes, so the
  full build is necessary to distinguish reviewed alpha builds. Duplicate,
  malformed, missing and unknown selectors leave the native catalog unchanged.
  Only the selected codec enum crosses into the web provider; transport headers
  and native authorization do not.
- Coordinator providers now accept an explicit immutable catalog snapshot. It is
  bound to installation/account/workspace/epoch and generation, checks executable
  route membership, rejects duplicate/contradictory entries, and publishes only
  the client codecs explicitly supplied by the activation owner. Construction
  does not itself qualify a route. Production activation remains disabled.
- The isolated client harnesses use the reviewed encoder selected from the actual
  executable's version, including for live tests. Both actual native backends
  accepted their text catalog, selected the owned model and completed synthetic
  requests. Both also passed the authenticated WebSocket read/apply_patch/final
  cycle with their coding catalogs: each recorded three web requests, two
  upgrades, zero native inference frames, zero provider failures and confirmed
  browser shutdown.
- Evidence: `cli-0.155.1.catalog-synthetic.json`,
  `app-backend-0.155.0-alpha.9.2.catalog-synthetic.json` and
  `cli-0.155.1.catalog-live-tools.json` plus
  `app-backend-0.155.0-alpha.9.2.catalog-live-tools.json` in
  `integration-tests/compatibility`.
  These are static isolated catalogs; they do not establish authenticated native
  subscription coexistence or the graphical App picker.
- Validation: 25 adapter and 99 runtime tests passed before the final provider
  publication regression was added. All nine catalog-related runtime tests then
  passed, including that regression and contradictory cross-client route
  rejection. Clippy with warnings denied, formatting, JavaScript syntax, debug
  CLI build and diff checks passed.
- A separate desktop experiment launched the installed package's `ChatGPT.exe`
  in an isolated home/UI directory. The user rejected this target as ChatGPT
  desktop. The test-owned instance was closed; the original window was preserved.
  The experiment did not verify picker selection or a round trip and is not
  counted as a Codex App result. Windows currently registers the installed
  `OpenAI.Codex` package's main application as `ChatGPT.exe` with a ChatGPT shortcut;
  this registration alone does not satisfy the requested App UI qualification.
  No further such desktop instance is launched by the backend tests.
- Follow-up read-only inspection of package 26.915.4065.0 found both executables
  signed by OpenAI. `owl-app.ini` identifies the Codex user-data directory and UI
  build 26.915.31945, while the manifest still targets `app/ChatGPT.exe`.
  The small `app/Codex.exe` contains the Windows updater trampoline build marker;
  its basename is not evidence of a separate supported UI launch path. No binary
  or bundle was changed or launched during this inspection. The next graphical
  qualification must establish the actual Codex task surface and an isolated
  supported launch path before input; backend success does not close that gate.
- Source review: the official pinned
  [model metadata contract](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/protocol/src/openai_models.rs),
  [catalog query version](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/models-manager/src/lib.rs)
  and [full-build User-Agent](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/login/src/auth/default_client.rs).

## Owned Responses WebSocket routing (2026-09-19)

- Added owned Responses dispatch to the existing native WebSocket relay. Native
  upstream negotiation, handshake status/headers and native message bytes remain
  preserved. Every owned frame goes through the same admission, durable ledger,
  coordinator and browser validation as HTTP; no owned frame reaches native
  inference. Realtime remains separate. Native account quota events can pass
  during owned work; native response events cannot become an owned response.
- A `generate:false` warmup validates identity, schema and the qualified route
  and returns an empty local completion without calling the browser. Per-frame
  session/thread correlation must match the handshake; turn/context identity
  comes from reviewed frame metadata, not the first handshake's turn metadata.
  Native authorization and other headers never enter the web provider.
- Connection-local continuation accepts only the latest completed local response
  with matching session/context/model/options. It rebuilds full history from the
  previous request, locally validated output and exact new input. Foreign IDs,
  changed scope/options, oversized history and cross-backend continuation are
  rejected. A native request clears the local continuation cache.
- Only our own output is projected into the native ResponseItem representation:
  message/function status and output-text annotations are wire-only fields;
  custom-call status, item/call IDs, namespace and literal arguments remain.
  Incoming history is not rewritten. A coordinator regression test confirms a
  reconstructed delta and the equivalent full HTTP retry share the same durable
  result and do not submit another browser message.
- Client close, overlapping create and unexpected native response events cancel
  the owned delivery. The admitted worker retains its drain lease through cleanup.
  Errors include native-compatible HTTP status in the WebSocket error envelope.
  Output remains buffered validated delivery, not live browser token streaming.
- Actual CLI 0.155.1 and App backend 0.155.0-alpha.9.2 both passed native
  `exec_command` read, exact `apply_patch` creation and final response through the
  authenticated browser with WebSocket enabled. Each completed three browser
  generations, with zero provider failures and confirmed browser shutdown.
  The App backend's per-request counters explicitly recorded all three requests
  over WebSocket, two upgrades, zero warmups and zero native inference frames.
- Actual CLI UI also passed its English picker, two visible exact fixed replies
  and two strict structured auxiliary responses. Four browser generations
  completed, six sockets upgraded, no native inference frames escaped, and both
  processes exited cleanly. This run preceded per-request transport counters;
  it is not claimed as a measured continuation-frame count.
- Reports: `cli-0.155.1.websocket-tools.json`,
  `app-backend-0.155.0-alpha.9.2.websocket-tools.json` and
  `cli-0.155.1.tui-websocket-live.json` under `integration-tests/compatibility`.
  Live probes use a disposable native home, synthetic loopback-only native key,
  static diagnostic catalog and local native rejection peer. They do not certify
  real subscription/native coexistence or the actual desktop App picker.
- Source for output projection and per-frame metadata behavior: the reviewed
  [native ResponseItem definitions](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/protocol/src/models.rs),
  plus the actual native wire observation recorded below. No competitor code
  was copied. Production activation and broader PRD acceptance remain open.
- Validation: all 96 runtime tests passed; runtime/CLI Clippy with warnings denied,
  formatting, JavaScript syntax and diff checks passed. The new tests cover real
  loopback mixed routing, warmup, continuation, identity/scope rejection, socket
  close, overlap, cleanup drain, native quota events and durable HTTP retry.
  Release CLI, daemon and desktop builds succeeded.

## Native WebSocket wire observation (2026-09-19)

- `probe --websocket-output` and `probe-tui.mjs --websocket` add an explicitly
  synthetic WebSocket diagnostic. No browser or native upstream is connected.
  The capture contains counts and correlation booleans only, without prompts,
  response content, identifiers or authorization. Production routing is unchanged.
- Unmodified CLI 0.155.1 passed an actual picker selection and two visible fixed
  response turns over WebSocket. Five frames were observed: initial empty-input
  warmup (`generate=false`), a three-item request referencing the warmup response,
  a separate auxiliary warmup, a full structured title request, and a one-item
  continuation referencing the prior main response. The native client also
  displayed the synthetic generated title in its resume instruction.
- Every observed frame carried reviewed `client_metadata`, matching handshake
  session/thread fields and consistent turn metadata including context-window
  identity. `stream` was present. This confirms the installed client's wire
  behavior; it does not certify browser WebSockets or native coexistence.
  Report: `cli-0.155.1.tui-websocket-synthetic.json`.
- All 87 runtime tests passed, including real loopback WebSocket warmup/continuation,
  content/auth exclusion and structural capture redaction. Runtime/CLI Clippy
  with warnings denied, debug build, script syntax and diff checks passed.
- Next: implement per-frame owned routing, validated connection-local history
  reconstruction, warmup without browser submission, cancellation/draining and
  mixed native forwarding, then test the actual App/CLI builds through that path.

## Structured output and auxiliary CLI requests (2026-09-19)

- Structural diagnostics confirmed that the six rejected auxiliary TUI requests
  exactly match the public native thread-title schema with `strict=true`.
  [Native title generation](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/app/thread_title.rs)
  runs a hidden temporary thread using the current model when the native title
  model is absent. This is part of the unmodified CLI behavior, not a cxweb task.
- The adapter now preserves `text.format` JSON Schema definitions, compiles them
  without external retrieval, includes them in the browser request, and validates
  the complete final text after transport-envelope validation. Invalid JSON,
  duplicate keys and schema violations are terminal, never repaired or resent.
  Intermediate validated client tool calls retain their existing contract.
  Unknown formats/options and external schema references fail before submission.
- Accepting the auxiliary request exposed overlapping preparation/observation in
  the serialized browser owner. The two-generation TUI run failed with browser
  observation/submission uncertainty; no retries sent duplicate browser work.
  The current default is one active browser generation and an eight-entry FIFO
  queue. Native clients can submit concurrent tasks; their browser work waits its
  turn. Parallel browser operation is not qualified or advertised.
- With sequential browser work, the main TUI answer completed but the first
  structured title failed the strict transport envelope. Prompt instructions now
  explicitly keep client schema properties inside the JSON-encoded final text.
  Diagnostic output records only fixed structural booleans/counts, never model
  content, arbitrary schema names or native correlation identifiers. The failed
  runs remain recorded as `*.tui-concurrent-incomplete.json` and
  `*.tui-envelope-incomplete.json`; they are not relabeled as successful.
- Browser answer selection now requires a unique assistant content block outside
  marked intermediate sections. Fenced-output validation waits for completion;
  incomplete rendering is not a final protocol reply. Three DOM-selection tests,
  10 browser-adapter Rust tests (one installed-browser test excluded), and the
  actual Chrome 153.0.8010.48 offscreen fixture passed. The fixture still reports
  initial taskbar exposure for its offscreen native window; it is not proof of
  a completely invisible production lifecycle. The final live title had one
  answer block, no intermediate blocks and no fence, so its earlier failure
  cannot be attributed to commentary selection alone.
- Unicode quote escaping in the structured text prompt resolved the live title
  scenario. The final structural observation confirms a valid four-field outer
  envelope, a string-valued inner answer and Unicode quote escapes. Both actual
  TUI requests completed in the durable ledger with no runtime failures. The
  terminal visibly showed the exact fixed main answer; the auxiliary answer
  passed the native JSON Schema locally. No claim is made about a persisted UI
  task name. The client and runtime exited with code 0 and browser closure was
  confirmed. Report: `cli-0.155.1.tui-live.json`.
- Verification: 27 adapter tests and 85 runtime tests passed, including
  schema rejection/no-resend, exact schema preservation, sequential auxiliary
  admission, and diagnostic content exclusion. Affected-crate Clippy with
  warnings denied and current release CLI/daemon/desktop builds passed. The
  actual TUI test uses a disposable static catalog and local native rejection
  stub; production mixed native/WebSocket/App qualification remains open.

## Optional hosted search and actual CLI UI (2026-09-19)

- Default native clients attach a hosted `web_search` declaration even to ordinary
  text/coding requests. The earlier isolated `web_search="disabled"` workaround
  has been removed from the probe. The adapter preserves the reviewed optional
  declaration in `unavailable_server_tools`, explains the route limitation to the
  model, and returns `x-cxweb-unavailable-tools: web_search`. It cannot emit a
  fabricated search call/result. Forced hosted search and ambiguous required-tool
  requests fail before browser submission. Unknown tool types, extra hosted-search
  options, duplicate declarations and malformed values continue to fail.
- This is an explicit V1 text/coding scope decision, not implemented hosted search.
  The desktop details disclose it. No native/global search configuration is
  changed; native model traffic remains byte-preserving. Reviewed source:
  [hosted tool construction](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/core/src/tools/hosted_spec.rs)
  and [native tool specification](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tools/src/tool_spec.rs).
- CLI 0.155.1 and App backend 0.155.0-alpha.9.2 each passed the complete native
  read/apply_patch/final-answer cycle with their default hosted-search declaration
  present in all three requests. The App backend also passed a separate request
  for hosted search: it returned the exact capability limitation rather than
  claiming a search result. Reports: `*.default-search-tools.json` and
  `app-backend-0.155.0-alpha.9.2.search-limit.json` under compatibility evidence.
- An earlier default-search coding run completed both native tools but withheld
  the final response because the account menu gained the public subtitle
  `Personal account`. Qualification now accepts exactly the observed personal
  subtitle variant with the same account/Settings corroboration and workspace
  exclusion. Organization markers, different layouts and mismatched identities
  still fail. Later fresh CLI/App runs passed; the failed run is not relabeled.
- `scripts/probe-tui.mjs` exercises the unmodified CLI through a PTY in a disposable
  home. The actual `/model` menu displayed `webbridge/diagnostic` with the explicit
  ChatGPT Web description; confirmation selected `medium`, and the terminal
  displayed the exact synthetic assistant response. The gateway independently
  confirmed the owned route and native request identity. The synthetic API key was
  installed only in this disposable home through the supported native login CLI;
  no real native authentication/configuration was read or changed. The terminal's
  optional sandbox setup was dismissed with its normal Back action; no sandbox
  configuration was changed and this UI scenario executed no tools.
- CLI 0.155.1 uses provider-level WebSocket negotiation; the obsolete model field
  `prefer_websockets` was removed. A 404 diagnostic handshake caused retries before
  HTTP fallback. The synthetic server and isolated native rejection stub now
  return the client's reviewed 426 fallback status, and the repeated TUI scenario
  completed immediately without those retry errors. This is not production mixed
  WebSocket certification: native passthrough must retain its native transport.
  [Native fallback source](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/core/src/client.rs).
- Verification: 23 adapter tests, 81 runtime tests, the account-scope regression,
  14 desktop/test-approval JavaScript tests, affected-crate Clippy with warnings
  denied, formatting, script syntax and diff checks passed. Current release builds
  of the CLI, daemon and desktop passed.
- The actual CLI picker also displayed the live owned route and selected `xhigh`;
  its terminal and isolated transcript both contained the exact authenticated
  browser response. Normal `/quit` exited with code 0, the runtime exited with
  code 0, and browser closure was confirmed. The complete scenario still FAILED:
  six auxiliary requests were rejected with `E_UNSUPPORTED_OUTPUT_FORMAT`.
  This is a request-format gap, not a cleanup failure. Native TUI source contains
  hidden structured title generation using the current model when the native
  title model is absent. The exact live request shape remains to be qualified.
  Report: `cli-0.155.1.tui-live-incomplete.json`. No rejected request was sent to
  ChatGPT; the successful visible response does not certify the whole client.
- Still open: actual App UI qualification, native subscription coexistence,
  production WebSocket handling for owned requests, dynamic catalog/activation,
  context compaction, namespaced dynamic-tool qualification and remaining gates.

## Native function and custom tool execution (2026-09-19)

- Actual CLI **0.155.1** and App backend **0.155.0-alpha.9.2** each passed a live
  native read and a full read/apply_patch/final-answer cycle through the real
  browser gateway, on **Latest / Extra High**. CLI updated from 0.153.4 during
  development; the successful tool reports belong to 0.155.1. The harness now
  fingerprints each executable immediately before launching it and checks that
  the file is unchanged afterward. It does not interfere with client updates.
- The harness creates a random marker in an isolated input file without putting
  the marker in the prompt. Codex executes `exec_command`, receives its own tool
  result, then executes a custom `apply_patch` to create a second file. Assertions
  require one successful native command, one successful native file-change item,
  exact output-file bytes and the exact final assistant text. The durable browser
  ledger contains three completed generations per full test and no failed turns.
  The product and harness do not perform the model's shell/file operations.
- Test-client approval handling accepts only its fixed read command in its exact
  fixture directory, and the one new output file with the exact expected content.
  Patch approval is bound to the native started-item/thread/turn identity. It does
  not grant a directory, change sandbox/approval policies or remember a session
  allowance. Early attempts correctly returned a denied tool result to ChatGPT
  when the harness declined the command; those failed execution tests remain
  separate from the later successful runs.
- The positive patch test first stopped before browser submission with
  `E_UNSUPPORTED_TOOL`. A synthetic native registry capture isolated an otherwise
  identical apply_patch grammar using CRLF instead of LF in the Windows asset.
  The adapter now accepts both exact reviewed hashes. Unknown grammar changes and
  malformed patch payloads still fail; it does not execute patches itself. The
  regression verifies both asset encodings, literal payload preservation, refusal
  of changed grammar rules and refusal of trailing unframed content.
- Sanitized `*.live-read.json` and `*.live-tools.json` reports contain versions,
  public model labels, counts/booleans and binary hashes, not raw tool payloads,
  account identifiers or the random marker. All runs used the saved English
  off-screen browser session and confirmed browser/profile cleanup. Native API
  traffic stayed on the local rejection stub with a synthetic key.
- Commands passed: `node scripts/probe-client.mjs <client> --live-read` and
  `--live-patch` for each backend; `--capture-tools` for CLI 0.155.1 (synthetic);
  `cargo test -p cxweb-codex-adapter` (21 tests); affected-crate Clippy with warnings
  denied; formatting/diff checks; three test-client approval regressions (also
  added to CI); and release CLI/daemon/desktop builds. A Clippy suggestion in the
  new regression was corrected before the successful rerun.
- Sources: [native approval command projection](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/app-server/src/bespoke_event_handling.rs),
  [quoted argv encoding](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/shell-command/src/parse_command.rs),
  [file-change approval content](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/app-server-protocol/src/protocol/item_builders.rs),
  and the [pinned apply_patch grammar](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/core/assets/tools/apply_patch.lark).
- This is a narrow authenticated coding-path result, not complete G2/release
  certification. Built-in server search remains disabled only in the disposable
  client; actual pickers, namespaced dynamic tool execution, broader coding and
  cancellation/error behavior, production activation and remaining gates are open.

## Authenticated native client text integration (2026-09-19)

- The isolated `live-probe` connects the real managed browser driver, durable
  coordinator, HTTP provider and gateway to the installed native app-server.
  It uses the existing authenticated cxweb profile and a disposable CODEX_HOME.
  Native traffic terminates at a local rejection stub; its synthetic key cannot
  reach an external native upstream. The capability descriptor is protected by
  the current-user directory ACL. No real Codex configuration/auth is changed.
- Actual CLI **0.153.4** and current App backend **0.155.0-alpha.9.2** both listed
  and selected `webbridge/live-probe` and returned the exact requested text from
  ChatGPT, on the observed **Latest / Extra High** route. These are authenticated
  backend tests, explicitly not actual graphical/terminal picker certification.
  Sanitized reports are in `integration-tests/compatibility/*.live-text.json`.
- Both runs used normal off-screen Chrome 153.0.8010.48, required English browser
  and page language, rechecked account-default scope before Send and completion,
  and confirmed complete browser exit/profile-lock release. The owned window's
  initial taskbar exposure remains unqualified; the final login-close UX is open.
- Live testing found that the account submenu can mount before its rows hydrate.
  The observer now waits for populated controls/selected evidence. A delayed-row
  Chrome fixture reproduces this race and passes without relaxing scope checks.
- A second live failure was exact message attribution: ChatGPT rendered 22 inline
  code spans, removing 44 backticks from the DOM text. Client JSON now uses valid
  Unicode escapes for Markdown delimiters inside strings only. Decoding preserves
  instructions, history and tool definitions, including literal backslash escapes
  and Unicode; budgeting applies to the expanded transport text. No prefix match,
  inferred text restoration or weaker attribution was introduced. The successful
  CLI/App messages matched all 22,437/25,997 characters respectively.
- Scope inspection now runs at preparation, submission and buffered completion,
  rather than opening Settings on every generation poll. Completion failure
  withholds output and replay data. Preparation preserves the original static
  error code; terminal validation/admission errors receive explicit HTTP statuses.
  Native clients can still retry some failures, but the ledger refuses another
  browser send. Earlier failed live attempts are not presented as successes.
- The disposable text-test client explicitly sets `web_search="disabled"`, since
  its built-in server-search definition is not yet supported by the web adapter.
  This is a narrow text qualification, not silent removal of incoming definitions
  or production search compatibility. See the official [web search configuration](https://learn.chatgpt.com/docs/config-file/config-basic#web-search-mode).
- Verification: `cargo test --workspace`, `cargo clippy --workspace --all-targets
  -- -D warnings`, all 38 browser/desktop JavaScript tests, and both authenticated
  `node scripts/probe-client.mjs <client> --live` runs passed. The real Chrome
  synthetic off-screen fixture passed hydration, selection, attribution, Stop and
  target cleanup. Two environment-dependent Rust tests remain opt-in/ignored.
  Formatting/diff checks and release CLI/daemon/desktop builds also passed.
- Next: native function/custom tool execution and result return, actual pickers,
  production activation/lifecycle and remaining PRD acceptance/release gates.

## English browser locale and menu interaction (2026-09-19)

- Managed and manual-comparison browser launches now request English UI and
  Accept-Language. The existing authenticated profile reported browser and page
  language `en-US`, and its actual picker reported `Thinking effort`. No account
  credentials, personal browser settings or Codex authentication were modified.
- Discovery labels now come from the same composer control as selection. Menu
  input checks the hit target, sends a full pointer button sequence, and waits
  for visible menu closure. A Chrome fixture with delayed closure exposed the
  previous race and now passes. Hidden CSS menus no longer block completion.
- Responsive profile entry points are checked for actionability. Scope discovery
  opens the actual account menu and exports bounded structural diagnostics only.
  The live menu contains no unique email or selected workspace ID, so account and
  workspace identity remain unqualified; routing remains disabled.
- All workspace tests, 13 browser DOM unit tests, Clippy for affected crates and
  format/diff checks passed. The opt-in real Chrome network test passed, including
  English Accept-Language. Its HTTP fixture ignores unused preconnected sockets.
- User requirement: only authentication/verification may require a visible
  browser window. Closing that window must preserve background operation. This
  behavior is not yet certified. Investigating Chromium's documented hidden
  targets without account rotation, copied credentials or private web APIs.
- References: [Chromium language switch](https://chromium.googlesource.com/chromium/src/+/70c2ca2727df7b8cdfca66990e671c9df7ae0afc/chrome/common/chrome_switches.cc),
  [Playwright Chromium input events](https://github.com/microsoft/playwright/blob/main/packages/playwright-core/src/server/chromium/crInput.ts),
  [Chromium hidden target contract](https://github.com/ChromeDevTools/devtools-protocol/blob/master/pdl/domains/Target.pdl).

## Background browser lifecycle (2026-09-19)

- Closing the login window is detected by exact target identity. The runtime
  waits for target destruction and graceful browser exit before reusing its
  dedicated profile. A private `browser-control background` diagnostic can also
  start the saved profile without opening a visible window. No browser profile
  or credential material is copied, and no native Codex settings are changed.
- Background execution now keeps ordinary Chrome rendering in an owned native
  window positioned outside the virtual desktop. Only windows belonging to the
  retained browser process handle/PID and the expected Chromium window class
  can be repositioned. Their taskbar/activation styles are suppressed. Existing
  parked windows are reused without repeated hide/show cycles. Authentication
  windows retain normal visible behavior and the browser identity is unchanged.
- The real saved session loaded after a cold background start, reported `en-US`
  for both browser/page, discovered five effort choices and verified Temporary
  Chat. One text test and one function/custom tool-envelope test then **PASSED**
  without a visible work window. Both attributed the exact submitted message and
  completed response with matching `Extra High` labels. These results establish
  background transport, not actual Codex tool execution or picker integration.
- Native hidden CDP targets had stalled CSS exit animations. Headless mode was
  also tested but required additional provider verification and is not used for
  saved-session background work. The independent off-screen implementation was
  informed by the reference application's render-surface lifetime, with no code
  copied. The fixture now requires a CSS animation to finish; it would reject
  the earlier stalled renderer. Normal Chromium sandbox/security remain enabled.
- Startup qualification remains incomplete: the off-screen fixture found no
  initial desktop exposure, but the initial window still had taskbar-capable
  styles before placement. A transient shell entry is not yet ruled out. Display
  changes and actual login-window closure need full regression coverage before
  claiming the final no-window UX. The diagnostic reports this limitation.
- The panel distinguishes background loading, expired sign-in and verification
  challenges, and does not retry challenges or qualify a retained composer while
  verification is required. No challenge automation is implemented.
- Scope observation now requires an expanded account entry point, so an unrelated
  model menu cannot accidentally be read as the account menu. Account/workspace
  identity, actual App/CLI integration and production activation remain open.
- All 119 workspace tests passed (two opt-in tests ignored); all 25 JavaScript
  tests passed. Workspace Clippy with warnings denied passed. The real Chrome
  synthetic background fixture and the opt-in local HTTP/English-language test
  passed. These local fixtures alone do not establish release readiness.
- After adding native off-screen placement, all 97 tests in platform, browser
  adapter and runtime passed (one opt-in ignored), and workspace Clippy passed.
  The Chrome fixture passed with CSS animation completion, menu/model selection,
  exact prompt attribution, Stop and complete owned-target cleanup.
- Windows references: [window visibility](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow),
  [extended window styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles),
  and [window positioning](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos).

## Account settings observation (2026-09-19)

- The account menu's Settings action now provides a fallback when the menu itself
  contains no account email. The adapter opens only the expanded profile menu's
  Settings item and selects the Account tab with ordinary pointer input. It reads
  only that selected tab's associated panel, waits for asynchronously loaded
  content and refuses missing/ambiguous account evidence. It changes no settings.
- The live background session exposed exactly one email after Account finished
  loading. Diagnostics contain only counts, known public field/tab labels and
  state booleans; the email remains an in-memory value, not serialized output.
- Menu/dialog cleanup is attempted even on failed reading/navigation, and dialog
  closure is verified. Foreign panels, unselected tabs, multiple dialogs and
  multiple emails cannot establish an account. The real Chrome fixture now
  requires a delayed settings-panel email instead of a direct menu email.
- All 82 browser-adapter/runtime tests passed (one opt-in ignored), all 16 browser
  JavaScript tests passed, affected-crate Clippy passed, and the off-screen Chrome
  fixture passed with the delayed account panel and full target cleanup.
- Workspace identity is still unqualified; the account menu exposes no selected
  workspace ID. The new account observation is not yet bound to production
  routing. Actual Codex integration and the remaining PRD gates stay open.
- Current official references distinguish [account switching](https://help.openai.com/en/articles/20001068)
  from [workspace selection in the profile menu](https://help.openai.com/en/articles/8542216).
  They do not specify stable DOM identifiers; missing workspace metadata is not
  treated as proof of a personal workspace.

## Observed effort labels and selection restoration (2026-09-19)

- Replaced fabricated `effort 1` through `effort 5` labels with the actual
  accessible labels of each selected slider position. Discovery visits each
  position without sending a message and restores the original selection,
  including after a failed read. A failed restoration invalidates discovery
  with an explicit error. Drafts and active generations block this operation.
- The current live English UI exposes Instant, Medium, High, Extra High and Pro.
  These are observed slider routes, not five independently verified model
  families. Separate advanced-menu choices were observed (Latest selected,
  GPT-5.6 Sol and GPT-5.5); family selection/binding is still required before
  publishing routes. No claim about underlying server weights is made.
- The selected-position announcement must agree with the slider's numeric value
  and range. Delayed hydration cannot assign the previous position's label to
  the next one. Missing labels remain unqualified rather than getting a guessed
  name. The new helper is bundled code; no dynamic script input was added.
- Live background discovery restored Extra High, and one subsequent real text
  qualification passed with exact submitted-message attribution and completed
  assistant output. Browser/page language remained en-US. This is browser
  transport evidence, not actual Codex picker or tool execution qualification.
- Bounded account-menu diagnostics now distinguish known public controls from
  redacted labels. The live menu shows Pro and no workspace selector/selected ID;
  this does not establish a workspace. The email remains absent from diagnostics.
- All 82 browser-adapter/runtime tests passed (one opt-in ignored); all 29 browser
  and desktop JavaScript tests passed. A real Chrome off-screen fixture passed
  delayed labels, restoration after a failed scan, preservation of an unsent
  draft, exact submission, cancellation and owned-target cleanup. Initial native
  taskbar-capable styles remain a previously recorded UX qualification gap.
- Affected-crate Clippy with warnings denied and format/diff checks passed.
  Release CLI, daemon and desktop builds succeeded.
  Production integration remains disabled; workspace scope, model-family binding,
  actual App/CLI integration and all remaining PRD gates are still open.

## Model family and effort identity (2026-09-19)

- Model candidates now include both the observed checked model family and the
  exact effort position/range in a versioned structured identity. A bare slider
  position can no longer select or qualify a different family. Display labels
  use the family name and actual effort name; secondary availability notes are
  excluded from the family name. The Latest selector remains explicitly Latest,
  without inventing the underlying server model.
- Discovery uses the normal Select model control and actionable radio options,
  verifies checked state, scans each available family's effort positions, then
  restores the original family/effort. Disabled options are skipped. Restoration
  is attempted even after a later family fails. Missing/ambiguous checked state
  fails explicitly; no unobserved family names are hardcoded in the adapter.
- Live background discovery observed 15 combinations: Latest, GPT-5.6 Sol and
  GPT-5.5, each with Instant, Medium, High, Extra High and Pro. The original
  Latest / Extra High selection was restored. These remain unadvertised candidate
  routes; selecting the controls does not establish every route's coding ability.
- Qualification and the managed generation driver now perform a read-only check
  of the full route after filling the composer and before Send. A same-effort
  wrong-family selection is rejected without changing it or sending the draft.
  The qualification preflight also verifies only the chosen route instead of
  traversing the entire model inventory before every test.
- The Chrome fixture passed multiple families, disabled options, delayed menu
  opening, restoration after failure in the second family, and wrong-family
  refusal with a populated composer. It exposed and fixed a race where reading
  retained checked state before the menu appeared could close the menu too soon.
- Reopening the live menu also exposed a pointer activation that completed its
  input sequence without actually opening the menu. The composer model button
  now uses verified focus plus ordinary CDP Enter input, followed by an expanded
  state check. No synthetic DOM keyboard events or model-state writes are used.
  One final live tool-envelope qualification passed on Latest / Extra High in
  background mode, including exact message attribution and a completed response.
  This verifies function/custom envelopes, not execution by a real Codex client.
  Earlier failing preflight checks stopped before the submission intent.
- Extended discovery exposed a false receipt timeout: the browser worker could
  finish successfully after the control wrapper had marked it failed at 30 seconds.
  Accepted operations now retain their actual worker outcome; client observation
  deadlines remain separate. Discovery has a longer client observation window.
  A paused-clock regression proves that queued/active operations are still pending
  after both former deadlines, then return the actual worker result. Tokio's
  existing test-only feature provides the virtual clock; no version changed.
- All 83 browser-adapter/runtime tests passed (one opt-in ignored), all 34 browser
  and desktop JavaScript tests passed, affected-crate Clippy with warnings denied
  and format/diff checks passed. Release CLI, daemon and desktop builds succeeded.
- Account/workspace binding, actual Codex execution/pickers, initial shell-window
  exposure and the remaining release gates remain incomplete. Production routing
  stays disabled and native Codex configuration/authentication remain unchanged.

## Observed account-default context and qualification binding (2026-09-19)

- The account observer now inspects the profile submenu through ordinary hover,
  without selecting an account or workspace. The live single-account variant
  places its email in a heading before the checked row, not inside that row.
  The adapter requires the known three-control layout and independently matches
  this candidate against the selected, loaded Settings / Account panel. Raw
  identifiers stay in memory; exported diagnostics contain bounded structural
  counts, known labels and booleans only.
- Deliberate specification adaptation: this verified UI variant exposes no
  workspace selector or provider workspace ID. It can establish a separately
  tagged **account-default context**, never an invented personal workspace ID.
  The precise recognized profile/submenu structure, one account, an individual
  plan marker and matching Account panel are required. Workspace controls,
  organization markers, ambiguity, failed loading or unknown layout invalidate
  it. Explicitly observed workspace IDs remain a different scope kind.
- Rust alone computes this qualification; a DOM payload cannot assert it.
  Installation-separated, domain-tagged hashes bind the account and either the
  observed workspace or account-default context. The diagnostic worker uses a
  fresh lifetime-specific installation value; production activation must supply
  its durable installation ID. No native account metadata is substituted.
- Text/tool diagnostics now require a qualified discovered scope, verify it in
  their own Temporary Chat before submission and again after completion, and
  persist only opaque scope IDs in the durable ledger. A mismatch prevents
  submission or withholds the completed response, with no automatic resend.
  Scope is discarded when browser/model observations are invalidated.
- The live check exposed Account-tab pointer activation during dialog animation:
  the dialog opened, but the Account tab remained unselected. It now uses verified
  focus and native Enter, with selected-panel evidence still required. Enter also
  carries its normal character event for ordinary HTML button default activation;
  the Chrome fixture covers this separately from the live site's key handler.
- Authenticated background discovery confirmed the account-default binding. One
  final live function/custom-envelope test passed on Latest / Extra High with
  matching scope before and after generation, exact submitted-message attribution,
  completed assistant output and browser/page language en-US. Earlier failed
  scope preflights sent no message. This proves scoped diagnostic transport, not
  execution by Codex or production model publication.
- Verification: `cargo test -p cxweb-browser-adapter -p cxweb-runtime` passed
  87 tests (one opt-in ignored). The desktop/browser Node suite passed 38 tests;
  affected-crate Clippy with warnings denied and format/diff checks passed.
  The real Chrome off-screen fixture passed account-submenu inspection, native
  Account-tab activation, no-send on preflight scope failure, withheld output
  after a completion-time scope failure, and complete owned-target cleanup.
  No additional generation is attempted on either scope failure path.
  Release CLI, daemon and desktop builds succeeded.
- Remaining integration: hand the single browser owner to the durable generation
  driver, apply account checks at appropriate generation boundaries without
  navigating Settings on every polling tick, then qualify actual Codex tool
  round trips and pickers. Production routing remains disabled. Initial shell
  exposure, full lifecycle/release qualification and all remaining gates stay open.

## Composer integrity correction (2026-09-18)

- Added an explicit `browser-control test-tools` operation through the same
  private runtime and durable qualification path. It requests exactly one
  function call and one custom call with fixed fixture inputs and executes
  neither. Strict schema, nonce and exact-input checks gate its separate
  `tool_protocol_qualified` result; it does not enable Codex routing.
- The authenticated tool protocol test passed: both expected requests validated,
  user content matched, an assistant response was attributed, generation had
  ended and the expected/observed control labels matched the localized extra-high effort label.
  This proves the two envelope formats can traverse the current web surface.
  It does not prove native tool execution, result delivery or actual pickers.
- The runtime suite passed 69 tests, workspace Clippy passed, and all three
  release binaries built. The panel distinguishes protocol verification from
  actual integration. Actual Codex execution remains the next integration step.

- The user supplied a screenshot showing the submitted prompt collapsed behind
  an expand control. User-message matching now excludes button labels on a clone
  of the message subtree and preserves structural newlines. It still compares
  the whole message. The Chrome fixture now includes clipped content and an
  expand control; full-content attribution passes while a prefix or an appended
  control label is rejected.
- A live attempt initially returned `E_MODEL_FIDELITY`. Bounded qualification
  diagnostics now report only model labels and attribution booleans, never the
  request, response, raw turn IDs or account data. A subsequent explicitly run
  test passed with matching localized extra-high effort labels, matched user content, an attributed
  completed assistant response and the required nonce-bound final envelope.
  This is authenticated text transport evidence, not tool execution or picker
  certification. A following fresh status observation correctly invalidated the
  cached qualification while retaining the last diagnostic result.
- Chrome 153.0.8010.48 synthetic checks, seven browser-adapter tests (one opt-in
  ignored), six control-protocol tests, all 12 JavaScript tests, workspace Clippy,
  formatting and diff checks passed. CLI and daemon release builds succeeded.
  Computer Use reinitialization and focusing the address bar did not resolve its
  URL-verification failure; no tool safety checks were altered.

- A subsequent user attempt passed the Send guard but failed with the generic
  live-qualification error. Stage-specific attribution codes now distinguish
  ambiguous turns, model changes, user-message mismatch and fenced output.
  One desktop-driven diagnostic attempt then returned `E_USER_MESSAGE_MISMATCH`.
  This establishes a post-click readback failure, not a verified response.
- At most one failed qualification tab is retained for inspection; the next
  explicit qualification closes the previous retained target. No automatic
  resubmission is added. Computer Use could read the cxweb result but refused
  the Chrome snapshot because it could not establish the browser URL. The live
  DOM cause therefore remains unconfirmed.
- Known no-click refusals now terminate the durable intent as failed, while
  unknown/post-click failures remain submission-uncertain. Regression tests
  verify recovery and repeated admission cannot resend either outcome.
  The full Rust workspace suite, all 12 JavaScript tests, Clippy with warnings
  denied, formatting and diff checks passed after these changes.

- The user confirmed five effort candidates and Temporary Chat, then reported
  `E_COMPOSER_MISMATCH` during the explicit text test. Send was not clicked.
- Replaced CDP live typing with the browser's plain-text editing command in the
  empty focused editor. A reviewed competitor comment identifies live-typing
  Markdown transformations as a failure mode; its implementation was not copied.
- The send guard still requires exact content. It additionally recognizes plain
  paragraph boundaries as single newlines when layout-derived `innerText` differs.
  It preserves blank lines and spaces and refuses unknown rich nodes in this
  fallback. No whitespace trimming or delimiter removal was introduced.
- Four new guard regressions passed, together with all eight existing JavaScript
  tests. Browser-adapter tests passed (six tests, one opt-in test ignored).
  Workspace Clippy, formatting and diff checks passed. Release CLI and daemon
  builds passed. The synthetic Chrome 153.0.8010.48 probe passed literal insertion,
  guarded sending and response attribution; an earlier probe exited during Chrome
  startup. These tests do not establish the cause or resolution on the live page.
- Restarted the runtime and rediscovered five candidates with Temporary Chat
  verified. A fresh user-initiated text test is pending; no live generation was
  submitted during this correction. Codex routing remains disabled.

## Observed environment (2026-09-17)

- Windows x64; Rust/Cargo 1.97.0.
- CLI: 0.153.4, npm installation.
- Desktop MSIX: OpenAI.Codex 26.911.7940.0.
- Running desktop backend: 0.155.0-alpha.2.6, installed runtime directory
  `OpenAI/Codex/bin/eab8377aebac6c07`. This differs from the CLI.
- The MSIX bundled executable cannot be launched directly (access denied);
  the running application uses the separately installed runtime above.
- Repository initially had no commits or origin remote; existing `.gitignore`
  excluded `/hidden-data`. Preserve that exclusion and keep supplied materials private.

## Work

- Established Rust workspace, pinned compiler, diagnostic CLI and authenticated
  loopback probe. Probe is synthetic only and rejects native inference explicitly.
- Added pure TOML preflight/removal planning, native catalog preservation tests,
  submission state machine and loopback boundary tests.
- No native config/auth files modified. No competitor source used.

## Source evidence

- Official config reference: https://learn.chatgpt.com/docs/config-file/config-reference
  confirms user-level `openai_base_url` and startup `model_catalog_json`.
- CLI release source: `openai/codex` tag `rust-v0.153.4`, commit
  `3d2ee51ca2d5db578f328aa75e20aa22c0197c9a`; reviewed model-provider-info,
  models-manager and protocol/openai_models. Downloads stay in ignored `.local`.
- `openai_base_url` preserves the built-in provider identity. Its native provider
  supports WebSockets. HTTP-only success must not certify all native features.

## Verification

Executed successfully: `codex --version`, desktop backend `--version`,
`rustc --version`, `cargo --version`, `codex app-server --help`,
`git ls-remote --tags https://github.com/openai/codex.git rust-v0.153.4*`.

Passed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace` (8 behavioral tests), `cargo build -p cxweb`, `git diff --check`.
The first clippy run caught a collapsible conditional; corrected and reran successfully.

`node scripts/probe-client.mjs <absolute executable>` passed independently for
CLI 0.153.4 and desktop backend 0.155.0-alpha.2.6. Both returned the exact owned
model from `model/list` with `hidden: false`, accepted `thread/start`, and completed
a real provider turn through the Rust loopback server with the expected synthetic
text. Reports are in `integration-tests/compatibility/*.synthetic.json`.
This uses a static synthetic catalog and a fake API key in the isolated process;
it proves neither the real subscription catalog nor the actual UI pickers.

Desktop exact source tag: `rust-v0.155.0-alpha.2.6`, commit
`bf6f0a4ec97919bf697cdc532e7b8af4ec482fc6`. Generated both client protocol schemas
using each actual binary's `app-server generate-json-schema`. Schema output is
ignored local research; no native authentication was accessed.

## Next

The isolated client harness, native mock upstream and private browser transport
now exist; their results are recorded below. The next integration step is to
qualify the authenticated account/workspace, selectable models and Temporary Chat
behavior, then connect a real browser driver to the coordinator and activation
owner. The desktop now includes an explicit non-generative model-discovery action;
its results remain candidate-only and cannot activate routing.

Windows Computer Use could not establish the current browser URL and explicitly
stopped external UI inspection. The product's own reviewed browser adapter now
opens the ordinary model menu only after an explicit `Verify available models`
action, reads the bounded structural effort surface, then closes the menu. It does
not read auth data or submit a prompt. The synthetic Chrome pipe fixture passed
this operation on Chrome 152. Do not report G0 complete from an app-server listing,
candidate discovery or a visible login alone.

Authenticated model discovery subsequently passed on the user's cxweb profile.
The current ChatGPT composer exposes a structural reasoning-effort slider rather
than a conventional model list: five positions were observed, with position four
selected and a visible localized thinking-effort label. The adapter maps
these positions to opaque `webbridge/` candidate IDs; it does not infer a hidden
server model name. A separate owned tab at
`https://chatgpt.com/?temporary-chat=true` passed exact URL, single visible composer
and signed-in/no-login-control checks, then was closed without submitting a prompt.

The supplied competitor implementation was inspected only after the user asked
for it as a behavioral hint. It showed that the current picker is scoped to the
composer form and can expose `composer-intelligence-picker-content`, a `role=group`
surface and an ARIA reasoning slider; it also validates Temporary Chat by exact
URL rather than by assuming a button exists. No source or fixed model/limit values
were copied. cxweb implements these observations independently in its Rust-owned,
fixed-operation adapter. Real Codex App and CLI picker qualification and all live
generation/tool evidence remain pending.

The behavioral review also exposed two incorrect assumptions in the initial
adapter: the ARIA effort input can have zero rendered width inside a visible
slider container, and the stable conversation identity lives on
`data-turn-id-container` rather than the display-indexed conversation test ID.
The fixed-operation adapter now targets the visible container, verifies the
selected effort with bounded arrow-key input, and attributes new user/assistant
turns by unique stable container identity. The fresh-profile Chrome 152 pipe
probe passed model selection, stable turn identity, literal prompt transport,
completion attribution, cancellation, selector-drift rejection and owned-process
cleanup. This is synthetic DOM evidence; no live prompt was sent.

## Additional implementation and evidence

- Added `ManagedDriver`, a real private-pipe implementation of the coordinator's
  `BrowserDriver` interface. Its owner thread serializes preparation, submission,
  observation and cleanup, retains the instance/profile lock and allocates a
  separate Temporary Chat target per admitted turn. The full canonical history
  remains supplied by the coordinator; no browser history is shared across turns.
- Runtime-owned bindings restrict installation/account/workspace/epoch/routes.
  Activation must provide an independent DOM scope verifier, checked before
  preparation, submission and observation. No production verifier or login-to-
  driver ownership handoff is wired yet; this is not an activation claim.
- Attempted sends are latched, queues and owned targets are bounded, and cleanup
  commands remain queued after a waiter disappears. Failed tab closure retains
  ownership rather than silently freeing a session slot. Adapter closure now
  requires acknowledgement or confirmed target absence.
- Four driver contract tests passed for scope boundaries, route validation,
  queue overload and dropped cleanup waiters. All 73 runtime tests passed before
  the final cleanup refinement; the four targeted tests and workspace Clippy
  passed afterward. The Chrome 153 synthetic probe passed with confirmed target
  closure. Actual native-client execution through this driver remains untested.

The user confirmed the release UI after commit e29fc9a: five effort candidates,
effort four selected, and Temporary Chat verified. This is authenticated manual
evidence for discovery and selected-state verification, not generation or Codex
picker evidence. No screenshot or account data is stored in the repository.

An explicit text-qualification action now sends one fixed diagnostic request in
an owned Temporary Chat target. It rechecks the selected candidate before opening
the target and verifies the target's label before inserting any prompt. The
existing durable ledger records submission intent before Send; uncertainty never
causes an automatic retry. A successful response must pass turn attribution,
strict nonce-bound envelope validation and exact expected text. The result is
text-only evidence and does not enable model publication, tools or routing.
The private control protocol retains an operation receipt while the UI is gone;
only allowlisted failure codes are returned. The CLI can invoke these same fixed
operations and reports only a bounded status summary.

Synthetic Chrome 152 tests passed wrong-route rejection, failed-durability refusal
without submission and a single successful qualification submission. Seven UI
tests passed, including no automatic generation, duplicate-click suppression,
unverified Temporary Chat refusal and visible failure without retry. The full
Rust suite passed 108 tests with two opt-in tests ignored. Clippy with warnings
denied passed. A Node regression test also verifies that a missing slider cannot
be interpreted as a zero-valued candidate; it runs in CI.

Live discovery passed again through the same runtime via CLI. Two text-test
preparations failed before durable submission intent (ledger state `failed`),
with the second isolating candidate selection. Bounded waits for the menu and
each arrow-key update were added, along with delayed synthetic fixtures. The
next live attempt reached Send but returned `E_SUBMISSION_UNCERTAIN`; it was
not automatically retried and does not qualify text transport. The updated
adapter now distinguishes explicit no-click preconditions (missing/disabled
Send, route mismatch, composer mismatch) from an uncertain CDP send result.
Fresh synthetic Chrome qualification, including the delayed fixture and no-send
guards, passed after these changes. Authenticated text qualification remains
unverified and requires a new explicitly initiated test.

The final targeted private-control run passed six tests, including replay of an
uncertain text-test receipt without a second backend call and invalidation of
stale success status. The release desktop, daemon and CLI built successfully.
The desktop was reopened after non-generative discovery confirmed all five
candidates and Temporary Chat again; no further live text test was initiated.

The G0 actual-picker gate remains open. Per quality document section 2, independent
implementation continues without claiming a live gate passed. No production UI
or real configuration mutation is enabled yet.

- Windows private browser transport implemented in the small OS-only platform
  crate, with explicit inherited-handle allowlist, suspended startup, job-object
  containment, bounded CDP frames and no debug TCP endpoint.
- `scripts/probe-browser.ps1 -Browser <Chrome executable>` passed with
  Chrome/152.0.7977.84, CDP 1.3: zero listeners, 10 owned processes, all cleaned up
  after forcibly terminating the launcher. Separate fresh profile, no login.
  Evidence: `integration-tests/compatibility/windows-chrome-pipe.local.json`.
  This uses the installed signed Chrome payload for the development spike;
  bundled browser delivery/signature/update qualification is still outstanding.
- Strict envelope parser rejects duplicate keys at all depths, trailing data,
  fences, wrong nonce/purpose, model-supplied IDs, unknown tools and bad schemas.
  Native function/custom/namespace identities are kept separate. All 15 supplied
  synthetic PRD envelope fixtures pass their expected accept/reject results.
  Only plain-text custom tools are currently supported; grammar tools are
  explicitly rejected pending a qualified grammar adapter.
- Native HTTP transport uses a fixed reviewed subscription host, TLS validation,
  disabled environment proxies and redirects, streaming backpressure and bounded
  concurrency. Mock tests prove native byte/header/status preservation. Browser
  branch receives no native transport headers. Native WebSocket and auxiliary
  endpoint qualification are still outstanding, so no production route is installed.
- Added bounded per-account scheduler (2 generations, 8 queued, 10 minute queue
  deadline), cancellation cleanup and session isolation. Added SQLite metadata
  ledger on blocking workers: request/session/input hashes only, durable state,
  replay rejection and explicit uncertainty after a crash during submission.
- Current `cargo test --workspace`: 25 tests pass (includes the 15-case fixture
  loop). `cargo clippy --workspace --all-targets -- -D warnings` passes.
  Initial ledger build found a MutexGuard coercion error, fixed before these passes.

Next independent work: canonical request/response wire mapping, native transport
completion, browser DOM fixture adapter and protected state/config lifecycle.
User login is not yet requested: the runnable app flow is still being assembled.

## Canonical protocol increment

- Added complete-history canonical decoding, role/tool-result preservation,
  nonportable reasoning/compaction rejection, explicit image/output-format errors,
  byte-budget refusal without truncation, and observed model/effort fidelity checks.
- Added complete buffered Responses/SSE encoding for text, native functions and
  custom calls, stable caller-owned IDs and ordered terminal events. Browser
  token counts are unavailable and are not fabricated.
- Added an original syntax-only recognizer for the exact native apply_patch Lark
  contract with SHA-256 `d6367f4826ed608c424b0a308f3d6163527df63c22513d089b91863552f8bfeb`.
  Unknown/custom grammar definitions still fail closed. This code never applies
  patches. The grammar source was inspected from the official pinned Codex release;
  no competitor source was used or copied.
- Extended the opt-in synthetic backend probe to capture ONLY tool definitions
  into ignored `.local`, never headers, prompts or conversation history. The
  desktop backend probe still passes. Its default synthetic model request includes
  `request_user_input`, `view_image`, `multi_agent_v1` namespace and server-side
  `web_search`. The canonical adapter currently rejects server-side built-ins;
  this integration gap must be resolved explicitly, not silently dropped.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` and
  `cargo test --locked --workspace` pass, now 31 tests. `cargo fmt --all` and
  `git diff --check` pass. No actual web model/tool loop is claimed yet.

Next: connect these modules into a browser/session provider with deterministic
DOM fixtures, qualify native WebSocket/catalog behavior, protect persistent
state and implement the login/control surface. Real App/CLI picker tests and
account login remain NOT RUN; user action is not requested prematurely.

## Browser operations and durable turn coordination

- Added bundled fixed DOM operations for baseline, literal composer insertion,
  one Send click, attributed observation and Stop. Prompt strings travel as CDP
  values, never as executable script. Origin is checked both via the frame tree
  and inside the fixed page function to close the navigation race.
- Added logical turn tracking: exclude historical IDs, require a matching new
  user message, reject replaced assistant identity, require explicit completion
  evidence, reject fenced output and committed-prefix revisions, and preserve
  uncertainty when cancellation happens during submission.
- Real Chrome/152.0.7977.84 over the private Windows pipe passed the bundled DOM
  fixture: literal quotes/Unicode/script-like text, old-message exclusion,
  response attribution, Stop action, and selector-drift rejection. Zero TCP
  listeners and full owned-process crash cleanup still pass. Evidence is
  `integration-tests/compatibility/windows-chrome-dom.local.json`.
  These selectors are candidates tested on synthetic markup, NOT qualified
  against the live ChatGPT UI. Login, model discovery and Temporary Chat evidence
  remain absent and cannot be inferred from this result.
- Added the runtime Coordinator joining canonical decode, scheduler, SQLite
  admission/state transitions, browser-driver operations, envelope validation,
  native wire delivery and a bounded 8 MiB replay cache. Completed responses can
  be delivered again without submitting again. Missing replay data fails explicitly.
- Coordinator work retains its own lifetime: dropping the client future triggers
  cancellation but allows browser stop/release and durable state writes to finish.
  Tests cover final response/replay, native tool output without local execution,
  uncertain submission, malformed output, mismatched assistant identity and
  dropped-client cancellation. BrowserDriver has a mock implementation in these
  tests; the real session-owning worker is the next integration step.
- Latest checks passed: `cargo fmt --all -- --check`,
  `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace` (39 tests), `cargo build -p cxweb`,
  `scripts/probe-browser.ps1 -Browser <installed Chrome path>`, `git diff --check`.

Next dependency-ready implementation: real browser worker with protected profile
and account/route/session leases, then wire Coordinator into Gateway through a
qualified provider. Keep production routing disabled until authentication,
model discovery, native WebSocket preservation and actual pickers are proven.

## Development login panel and protected state

- Windows state now uses the OS LocalAppData location and protected directories
  owned by the current user, with full access only for that user and SYSTEM.
  Existing unexpected ownership, ACLs and reparse points are rejected. Tests
  exercise ACL verification and exclusive instance-lock release.
- Added a Tauri 2 development panel with two local IPC commands: connect and
  status. A bounded browser worker opens the official sign-in page in the app's
  dedicated profile over the private pipe. Remote pages cannot navigate the
  privileged control window. No Codex configuration is installed by this panel.
- Structural login observations do not certify account identity or readiness.
  The displayed state remains awaiting verification. Reopening an unavailable
  login browser requires an explicit action and preserves its dedicated profile.
  This worker is non-generative; production daemon lifetime and real session
  leases remain separate unfinished work.
- The desktop executable built and its native window opened and closed with
  exit code zero in a local smoke check. This is not visual or live login proof.
- All project-facing text is English, including UI, window titles and fixtures.
  Unicode tests use emoji instead of Czech words. User-supplied ignored source
  material remains unchanged. README remains the minimal development placeholder.
- Verification: `cargo test --locked --workspace` passed 41 Rust tests;
  `node --test apps/desktop/tests/app.test.mjs` passed 3 UI behavior tests.
  CI now includes the UI tests. Full live authentication/model discovery,
  actual Codex pickers and coding qualification are still outstanding.

## Live login attempt and disconnect planning

- Inspected the actual Tauri window through Windows Computer Use. English text
  rendered correctly, and Connect ChatGPT opened the dedicated Chrome window.
  The panel rendered Waiting for sign-in. This verifies the UI-to-worker path,
  not an authenticated session.
- The user attempted login and reported an OpenAI Route Error 400 with an HTML
  content-type mismatch after entering credentials and submitting Login. Their
  later clarification supersedes the initial report that the error occurred
  before the form. A recent fresh login by the same method worked in their normal
  browser. Root cause remains unknown; no credential/cookie export, stealth
  patch, automated login retry or routing installation was performed. G1 fails
  pending a successful supported login and actual account/model verification.
- Disconnect planning now accepts exact published route receipts. A persisted
  owned model selection restores the pre-connect native selection only when
  that selection is currently verified; otherwise the owned selection is removed.
  Native, third-party and unpublished webbridge selections are preserved.
- Regression tests exposed comment loss when deleting TOML keys. Removal now
  retains attached user comments, including when the route was already removed.
  This remains pure planning. Durable journals, atomic replacement and live
  editor race qualification are still required before real configuration writes.

## Dynamic catalog transport

- Gateway now accepts a request-specific catalog snapshot from the web provider.
  The provider receives the client query but no native authorization headers.
  Unknown codecs and the unqualified provider keep the original native path.
  No production provider advertises entries before live qualification.
- Added bounded native catalog retrieval and semantic append without changing
  existing entries or unknown metadata. Native authorization failures remain
  native failures; the gateway never substitutes a synthetic native catalog.
- Native validators are removed from augmentation requests. Local ETags include
  credential/account partition, exact client query, codec, web scope, generation
  and merged bytes. No cross-request native cache exists. Changed snapshots or
  account partitions cannot reuse the previous local 304 response.
- Fixed native transport incorrectly treating 304 as a redirect. Real redirects
  remain rejected. Unexpected upstream 304 on unconditional augmentation fails.
- Tests use local HTTP servers and synthetic credentials only. They cover full
  gateway dispatch, unknown codecs, native metadata preservation, conditional
  requests, scope/generation changes, duplicate JSON keys and native denial.
  `cargo test --locked --workspace` passed 49 tests; workspace Clippy passed with
  warnings denied. This is local integration evidence, not genuine picker proof.

## Manual login comparison pending

- Added development-only `cxweb manual-login-probe` to isolate the failed login.
  It opens the same protected cxweb profile with ordinary installed Chrome,
  without CDP, automation, cookie export, profile copying or security overrides.
  The user completes login manually. This is a diagnostic comparison, not a
  production authentication workaround or a claim that pipe login now works.
- The command holds the normal installation lock while its browser process runs.
  It refuses an occupied profile before launching; that refusal was exercised
  against the currently running desktop app. The diagnostic process must remain
  running, and its browser must be closed before restarting cxweb.
- User was asked to close cxweb before this comparison. The browser launch and
  login result remain pending; no existing browser was forcibly closed.
- `cargo build -p cxweb`, workspace Clippy and all 49 Rust tests passed.

## Native Responses WebSocket transport

- Added an authenticated gateway upgrade path for native Responses WebSockets,
  using the same fixed upstream and shared connection limit as native HTTP.
  Native credentials never enter browser code. Each hop negotiates its own
  handshake; native response metadata is retained without copying handshake keys.
- The transport forwards native create messages unchanged, including reuse with
  previous_response_id, and checks each create for owned model IDs. Owned routes
  receive an explicit unqualified-WebSocket error rather than native execution.
  Unknown/binary client message formats close the connection; no repair or retry.
- Added 32 MiB message/frame bounds, a connection timeout and an idle deadline.
  The connector uses TLS certificate validation and does not follow redirects.
  Production web routes still require their separately qualified HTTP transport.
- Local socket tests cover native text fidelity, connection reuse, query/auth
  forwarding to the fixed test upstream, turn-state response metadata, owned
  model isolation, HTTP 401 preservation and redirect rejection. Workspace tests
  passed 51 cases, and Clippy passed with warnings denied. Authenticated native
  WebSocket behavior, auxiliary endpoints and full client qualification remain
  outstanding; no claim of G0/G4 completion follows from local socket tests.

## Windows configuration replacement primitive

- Added a bounded snapshot/stage/commit primitive. Snapshots record original bytes
  and Windows volume/file identity. Hard-linked and reparse-point target files
  are refused. The caller must still qualify canonical parent/home ownership.
- Staging files are created with the private owner/SYSTEM descriptor from the
  first CreateFile call, written and flushed. Existing files use ReplaceFileW
  without ignore-ACL flags; absent files use a move without overwrite permission.
  Destination bytes and identity are rechecked immediately before replacement,
  and candidate bytes are checked afterward. Failed staging remains for recovery.
- Existing-file sharing denies in-place writers during check/replacement, but
  rename races are not mathematically eliminated. This is explicitly not a
  general cross-process compare-and-swap operation. Durable journaling and real
  editor/active-Codex qualification remain prerequisites for enabling real writes.
- Four new Windows filesystem tests cover existing/absent writes, user edits,
  same-content file substitution, concurrent creation, modified staging,
  traversal rejection, hard links and active writers. All 55 workspace Rust
  tests and Clippy with warnings denied passed. Only temporary synthetic files
  were changed; the user's actual Codex configuration remains untouched.

## User-reported startup and login issues

- Fixed debug builds opening a console: the Windows GUI subsystem now applies
  to all desktop build profiles. Verified PE subsystem 2 and a single desktop
  app window in the native window inventory after launching the rebuilt app.
- Removed the browser launcher's initial blank window with Chromium's
  no-startup-window option; the login action creates its own single window.
  Reusing an arbitrary blank target was rejected after an integration check
  exposed ambiguous restored/closing blank targets. No restored chat is borrowed.
- The actual Chrome probe now checks zero unsolicited startup pages; optional
  --open-login verifies that the official page can subsequently be opened. This
  passed on Chrome 152.0.7977.84 alongside the existing DOM fixture checks.
- Removed automatic polling and focus-triggered inspection during manual login.
  Connect opens the page without immediate DOM inspection; the user explicitly
  checks status afterward. This removes unnecessary activity but is NOT evidence
  that the reported email/Continue loading loop is fixed. Authentication is
  still pending user verification. No cookies or profile data were deleted.

## Manual authentication comparison

- The user reports successful email/password authentication in the same installed
  Chrome and dedicated cxweb profile when launched by manual-login-probe without
  CDP. This narrows the investigation to differences in the managed launch and
  session lifecycle; it does not establish the exact cause of the earlier route
  error or qualify authentication through the desktop app.
- The comparison browser remains open and holds the profile through the probe.
  Computer Use stopped because it could not reliably determine the browser URL.
  No alternate method was used to close or control that window. Await normal
  manual closure before reopening cxweb and checking the persisted session.
- Managed session reuse, account/workspace identification, and end-to-end Codex
  App/CLI behavior remain unverified. No cookies or profile data were deleted.

## Managed browser navigation and persisted-session verification

- After the manual comparison, the user reported an indefinitely blank loading
  page when reopening ChatGPT through cxweb. A status check in that running build
  still reported waiting for sign-in.
- The launcher now supplies ordinary absolute Windows paths to Chromium instead
  of Rust's verbatim-path spelling. Both forms are canonicalized and compared
  before launch to preserve path identity. Dedicated-profile isolation, private
  CDP pipes, sandbox settings and process-tree containment remain in place.
- The browser diagnostic now creates a profile with the same protected directory
  permissions as the app and actually waits for a usable ChatGPT interface.
  Previously, opening a target alone was counted as successful navigation. The
  diagnostic can leave the page idle first and fails if readiness times out.
- Fresh-profile comparisons observed failures and successes with the original
  path spelling; ordinary-path probes loaded successfully. These results alone
  do not establish path spelling as the sole cause of the intermittent failure.
  A temporary event counter found four CDP events, not a saturated reply queue.
- After normally closing the old cxweb instance and launching the rebuilt app,
  its original saved profile loaded ChatGPT. An explicit Check status displayed
  Session detected / Session awaiting verification, observed through Computer
  Use. No password was entered and no cookies/profile contents were copied or
  deleted. Account identity, model discovery and Codex App/CLI remain unqualified.
- Added a path-identity regression test including spaces and Unicode, and an
  opt-in real-Chrome test that loads a loopback HTTP page and completes a fetch
  using a fresh protected profile. Workspace tests, the opt-in browser test,
  Clippy with warnings denied, formatting and the three desktop UI tests passed.
- The rebuilt CLI also passed scripts/probe-browser.ps1: no browser TCP listeners
  and all eight owned processes cleaned up when the diagnostic launcher exited.

## Durable configuration preparation and recovery evidence

- Added a private, exclusively locked configuration journal. It durably stores
  original text/existence, the planned candidate, hashes and the exact selected
  target before any configuration write. The original backup is embedded in this
  internal receipt; it is not a public export of the PRD interchange schema.
- Apply retains the original live file-identity snapshot and checks the journal
  has not changed before touching the target. Restart reads classify original,
  candidate and user-modified states without automatically rewriting any of them.
  A reopened prepared journal cannot apply without fresh preflight.
- Loading verifies the independently selected target and reproduces the exact
  allowed TOML plan in addition to checking hashes. A modified candidate with a
  recomputed hash, corrupt/redirected receipts, and identity substitutions fail.
- Six temporary-filesystem tests cover preparation, exclusive ownership, restart,
  a crash between config replacement and receipt update, user changes, absent
  versus empty files, corruption and substitution. All 62 workspace tests passed
  (the opt-in Chrome test was not rerun); Clippy, formatting and diff checks passed.
- This supplies the prepare/apply/recovery layer, not the complete installation
  lifecycle. Disconnect journaling, stale staging cleanup, daemon supervision,
  activation receipts and client/editor qualification remain to integrate. It is
  not wired to the desktop controller and no real Codex configuration was changed.

## Durable disconnect and owned-file removal

- Extended the private journal with disconnecting/config-restored phases and a
  validated before/after undo receipt. Disconnect uses the existing key-level
  three-way planner and exact published model IDs; unrelated edits and comments
  survive, while a user-replaced route reports a conflict without rewriting it.
- A crash before mutation triggers a fresh plan against current content. A crash
  after mutation can finalize the receipt from the observed restored result.
  Restored configuration does not imply already-running clients have restarted;
  the native-only listener lifecycle remains a separate integration requirement.
- Originally absent configuration is deleted only when removing owned material
  leaves it empty. Pre-existing empty files, user-created empty files, comments
  and additional values are retained. Windows removal uses a checked file handle
  denying concurrent writes/renames, rather than a later path-based delete.
- Four new journal tests cover owned model restoration, preservation, repeated
  disconnect, absent/empty files, both crash boundaries and conflicting edits.
  A platform test covers edited content, same-content identity replacement and
  an active writer before deletion. All 67 workspace tests passed; Clippy with
  warnings denied, formatting and diff checks passed. The opt-in Chrome test was
  not rerun. All filesystem mutations were confined to generated test fixtures.
- Desktop wiring, live editor qualification, staging cleanup, activation and
  daemon/client restart coordination are still outstanding. No real Codex
  configuration or authenticated browser session was changed in this work.

## Gateway disconnect admission and native compatibility

- The shared gateway now closes web admission irreversibly, propagates per-request
  cancellation and waits for admitted providers to finish their cleanup. Dropping
  an HTTP request cancels its work without dropping the provider cleanup future
  or prematurely reporting a successful drain. Providers must return buffered
  delivery after cleanup, consistent with the current turn coordinator.
- Owned HTTP responses/compaction receive E_WEB_DISCONNECTED after closure and
  later model-list requests use the native catalog. Native forwarding retains
  the original listener, URL/capability and transport behavior. Unqualified owned
  WebSocket messages retain their existing explicit rejection; they never escape
  to native generation.
- Timeout does not reopen admission or certify cleanup. Worker panic closes web
  admission and reports E_WEB_CLEANUP_UNCONFIRMED, so lifecycle integration must
  not proceed as if browser work had safely finished.
- Added mock-integration tests for native HTTP after disconnect, owned request
  rejection, provider cleanup after cancellation/client drop, timeout and panic.
  Extended catalog checks and the real local WebSocket relay test to verify an
  established native socket remains usable across web disconnect. All 70 Rust
  workspace tests, Clippy with warnings denied, formatting and diff checks passed.
- Production daemon ownership and orchestration with the configuration journal
  still need wiring. No real account generation or native config mutation was
  performed; actual App/CLI qualification remains outstanding.

## Disconnect lifecycle orchestration

- Added a controller binding a configuration journal to its exact gateway URL
  and capability. Concurrent disconnects are serialized; accepted operations
  continue if a UI caller stops waiting. Blocking filesystem work runs outside
  the async reactor and retains journal ownership through completion.
- The ordered operation closes/drains web work before restoring configuration.
  Drain failure leaves configuration unchanged. Restore failure reports a
  separate state and preserves the listener; success reports pending restart,
  never a claim that existing clients have already updated their configuration.
- Four integration tests cover drain timeout/retry, caller cancellation, wrong
  gateway binding, user-edited configuration, and a real loopback HTTP listener
  serving native requests before and after actual temporary-file restoration.
  The same listener rejects owned web requests afterward. Upstream data and
  configuration are synthetic fixtures, not live Codex/account qualification.
- All 74 workspace tests passed (the opt-in Chrome test was skipped); Clippy,
  formatting and diff checks passed. Production process supervision, protected
  control IPC, startup recovery and desktop wiring still remain to implement.

## Private Windows control transport

- Added a local named-pipe transport with the current user's SID and a validated
  installation identifier in its address. The owner/SYSTEM DACL is applied at
  creation, remote clients are rejected, and both endpoints verify the peer's
  OS-reported process user. Client opens use identification-only security QoS.
- The listener retains a pending instance while handing off each connection,
  preserving name ownership between clients. Fresh I/O objects avoid stale read
  errors; bounded retry allows cancelled overlapped I/O to release its previous
  kernel instance. Frames are limited to 64 KiB before allocation.
- Three Windows tests cover 32 successive connections without a name ownership
  gap, exact owner/DACL verification, and malformed or oversized frames. These
  are local transport tests, not cross-user adversarial qualification.
- All 77 workspace tests passed; the opt-in Chrome test was skipped. Clippy with
  warnings denied, formatting and diff checks passed. Typed commands, exchange
  deadlines, process supervision and desktop/runtime integration remain pending.
  This work did not change the running browser or real Codex configuration.

## Typed lifecycle control service

- Added versioned status, disconnect and operation-result commands over the
  private Windows pipe, backed by the real disconnect controller. Unknown fields,
  duplicate JSON keys, unsupported commands and protocol versions are rejected.
  Requests cannot supply paths, URLs, scripts, shutdown commands or drain limits.
- Disconnect operation IDs are bound to a random runtime instance. Receipts are
  bounded and never evicted to re-execute an old ID; concurrent different mutations
  are rejected while one runs. Accepted work survives loss of its UI connection
  or closure of control admission. Receipts are in memory: restart invalidates
  their instance, while configuration crash recovery remains journal-owned.
- Each connection has a two-second exchange deadline and one bounded request.
  The server retains its response until a bounded receipt acknowledgement to
  prevent Windows pipe closure from discarding unread output. Client replies are
  checked against the requested command and operation ID.
- Four new tests cover malformed/stale commands, duplicate operation admission,
  receipt capacity, stalled and slow-reading pipe clients, operation survival, and
  wrong-operation replies. The existing lifecycle integration test now performs
  disconnect through the actual Windows pipe, restores its temporary configuration,
  and verifies native HTTP forwarding on the same listener afterward.
- All 81 workspace tests passed (one opt-in Chrome test skipped), as did Clippy
  with warnings denied, formatting and diff checks. Production daemon startup,
  desktop command wiring, connect/relogin commands and live App/CLI qualification
  remain incomplete. No real account or Codex configuration was changed.

## Runtime host recovery and exclusive listener

- Added a runtime host that reopens a validated private journal, binds its exact
  prior port/capability and owns both the native gateway and private control
  service. Recovery never reapplies configuration or enables web model execution.
  A private-control failure is retried without dropping native compatibility.
- Added Windows SO_EXCLUSIVEADDRUSE before loopback bind. An occupied address
  fails closed without selecting a new port or changing the selected config.
  This protects an existing listener; it does not remove the documented risk of
  another process occupying a stale route while the runtime is absent.
- Restored journals can retain their original compatibility listener and report
  pending restart. A subsequently changed configuration reports restore failure
  instead of claiming that the recorded restored state is still current.
- The new host test reopens a persisted journal, proves occupied-port failure
  preserves config, performs private-IPC disconnect, restarts after restoration,
  and preserves later user edits. Native HTTP uses fresh connections against the
  same route; owned web requests are rejected. A separate actual Windows socket
  test rejects a competing bind even when that socket enables address reuse.
- All 83 workspace tests passed, with one opt-in Chrome test skipped. The expanded
  host test passed again after adding the user-edit case; Clippy, formatting and
  diff checks passed. These are local fixture tests, not real client qualification.
- This host is not yet the desktop's process entry point. Executable supervision,
  qualified installation/activation, browser ownership transfer, desktop wiring
  and proof of client exit before final listener cleanup are still outstanding.

## Standalone recovery executable and catalog ownership receipt

- Added the Windows GUI-subsystem cxweb-daemon executable. It takes explicit
  absolute journal/config paths, independently matches the selected config to
  the journal, and runs the recovered host. It never installs configuration,
  starts a browser or exposes raw startup errors/capabilities on stdout/stderr.
  Exit codes distinguish invalid launch inputs (2), recovery refusal (3), and
  a failed serving future (4). Desktop launch/supervision is not wired yet.
- Journals can now durably record exact published and native model IDs before
  application. The receipt is bounded, validated and immutable after its initial
  preparation. Recovered hosts obtain removal ownership from this record, not
  caller-supplied model lists. Missing receipts refuse this recovery entry point;
  legacy fixture journals remain readable for explicit diagnostic recovery.
- The real executable integration test launches hidden child processes against
  temporary protected state, checks private readiness and exclusive ownership,
  removes an exact owned selection while retaining the original native selection
  and user comment, then restarts and checks the original address and pending
  restart state. Stale operation instances are rejected. The binary's Windows
  subsystem is inspected. Children are terminated only through test-owned handles.
- All 85 workspace tests passed, including this process test and a catalog receipt
  validation/persistence test. One opt-in Chrome test was skipped. Clippy with
  warnings denied, formatting and diff checks passed. No real account requests,
  native credentials, user config changes or desktop/browser restart occurred.
- This establishes the recovery executable, not a supervised active integration.
  Browser ownership transfer, first activation, desktop launch/attachment,
  restart policy and live Codex qualification remain required.

## Windows user-level supervision qualification

- Added a native Task Scheduler COM adapter. A plan binds the installation ID,
  current user SID, installed executable and explicit journal/config arguments.
  It uses the interactive user token, least privilege and an owner/SYSTEM DACL;
  no password, shell command or remote scheduler connection is involved.
- Create-only registration refuses an existing task. Subsequent start/status/
  removal checks the exact registered definition. Removal refuses running or
  queued instances; dropping the desktop's registration handle does not stop or
  delete the task. Durable task ownership and activation/cleanup wiring remain
  necessary before this adapter can be enabled by the product.
- Live tests showed that RestartOnFailure did not re-execute the failing action
  here, including HRESULT failure and automatic registration-trigger variants.
  The final policy uses registration/logon startup and a repeating one-minute
  trigger with IgnoreNew. Periodic attempts continue while registered, rather
  than stopping after three failures. A running process is left untouched;
  supervision does not restart a process merely because IPC is slow.
- The explicit OS test compiles an isolated action fixture, observes its first
  failure and next execution, holds the second process alive, verifies repeated
  start requests create no third process, and checks running-task deletion is
  refused. It then releases the fixture and removes its registration. The test
  passed in 60.87 seconds. This is scheduler behavior qualification; the separate
  daemon process recovery test continues to cover real cxweb-daemon execution.
- All 87 ordinary workspace tests passed; the Chrome and scheduler tests are
  opt-in. The scheduler test additionally passed when explicitly invoked. Clippy
  with warnings denied, formatting and diff checks passed. No cxweb scheduled
  task or scheduler fixture directory remained after cleanup. Real desktop,
  browser/account and Codex configuration were not changed.
- Windows API references: [repetition patterns](https://learn.microsoft.com/en-us/windows/win32/taskschd/repetitionpattern),
  [IgnoreNew policy](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-multipleinstancespolicy-settingstype-element),
  and [create-only registration](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskfolder-registertask).

## Durable scheduled-task ownership

- The private configuration journal now stores a scheduler plan before external
  registration. Its name is bound to the current user's SID and installation ID;
  its executable and arguments come from the independently selected installation.
  A pending registration prevents configuration application and is never adopted
  or overwritten based only on its task name after an interrupted operation.
- Successful registration records both the original plan and the exact definition
  returned by Windows. Reopening ownership checks the selected installation,
  original plan and current OS definition without executing XML from the receipt.
  Configuration application rechecks recorded scheduler ownership before writing.
- Added tests for write-ahead plan persistence, uncertain-application refusal,
  redirected task names, and receipt binding to the user/installation/plan. The
  opt-in scheduler test now persists its registration, releases the journal and
  reopens ownership from disk before checking restart, duplicate suppression and
  removal. This live test passed in 60.84 seconds and left no scheduled tasks.
- All 89 ordinary workspace tests passed; the opt-in scheduler test additionally
  passed explicitly. Clippy with warnings denied, formatting and diff checks
  passed. Desktop/account/native config were unchanged.
- An interrupted registration without its final receipt remains a conservative
  pending recovery case; automatic adoption is intentionally not implemented.
  Full activation, client-exit-qualified scheduler cleanup and desktop/browser
  ownership transfer still require orchestration before production use.

## Desktop attachment to the login runtime

- The desktop now attaches to a separate sibling daemon through the private
  Windows control pipe. Only an absent endpoint permits a hidden process launch;
  a slow, busy or rejected peer does not trigger a competing launch or restart.
  The login runtime owns the browser worker independently of desktop windows.
- Startup reads cached status without inspecting the browser. Connect and refresh
  remain explicit user actions. Accepted operations survive client loss, retries
  preserve their operation ID and runtime instance, and reusing an ID for another
  command is rejected. The UI clears stale session indicators after runtime loss.
- Regression tests use real private pipes and a fake browser backend to check
  reopening without browser reads, completion after a UI waiter is closed, and
  refusal to launch another runtime when an existing peer is slow. No account
  interaction occurs in these tests.
- All 92 ordinary workspace tests passed; Chrome and scheduler qualification tests
  remain opt-in. Clippy with warnings denied, four desktop JavaScript tests,
  formatting and diff checks passed. Both desktop and daemon release builds
  succeeded. The existing desktop and browser were left running unchanged.
- This is the login bootstrap runtime, not a fully supervised active integration.
  Activation, browser execution, live model qualification and Codex App/CLI checks
  remain outstanding. The operation receipt bound is 256 per process; long-lived
  receipt retirement still needs an explicit protocol before production use.

## Native auxiliary HTTP routes

- Reviewed the official `codex-api` endpoint implementations for CLI tag
  `rust-v0.153.4` and App backend tag `rust-v0.155.0-alpha.2.6`. Both use POST
  `memories/trace_summarize`, `alpha/search`, `images/generations` and
  `images/edits`. These exact paths now forward to the fixed native destination.
  The client's JSON payload bytes, query, end-to-end headers, response status and
  body are preserved; no auxiliary request is delegated to the browser adapter.
- An owned web model on a native-only endpoint is explicitly rejected rather
  than silently selecting native inference. Unknown paths, encoded path aliases
  and unsupported methods still fail locally. Native auxiliary traffic remains
  available after web admission is disconnected.
- Added loopback integration tests for all four paths, native authorization and
  account-header preservation, rate-limit response preservation, and rejection
  without any upstream connection or browser call. The full workspace passed
  94 tests with two opt-in tests ignored. After tightening model validation to
  the reviewed request structs, all eight gateway tests passed again. Clippy
  with warnings denied and diff checks passed; formatting was applied.
- Source references: [CLI endpoint implementations](https://github.com/openai/codex/tree/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/codex-api/src/endpoint)
  and [App endpoint implementations](https://github.com/openai/codex/tree/bf6f0a4ec97919bf697cdc532e7b8af4ec482fc6/codex-rs/codex-api/src/endpoint).
  Downloaded research stays ignored under `.local/research/native-endpoints`.
- This closes four known HTTP gaps, not the entire native feature gate. Realtime
  call/signaling transports, the complete client call-site audit and authenticated
  compatibility checks remain outstanding. No real account request or user
  configuration change was made.

## Subscription request shape and realtime call creation

- Both reviewed clients choose JSON versus multipart realtime call bodies using
  `provider.base_url.contains("/backend-api")`. The former local `/v1` base URL
  changed that branch. New route plans and the synthetic harness now end in
  `/backend-api/codex`, preserving the subscription request-shape decision.
- New private journals use version 2. Version 1 plans remain reproducible for
  recovery and exact undo, without rewriting their configuration. The gateway
  accepts the original `/v1` alias under the same private capability. Tests prove
  a v1 applied journal can be reopened and removed, and a version/candidate
  mismatch is rejected. Old binaries do not understand v2 journals; installation
  updates must replace the runtime before activating a new-format integration.
- Added fixed-destination POST `realtime/calls` forwarding for the reviewed
  subscription JSON shape and raw SDP. It preserves body bytes, status, media
  type and Location (the client extracts its call ID from this header). Nested
  owned model IDs, duplicate JSON keys and unqualified media types fail before
  an upstream connection. Multipart is not translated into a different request.
- Re-ran the isolated app-server harness against actual CLI 0.153.4 and App
  backend 0.155.0-alpha.2.6 on the new route layout. Both listed the synthetic
  owned model, selected it and completed the expected mock response. Updated
  `integration-tests/compatibility/*.synthetic.json` records the route layout;
  neither report claims an actual picker or authenticated realtime test.
- All 98 ordinary workspace tests passed, with two opt-in tests ignored. Clippy
  with warnings denied, formatting and diff checks passed. The diagnostic CLI
  build and release desktop/daemon builds succeeded. No real configuration,
  authentication or browser interaction was involved.
- Request-shape source: [CLI realtime calls](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/codex-api/src/endpoint/realtime_call.rs)
  and [App realtime calls](https://github.com/openai/codex/blob/bf6f0a4ec97919bf697cdc532e7b8af4ec482fc6/codex-rs/codex-api/src/endpoint/realtime_call.rs).
  Realtime WebSocket normalization/sideband routing and live feature qualification
  remain open; HTTP call creation alone does not qualify complete voice support.

## Standalone native realtime WebSocket forwarding

- Reviewed both client call sites as well as their URL normalization. Standalone
  realtime uses the provider's API-key mode and defaults to api.openai.com/v1;
  subscription Responses and WebRTC call creation use the separate ChatGPT
  destination. WebRTC/existing-call sidebands use their direct client-controlled
  address and are not rewritten by cxweb. The product does not supply an API key
  or substitute web login for this native optional feature's authentication.
- The private gateway now forwards standalone realtime to fixed `/realtime` or
  `/live` destinations selected by the reviewed protocol header. Client query
  parameters cannot choose the destination. Both the current subscription-shaped
  local URL and the old normalized v1 URL layouts are accepted under the original
  capability. Unknown protocol modes fail explicitly.
- Native JSON events, audio payloads, cancellation and handshake metadata retain
  their bytes. Each outgoing frame rechecks protocol model fields; query model
  IDs are decoded and checked before connecting. Owned web routes cannot escape
  through nested session/response model fields or duplicate model parameters.
  No browser operation or tool execution occurs in this transport.
- Tests cover three protocol variants across both local URL layouts, operation
  after web disconnect, exact frame/query/header preservation and refusal of
  owned-model changes. Encoded owned query models and unknown modes produce no
  upstream connection. Responses framing retains its separate classifier.
- All 101 workspace tests passed, with the two opt-in tests ignored. After adding
  legacy URL coverage, all five WebSocket tests passed again. Clippy with warnings
  denied passed. Tests used local mock servers and synthetic credentials only.
- Sources: [App realtime preparation/authentication](https://github.com/openai/codex/blob/bf6f0a4ec97919bf697cdc532e7b8af4ec482fc6/codex-rs/core/src/realtime_conversation.rs),
  [CLI realtime preparation/authentication](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/core/src/realtime_conversation.rs),
  and [App URL normalization and sideband ownership](https://github.com/openai/codex/blob/bf6f0a4ec97919bf697cdc532e7b8af4ec482fc6/codex-rs/codex-api/src/endpoint/realtime_websocket/methods.rs).
- Live voice/media negotiation, account authentication, client pickers and complete
  native feature qualification remain open. Outgoing client binary frames remain
  unqualified; the reviewed clients serialize their outgoing messages as JSON text.

## Web gateway binding to the turn coordinator

- Added a WebProvider implementation that binds admitted HTTP requests to the
  durable coordinator, published route allowlist and runtime-owned account/
  workspace scope. Request-supplied account metadata cannot select that scope.
  The activation/browser qualification owner must provide the scope and routes;
  this adapter does not advertise models or activate production routing itself.
- Extract only validated session-id, thread-id and turn metadata correlation
  fields before dropping transport headers. Conflicting or duplicate identities
  fail before browser preparation. Context-window changes separate browser scope.
  Only framed hashes leave extraction; bearer/account/routing headers and raw
  diagnostic metadata are absent from the browser operation.
- Request identity combines runtime scope, native turn identity and normalized
  generation payload. Delivery format and client tracing metadata do not cause
  a second submission. A new native turn can intentionally repeat the same text;
  retries of a completed request reuse its response/tool IDs. This depends on
  stable native turn metadata and does not certify all client reconnect behavior.
- The provider returns native JSON or buffered SSE with explicit buffered
  diagnostics. Gateway cancellation reaches the actual coordinator and its stop
  cleanup. Compaction remains explicitly unavailable until its browser protocol
  is qualified; no history truncation or fake checkpoint is introduced.
- Three integration tests cover gateway-to-coordinator response/replay, new turns
  and contexts, missing/conflicting identities, unpublished routes, and disconnect
  waiting for browser stop. They use a fake browser driver and no account access.
  All 104 workspace tests passed, with two opt-in tests ignored. Clippy with
  warnings denied, formatting and diff checks passed.
- Extended the isolated client harness with a boolean identity-contract report.
  Actual CLI 0.153.4 and App backend 0.155.0-alpha.2.6 both supplied the required
  headers and completed the synthetic response. Reports contain no raw IDs.
  Diagnostic CLI build and both updated harness runs passed; the harness still
  marks actual picker and authenticated browser checks as not run.
- Sources: [App HTTP correlation headers](https://github.com/openai/codex/blob/bf6f0a4ec97919bf697cdc532e7b8af4ec482fc6/codex-rs/codex-api/src/endpoint/responses.rs),
  [App turn metadata](https://github.com/openai/codex/blob/bf6f0a4ec97919bf697cdc532e7b8af4ec482fc6/codex-rs/core/src/responses_metadata.rs),
  and [CLI turn metadata](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/core/src/responses_metadata.rs).
  x-client-request-id equals the thread ID in these clients and is not used as
  a per-generation idempotency key.
- Binding the real managed browser driver, live route discovery/qualification,
  tool round trips, compressed request classification and activation remain open.

## Compressed request classification

- Added Zstandard request inspection for the reviewed Codex compression mode.
  The gateway decodes a separate copy for model classification and validation;
  native requests retain their exact compressed bytes and Content-Encoding.
  Browser requests receive only the decoded, classified payload.
- Decompression runs on bounded blocking workers with four shared permits,
  a 32 MiB output limit and a 32 MiB decoder window bound. Dropping a client does
  not release a worker's permit before decoding finishes. Unsupported/stacked
  encodings, invalid frames and truncated/trailing input fail explicitly.
  The stricter 8 MiB web request limit applies after decompression.
- Added zstd 0.14.0 with default features disabled and its two locked dependencies.
  No unrelated dependency versions changed. Decoder behavior and window controls
  were checked against the [library documentation](https://docs.rs/zstd/0.14.0/zstd/stream/read/struct.Decoder.html).
- Regression tests verify byte-preserving native forwarding, correct web routing,
  duplicate JSON rejection after decompression, bounded expansion and malformed
  streams. A request smaller than 8 KiB on the wire but larger than the web limit
  after expansion is refused. The coordinator retry test now switches from plain
  JSON to compressed SSE delivery without a second browser submission.
- All 107 workspace tests passed, with two opt-in tests ignored. Clippy with
  warnings denied, formatting and diff checks passed. Release desktop and daemon
  builds succeeded. These are local synthetic tests; real subscription
  compression and authenticated browser generation still require qualification.
- Reviewed source condition: [App compression selection](https://github.com/openai/codex/blob/bf6f0a4ec97919bf697cdc532e7b8af4ec482fc6/codex-rs/core/src/client.rs).
  The existing isolated harness uses an API key, so its successful runs do not
  establish the subscription-only compression branch as a live client result.

## 2026-09-21: Windows 0.1.1 release verification

- e70b8f9 adds acknowledged browser shutdown on disconnect and optional scoped local session removal. Existing personal and Codex authentication data are preserved.
- Full locked workspace tests, workspace Clippy with warnings denied, formatting, 34 installed UI tests, 10 NSIS lifecycle scenarios and the opt-in real fixture-browser shutdown test passed locally.
- The subsequent complete JavaScript suite caught missing desktop command permissions. 5c34e49 registers the command in the Tauri build manifest, grants it only to the main local window and includes the generated permission file. Complete JavaScript suite, desktop Clippy and formatting then passed.
- Superseded release run 35604844730 was cancelled before publication. Replacement release run: https://github.com/tomasmarekk/cxweb/actions/runs/35605062591 (pending at this observation). Do not claim 0.1.1 publication until GitHub confirms success and assets.
- v0.1.0 is published. The user's existing runtime and signed-in profile were not replaced or cleared during this patch. Signed distribution, automatic updates and full V1 qualification remain incomplete.

### 0.1.1 publication confirmed

- Release run 35605062591 completed successfully and published v0.1.1 from 5c34e49aea2353e25c242a23e656ab63d02968ae. The release has exactly the expected installer and checksum assets.
- Downloaded the published 15,343,603-byte installer and verified SHA-256: 70fedeaac1fcf91add9248fda67dafbd5362a4db7e30b081e46e83f514ada08a. Authenticode reports NotSigned, as disclosed in release notes.
- Private download receipt: .local/github-release-v0.1.1/verification.json. This patch release has not replaced the user's running private runtime or cleared their session.

### Published installer upgrade verified locally

- Installed the actual downloaded v0.1.1 release using its silent update mode after the existing host reported idle/ready and prepare-uninstall --check passed.
- All three installed executable hashes match the extracted CI installer payload. Installation exited with code 0.
- Codex config.toml hash and the private daemon process IDs remained unchanged. Private IPC health remained ready. Reopened the installed desktop executable; no login or session deletion was performed.
- Local verification receipt was updated under .local/github-release-v0.1.1/verification.json. The private host remains its existing version; replacing the desktop payload does not hot-swap running hosts.

## Portable contract CI

- Added the PRD-required Linux Rust contract job for the workspace excluding the Windows desktop package, and made Windows CI discover every JavaScript test using the same directories as release CI. This does not claim Linux desktop or browser support.
- First Linux run 35606854365 exposed Windows-only clock imports in portable gateway/native health code and a health accessor unavailable to WebSocket tests. Added UTC observations on non-Windows using the already locked time 0.3.55 package; existing Windows time behavior is retained. Gateway health is also compiled for tests on every platform.
- Local format check, clock contract test, all ten WebSocket tests, platform/runtime Clippy with warnings denied and the complete JavaScript suite passed. Linux verification of the fix remains pending the next CI run.

## Manual release collision fix

- User-triggered release 35608014150 built and tested successfully, then failed because the default v0.1.1 release already existed. Existing published assets were preserved.
- Manual release tags are now optional. Empty selects the next unused patch version; explicit collisions fail before compilation. Serialized release runs stage matching Cargo/Tauri/lock versions in the CI checkout without rewriting the repository or changing external dependencies.
- Artifacts include only the selected version's installer, checksum and source/version provenance. Publishing verifies the checksum before creating a new immutable release.
- Four release regression tests and the complete JavaScript suite passed. An isolated copy of the real source successfully staged v0.1.2 and passed locked offline Cargo metadata; external dependency entries were unchanged. CI publication verification follows the pushed fix.
- Separately, portability CI run 35607390918 passed on Windows and Linux; the Linux job ran 158 passing tests with no failures.

### Release allocation verified

- Run 35610482615 succeeded and published v0.1.2 from a1d71689c1d46bd8e105c2479aa8def64d1892bf. Downloaded installer SHA-256 matched 89b65d93444854b6e76e9adc26dab21cf24396add471afe4c4181451bc11bf71; provenance matched the source and staged version.

## Lossless tool-envelope transport and live App verification

- The reported task failed with E_TOOL_ENVELOPE_JSON. Its original raw response was no longer available, so its exact malformed bytes were not recovered. Replaced fragile Markdown-rendered plain JSON with one code block whose literal code text preserves escaping; plain JSON remains compatible. Multiple blocks and surrounding prose are rejected, and contextual Rust validation is unchanged.
- A temporary structural diagnostic established that the current ChatGPT code viewer nests PRE elements. Count outer blocks, excluding language/copy controls. The diagnostic captured only DOM structure and fixed labels, was removed from source, and is absent from the optimized deployed runtime.
- Accept the observed Codex App send_message_to_thread context shape without a call_id only for its exact namespace/name and string output. Preserve it in history without resolving any pending tool execution; arbitrary uncorrelated outputs remain rejected.
- Regression coverage includes quoted MCP JavaScript, paths, patches, nested code viewers, prose/multiple-block refusal and App context correlation. Complete JavaScript suite, cargo test --locked --workspace, cargo clippy --locked --workspace --all-targets -- -D warnings and the opt-in real Chrome answer_projection_preserves_dom_whitespace_inside_json_strings test passed.
- Deployed the optimized daemon to the existing idle private host with its scheduler restored and Codex configuration hash unchanged. Browser login was preserved. Binary SHA-256: fad13b54861689392a9300596ccd19ada90ceeb76c3ada19749d51cb6e785653.
- Actual user task 01a0c4d3-11ef-7d23-9202-87e17f765a58 completed in turn 01a0c4e8-46d1-7202-b23a-5e7a7050888f (120679 ms). The web model discovered tools, called Chrome MCP new_page and take_snapshot on the requested Migrolino page, and returned the requested Czech explanation. Earlier failed attempts are preserved. Post-run runtime health was ready with zero active web turns.
- This verifies the reported App/MCP task and shared transport, not all possible tool/model combinations or complete V1 qualification. Signed distribution and automatic updates remain incomplete.

## Ordinary installer upgrade preserves the connection

- The user installed v0.1.3 through the ordinary installer and reached removal_pending_restart. The Tauri default interactive replacement path invoked the previous uninstaller without /UPDATE, so its legitimate uninstall hook restored Codex configuration and closed the browser. Earlier installer verification used /UPDATE and missed this path.
- Pin the upstream Tauri CLI 2.11.2 NSIS template with source/hash/license provenance. Different-version replacement now uses the same in-place payload replacement as updater mode, retaining the existing PREINSTALL idle guard. Same-version explicit removal and standalone uninstall still disconnect normally.
- Added compiled NSIS fixtures executing the actual PageLeaveReinstall function: ordinary upgrade, downgrade decision, same-version reinstall and explicit removal. All 14 installer cases pass. The same suite against the original template fails specifically at ordinary-upgrade, proving the regression test detects the original defect.
- Local Windows installer packaging passed. Recovery tests passed (11, one opt-in browser test ignored) and all 34 installed UI tests passed. A transient Temporary Chat navigation failure during startup can now be explicitly retried through the existing recovery button; no message is resubmitted.
- Restored this user's installation-owned route from its private uninstall receipt, preserving all other current settings and retaining a private backup. Reused the original browser profile and qualification receipt. Live catalog and published upgrade checks follow; do not infer readiness solely from restored configuration.

### Signed-out surface detection after upgrade

- Direct observation of the owned browser showed a signed-out ChatGPT page with a visible English Log in button. The legacy data-testid/href selectors missed it, producing a startup timeout. Recognize exact visible English sign-in buttons outside message/article content, without clicking or reading credentials. New regression tests reject hidden buttons and quoted login text.
- Health mapping also mislabeled E_BACKGROUND_NAVIGATION as Temporary Chat navigation. Preserve the startup code and Check action; a dedicated regression verifies both codes stay distinct.
- All 157 JavaScript tests and nine health-related Rust tests passed. Temporary structural/screenshot diagnostics were removed from source and their opt-in marker was removed. Renewed account login is required for live verification; no claim that the existing session remains authenticated.

### Live recovery verified after renewed sign-in

- After the user signed in and closed the login window, recovery verified browser, account and all five reasoning choices. A restart of the optimized daemon preserved the session. CLI 0.155.1 returned the exact live text response with public reasoning events.
- A byte-identical copy of Codex App's packaged 0.155.0-alpha.9.2 backend loaded native and owned models and completed native read/apply_patch with real file and final-answer checks. Direct spawning inside WindowsApps was denied; the packaged binary was copied unchanged for this test. The GUI picker was not directly observed. Both probes preserved configuration and executable hashes. Post-test health was ready, with both clients healthy and no active work.
- Release 35633799632 succeeded and published the installer fixes. Installation of that downloaded artifact was superseded by the user's next timeout report before the final local upgrade check.

## Remove generation time limits

- The Pro task 01a0c525-fad8-7c81-8505-db460bf7a999 failed with E_GENERATION_TIMEOUT after an initial completed tool call. The coordinator terminated responses after five minutes without new answer text, independent of visible ongoing reasoning, and also enforced a thirty-minute total limit.
- At the user's explicit request, remove both generation limits. Browser/protocol failure detection, scope validation, explicit cancellation, single-submission behavior and cleanup remain active. Long generation is not automatically retried.
- All 16 coordinator tests passed, including virtual-clock regressions covering six hours of unchanged text followed by successful completion, and six hours followed by explicit cancellation with exactly one submission/stop/release. Runtime Clippy with warnings denied passed. This is deterministic long-duration simulation, not a six-hour live generation claim.

## Preserve web generation across native WebSocket expiry

- Task 01a0c53e-b3bf-7aa3-9b95-1c58e6fd4f1f completed its first tool request, then ended with E_REQUEST_ALREADY_ADMITTED. Its later ledger entries were cancelled. The error is duplicate-submission protection after an interrupted request, not evidence that another browser submission is safe. Original native close frames were not retained.
- Found and fixed a concrete transport cancellation path: an idle native upstream close, transport error or error event cancelled the independent active web response. Detach the expired native peer, retain the client socket and continuation context, and continue the same single browser submission. Keep explicit client cancellation and rejection of unrelated native response injection.
- Removed the remaining 35-minute wrapper around owned WebSocket generation. Native realtime idle handling remains separate; web generation has no duration cap.
- Real local socket regressions cover native TCP disconnection, native error events, continuation and ping/pong after detachment, plus six simulated hours before successful completion. All 13 WebSocket tests passed; runtime Clippy with warnings denied passed. The complete runtime suite before the final ping/error coverage passed 242 tests with 11 opt-in tests ignored.
- Corrected earlier installer evidence: the downloaded v0.1.4 ordinary /P installation without /UPDATE did complete with exit code 0, unchanged configuration hash and unchanged daemon PID; its private receipt is dated 2026-09-21T18:21:30Z.

## Context budget recovery for long tool-heavy tasks

- Codex task 01a0c5a9-3349-79c3-93c3-1758e2f8f882 accumulated many large tool outputs and failed repeatedly with E_CONTEXT_BUDGET. The exact rejected prompt bytes were not retained, so the exact first oversized item is unknown. The runtime's 384 KiB normal prompt threshold left only 128 KiB for the full-history compaction request; once that request exceeded 512 KiB, the provider rejected it before browser submission with a plain HTTP error and the task could not recover.
- Lower the advertised and enforced normal encoded prompt budget to 256 KiB (64 KiB local token estimate) and allow a 1 MiB full-history compaction request. This triggers recovery earlier in new tasks and allows an older oversized task to request one checkpoint without dropping any history. The limit remains a local byte guardrail, not a claim of ChatGPT's context capacity or usage. Irreducible user, policy, schema and pending-tool content still fail explicitly. No tool result is truncated or silently replayed.
- The full codex-adapter and runtime test suites passed (44 adapter unit tests; 243 runtime tests with 11 opt-in cases ignored), including a 700 KiB summarizable history and an oversize refusal, as did runtime Clippy with warnings denied, format and diff checks. The affected live Codex task has not yet been retried on this build.

### Browser submission ceiling aligned with context recovery

- A live continuation of the affected SEO task on the first patch reached the compaction path but failed after about nine seconds with E_SUBMISSION_UNCERTAIN. Inspection found a second, older 512 KiB pre-insertion guard in the browser adapter. The new 1 MiB compaction prompt could pass the coordinator and then fail at this guard before Send was clicked; the coordinator conservatively classified every submit failure as uncertain. The exact attempted prompt length was not retained, so the guard mismatch is the concrete failure path, not a claim about exact browser content.
- Share the codex-adapter maximum prompt constant with the browser adapter. The browser can now insert any prompt the coordinator already accepted. Existing single-click, attribution and no-resubmit rules remain intact. The stale release run 35785390508 was cancelled before publication so its incomplete fix is not shipped.
- All 15 browser adapter tests passed (nine opt-in Chrome tests ignored), with browser adapter Clippy warnings denied, format and diff checks. A new live continuation and release verification follow.

### Release and active live continuation

- Release run 35786089906 succeeded and published v0.1.7 from source commit 8ea2f6b6d3367d029dfc1559146109c8e989ca82. The downloaded installer SHA-256 matched its published checksum: 557120c2643d259e073d05cd1dd9e7072da5a620836b302b4be83f869a84b206. The installer has not been run locally during active user web tasks.
- Deployed optimized daemon SHA-256 c9140767faaa1956bb8fc156631bed73aaf30e58d4f09954cdc7c8e99b767859 to the existing idle owner, preserving configuration and browser profile. Passive browser, account and model health recovered after restart.
- A live continuation of task 01a0c5a9-3349-79c3-93c3-1758e2f8f882 passed the former immediate E_CONTEXT_BUDGET and pre-insertion E_SUBMISSION_UNCERTAIN failures. Read-only ledger metadata for the same native session records several completed web requests after the earlier uncertain attempt and a subsequent active generation. The entire Codex task is still in progress, so a final result and successful tool continuation are not yet claimed.

### Explicit ChatGPT generation failure during the SEO task

- A read-only inspection of the owned, offscreen ChatGPT window on 2026-09-23 showed the exact visible status "Thinking failed" for the still-active request in task 01a0c5a9-3349-79c3-93c3-1758e2f8f882. Codex still showed that task in progress with no new output, and the private ledger still marked the web turn generating. This is evidence of a failed ChatGPT generation and an cxweb observation gap, not a successful long-running response. The model's reason for failure is unknown.
- The browser adapter now recognizes that exact visible status after confirming the new user turn, excluding the user's quoted text, hidden elements and historical assistant cards. The turn tracker ends it with `E_CHATGPT_THINKING_FAILED` and the native HTTP response preserves that terminal code. No automatic retry or generation time limit is introduced.
- The existing installed daemon still owns the active failed turn. Replacing it while that turn is active would interrupt an admitted request, so this source fix does not claim to repair or complete the original task retroactively.
- Commit 9b9b431 was pushed to main. Release workflow 35821305830 passed all checks and published v0.1.8. The downloaded installer SHA-256 `2a5e14b6879e958c53f1cf5ec0044cb354015af225db16f34a3b25161e492830` matched its published checksum. The installer was not run locally while the original task remained active; a later read-only task poll still showed that turn in progress with no output.

### Recover an explicit ChatGPT response failure

- The reference implementation treats visible terminal ChatGPT response errors as retryable service failures, retires the failed browser surface and bounds retries to three after the initial attempt. It also checks browser message and account context limits before sending. This is behavioral evidence only; no reference code was copied. Its limits do not establish the cause of the SEO task's `Thinking failed` screen. The last observed screen showed an ordinary response prompt, not a compaction prompt, and the failed prompt's exact size was not retained.
- cxweb now retries only the positively attributed `E_CHATGPT_THINKING_FAILED` path, at most three times, inside the same durable native admission. Each attempt gets a freshly prepared Temporary Chat page after confirmed cleanup of the failed page. The prompt and nonce remain bound to the same native request, and only a validated final answer/tool envelope is delivered. Existing no-time-limit behavior remains unchanged. Uncertain Send outcomes, browser scope changes, malformed output, cancellation and failed cleanup never trigger another Send.
- The retry carries the same response ID, timestamp, public-summary prefix and event sequence across attempts; already-streamed reasoning cannot be duplicated or rewritten. Mock-browser regressions cover one delivered tool call after a recovered failure, exhausted attempts, public status continuity, cancellation between attempts, failed cleanup and an uncertain retry Send. These are deterministic contract tests, not evidence that the real ChatGPT site always recovers or that the old stuck SEO task completed.
