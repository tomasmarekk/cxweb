import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createInterface } from 'node:readline';
import { resolve, join } from 'node:path';
import { randomBytes, createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { source, cases, fixtureFiles } from './coding-fixture.mjs';
import { approvePatch, completedProgress, readCommand, testCommand } from './coding-fixture-approval.mjs';
import { approveFixtureCommand } from './probe-client-approval.mjs';
import runner from './coding-fixture-runner.cjs';
const exe = process.argv[2];
const repair = process.argv[3] === '--repair';
assert.ok(process.argv.length <= 4 && (!process.argv[3] || repair));
const hash = createHash('sha256').update(await readFile(exe)).digest('hex');
assert.ok(['eba0f32c976667cb9298efafd98513e823eeda7b576a03ec658bb8be8d336316',
  'bc45017e8239dc150258f69309ced9df6bbcdf5b8e4f346decf780ac0999e226'].includes(hash), 'E_UNREVIEWED_CLIENT');
await mkdir(resolve('.local/probes'), { recursive: true });
const root = await mkdtemp(resolve('.local/probes/arithmetic-native-'));
const cwd = join(root, 'workspace'), home = join(root, 'home');
await mkdir(cwd); await mkdir(home);
await writeFile(join(cwd, 'solve.cjs'), source('a - b'));
const files = fixtureFiles(cases[0]);
if (repair) {
  await writeFile(join(cwd, 'cases.json'), files['cases.json']);
  await writeFile(join(cwd, 'tests.cjs'), await readFile(new URL('./coding-fixture-runner.cjs', import.meta.url)));
}
const run = { initial: source('a - b'), caseJson: files['cases.json'], testCommand: testCommand(process.execPath),
  before: runner.check(source('a - b'), cases[0].tests), approvedSteps: new Set() };
const shells = [];
for (const name of ['pwsh.exe', 'powershell.exe']) {
  try { shells.push(...execFileSync('where.exe', [name], { encoding: 'utf8', windowsHide: true }).trim().split(/\r?\n/)); } catch { /* optional shell */ }
}
const capability = '/' + randomBytes(24).toString('hex');
let requests = 0;
const patch = '*** Begin Patch\n*** Update File: solve.cjs\n@@\n-  return a - b;\n+  return a + b;\n*** End Patch';
const server = createServer(async (req, res) => {
  if (req.url !== capability + '/responses') { res.writeHead(403).end(); return; }
  if (req.method === 'GET') { res.writeHead(426).end(); return; }
  if (req.method !== 'POST' || requests >= (repair ? 5 : 2)) { res.writeHead(403).end(); return; }
  let bytes = 0; for await (const chunk of req) { bytes += chunk.length; if (bytes > 4e6) { req.destroy(); return; } }
  const n = ++requests;
  const item = repair && [1, 2, 4].includes(n)
    ? { type: 'function_call', name: 'exec_command', namespace: 'functions', id: `item_${n}`, call_id: `call_${n}`,
      arguments: JSON.stringify({ cmd: n === 1 ? readCommand : run.testCommand, login: false, workdir: cwd }), status: 'completed' }
    : n === (repair ? 3 : 1) ? { type: 'custom_tool_call', name: 'apply_patch', namespace: 'functions', id: 'item_patch', call_id: 'call_patch', input: patch, status: 'completed' }
    : { type: 'message', id: 'item_final', role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: 'fixture done', annotations: [] }] };
  const response = { id: `resp_${n}`, object: 'response', status: 'completed', output: [item] };
  const events = [
    { type: 'response.created', response: { ...response, output: [], status: 'in_progress' } },
    { type: 'response.output_item.added', output_index: 0, item: { ...item, status: 'in_progress' } },
    { type: 'response.output_item.done', output_index: 0, item },
    { type: 'response.completed', response },
  ];
  res.writeHead(200, { 'content-type': 'text/event-stream' });
  res.end(events.map((event, sequence_number) => `event: ${event.type}\ndata: ${JSON.stringify({ ...event, sequence_number })}\n\n`).join(''));
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const version = execFileSync(exe, ['--version'], { encoding: 'utf8', windowsHide: true }).trim().split(' ')[1];
const catalog = execFileSync(resolve('target/debug/cxweb.exe'), ['probe-catalog', '--client-build', version, '--coding'], { encoding: 'utf8', windowsHide: true });
await writeFile(join(home, 'catalog.json'), catalog);
await writeFile(join(home, 'config.toml'), `openai_base_url = "http://127.0.0.1:${server.address().port}${capability}"\nmodel_catalog_json = ${JSON.stringify(join(home, 'catalog.json').replaceAll('\\', '/'))}\n`);
const env = { ...process.env };
for (const key of Object.keys(env)) if (/^(CODEX_|OPENAI_|CHATGPT_)/i.test(key)) delete env[key];
env.CODEX_HOME = home; env.OPENAI_API_KEY = 'synthetic-fixture-only';
const child = spawn(exe, ['app-server'], { cwd, env, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
const events = [], pending = new Map(), approvals = []; let id = 0, approvedPatches = 0;
const send = message => child.stdin.write(JSON.stringify(message) + '\n');
createInterface({ input: child.stdout }).on('line', line => {
  const message = JSON.parse(line);
  if (message.method) {
    events.push(message);
    if (message.method === 'turn/started' && message.params?.threadId === run.thread) run.turn = message.params.turn.id;
    if (message.id !== undefined) {
      const started = events.findLast(event => event.method === 'item/started' && event.params?.item?.id === message.params?.itemId);
      const progress = repair ? completedProgress(events, run, cwd, shells) : 0;
      let accepted = message.method === 'item/fileChange/requestApproval' && approvePatch(message.params, started, cwd, source('a - b'));
      if (repair) {
        accepted = message.params?.threadId === run.thread && message.params?.turnId === run.turn
          && !run.approvedSteps.has(progress) && (progress === 2 ? accepted : [0, 1, 3].includes(progress)
            && message.method === 'item/commandExecution/requestApproval'
            && approveFixtureCommand(message.params, cwd, shells, progress === 0 ? readCommand : run.testCommand));
        if (accepted) run.approvedSteps.add(progress);
      }
      approvals.push(accepted);
      if (accepted && message.method === 'item/fileChange/requestApproval') approvedPatches++;
      send({ id: message.id, result: { decision: accepted ? 'accept' : 'decline' } });
    }
  } else { const callback = pending.get(message.id); pending.delete(message.id); callback?.(message); }
});
const rpc = (method, params) => new Promise((resolve, reject) => { const requestId = ++id; const timer = setTimeout(() => reject(new Error('RPC timeout')), 15000); pending.set(requestId, message => { clearTimeout(timer); message.error ? reject(new Error('RPC error')) : resolve(message.result); }); send({ id: requestId, method, params }); });
const evidence = { schema: 'cxweb.native-patch-fixture.v1', evidence_level: 'mock_integration',
  syntheticModel: true, actualNativeClient: true, accountGeneration: false, build: version,
  executableSha256: hash, result: 'started' };
evidence.repair = repair;
try {
  await rpc('initialize', { clientInfo: { name: 'arithmetic_shape_fixture', version: '0.1' }, capabilities: { experimentalApi: true } });
  send({ method: 'initialized', params: {} });
  const thread = await rpc('thread/start', { cwd, model: 'webbridge/diagnostic', ephemeral: true, approvalPolicy: 'untrusted', sandbox: 'read-only' });
  run.thread = thread.thread.id;
  const turn = await rpc('turn/start', { threadId: thread.thread.id, input: [{ type: 'text', text: 'Synthetic local fixture: apply the provided single source patch.', text_elements: [] }] });
  const until = Date.now() + 30000;
  while (!events.some(event => event.method === 'turn/completed' && event.params?.turn?.id === turn.turn.id) && Date.now() < until) await new Promise(resolve => setTimeout(resolve, 100));
  assert.equal(events.find(event => event.method === 'turn/completed' && event.params?.turn?.id === turn.turn.id)?.params?.turn?.status, 'completed');
  if (repair) assert.equal(completedProgress(events, run, cwd, shells), 4);
  assert.deepEqual(approvals, repair ? [true, true, true, true] : [true]);
  assert.equal(requests, repair ? 5 : 2);
  assert.equal(await readFile(join(cwd, 'solve.cjs'), 'utf8'), source('a + b'));
  evidence.result = 'PASS'; evidence.exactNativePatch = true;
} catch {
  evidence.result = 'FAIL'; process.exitCode = 1;
} finally {
  child.kill(); server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
  evidence.observedAt = new Date().toISOString(); evidence.modelRequests = requests;
  evidence.approvedPatches = approvedPatches;
  evidence.approvedActions = approvals.filter(Boolean).length;
  if (repair) {
    evidence.approvedSteps = [...run.approvedSteps];
    evidence.nativeTests = events.filter(event => event.method === 'item/completed' && event.params?.item?.type === 'commandExecution'
      && approveFixtureCommand(event.params.item, cwd, shells, run.testCommand)).map(event => {
      const item = event.params.item;
      let counts;
      try { const value = JSON.parse(item.aggregatedOutput); counts = { total: value.total, passed: value.passed, failed: value.failed,
        error: ['E_FIXTURE_SOURCE', 'E_FIXTURE_CASES', 'E_FIXTURE_INPUT', 'E_FIXTURE_TIMEOUT', 'E_FIXTURE_EVALUATION'].includes(value.error) ? value.error : undefined }; } catch { /* non-JSON failure */ }
      return { exitCode: item.exitCode, counts };
    });
  }
  await writeFile(join(root, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n');
  console.log(JSON.stringify(evidence, null, 2));
}
