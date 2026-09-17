# cxweb implementation status

## Acceptance and current gate

Target: the complete supplied PRD, not a substitute chat application. No subagents.
Current gate: **G0 IN PROGRESS**. No client or browser combination is certified.
Manual login and managed reuse of the saved session are user-confirmed. Account/
model qualification and actual App/CLI picker checks remain incomplete. Production
integration must remain disabled until the relevant evidence exists.

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

Build and run the isolated client harness against both backend binaries. Record
model/list and provider request evidence separately from actual renderer/picker
evidence. Inspect desktop backend exact source/schema, then implement the native
mock upstream and private browser transport. Do not report G0 complete from an
app-server listing alone.

## Additional implementation and evidence

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
