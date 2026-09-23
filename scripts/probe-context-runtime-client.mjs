// Called by the opt-in Rust integration test; no real home, browser or tools.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join } from 'node:path';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';

const [executable, descriptorPath, reportPath] = process.argv.slice(2);
assert.equal(process.argv.length, 5);
assert.ok([executable, descriptorPath, reportPath].every(isAbsolute));
const fingerprint = createHash('sha256').update(await readFile(executable)).digest('hex');
const descriptor = JSON.parse(await readFile(descriptorPath, 'utf8'));
const delayed = descriptor.delayed_response === true;
const automatic = descriptor.automatic_context === true;
const terminalRefusal = descriptor.terminal_refusal === true;
const expectedError = descriptor.expected_error ?? null;
assert.ok(!terminalRefusal || ['E_MODEL_FIDELITY', 'E_SUBMISSION_UNCERTAIN'].includes(expectedError));
assert.equal(new URL(descriptor.base_url).hostname, '127.0.0.1');
const root = dirname(descriptorPath);
const home = join(root, 'home'); const cwd = join(root, 'workspace');
await mkdir(home); await mkdir(cwd);
const catalogPath = join(home, 'catalog.json');
await writeFile(catalogPath, JSON.stringify(descriptor.catalog));
await writeFile(join(home, 'config.toml'), `openai_base_url = ${JSON.stringify(descriptor.base_url)}\nmodel_catalog_json = ${JSON.stringify(catalogPath.replaceAll('\\', '/'))}\n`);
// The built-in provider cannot be overridden. This isolated synthetic probe
// uses a custom provider solely to shorten the native stream watchdog.
if (delayed) {
  await writeFile(join(home, 'config.toml'), `openai_base_url = ${JSON.stringify(descriptor.base_url)}\nmodel_catalog_json = ${JSON.stringify(catalogPath)}\nmodel_provider = "cxweb_delay_fixture"\n[model_providers.cxweb_delay_fixture]\nname = "Local delay fixture"\nsupports_websockets = true\nbase_url = ${JSON.stringify(descriptor.base_url)}\nwire_api = "responses"\nstream_idle_timeout_ms = 20000\nstream_max_retries = 0\nrequest_max_retries = 0\n`);
}
const env = { ...process.env };
for (const key of Object.keys(env)) if (/^(CODEX_|OPENAI_|CHATGPT_)/i.test(key)) delete env[key];
env.CODEX_HOME = home;
env.OPENAI_API_KEY = 'cxweb-synthetic-runtime-context-probe';
const client = spawn(executable, ['app-server', '--listen', 'stdio://'], { env, cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
const exited = new Promise(resolve => client.once('exit', resolve));
const pending = new Map(); const notifications = []; let id = 0; let phase = 'initialize';
createInterface({ input: client.stdout }).on('line', line => {
  let message; try { message = JSON.parse(line); } catch { return; }
  if (pending.has(message.id)) {
    const entry = pending.get(message.id); pending.delete(message.id); clearTimeout(entry.timer);
    if (message.error) entry.reject(new Error('E_RUNTIME_PROBE_RPC')); else entry.resolve(message.result);
  } else if (message.method) {
    notifications.push(message);
    if (message.id !== undefined) client.stdin.write(JSON.stringify({ id: message.id, error: { code: -32601, message: 'No actions are authorized' } }) + '\n');
  }
});
const rpc = (method, params) => new Promise((resolve, reject) => {
  const key = ++id;
  const timer = setTimeout(() => { pending.delete(key); reject(new Error('E_RUNTIME_PROBE_TIMEOUT')); }, 20000);
  pending.set(key, { resolve, reject, timer });
  client.stdin.write(JSON.stringify({ id: key, method, params }) + '\n');
});
const report = { synthetic: true, real_browser: false, executable_sha256: fingerprint, expected_local_error: expectedError, turns: [], native_errors: [], delayed_response: delayed, configured_idle_ms: delayed ? 20000 : null, simulated_response_delay_ms: delayed ? 35000 : null, context_errors: 0, local_error_visible: false, result: 'INCOMPLETE' };
try {
  await rpc('initialize', { clientInfo: { name: 'cxweb_runtime_context_probe', version: '0.1.0' }, capabilities: { experimentalApi: true } });
  client.stdin.write(JSON.stringify({ method: 'initialized', params: {} }) + '\n');
  phase = 'config';
  if (delayed) {
    const { config } = await rpc('config/read', { includeLayers: false, cwd });
    assert.equal(config.model_provider, 'cxweb_delay_fixture');
    const provider = config.model_providers?.cxweb_delay_fixture;
    assert.equal(provider?.stream_idle_timeout_ms, 20000);
    assert.equal(provider?.stream_max_retries, 0);
    assert.equal(provider?.supports_websockets, true);
  }
  phase = 'thread_start';
  const thread = await rpc('thread/start', { cwd, model: 'webbridge/test', ephemeral: true });
  phase = 'turns';
  for (let index = 0; index < (delayed ? 1 : terminalRefusal ? 2 : 4); index++) {
    const turn = await rpc('turn/start', { threadId: thread.thread.id, input: [{ type: 'text', text: `Synthetic runtime context turn ${index}.`, text_elements: [] }] });
    let completed;
    for (let poll = 0; poll < 1200; poll++) {
      completed = notifications.find(n => n.method === 'turn/completed' && n.params?.turn?.id === turn.turn.id);
      if (completed) break;
      await delay(50);
    }
    assert.ok(completed, 'native turn reaches a terminal notification');
    report.turns.push(completed.params.turn.status);
    const nativeError = completed.params.turn.error?.codexErrorInfo;
    if (nativeError) {
      const tag = typeof nativeError === 'string' ? nativeError : Object.keys(nativeError)[0];
      report.native_errors.push(['contextWindowExceeded','badRequest','responseStreamDisconnected','responseTooManyFailedAttempts'].includes(tag) ? tag : 'other');
    }
    if (completed.params.turn.error?.codexErrorInfo === 'contextWindowExceeded') report.context_errors++;
    if (expectedError && completed.params.turn.error?.message?.includes(expectedError)) report.local_error_visible = true;
  }
  if (automatic) {
    assert.deepEqual(report.turns, ['completed', 'completed', 'completed', 'completed'], 'overflow is summarized inside the original request');
    assert.equal(report.context_errors, 0);
  } else if (delayed) {
    assert.deepEqual(report.turns, ['completed'], 'buffered response survives a shorter native stream idle limit');
    assert.equal(report.context_errors, 0);
  } else if (terminalRefusal) {
    assert.deepEqual(report.turns, ['failed', 'completed'], 'terminal refusal then a new successful turn');
    assert.equal(report.context_errors, 0, 'a terminal local failure does not pretend context is full');
    assert.equal(report.local_error_visible, true, 'native failure preserves the specific local error');
  } else {
    assert.equal(report.context_errors, 1, 'one native context failure');
    assert.equal(report.turns.filter(status => status === 'completed').length, 3, 'three completed turns');
    assert.equal(report.turns.at(-1), 'completed', 'native continuation completes after compaction');
  }
  assert.ok(!notifications.some(n => n.method === 'item/started' && ['commandExecution', 'fileChange'].includes(n.params?.item?.type)), 'no native tools executed');
  report.result = 'PASS actual native client through cxweb runtime';
} catch (error) {
  report.result = 'FAIL'; report.failed_phase = phase;
  report.error = error.code === 'ERR_ASSERTION' ? error.message.split('\n')[0] : 'E_RUNTIME_CONTEXT_PROBE';
  process.exitCode = 1;
} finally {
  for (const entry of pending.values()) clearTimeout(entry.timer);
  client.kill(); await exited;
}
report.executable_unchanged = createHash('sha256').update(await readFile(executable)).digest('hex') === fingerprint;
if (!report.executable_unchanged) process.exitCode = 1;
await writeFile(reportPath, JSON.stringify(report, null, 2) + '\n');
