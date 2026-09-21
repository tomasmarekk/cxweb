# Windows release

In GitHub, open **Actions → Windows release → Run workflow**. Select the source
branch, enter its version as a tag (initially `v0.1.0`), and keep **Mark this release
as a preview** enabled for development builds. The workflow runs Rust and
JavaScript checks, builds the per-user installer, tests its lifecycle hooks, and
publishes the installer and SHA-256 file to GitHub Releases. The build artifact is
also retained for 30 days, even if the later publishing job fails.

For a new version, update both `[workspace.package].version` in `Cargo.toml` and
`version` in `apps/desktop/src-tauri/tauri.conf.json`, refresh `Cargo.lock`, commit,
then run the workflow with the corresponding new tag. Existing releases and tags
are never overwritten. The release points at the exact commit selected when the
workflow was dispatched.

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

The main payload contains sibling `cxweb-desktop.exe`, `cxweb-daemon.exe`, and
`cxweb.exe`. Dedicated runtime installations remain outside this directory, so
replacing the GUI payload does not replace a running host. Uninstall calls
`cxweb prepare-uninstall`; it never silently cancels active web tasks. The
`--check` variant is read-only. Existing clients' native compatibility listeners
and private profile data are retained. Managed browser delivery, profile deletion,
and signed automatic runtime updates are not provided by this preview installer.
