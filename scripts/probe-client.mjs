// Development harness only. No real auth files or config are read or modified.
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import assert from 'node:assert/strict';

const executable = process.argv[2];
if (!executable) throw new Error('Usage: node scripts/probe-client.mjs <absolute codex executable>');
const root = resolve('.local/probes');
await mkdir(root, { recursive: true });
const work = await mkdtemp(join(root, 'client-'));
const home = join(work, 'home');
const cwd = join(work, 'workspace');
await mkdir(home); await mkdir(cwd);
const bridge = resolve('target/debug/cxweb.exe');
const descriptor = join(work, 'probe.json');
const server = spawn(bridge, ['probe', '--output', descriptor, '--tools-output', join(work, 'tools.json')], { windowsHide: true, stdio: 'ignore' });
let client;
const evidence = { schema: 'cxweb.client-probe.v1', client: execFileSync(executable, ['--version'], { encoding: 'utf8', windowsHide: true }).trim(), synthetic: true, actualPicker: 'NOT RUN', authentication: 'synthetic API key in isolated child environment', events: [] };
try {
  let endpoint;
  for (let i = 0; i < 100; i++) {
    try { endpoint = JSON.parse(await readFile(descriptor, 'utf8')).base_url; break; } catch { await delay(50); }
  }
  assert.ok(endpoint, 'diagnostic gateway starts');
  const catalog = execFileSync(bridge, ['probe-catalog'], { encoding: 'utf8', windowsHide: true });
  await writeFile(join(home, 'catalog.json'), catalog);
  await writeFile(join(home, 'config.toml'), `openai_base_url = ${JSON.stringify(endpoint)}\nmodel_catalog_json = ${JSON.stringify(join(home, 'catalog.json').replaceAll('\\', '/'))}\n`);
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
  const model = models.data.find(m => m.model === 'webbridge/diagnostic');
  assert.ok(model, 'real backend lists namespaced model');
  evidence.model = { id: model.id, model: model.model, displayName: model.displayName, hidden: model.hidden };
  evidence.events.push('model/list accepted owned model');
  const thread = await rpc('thread/start', { cwd, model: 'webbridge/diagnostic', ephemeral: true });
  const turn = await rpc('turn/start', { threadId: thread.thread.id, input: [{ type: 'text', text: 'Synthetic diagnostic only. Reply with the diagnostic response.', text_elements: [] }] });
  for (let i = 0; i < 400; i++) {
    if (notifications.some(n => n.method === 'turn/completed' && n.params?.turn?.id === turn.turn.id)) break;
    await delay(50);
  }
  const completed = notifications.find(n => n.method === 'turn/completed' && n.params?.turn?.id === turn.turn.id);
  const messages = notifications.filter(n => n.method === 'item/completed').map(n => n.params?.item).filter(i => i?.type === 'agentMessage');
  assert.equal(completed?.params?.turn?.status, 'completed', JSON.stringify(completed?.params ?? notifications.slice(-5)));
  assert.ok(messages.some(m => m.text.includes('cxweb diagnostic round-trip succeeded')), 'client receives actual gateway text');
  evidence.events.push('thread/start selected owned model', 'turn/start completed through loopback', 'expected synthetic assistant text received');
  evidence.result = 'PASS backend-only synthetic test';
} catch (error) {
  evidence.result = 'FAIL';
  // Synthetic harness errors only; redact its capability-bearing loopback URL.
  evidence.error = String(error.message).replace(/\/wb\/[A-Za-z0-9_-]+\//g, '/wb/[redacted]/');
  process.exitCode = 1;
} finally {
  client?.kill(); server.kill();
  await writeFile(join(work, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n');
  console.log(JSON.stringify(evidence, null, 2));
  console.log(`Evidence: ${join(work, 'evidence.json')}`);
}
