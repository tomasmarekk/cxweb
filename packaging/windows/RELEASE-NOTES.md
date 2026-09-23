Windows x64 preview of cxweb, connecting ChatGPT web models to Codex App and Codex CLI.

This release adds a **Compaction model** selector directly in cxweb. Choose from
the verified WebGPT models and reasoning levels offered by your account. For
example, the task can stay on Pro while new context summaries use Medium.
The preference is saved per connection and applies to both Codex App and CLI.
The default, **Same as task**, preserves existing behavior. A running compaction
finishes with its original choice; changing the preference does not cancel it.
Unavailable saved choices are reported explicitly rather than silently replaced.

Large context recovery splits historical content into bounded summarization
stages and preserves pending tool calls and their results. Installed connections
can recover an oversized request within that request instead of requiring another
user turn after a context-budget error. Summaries remain bound to the original
task, account, workspace and model even when another model performs compaction.
A slow browser observation no longer imposes a five-second overall deadline on
Pro generation. There is no absolute generation or inactivity timeout.

Installer upgrades now stage the new daemon into existing private runtime
installations while preserving configuration, login state and active processes.
**If the installer requests a Windows restart, restart after your current work
finishes to activate the staged runtime.** The installer does not kill ongoing
model requests. A failed private-runtime update is reported explicitly.

Verification includes Rust workspace tests, desktop behavior tests, an isolated
Windows executable replacement and scheduler test, and installed App/CLI backend
HTTP/WebSocket context-recovery fixtures. These fixtures verify transport and
routing; they are not a claim that every live ChatGPT task is qualified. The
long Pro compaction already in progress is not accelerated retroactively.

Download the `-setup.exe` installer. It includes the desktop app, background
runtime, CLI support and WebView2 bootstrapper. No Rust, Node.js or Python is
required to run it. An existing Chrome or Edge installation is used with a
separate cxweb profile. Sign in through cxweb, then close the login window.
Select **ChatGPT Web · Latest** and the desired reasoning effort in Codex.
Codex executes tools and enforces its normal permissions.

The installer is unsigned. The accompanying SHA-256 file verifies download
integrity, not publisher identity. This is a preview, not complete V1 qualification.
