// Verify an explicitly selected native client's real installed routing.
// Uses its existing subscription. --text consumes web allowance; --coexistence
// also exercises a native subscription model between two independent web turns.
// --tools runs one exact native read/patch exercise in a disposable workspace.
// --reasoning verifies all five qualified choices through actual native turns.
// --denial refuses one exact read and verifies the model receives that refusal.
// --repair observes a failing test, approves one exact correction, then retests.
// No auth files, routing overrides, model catalogs or client binaries are changed.
import { spawn, execFileSync } from 'node:child_process';
import { readFile, readdir, mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createHash, randomBytes } from 'node:crypto';
import { isAbsolute, resolve, join } from 'node:path';
import { createInterface } from 'node:readline';
import assert from 'node:assert/strict';
import { approveFixtureRead, approveFixturePatch, completedFixtureRead, completedFixtureDenial, fixtureReadCommand } from './probe-client-approval.mjs';
import { approveFixtureTest, approveFixtureRepair, fixtureRepairProgress, fixtureTestCommand, fixtureTestPassed, fixtureTestFailed, fixtureBrokenOutput } from './probe-client-approval.mjs';

const [client, home, model, option] = process.argv.slice(2);
assert.ok(client && home && model?.startsWith('webbridge/') && isAbsolute(client) && isAbsolute(home));
assert.ok(process.argv.length <= 6 && (!option || ['--text', '--coexistence', '--tools', '--reasoning', '--denial', '--repair'].includes(option)));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const builds = new Map([
  ['eba0f32c976667cb9298efafd98513e823eeda7b576a03ec658bb8be8d336316', '0.155.1'],
  ['bc45017e8239dc150258f69309ced9df6bbcdf5b8e4f346decf780ac0999e226', '0.155.0-alpha.9.2'],
]);
const hash = sha256(await readFile(client));
assert.ok(builds.has(hash), 'E_UNREVIEWED_CLIENT');
for (const key of ['OPENAI_BASE_URL', 'OPENAI_API_KEY', 'CODEX_API_KEY']) assert.ok(!process.env[key], 'E_ENVIRONMENT_OVERRIDE');
const configPath = join(home, 'config.toml');
const configBefore = sha256(await readFile(configPath));
await mkdir(resolve('.local/probes'), { recursive: true });
const cwd = await mkdtemp(resolve('.local/probes/installed-'));
const shells = [];
if (['--tools', '--denial', '--repair'].includes(option)) for (const name of ['pwsh.exe', 'powershell.exe']) {
  try { shells.push(...execFileSync('where.exe', [name], { encoding: 'utf8', windowsHide: true }).trim().split(/\r?\n/)); } catch { /* optional shell absent */ }
}
const child = spawn(client, ['app-server'], { cwd, env: { ...process.env, CODEX_HOME: home }, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
const pending = new Map();
const events = [];
let failure, bytes = 0, id = 0;
let toolRun;
const send = value => child.stdin.write(JSON.stringify(value) + '\n');
const lines = createInterface({ input: child.stdout });
lines.on('line', line => {
  bytes += Buffer.byteLength(line);
  if (bytes > 64 * 1024 * 1024 || events.length > 4096) { failure = 'E_OUTPUT_LIMIT'; child.kill(); return; }
  let message;
  try { message = JSON.parse(line); } catch { failure = 'E_PROTOCOL'; child.kill(); return; }
  if (message.method && message.id !== undefined) {
    const command = message.method === 'item/commandExecution/requestApproval';
    const patch = message.method === 'item/fileChange/requestApproval';
    const params = message.params;
    const scoped = toolRun?.turn && params?.threadId === toolRun.thread && params?.turnId === toolRun.turn;
    let accepted = false, intentionalDenial = false;
    if (scoped && toolRun.repair) {
      const progress = fixtureRepairProgress(events, toolRun.thread, toolRun.turn, cwd, toolRun.marker, shells);
      if (progress >= 0 && progress < 4 && !toolRun.approvedSteps.has(progress)) {
        if (command && progress === 0) accepted = approveFixtureRead(params, cwd, shells);
        else if (command && [1, 3].includes(progress)) accepted = approveFixtureTest(params, cwd, shells);
        else if (patch && progress === 2) {
          const started = events.findLast(event => event.method === 'item/started' && event.params?.item?.id === params.itemId);
          accepted = approveFixtureRepair(params, started, cwd, toolRun.marker);
        }
        if (accepted) toolRun.approvedSteps.add(progress);
      }
    } else if (scoped && toolRun.denial) {
      if (command && !toolRun.readDenied && approveFixtureRead(params, cwd, shells)) {
        intentionalDenial = toolRun.readDenied = true;
      }
    } else if (scoped && command && !toolRun.readApproved && !toolRun.patchApproved && approveFixtureRead(params, cwd, shells)) {
      accepted = toolRun.readApproved = true;
    } else if (scoped && patch && !toolRun.patchApproved && completedFixtureRead(events, toolRun.thread, toolRun.turn, cwd, toolRun.marker, shells)) {
      const started = events.findLast(event => event.method === 'item/started' && event.params?.item?.id === params.itemId);
      accepted = approveFixturePatch(params, started, cwd, toolRun.marker);
      toolRun.patchApproved = accepted;
    }
    send(command || patch
      ? { id: message.id, result: { decision: accepted ? 'accept' : 'decline' } }
      : { id: message.id, error: { code: -32601, message: 'No other actions approved by this fixture' } });
    if (!accepted && !intentionalDenial) failure = 'E_UNEXPECTED_ACTION';
  } else if (message.id !== undefined) {
    const callback = pending.get(message.id);
    pending.delete(message.id);
    callback?.(message);
  } else {
    if (toolRun && message.method === 'turn/started' && message.params?.threadId === toolRun.thread) {
      const turn = message.params.turn?.id;
      if (!turn || (toolRun.turn && toolRun.turn !== turn)) failure = 'E_TURN_IDENTITY';
      else toolRun.turn = turn;
    }
    events.push(message);
  }
});
child.on('error', () => { failure = 'E_CLIENT_START'; });
async function rpc(method, params) {
  const requestId = ++id;
  let timer;
  try {
    const reply = await Promise.race([
      new Promise(resolve => { pending.set(requestId, resolve); send({ id: requestId, method, params }); }),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('E_RPC_TIMEOUT')), 45000); }),
    ]);
    assert.ok(!reply.error, `E_RPC_${method}`);
    return reply.result;
  } finally { clearTimeout(timer); pending.delete(requestId); }
}
const evidence = { schema: 'cxweb.installed-client.v1', startedAt: new Date().toISOString(), build: builds.get(hash), executableSha256: hash, synthetic: false, configurationOverride: false, actualGuiPicker: 'not observed', text: 'not requested' };
async function verifyText(selectedModel, effort) {
  const check = { model: selectedModel.id, effort, route: selectedModel.id.startsWith('webbridge/') ? 'web' : 'native', result: 'started' };
  (evidence.textChecks ??= []).push(check);
  console.log(JSON.stringify({ phase: 'text', route: check.route, model: check.model, effort }));
  const expected = `CXWEB_INSTALLED_${randomBytes(8).toString('hex')}`;
  const started = (await rpc('thread/start', { cwd, model: selectedModel.id, ephemeral: true, approvalPolicy: 'untrusted', sandbox: 'read-only' }));
  assert.equal(started.model, selectedModel.id, 'E_SELECTED_MODEL');
  assert.equal(started.modelProvider, 'openai', 'E_NATIVE_PROVIDER');
  const thread = started.thread.id;
  const turn = (await rpc('turn/start', { threadId: thread, effort, input: [{ type: 'text', text: `Use no tools. Return a final answer with exactly this text: ${expected}`, text_elements: [] }] })).turn.id;
  const deadline = Date.now() + 240000;
  let completed;
  while (Date.now() < deadline) {
    assert.ok(!failure, failure);
    completed = events.find(event => event.method === 'turn/completed' && event.params?.threadId === thread && event.params?.turn?.id === turn);
    if (completed) break;
    assert.equal(child.exitCode, null, 'E_CLIENT_EXIT');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal(completed?.params?.turn?.status, 'completed', 'E_TURN_FAILED');
  const answers = events.filter(event => event.method === 'item/completed' && event.params?.threadId === thread && event.params?.turnId === turn && event.params?.item?.type === 'agentMessage');
  assert.equal(answers.length, 1, 'E_ANSWER_COUNT');
  assert.equal(answers[0].params.item.text, expected, 'E_ANSWER_TEXT');
  assert.ok(!events.some(event => event.method === 'item/started' && event.params?.threadId === thread && event.params?.turnId === turn && ['commandExecution', 'fileChange', 'mcpToolCall', 'dynamicToolCall'].includes(event.params?.item?.type)), 'E_UNEXPECTED_TOOL');
  check.result = 'passed';
}
async function verifyTools(selectedModel) {
  evidence.nativeTools = { result: 'started', harnessExecutedTools: false, fixtureMarkerInPrompt: false };
  const marker = `CXWEB_INSTALLED_TOOLS_${randomBytes(16).toString('hex')}`;
  await writeFile(join(cwd, 'probe-input.txt'), marker + '\n', { flag: 'wx' });
  const started = await rpc('thread/start', { cwd, model, ephemeral: true, approvalPolicy: 'untrusted', sandbox: 'read-only' });
  assert.equal(started.model, model, 'E_SELECTED_MODEL');
  assert.equal(started.modelProvider, 'openai', 'E_NATIVE_PROVIDER');
  toolRun = { thread: started.thread.id, marker, readApproved: false, patchApproved: false };
  const prompt = `Use exec_command exactly once with cmd exactly ${JSON.stringify(fixtureReadCommand)}, login=false and the current working directory. After reading probe-input.txt, use apply_patch exactly once to add probe-output.txt containing that exact line followed by a newline. Wait for each actual tool result before continuing. Do not run other commands, change other files, request elevated permissions or access the network. Return exactly the line read as the final answer without extra text.`;
  const turn = (await rpc('turn/start', { threadId: toolRun.thread, effort: selectedModel.defaultReasoningEffort, input: [{ type: 'text', text: prompt, text_elements: [] }] })).turn.id;
  assert.ok(!toolRun.turn || toolRun.turn === turn, 'E_TURN_IDENTITY');
  toolRun.turn = turn;
  console.log(JSON.stringify({ phase: 'native read and patch', model }));
  const deadline = Date.now() + 600000;
  let done;
  while (Date.now() < deadline) {
    assert.ok(!failure, failure);
    done = events.find(event => event.method === 'turn/completed' && event.params?.threadId === toolRun.thread && event.params?.turn?.id === turn);
    if (done) break;
    assert.equal(child.exitCode, null, 'E_CLIENT_EXIT');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal(done?.params?.turn?.status, 'completed', 'E_TURN_FAILED');
  assert.ok(completedFixtureRead(events, toolRun.thread, turn, cwd, marker, shells), 'E_READ_RESULT');
  const completed = events.filter(event => event.method === 'item/completed' && event.params?.threadId === toolRun.thread && event.params?.turnId === turn);
  const tools = completed.filter(event => !['userMessage', 'agentMessage', 'reasoning'].includes(event.params?.item?.type));
  assert.deepEqual(tools.map(event => event.params.item.type), ['commandExecution', 'fileChange'], 'E_TOOL_ORDER');
  const patch = tools[1].params.item;
  assert.equal(patch.status, 'completed', 'E_PATCH_FAILED');
  const patchStarted = events.findLast(event => event.method === 'item/started' && event.params?.item?.id === patch.id);
  assert.ok(approveFixturePatch({ itemId: patch.id, threadId: toolRun.thread, turnId: turn }, patchStarted, cwd, marker), 'E_PATCH_TARGET');
  assert.equal(await readFile(join(cwd, 'probe-output.txt'), 'utf8'), marker + '\n', 'E_PATCH_CONTENT');
  assert.equal(await readFile(join(cwd, 'probe-input.txt'), 'utf8'), marker + '\n', 'E_INPUT_CHANGED');
  const answers = completed.filter(event => event.params.item.type === 'agentMessage');
  assert.equal(answers.length, 1, 'E_ANSWER_COUNT');
  assert.equal(answers[0].params.item.text, marker, 'E_ANSWER_TEXT');
  evidence.nativeTools = { ...evidence.nativeTools, result: 'passed', exactRead: true, exactPatch: true, realFileVerified: true, exactFinalAnswer: true, readApprovalObserved: toolRun.readApproved, patchApprovalObserved: toolRun.patchApproved };
  toolRun = undefined;
}
async function verifyRepair(selectedModel) {
  evidence.nativeRepair = { result: 'started', harnessExecutedTools: false, fixtureMarkerInPrompt: false };
  const marker = `CXWEB_REPAIR_${randomBytes(16).toString('hex')}`;
  await writeFile(join(cwd, 'probe-input.txt'), marker + '\n', { flag: 'wx' });
  await writeFile(join(cwd, 'probe-output.txt'), fixtureBrokenOutput + '\n', { flag: 'wx' });
  const started = await rpc('thread/start', { cwd, model, ephemeral: true, approvalPolicy: 'untrusted', sandbox: 'read-only' });
  assert.equal(started.model, model, 'E_SELECTED_MODEL');
  assert.equal(started.modelProvider, 'openai', 'E_NATIVE_PROVIDER');
  toolRun = { thread: started.thread.id, repair: true, marker, approvedSteps: new Set() };
  const prompt = `Use exec_command with cmd exactly ${JSON.stringify(fixtureReadCommand)}, login=false and the current working directory. Then use exec_command with cmd exactly ${JSON.stringify(fixtureTestCommand)}, login=false and the current working directory. The existing probe-output.txt contains the single incorrect line ${JSON.stringify(fixtureBrokenOutput)}. Wait for the actual test failure: exit code 1 and output ${JSON.stringify(fixtureTestFailed)}. Only after observing that failure, use apply_patch exactly once to update probe-output.txt, replacing the incorrect line with the exact line read from probe-input.txt, followed by a newline. Wait for the patch result, then run the same test command again. Return exactly ${JSON.stringify(fixtureTestPassed)} only after the second test exits 0 and prints that text. Wait for every actual tool result. Do not run other commands, edit other files, request elevated permissions or access the network.`;
  const turn = (await rpc('turn/start', { threadId: toolRun.thread, effort: selectedModel.defaultReasoningEffort, input: [{ type: 'text', text: prompt, text_elements: [] }] })).turn.id;
  assert.ok(!toolRun.turn || toolRun.turn === turn, 'E_TURN_IDENTITY');
  toolRun.turn = turn;
  console.log(JSON.stringify({ phase: 'native failing test and repair', model }));
  const deadline = Date.now() + 600000;
  let done;
  while (Date.now() < deadline) {
    assert.ok(!failure, failure);
    done = events.find(event => event.method === 'turn/completed' && event.params?.threadId === toolRun.thread && event.params?.turn?.id === turn);
    if (done) break;
    assert.equal(child.exitCode, null, 'E_CLIENT_EXIT');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal(done?.params?.turn?.status, 'completed', 'E_TURN_FAILED');
  assert.equal(fixtureRepairProgress(events, toolRun.thread, turn, cwd, marker, shells), 4, 'E_REPAIR_SEQUENCE');
  const completed = events.filter(event => event.method === 'item/completed' && event.params?.threadId === toolRun.thread && event.params?.turnId === turn);
  const answers = completed.filter(event => event.params.item.type === 'agentMessage');
  assert.equal(answers.length, 1, 'E_ANSWER_COUNT');
  assert.equal(answers[0].params.item.text, fixtureTestPassed, 'E_ANSWER_TEXT');
  assert.equal(fixtureRepairProgress(completed.slice(0, completed.indexOf(answers[0])), toolRun.thread, turn, cwd, marker, shells), 4, 'E_REPAIR_ANSWER_ORDER');
  assert.equal(await readFile(join(cwd, 'probe-input.txt'), 'utf8'), marker + '\n', 'E_INPUT_CHANGED');
  assert.equal(await readFile(join(cwd, 'probe-output.txt'), 'utf8'), marker + '\n', 'E_REPAIR_CONTENT');
  assert.deepEqual((await readdir(cwd)).sort(), ['probe-input.txt', 'probe-output.txt'], 'E_UNEXPECTED_FILE');
  evidence.nativeRepair = { ...evidence.nativeRepair, result: 'passed', exactRead: true, realTestFailed: true, exactRepair: true, realRetestPassed: true, finalAfterRetest: true, inputUnchanged: true, outputCorrect: true, approvedSteps: [...toolRun.approvedSteps] };
  toolRun = undefined;
}
async function verifyDenial(selectedModel) {
  evidence.nativeDenial = { result: 'started', harnessExecutedTools: false, fixtureMarkerInPrompt: false };
  const marker = `CXWEB_UNREAD_${randomBytes(16).toString('hex')}`;
  const expected = `CXWEB_DENIED_${randomBytes(8).toString('hex')}`;
  await writeFile(join(cwd, 'probe-input.txt'), marker + '\n', { flag: 'wx' });
  const started = await rpc('thread/start', { cwd, model, ephemeral: true, approvalPolicy: 'untrusted', sandbox: 'read-only' });
  assert.equal(started.model, model, 'E_SELECTED_MODEL');
  assert.equal(started.modelProvider, 'openai', 'E_NATIVE_PROVIDER');
  toolRun = { thread: started.thread.id, denial: true, readDenied: false };
  const prompt = `This is an approval-denial test. Request exec_command exactly once with cmd exactly ${JSON.stringify(fixtureReadCommand)}, login=false and the current working directory. Wait for its actual result. The user will decline the request. Only after receiving the refusal, return exactly ${expected}. Do not retry, read the file another way, use another tool, create or change files, request elevated permissions or access the network. Do not guess the file contents.`;
  const turn = (await rpc('turn/start', { threadId: toolRun.thread, effort: selectedModel.defaultReasoningEffort, input: [{ type: 'text', text: prompt, text_elements: [] }] })).turn.id;
  assert.ok(!toolRun.turn || toolRun.turn === turn, 'E_TURN_IDENTITY');
  toolRun.turn = turn;
  console.log(JSON.stringify({ phase: 'native command denial', model }));
  const deadline = Date.now() + 600000;
  let done;
  while (Date.now() < deadline) {
    assert.ok(!failure, failure);
    done = events.find(event => event.method === 'turn/completed' && event.params?.threadId === toolRun.thread && event.params?.turn?.id === turn);
    if (done) break;
    assert.equal(child.exitCode, null, 'E_CLIENT_EXIT');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal(done?.params?.turn?.status, 'completed', 'E_TURN_FAILED');
  assert.ok(toolRun.readDenied, 'E_DENIAL_NOT_OBSERVED');
  const completed = events.filter(event => event.method === 'item/completed' && event.params?.threadId === toolRun.thread && event.params?.turnId === turn);
  const tools = completed.filter(event => !['userMessage', 'agentMessage', 'reasoning'].includes(event.params?.item?.type));
  evidence.nativeDenial.observedCommands = tools.filter(event => event.params.item.type === 'commandExecution').map(event => {
    const item = event.params.item;
    return { status: ['completed', 'failed', 'declined'].includes(item.status) ? item.status : 'other', exitCode: Number.isInteger(item.exitCode) ? item.exitCode : null, exactCommand: approveFixtureRead(item, cwd, shells), outputPresent: typeof item.aggregatedOutput === 'string', markerAbsent: !JSON.stringify(item).includes(marker) };
  });
  assert.ok(completedFixtureDenial(events, toolRun.thread, turn, cwd, marker, shells), 'E_DENIAL_RESULT');
  assert.deepEqual(tools.map(event => event.params.item.type), ['commandExecution'], 'E_TOOL_ORDER');
  const answers = completed.filter(event => event.params.item.type === 'agentMessage');
  assert.equal(answers.length, 1, 'E_ANSWER_COUNT');
  assert.equal(answers[0].params.item.text, expected, 'E_ANSWER_TEXT');
  assert.ok(completed.indexOf(answers[0]) > completed.indexOf(tools[0]), 'E_DENIAL_ANSWER_ORDER');
  assert.ok(!JSON.stringify(events).includes(marker), 'E_DENIED_CONTENT_EXPOSED');
  assert.equal(await readFile(join(cwd, 'probe-input.txt'), 'utf8'), marker + '\n', 'E_INPUT_CHANGED');
  assert.deepEqual(await readdir(cwd), ['probe-input.txt'], 'E_UNEXPECTED_FILE');
  evidence.nativeDenial = { ...evidence.nativeDenial, result: 'passed', exactCommandDeclined: true, noExecutionResult: true, noAlternativeTool: true, noFileChanges: true, unreadMarkerAbsent: true, exactAcknowledgement: true };
  toolRun = undefined;
}
try {
  await rpc('initialize', { clientInfo: { name: 'cxweb_installed_check', version: '0.1.0' }, capabilities: { experimentalApi: true } });
  send({ method: 'initialized', params: {} });
  const config = (await rpc('config/read', { includeLayers: false, cwd })).config;
  const endpoint = new URL(config.openai_base_url);
  assert.ok(endpoint.protocol === 'http:' && endpoint.hostname === '127.0.0.1' && /^\/wb\/[A-Za-z0-9_-]{43}\/backend-api\/codex\/?$/.test(endpoint.pathname), 'E_NOT_INSTALLED');
  assert.ok(!config.model_catalog_json, 'E_STATIC_CATALOG');
  evidence.installedLoopbackRoute = true;
  const account = (await rpc('account/read', { refreshToken: false })).account;
  assert.equal(account?.type, 'chatgpt', 'E_SUBSCRIPTION_REQUIRED');
  evidence.subscriptionAuthentication = true;
  const models = [];
  let cursor = null;
  for (let page = 0; page < 16; page++) {
    const result = await rpc('model/list', { limit: 100, cursor });
    assert.ok(Array.isArray(result.data), 'E_MODELS_SCHEMA');
    models.push(...result.data);
    cursor = result.nextCursor;
    if (!cursor) break;
    assert.ok(page < 15 && typeof cursor === 'string', 'E_MODELS_LIMIT');
  }
  evidence.models = models.map(row => ({ id: row.id, name: row.displayName }));
  const selectedModel = models.find(row => row.id === model);
  assert.ok(selectedModel, 'E_OWNED_MODEL_MISSING');
  assert.ok(['none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'].includes(selectedModel.defaultReasoningEffort), 'E_MODEL_EFFORT');
  evidence.selectedEffort = selectedModel.defaultReasoningEffort;
  evidence.reasoningChoices = selectedModel.supportedReasoningEfforts;
  assert.ok(models.some(row => !row.id.startsWith('webbridge/')), 'E_NATIVE_MODELS_MISSING');
  evidence.ownedAndNativeCatalog = true;
  if (option === '--reasoning') {
    const pro = selectedModel.supportedReasoningEfforts.find(row => row.reasoningEffort === 'max')?.description;
    assert.ok(['Pro', '6 PRO'].includes(pro), 'E_REASONING_PRO_LABEL');
    const expected = [['low', 'Instant'], ['medium', 'Medium'], ['high', 'High'], ['xhigh', 'Extra High'], ['max', pro]];
    assert.deepEqual(selectedModel.supportedReasoningEfforts.map(row => [row.reasoningEffort, row.description]), expected, 'E_REASONING_CATALOG');
    evidence.text = 'started';
    for (const [effort] of expected) await verifyText(selectedModel, effort);
    evidence.text = 'passed';
  } else if (option === '--repair') {
    await verifyRepair(selectedModel);
  } else if (option === '--denial') {
    await verifyDenial(selectedModel);
  } else if (option === '--tools') {
    await verifyTools(selectedModel);
  } else if (option) {
    evidence.text = 'started';
    await verifyText(selectedModel, selectedModel.defaultReasoningEffort);
    if (option === '--coexistence') {
      const native = models.find(row => !row.id.startsWith('webbridge/') && row.isDefault)
        ?? models.find(row => !row.id.startsWith('webbridge/'));
      assert.ok(native, 'E_NATIVE_MODELS_MISSING');
      const effort = native.supportedReasoningEfforts.some(entry => entry.reasoningEffort === 'low') ? 'low' : native.defaultReasoningEffort;
      await verifyText(native, effort);
      await verifyText(selectedModel, selectedModel.defaultReasoningEffort);
      evidence.nativeSubscriptionCoexistence = 'passed: web, native, web in the same client process';
    }
    evidence.text = 'passed';
  }
  evidence.result = 'PASS';
} catch (error) {
  evidence.result = 'FAIL';
  evidence.error = error.message?.match(/E_[A-Z_\/]+/)?.[0] ?? 'E_VERIFICATION';
  evidence.providerErrorCodes = [...new Set(events.flatMap(event => JSON.stringify(event.params?.turn?.error ?? event.params?.error ?? {}).match(/\bE_[A-Z0-9_]+\b/g) ?? []))];
  evidence.turnStatuses = events.filter(event => event.method === 'turn/completed').map(event => event.params?.turn?.status).filter(status => ['completed', 'failed', 'interrupted'].includes(status));
  evidence.errorKinds = [...new Set(events.flatMap(event => {
    const info = event.params?.turn?.error?.codexErrorInfo ?? event.params?.error?.codexErrorInfo;
    return typeof info === 'string' && /^[A-Za-z]{1,64}$/.test(info) ? [info] : info && typeof info === 'object' ? Object.keys(info).filter(key => /^[A-Za-z]{1,64}$/.test(key)) : [];
  }))];
  if (toolRun?.repair) {
    evidence.nativeRepair = { ...evidence.nativeRepair, result: 'failed', completedPrefix: fixtureRepairProgress(events, toolRun.thread, toolRun.turn, cwd, toolRun.marker, shells), approvedSteps: [...toolRun.approvedSteps] };
  } else if (toolRun?.denial) {
    evidence.nativeDenial = { ...evidence.nativeDenial, result: 'failed', denialObserved: toolRun.readDenied };
  } else if (toolRun) {
    const completed = events.filter(event => event.method === 'item/completed' && event.params?.threadId === toolRun.thread && event.params?.turnId === toolRun.turn);
    evidence.nativeTools = {
      ...evidence.nativeTools,
      result: 'failed',
      completedReads: completed.filter(event => event.params?.item?.type === 'commandExecution' && event.params.item.status === 'completed').length,
      completedPatches: completed.filter(event => event.params?.item?.type === 'fileChange' && event.params.item.status === 'completed').length,
      readApprovalObserved: toolRun.readApproved,
      patchApprovalObserved: toolRun.patchApproved,
    };
  }
  process.exitCode = 1;
} finally {
  lines.close();
  child.stdin.end();
  if (child.exitCode === null) child.kill();
  evidence.configUnchanged = sha256(await readFile(configPath)) === configBefore;
  evidence.executableUnchanged = sha256(await readFile(client)) === hash;
  evidence.observedAt = new Date().toISOString();
  if (!evidence.configUnchanged || !evidence.executableUnchanged) { evidence.result = 'FAIL'; process.exitCode = 1; }
  await writeFile(join(cwd, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n');
  console.log(JSON.stringify(evidence, null, 2));
}
