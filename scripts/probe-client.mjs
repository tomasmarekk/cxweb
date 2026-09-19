// Isolated native client. --live uses the already authenticated cxweb browser;
// native auth files and the user's native configuration are never accessed.
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import assert from 'node:assert/strict';
import { randomUUID, createHash } from 'node:crypto';
import { approveFixtureRead, approveFixturePatch, fixtureReadCommand } from './probe-client-approval.mjs';

const executable = process.argv[2];
const modes = process.argv.slice(3);
if (!executable || modes.length > 1 || modes.some(mode => !['--live', '--live-read', '--live-patch', '--live-websocket-patch', '--live-search-limit', '--capture-tools'].includes(mode))) {
  throw new Error('Usage: node scripts/probe-client.mjs <absolute codex executable> [--live | --live-read | --live-patch | --live-websocket-patch | --live-search-limit | --capture-tools]');
}
const searchLimitProbe = modes.includes('--live-search-limit');
const liveWebsocket = modes.includes('--live-websocket-patch');
const patchProbe = process.argv.includes('--live-patch') || liveWebsocket;
const captureTools = process.argv.includes('--capture-tools');
const readProbe = process.argv.includes('--live-read') || patchProbe;
const live = process.argv.includes('--live') || readProbe || searchLimitProbe;
const root = resolve('.local/probes');
await mkdir(root, { recursive: true });
const work = await mkdtemp(join(root, 'client-'));
const home = join(work, 'home');
const cwd = join(work, 'workspace');
await mkdir(home); await mkdir(cwd);
const toolMarker = `cxweb-native-tool-${randomUUID()}`;
if (readProbe) await writeFile(join(cwd, 'probe-input.txt'), toolMarker + '\n');
// Resolve the host's actual bundled shell before any model response is read.
const hostShellExecutables = [];
if (readProbe) for (const name of ['pwsh.exe', 'powershell.exe']) {
  try { hostShellExecutables.push(...execFileSync('where.exe', [name], { encoding: 'utf8', windowsHide: true }).trim().split(/\r?\n/)); } catch { /* absent optional shell */ }
}
const bridge = resolve('target/debug/cxweb.exe');
const descriptor = live ? join(work, 'runtime', 'connection.json') : join(work, 'probe.json');
const server = spawn(bridge, live ? ['live-probe', '--output', descriptor, ...(liveWebsocket ? ['--websocket'] : [])] : ['probe', '--output', descriptor, '--tools-output', join(work, 'tools.json'), '--identity-output', join(work, 'identity.json')], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
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
  const catalog = live ? connection.catalog : JSON.parse(execFileSync(bridge, ['probe-catalog'], { encoding: 'utf8', windowsHide: true }));
  if (readProbe || captureTools) catalog.models[0].shell_type = 'unified_exec';
  if (patchProbe || captureTools) catalog.models[0].apply_patch_tool_type = 'freeform';
  await writeFile(join(home, 'catalog.json'), JSON.stringify(catalog));
  // Exercise the client's default optional hosted-search definition. cxweb
  // reports hosted search as unavailable on its text/coding route; no global
  // search override is written even in this disposable client.
  await writeFile(join(home, 'config.toml'), `openai_base_url = ${JSON.stringify(endpoint)}\nmodel_catalog_json = ${JSON.stringify(join(home, 'catalog.json').replaceAll('\\', '/'))}\n`);
  if (live) evidence.builtinWebSearch = 'native default preserved; hosted search explicitly unavailable on this web route';
  // Sanitize inherited route/auth/home overrides. Only this subprocess sees the mock key.
  const env = { ...process.env };
  for (const key of Object.keys(env)) if (/^(CODEX_|OPENAI_|CHATGPT_)/i.test(key)) delete env[key];
  env.CODEX_HOME = home;
  env.OPENAI_API_KEY = 'cxweb-synthetic-not-a-real-key';
  evidence.executableSha256 = createHash('sha256').update(await readFile(executable)).digest('hex');
  evidence.client = execFileSync(executable, ['--version'], { encoding: 'utf8', windowsHide: true }).trim();
  client = spawn(executable, ['app-server', '--listen', 'stdio://'], { env, cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  const pending = new Map();
  const notifications = [];
  const approvals = [];
  const patchApprovals = [];
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
      // This test client approves only its explicit fixture operations. The product
      // never executes model output or decides native approvals.
      if (msg.id !== undefined) {
        const commandApproval = msg.method === 'item/commandExecution/requestApproval';
        const fileApproval = msg.method === 'item/fileChange/requestApproval';
        const approval = commandApproval || fileApproval;
        let accepted = readProbe && commandApproval
          && approvals.length === 0 && approveFixtureRead(msg.params, cwd, hostShellExecutables);
        if (fileApproval && patchProbe && patchApprovals.length === 0) {
          const started = notifications.findLast(n => n.method === 'item/started' && n.params?.item?.id === msg.params?.itemId);
          accepted = approveFixturePatch(msg.params, started, cwd, toolMarker);
        }
        if (commandApproval) approvals.push(accepted);
        if (fileApproval) patchApprovals.push(accepted);
        if (approval && live) console.log(`Native fixture approval: ${accepted ? 'accepted' : 'declined'}`);
        client.stdin.write(JSON.stringify(approval
          ? { id: msg.id, result: { decision: accepted ? 'accept' : 'decline' } }
          : { id: msg.id, error: { code: -32601, message: 'Probe does not authorize tools' } }) + '\n');
      }
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
  const expected = searchLimitProbe ? 'Built-in web search is unavailable on this ChatGPT Web route. Use a native Codex model for web search.' : readProbe ? toolMarker : live ? 'cxweb live client round-trip succeeded' : 'cxweb diagnostic round-trip succeeded';
  const prompt = readProbe
    ? `Use exec_command once to run exactly ${fixtureReadCommand} in the current working directory. ${patchProbe ? 'After reading it, use the apply_patch custom tool once to create probe-output.txt containing that exact line followed by a newline. Wait for the successful patch result before your final answer. Do not modify other files.' : 'This is a read-only fixture test. Do not modify files.'} Do not request elevated permissions or run other commands. Return a final answer containing exactly the single line read from the input file, without extra text.`
    : searchLimitProbe ? `Use the built-in hosted web_search tool to check the current stable Rust release. If that hosted tool is unavailable, do not guess or use other tools: return exactly this final text: ${expected}`
    : live ? `Return a final answer with exactly this text: ${expected}` : 'Synthetic diagnostic only. Reply with the diagnostic response.';
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
  if (readProbe) {
    const commands = notifications.filter(n => n.method === 'item/completed' && n.params?.item?.type === 'commandExecution').map(n => n.params.item);
    evidence.nativeTools = {
      fixture: 'random marker not supplied in the prompt',
      commandCount: commands.length,
      completed: commands.filter(item => item.status === 'completed' && item.exitCode === 0).length,
      resultContainsMarker: commands.some(item => item.aggregatedOutput?.includes(toolMarker)),
      approvalRequests: notifications.filter(n => n.method === 'item/commandExecution/requestApproval').length,
      exactReadApprovals: approvals.filter(Boolean).length,
      harnessExecutedTools: false,
    };
    assert.equal(commands.length, 1, 'Codex executes one native read');
    assert.equal(evidence.nativeTools.completed, 1, 'native command succeeds');
    assert.ok(evidence.nativeTools.resultContainsMarker, 'native result contains the undisclosed fixture marker');
    if (patchProbe) {
      const patches = notifications.filter(n => n.method === 'item/completed' && n.params?.item?.type === 'fileChange').map(n => n.params.item);
      let actual;
      try { actual = await readFile(join(cwd, 'probe-output.txt'), 'utf8'); } catch { /* missing is a failed test */ }
      evidence.nativeTools.patchCount = patches.length;
      evidence.nativeTools.patchesCompleted = patches.filter(item => item.status === 'completed').length;
      evidence.nativeTools.exactPatchApprovals = patchApprovals.filter(Boolean).length;
      evidence.nativeTools.fileMatchesMarker = actual === toolMarker + '\n';
      assert.equal(patches.length, 1, 'Codex applies one native patch');
      assert.equal(evidence.nativeTools.patchesCompleted, 1, 'native patch succeeds');
      assert.ok(evidence.nativeTools.fileMatchesMarker, 'Codex writes the exact fixture contents');
    }
  }
  assert.equal(completed?.params?.turn?.status, 'completed', JSON.stringify(completed?.params ?? notifications.slice(-5)));
  assert.ok(messages.some(m => live ? m.text === expected : m.text.includes(expected)), 'client receives exact gateway text');
  evidence.events.push('thread/start selected owned model', 'turn/start completed through loopback', live ? 'expected browser assistant text received' : 'expected synthetic assistant text received');
  if (!live) {
    const identity = JSON.parse(await readFile(join(work, 'identity.json'), 'utf8'));
    assert.equal(identity.verified_native_identity, true, 'client supplies unambiguous thread/session/turn identity');
    assert.equal(identity.raw_identifiers_recorded, false);
    evidence.identity = identity;
  }
  evidence.result = searchLimitProbe ? 'PASS hosted-search limitation returned through native backend' : patchProbe ? 'PASS native function and custom tools through authenticated browser' : readProbe ? 'PASS native read tool through authenticated browser' : live ? 'PASS native backend through authenticated browser' : 'PASS backend-only synthetic test';
} catch (error) {
  evidence.result = 'FAIL';
  evidence.failedStage = stage;
  // Synthetic harness errors only; redact its capability-bearing loopback URL.
  evidence.error = live ? 'E_LIVE_CLIENT_PROBE' : String(error.message).replace(/\/wb\/[A-Za-z0-9_-]+\//g, '/wb/[redacted]/');
  process.exitCode = 1;
} finally {
  client?.kill();
  if (evidence.executableSha256) {
    try {
      evidence.executableUnchanged = createHash('sha256').update(await readFile(executable)).digest('hex') === evidence.executableSha256;
    } catch { evidence.executableUnchanged = false; }
    if (!evidence.executableUnchanged) { evidence.result = 'FAIL'; evidence.error = 'E_CLIENT_CHANGED'; process.exitCode = 1; }
  }
  if (live) {
    if (server.exitCode === null && server.signalCode === null) server.stdin.end('stop\n');
    const exited = await Promise.race([serverExit, delay(65000, null, { ref: false })]);
    if (!exited) { server.kill(); evidence.cleanup = 'unconfirmed'; process.exitCode = 1; }
    else {
      try {
        evidence.runtime = JSON.parse(serverOutput);
        assert.deepEqual(evidence.runtime.failures, [], 'no rejected browser requests');
        if (liveWebsocket) {
          assert.ok(evidence.runtime.websocket_upgrades > 0);
          assert.equal(evidence.runtime.native_websocket_frames, 0);
          assert.equal(evidence.runtime.websocket_requests, evidence.runtime.output_formats.length);
          assert.ok(evidence.runtime.websocket_requests > 0);
        }
        assert.ok(evidence.runtime.optional_web_search_requests > 0, 'native client supplied optional hosted search');
      } catch {
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
