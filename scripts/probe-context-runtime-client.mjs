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
assert.ok([
  'eba0f32c976667cb9298efafd98513e823eeda7b576a03ec658bb8be8d336316',
  'bc45017e8239dc150258f69309ced9df6bbcdf5b8e4f346decf780ac0999e226',
].includes(fingerprint), 'only a reviewed native backend may run');
const descriptor = JSON.parse(await readFile(descriptorPath, 'utf8'));
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
const env = { ...process.env };
for (const key of Object.keys(env)) if (/^(CODEX_|OPENAI_|CHATGPT_)/i.test(key)) delete env[key];
env.CODEX_HOME = home;
env.OPENAI_API_KEY = 'cxweb-synthetic-runtime-context-probe';
const client = spawn(executable, ['app-server', '--listen', 'stdio://'], { env, cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
const exited = new Promise(resolve => client.once('exit', resolve));
const pending = new Map(); const notifications = []; let id = 0;
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
const report = { synthetic: true, real_browser: false, executable_sha256: fingerprint, expected_local_error: expectedError, turns: [], context_errors: 0, local_error_visible: false, result: 'INCOMPLETE' };
try {
  await rpc('initialize', { clientInfo: { name: 'cxweb_runtime_context_probe', version: '0.1.0' }, capabilities: { experimentalApi: true } });
  client.stdin.write(JSON.stringify({ method: 'initialized', params: {} }) + '\n');
  const thread = await rpc('thread/start', { cwd, model: 'webbridge/test', ephemeral: true });
  for (let index = 0; index < (terminalRefusal ? 2 : 4); index++) {
    const turn = await rpc('turn/start', { threadId: thread.thread.id, input: [{ type: 'text', text: `Synthetic runtime context turn ${index}.`, text_elements: [] }] });
    let completed;
    for (let poll = 0; poll < 600; poll++) {
      completed = notifications.find(n => n.method === 'turn/completed' && n.params?.turn?.id === turn.turn.id);
      if (completed) break;
      await delay(50);
    }
    assert.ok(completed, 'native turn reaches a terminal notification');
    report.turns.push(completed.params.turn.status);
    if (completed.params.turn.error?.codexErrorInfo === 'contextWindowExceeded') report.context_errors++;
    if (expectedError && completed.params.turn.error?.message?.includes(expectedError)) report.local_error_visible = true;
  }
  if (terminalRefusal) {
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
  report.result = 'FAIL';
  report.error = error.code === 'ERR_ASSERTION' ? error.message.split('\n')[0] : 'E_RUNTIME_CONTEXT_PROBE';
  process.exitCode = 1;
} finally {
  for (const entry of pending.values()) clearTimeout(entry.timer);
  client.kill(); await exited;
}
report.executable_unchanged = createHash('sha256').update(await readFile(executable)).digest('hex') === fingerprint;
if (!report.executable_unchanged) process.exitCode = 1;
await writeFile(reportPath, JSON.stringify(report, null, 2) + '\n');
