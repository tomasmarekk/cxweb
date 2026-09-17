# cxweb implementation status

## Acceptance and current gate

Target: the complete supplied PRD, not a substitute chat application. No subagents.
Current gate: **G0 IN PROGRESS**. No client or browser combination is certified.
User login and actual App/CLI picker checks remain NOT RUN. Production integration
must remain disabled until the relevant evidence exists.

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
