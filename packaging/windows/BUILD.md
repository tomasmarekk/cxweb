# Windows release

In GitHub, open **Actions → Windows release → Run workflow**. Select the source
branch, leave the tag empty to select the next available version, and keep **Mark this release
as a preview** enabled for development builds. The workflow runs Rust and
JavaScript checks, builds the per-user installer, tests its lifecycle hooks, and
publishes the installer and SHA-256 file to GitHub Releases. The build artifact is
also retained for 30 days, even if the later publishing job fails.

No source edit is required for each manual release. The workflow selects the source
version if it is unused, or the next patch version after existing release tags.
For a deliberate version, enter a new tag such as `v0.2.0`; an existing tag is
rejected before compilation. Release runs are serialized to avoid version collisions.
Existing releases and tags are never overwritten.

The chosen version is staged in Cargo and Tauri manifests and the workspace lock
entries only inside the CI checkout. External dependency changes are rejected.
The release tag points at the selected source commit; the published
`release-provenance.json` records that commit, the original source version, the
staged release version and the workflow run. The repository is not rewritten by CI.

Local build on Windows x64:

```powershell
cargo install tauri-cli --version 2.11.2 --locked
./scripts/build-windows.ps1
./scripts/test-windows-installer.ps1
```

The installer is under `target/release/bundle/nsis`. Tauri's official pinned CLI
provides NSIS packaging and embeds Microsoft's WebView2 bootstrapper. CI verifies
the CLI archive against its pinned SHA-256. No signing credential is currently
configured; the installer is explicitly distributed as unsigned.

The checked-in NSIS template is from Tauri CLI 2.11.2, licensed under MIT
(see TAURI-LICENSE-MIT and TAURI-LICENSE.spdx). Upstream source:
https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.2/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi
The original SHA-256 is ee84148e405adc4d736a46456dd8345a644751bd1f28a335dd7fd833a32d7c3e.
The local change replaces a different version in place, including ordinary
interactive upgrades, instead of running the old uninstaller. Existing
PREINSTALL idle checks still run before file replacement. Same-version explicit
uninstall and standalone uninstall retain normal removal behavior. Lifecycle
tests compile and execute the actual template's replacement decision as well
as the hooks; testing only /UPDATE does not cover interactive upgrades.

The main payload contains sibling `cxweb-desktop.exe`, `cxweb-daemon.exe`, and
`cxweb.exe`. Dedicated runtime installations remain outside this directory, so
replacing the GUI payload does not replace a running host. Uninstall calls
`cxweb prepare-uninstall`; it never silently cancels active web tasks. The
`--check` variant is read-only. Existing clients' native compatibility listeners
are retained. Profile deletion is optional during interactive uninstall; silent
uninstall keeps it unless `/CLEARSESSION` is supplied. `/UPDATE` always preserves
the session. After removing all connections, **Clear local ChatGPT session** in
cxweb or `cxweb clear-local-session` performs the same scoped deletion. A busy
profile or uncertain connection ownership blocks deletion. This clears local
sign-in data, not remote sessions or ChatGPT server records. Managed browser
delivery and signed automatic runtime updates are not provided by this preview.
