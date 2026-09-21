Windows x64 preview of cxweb, connecting ChatGPT web models to Codex App and Codex CLI.

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

Uninstall restores cxweb-owned Codex configuration when web work is idle. Private
session data and native compatibility runtimes are retained for already open
clients. Restart those clients after disconnecting. Unavailable hosts or active
web work stop removal rather than leaving an unverified configuration behind.
