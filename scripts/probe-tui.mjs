// Actual native terminal UI with a disposable home and a local gateway.
// --live uses the saved cxweb browser session and consumes ChatGPT allowance.
// Run in a PTY. No native account, home, configuration or history is reused.
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises';
import { resolve, join, isAbsolute } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';

const executable = process.argv[2];
const live = process.argv[3] === '--live';
if (!executable || !isAbsolute(executable) || process.argv.length !== (live ? 4 : 3)) {
  throw new Error('Usage: node scripts/probe-tui.mjs <absolute codex executable> [--live]');
}
const root = resolve('.local/probes');
await mkdir(root, { recursive: true });
const work = await mkdtemp(join(root, 'tui-'));
const home = join(work, 'home'), cwd = join(work, 'workspace');
await mkdir(home); await mkdir(cwd);
const bridge = resolve('target/debug/cxweb.exe');
const descriptor = live ? join(work, 'runtime', 'connection.json') : join(work, 'connection.json');
const identityPath = join(work, 'identity.json');
const server = spawn(bridge, live ? ['live-probe', '--output', descriptor] : ['probe', '--output', descriptor, '--identity-output', identityPath], {
  windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
});
let serverOutput = '';
server.stdout.on('data', chunk => { if (serverOutput.length < 16000) serverOutput += chunk; });
server.stderr.on('data', () => {});
const serverExit = new Promise(resolve => server.once('close', resolve));
const fingerprint = async () => createHash('sha256').update(await readFile(executable)).digest('hex');
const evidence = {
  schema: 'cxweb.tui-probe.v1',
  client: execFileSync(executable, ['--version'], { encoding: 'utf8', windowsHide: true }).trim(),
  executableSha256: await fingerprint(),
  authentication: live ? 'saved cxweb browser session; synthetic native key restricted to loopback' : 'synthetic key restricted to local diagnostic server',
  actualPicker: 'requires independent observation of the terminal UI',
  catalogStrategy: 'isolated static diagnostic catalog', synthetic: !live,
};
let client;
try {
  let connection;
  for (let i = 0; i < (live ? 2400 : 100); i++) {
    if (server.exitCode !== null) throw new Error('E_PROBE_START');
    try { connection = JSON.parse(await readFile(descriptor, 'utf8')); break; } catch { await delay(50); }
  }
  assert.ok(connection?.base_url?.endsWith('/backend-api/codex'));
  const catalog = live ? JSON.stringify(connection.catalog) : execFileSync(bridge, ['probe-catalog'], { windowsHide: true });
  await writeFile(join(home, 'catalog.json'), catalog);
  await writeFile(join(home, 'config.toml'), `openai_base_url = ${JSON.stringify(connection.base_url)}\nmodel_catalog_json = ${JSON.stringify(join(home, 'catalog.json').replaceAll('\\', '/'))}\n`);
  const env = { ...process.env };
  for (const key of Object.keys(env)) if (/^(CODEX_|OPENAI_|CHATGPT_)/i.test(key)) delete env[key];
  env.CODEX_HOME = home;
  env.OPENAI_API_KEY = 'cxweb-synthetic-not-a-real-key';
  if (!env.TERM || env.TERM === 'dumb') env.TERM = 'xterm-256color';
  // The interactive first-run screen requires an auth receipt even when the
  // backend accepts the environment key. Use the supported CLI login in this
  // disposable home, with the same synthetic key and local-only destination.
  execFileSync(executable, ['login', '--with-api-key'], {
    env, cwd, windowsHide: true, input: env.OPENAI_API_KEY + '\n', stdio: ['pipe', 'pipe', 'pipe'],
  });
  console.log(live ? 'Isolated live terminal probe: inspect /model, select the observed ChatGPT Web route, test the fixed response, then exit.' : 'Isolated terminal probe: inspect /model, select ChatGPT Web · Diagnostic, send a synthetic test, then exit.');
  client = spawn(executable, ['--no-alt-screen', '-C', cwd], { env, cwd, windowsHide: true, stdio: 'inherit' });
  evidence.clientExitCode = await new Promise((resolve, reject) => {
    client.once('error', reject); client.once('exit', resolve);
  });
  if (!live) {
    evidence.identity = JSON.parse(await readFile(identityPath, 'utf8'));
    assert.equal(evidence.identity.verified_native_identity, true);
    assert.equal(evidence.identity.raw_identifiers_recorded, false);
  }
  // Inspect only this disposable client's own transcript, retaining no content.
  const expected = live ? 'cxweb live terminal round-trip succeeded' : 'cxweb diagnostic round-trip succeeded';
  let matches = 0;
  const sessions = join(home, 'sessions');
  for (const relative of await readdir(sessions, { recursive: true })) {
    if (!relative.endsWith('.jsonl')) continue;
    for (const line of (await readFile(join(sessions, relative), 'utf8')).split('\n')) {
      let item; try { item = JSON.parse(line); } catch { continue; }
      if (item.type === 'response_item' && item.payload?.role === 'assistant') {
        matches += (item.payload.content ?? []).filter(part => part.type === 'output_text' && part.text === expected).length;
      }
    }
  }
  assert.ok(matches > 0, 'native transcript contains the exact fixed assistant response');
  evidence.exactAssistantResponse = true;
  evidence.result = 'owned request observed at gateway; verify visible picker and response separately';
} catch {
  evidence.result = 'FAIL'; process.exitCode = 1;
} finally {
  client?.kill();
  if (live) {
    if (server.exitCode === null) server.stdin.end('stop\n');
    const exit = await Promise.race([serverExit, delay(65000, 'timeout', { ref: false })]);
    evidence.runtimeExitCode = exit;
    if (exit !== 0) { server.kill(); evidence.result = 'FAIL'; evidence.cleanupError = 'E_RUNTIME_EXIT'; process.exitCode = 1; }
    else {
      try {
        const report = JSON.parse(serverOutput);
        evidence.browserClosed = report.browser_closed === true;
        evidence.runtimeFailures = report.failures;
        evidence.optionalWebSearchRequests = report.optional_web_search_requests;
        assert.equal(report.browser_closed, true);
        assert.deepEqual(report.failures, []);
        evidence.lastMessageMatchedInFull = report.diagnostic.attribution.text_content_matches === 1;
      } catch { evidence.result = 'FAIL'; evidence.cleanupError = 'E_RUNTIME_REPORT'; process.exitCode = 1; }
    }
  } else { server.kill(); await serverExit; }
  evidence.executableUnchanged = await fingerprint() === evidence.executableSha256;
  if (!evidence.executableUnchanged) { evidence.result = 'FAIL'; process.exitCode = 1; }
  await writeFile(join(work, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n');
  console.log(JSON.stringify(evidence, null, 2));
  console.log(`Evidence: ${join(work, 'evidence.json')}`);
}
