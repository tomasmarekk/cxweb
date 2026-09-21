Windows x64 preview of cxweb, connecting ChatGPT web models to Codex App and Codex CLI.

Fixes tool-envelope transport failures during real Codex App tasks. A single JSON
code block preserves quoted MCP arguments, Windows paths and patch text through
ChatGPT's renderer, including its nested code viewer. Envelope, nonce and tool
schema validation remain enforced. Codex App cross-task context is also retained
without treating it as an executed tool result.

Verified in an existing Codex App task: the web model discovered Chrome MCP tools,
opened the requested page, read its snapshot and completed the answer.

Manual releases now select the next unused version automatically. Release assets
include provenance linking the packaged version to its source commit and CI run.

This update closes the dedicated browser when disconnecting and adds optional
local ChatGPT session removal in the app and uninstaller. Session removal refuses
active profiles and connected Codex homes. Native forwarding for already open
clients remains available after disconnecting.

Download the `-setup.exe` installer. It installs for the current Windows user and
includes the desktop app, background runtime, CLI support, and the WebView2
bootstrapper. No Rust, Node.js, or Python installation is required to run it.
This preview uses an existing Chrome or Edge installation with a dedicated cxweb
profile. Sign in through cxweb; the login window can be closed afterward.

Select **ChatGPT Web · Latest** in Codex and choose the available reasoning effort.
The local Codex client executes tools and enforces its normal permissions.

The installer is currently unsigned. The accompanying SHA-256 file checks download
integrity; it does not replace publisher signing. This is a Windows preview, not a
claim of complete V1 qualification or a signed automatic update channel.

Uninstall restores cxweb-owned Codex configuration when web work is idle. It
offers to clear the dedicated local ChatGPT profile; keeping the profile is the
default. The same option is available in cxweb after all connections are removed.
Codex sign-in and personal browser profiles are preserved; remote logout and
ChatGPT server-side deletion are not performed. Native compatibility runtimes
are retained for already open clients. Restart those clients after disconnecting.
Unavailable hosts or active web work stop removal rather than leaving an
unverified configuration behind.
