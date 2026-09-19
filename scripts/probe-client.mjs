// Isolated native client. --live uses the already authenticated cxweb browser;
// native auth files and the user's native configuration are never accessed.
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import assert from 'node:assert/strict';

const executable = process.argv[2];
const live = process.argv.includes('--live');
if (!executable) throw new Error('Usage: node scripts/probe-client.mjs <absolute codex executable> [--live]');
const root = resolve('.local/probes');
await mkdir(root, { recursive: true });
const work = await mkdtemp(join(root, 'client-'));
const home = join(work, 'home');
const cwd = join(work, 'workspace');
await mkdir(home); await mkdir(cwd);
const bridge = resolve('target/debug/cxweb.exe');
const descriptor = live ? join(work, 'runtime', 'connection.json') : join(work, 'probe.json');
const server = spawn(bridge, live ? ['live-probe', '--output', descriptor] : ['probe', '--output', descriptor, '--tools-output', join(work, 'tools.json'), '--identity-output', join(work, 'identity.json')], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
let serverOutput = '', serverError = '';
server.stdout.on('data', data => { if (serverOutput.length < 16000) serverOutput += data; });
server.stderr.on('data', data => { if (serverError.length < 16000) serverError += data; });
const serverExit = new Promise(resolve => server.once('exit', (code, signal) => resolve({ code, signal })));
let client;
const evidence = { schema: 'cxweb.client-probe.v1', client: execFileSync(executable, ['--version'], { encoding: 'utf8', windowsHide: true }).trim(), synthetic: !live, actualPicker: 'NOT RUN', authentication: live ? 'saved cxweb browser session; synthetic native key restricted to loopback' : 'synthetic API key in isolated child environment', events: [] };
let stage = 'gateway startup';
try {
  let endpoint;
  let connection;
  for (let i = 0; i < (live ? 2400 : 100); i++) {
    if (server.exitCode !== null || server.signalCode !== null) throw new Error('E_PROBE_START');
    try { connection = JSON.parse(await readFile(descriptor, 'utf8')); endpoint = connection.base_url; break; } catch { await delay(50); }
  }
  assert.ok(endpoint, 'diagnostic gateway starts');
  stage = 'client initialization';
  assert.ok(endpoint.endsWith('/backend-api/codex'), 'probe uses the subscription-shaped product route');
  evidence.routeLayout = 'subscription backend-api/codex';
  const route = live ? connection.model : 'webbridge/diagnostic';
  const catalog = live ? JSON.stringify(connection.catalog) : execFileSync(bridge, ['probe-catalog'], { encoding: 'utf8', windowsHide: true });
  await writeFile(join(home, 'catalog.json'), catalog);
  // The first live qualification exercises text/function transport. Built-in
  // server search remains a separate compatibility gate; disable it explicitly
  // only in this disposable client, never by dropping incoming definitions.
  await writeFile(join(home, 'config.toml'), `openai_base_url = ${JSON.stringify(endpoint)}\nmodel_catalog_json = ${JSON.stringify(join(home, 'catalog.json').replaceAll('\\', '/'))}\n${live ? 'web_search = "disabled"\n' : ''}`);
  if (live) evidence.builtinWebSearch = 'disabled in isolated test client; production compatibility remains unqualified';
  // Sanitize inherited route/auth/home overrides. Only this subprocess sees the mock key.
  const env = { ...process.env };
  for (const key of Object.keys(env)) if (/^(CODEX_|OPENAI_|CHATGPT_)/i.test(key)) delete env[key];
  env.CODEX_HOME = home;
  env.OPENAI_API_KEY = 'cxweb-synthetic-not-a-real-key';
  client = spawn(executable, ['app-server', '--listen', 'stdio://'], { env, cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  const pending = new Map();
  const notifications = [];
  let id = 0;
  let stderr = '';
  client.stderr.on('data', data => { if (stderr.length < 16000) stderr += data.toString(); });
  createInterface({ input: client.stdout }).on('line', line => {
    let msg; try { msg = JSON.parse(line); } catch { return; }
    if (msg.id !== undefined && pending.has(msg.id)) {
      const { resolve, reject, timer } = pending.get(msg.id); pending.delete(msg.id); clearTimeout(timer);
      if (msg.error) reject(new Error(JSON.stringify(msg.error))); else resolve(msg.result);
    } else if (msg.method) {
      notifications.push(msg);
      // Never authorize or execute anything requested by a probe.
      if (msg.id !== undefined) client.stdin.write(JSON.stringify({ id: msg.id, error: { code: -32601, message: 'Probe does not authorize tools' } }) + '\n');
    }
  });
  const rpc = (method, params) => new Promise((resolve, reject) => {
    const requestId = ++id;
    const timer = setTimeout(() => { pending.delete(requestId); reject(new Error(`RPC timeout: ${method}; ${stderr.slice(-3000)}`)); }, 25000);
    pending.set(requestId, { resolve, reject, timer });
    client.stdin.write(JSON.stringify({ id: requestId, method, params }) + '\n');
  });
  await rpc('initialize', { clientInfo: { name: 'cxweb_diagnostic', version: '0.1.0' }, capabilities: { experimentalApi: true } });
  client.stdin.write(JSON.stringify({ method: 'initialized', params: {} }) + '\n');
  const models = await rpc('model/list', { includeHidden: true });
  stage = 'model discovery';
  const model = models.data.find(m => m.model === route);
  assert.ok(model, 'real backend lists namespaced model');
  evidence.model = { id: model.id, model: model.model, displayName: model.displayName, hidden: model.hidden, defaultReasoningEffort: model.defaultReasoningEffort };
  evidence.events.push('model/list accepted owned model');
  stage = 'thread creation';
  const thread = await rpc('thread/start', { cwd, model: route, ephemeral: true });
  evidence.selectedReasoningEffort = thread.reasoningEffort;
  const expected = live ? 'cxweb live client round-trip succeeded' : 'cxweb diagnostic round-trip succeeded';
  const prompt = live ? `Return a final answer with exactly this text: ${expected}` : 'Synthetic diagnostic only. Reply with the diagnostic response.';
  const turn = await rpc('turn/start', { threadId: thread.thread.id, input: [{ type: 'text', text: prompt, text_elements: [] }] });
  stage = 'generation';
  for (let i = 0; i < (live ? 12000 : 400); i++) {
    if (notifications.some(n => n.method === 'turn/completed' && n.params?.turn?.id === turn.turn.id)) break;
    await delay(50);
  }
  const completed = notifications.find(n => n.method === 'turn/completed' && n.params?.turn?.id === turn.turn.id);
  if (live) {
    const known = /\bE_(?:MODEL_FIDELITY|BROWSER_PREPARE|BROWSER_OBSERVATION|SUBMISSION_UNCERTAIN|SESSION_SCOPE|INVALID_TOOL_ENVELOPE|UNSUPPORTED_TOOL|NONPORTABLE_CONTEXT|REQUEST_IDENTITY|UNSUPPORTED_REQUEST|CONTEXT_BUDGET|GENERATION_TIMEOUT|REQUEST_ALREADY_ADMITTED)\b/;
    evidence.gatewayError = JSON.stringify(completed?.params?.turn?.error ?? {}).match(known)?.[0] ?? null;
  }
  const messages = notifications.filter(n => n.method === 'item/completed').map(n => n.params?.item).filter(i => i?.type === 'agentMessage');
  assert.equal(completed?.params?.turn?.status, 'completed', JSON.stringify(completed?.params ?? notifications.slice(-5)));
  assert.ok(messages.some(m => live ? m.text === expected : m.text.includes(expected)), 'client receives exact gateway text');
  evidence.events.push('thread/start selected owned model', 'turn/start completed through loopback', live ? 'expected browser assistant text received' : 'expected synthetic assistant text received');
  if (!live) {
    const identity = JSON.parse(await readFile(join(work, 'identity.json'), 'utf8'));
    assert.equal(identity.verified_native_identity, true, 'client supplies unambiguous thread/session/turn identity');
    assert.equal(identity.raw_identifiers_recorded, false);
    evidence.identity = identity;
  }
  evidence.result = live ? 'PASS native backend through authenticated browser' : 'PASS backend-only synthetic test';
} catch (error) {
  evidence.result = 'FAIL';
  evidence.failedStage = stage;
  // Synthetic harness errors only; redact its capability-bearing loopback URL.
  evidence.error = live ? 'E_LIVE_CLIENT_PROBE' : String(error.message).replace(/\/wb\/[A-Za-z0-9_-]+\//g, '/wb/[redacted]/');
  process.exitCode = 1;
} finally {
  client?.kill();
  if (live) {
    if (server.exitCode === null && server.signalCode === null) server.stdin.end('stop\n');
    const exited = await Promise.race([serverExit, delay(65000, null, { ref: false })]);
    if (!exited) { server.kill(); evidence.cleanup = 'unconfirmed'; process.exitCode = 1; }
    else {
      try { evidence.runtime = JSON.parse(serverOutput); } catch {
        evidence.runtimeError = serverError.match(/\bE_[A-Z_]+\b/)?.[0] ?? 'E_PROBE_RUNTIME';
        process.exitCode = 1;
      }
      if (exited.code !== 0) process.exitCode = 1;
    }
    if (process.exitCode) evidence.result = 'FAIL';
  } else server.kill();
  await writeFile(join(work, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n');
  console.log(JSON.stringify(evidence, null, 2));
  console.log(`Evidence: ${join(work, 'evidence.json')}`);
}
