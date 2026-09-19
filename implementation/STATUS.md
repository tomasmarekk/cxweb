# cxweb implementation status

## Acceptance and current gate

Target: the complete supplied PRD, not a substitute chat application. No subagents.
Current gate: **G0 IN PROGRESS**. No client or browser combination is certified.
Manual login and managed reuse of the saved session are user-confirmed. Scoped
background text generation and native read/apply_patch cycles now pass through
both actual native backends. The actual CLI picker passed isolated synthetic and
authenticated browser round trips, including its structured auxiliary request.
Actual App picker, native subscription coexistence, broader coding/model
qualification and release gates remain incomplete. Production integration remains
disabled.

Earlier manual check: the user reports `ChatGPT: Session detected` and
`Codex connection: Awaiting verification` after checking the desktop status.
This confirms the login controller recognized the saved session in this run;
it does not establish the selected account/workspace, model inventory, Temporary
Chat behavior or a successful generation. After restarting Codex App, its real
picker still contained native entries only: Default, GPT-6 Astra, GPT-5.6 Sol,
GPT-5.6 Terra, GPT-5.6 Luna and GPT-5.5, with GPT-5.6 Sol selected. No owned cxweb
entry was visible, which is the expected evidence while activation remains absent.

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
