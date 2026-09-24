Windows x64 preview of cxweb, connecting ChatGPT web models to Codex App and Codex CLI.

This update makes pending tool-call identity runtime-owned during compaction.
The model summarizes task state; cxweb preserves the exact unresolved native
calls and their arguments in the authenticated checkpoint. The model no longer
has to reproduce internal call IDs. Existing checkpoints remain readable, and
unknown, duplicate or mismatched tool results are still rejected.

Large compaction sources can now reach the existing staged summarizer before
the one-message browser ceiling is applied. Every submitted stage remains
bounded; this does not truncate history or increase the browser message limit.
It addresses the case where a large completed tool result caused an immediate
E_CONTEXT_BUDGET despite the history being suitable for staged compaction.

Within a running runtime, a later attempt can reuse already completed summary
stages for the same authenticated task and identical source. Appending history
recomputes the changed suffix instead of resending every earlier fragment.
Account/workspace/model/codec changes prevent reuse. The bounded cache does not
survive a runtime restart and never stores failed or uncertain submissions.

Qualification includes passing Rust workspace tests and installed App/CLI backend
checkpoint tests over HTTP and WebSocket with a synthetic browser. A live Codex
App WebSocket test also passed native file read, compaction, exact result recall,
native patch and execution of the test command: three actual native tools and
no provider failures. The following live CLI test was interrupted at compaction
by ChatGPT's temporary Too many requests restriction. The original long-running
App task is not yet verified end to end; this remains a preview.

The **Compaction model** selector in cxweb lets you choose from
the verified WebGPT models and reasoning levels offered by your account. For
example, the task can stay on Pro while new context summaries use Medium.
The preference is saved per connection and applies to both Codex App and CLI.
The default, **Same as task**, preserves existing behavior. A running compaction
finishes with its original choice; changing the preference does not cancel it.
Unavailable saved choices are reported explicitly rather than silently replaced.
New summaries use a direct JSON object in a code block, avoiding fragile double
JSON escaping; previously encoded summaries remain readable with the same strict
field and pending-tool validation.

Large context recovery splits historical content into bounded summarization
stages and preserves pending tool calls and their results. Installed connections
can recover an oversized request within that request instead of requiring another
user turn after a context-budget error. Summaries remain bound to the original
task, account, workspace and model even when another model performs compaction.
A slow browser observation no longer imposes a five-second overall deadline on
Pro generation. Timed-out observation reads keep watching the same submission,
without resending or cancelling it. A disconnected browser pipe is distinguished
from a slow read and remains a terminal error. There is no absolute generation
or inactivity timeout.

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

Native App follow-up messages now remain exact across automatic compaction.
Completed summary prefixes are reused when only pending tool state changes;
the final checkpoint attaches exact native call identities and arguments.
Current tool schemas remain unchanged in continuation and no longer consume
summary stages. Provider rate limits are still honored.
