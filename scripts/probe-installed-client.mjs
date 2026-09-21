// Verify an explicitly selected native client's real installed routing.
// Uses its existing subscription. --text consumes web allowance; --coexistence
// also exercises a native subscription model between two independent web turns.
// --tools runs one exact native read/patch exercise in a disposable workspace.
// --reasoning verifies all five qualified choices through actual native turns.
// --unicode verifies exact non-BMP text, accents, quotes and a literal path.
// --denial refuses one exact read and verifies the model receives that refusal.
// --repair observes a failing test, approves one exact correction, then retests.
// --coding=<id> repairs one bounded arithmetic function and runs native tests.
// --namespaces invokes two same-named dynamic tools in separate namespaces.
// --batch requires one response containing both calls, with native serial dispatch.
// --concurrent overlaps two tasks in one native process and checks result isolation.
// No auth files, routing overrides, model catalogs or client binaries are changed.
import { spawn, execFileSync } from 'node:child_process';
import { readFile, readdir, mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createHash, randomBytes } from 'node:crypto';
import { isAbsolute, resolve, join } from 'node:path';
import { createInterface } from 'node:readline';
import assert from 'node:assert/strict';
import { approveFixtureRead, approveFixturePatch, completedFixtureRead, completedFixtureDenial, fixtureReadCommand } from './probe-client-approval.mjs';
import { approveFixtureTest, approveFixtureRepair, fixtureRepairProgress, fixtureTestCommand, fixtureTestPassed, fixtureTestFailed, fixtureBrokenOutput } from './probe-client-approval.mjs';
import { approveFixtureCommand, fixtureCommandDiagnostic } from './probe-client-approval.mjs';
import { cases as codingCases, fixtureFiles, fingerprint as codingFingerprint, version as codingVersion } from './coding-fixture.mjs';
import codingRunner from './coding-fixture-runner.cjs';
import * as codingApproval from './coding-fixture-approval.mjs';
import { NamespaceFixture, definitions as namespaceTools, namespaces, argument as namespaceArgument } from './namespace-fixture.mjs';

const [client, home, model, option] = process.argv.slice(2);
const codingCase = option?.startsWith('--coding=') ? codingCases.find(fixture => fixture.id === option.slice(9)) : undefined;
assert.ok(client && home && model?.startsWith('webbridge/') && isAbsolute(client) && isAbsolute(home));
assert.ok(process.argv.length <= 6 && (!option || codingCase || ['--text', '--unicode', '--coexistence', '--tools', '--reasoning', '--reasoning-trace', '--denial', '--repair', '--namespaces', '--batch', '--concurrent', '--web-tools', '--web-tools-deferred'].includes(option)));
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
if (codingCase || ['--tools', '--denial', '--repair'].includes(option)) for (const name of ['pwsh.exe', 'powershell.exe']) {
  try { shells.push(...execFileSync('where.exe', [name], { encoding: 'utf8', windowsHide: true }).trim().split(/\r?\n/)); } catch { /* optional shell absent */ }
}
const deferred = option === '--web-tools-deferred';
const child = spawn(client, ['app-server', ...(deferred ? ['-c', 'features.tool_search=true', '-c', 'features.tool_search_always_defer_mcp_tools=true'] : [])], { cwd, env: { ...process.env, CODEX_HOME: home }, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
const pending = new Map();
const events = [];
let failure, bytes = 0, id = 0;
let toolRun;
let namespaceRun;
const send = value => child.stdin.write(JSON.stringify(value) + '\n');
const lines = createInterface({ input: child.stdout });
lines.on('line', line => {
  bytes += Buffer.byteLength(line);
  if (bytes > 64 * 1024 * 1024 || events.length > 4096) { failure = 'E_OUTPUT_LIMIT'; child.kill(); return; }
  let message;
  try { message = JSON.parse(line); } catch { failure = 'E_PROTOCOL'; child.kill(); return; }
  if (message.method && message.id !== undefined) {
    if (message.method === 'item/tool/call' && namespaceRun) {
      try { send({ id: message.id, result: namespaceRun.answer(message.params) }); }
      catch (error) {
        failure = error.message?.match(/E_[A-Z_]+/)?.[0] ?? 'E_DYNAMIC_TOOL';
        send({ id: message.id, error: { code: -32602, message: failure } });
      }
      return;
    }
    const command = message.method === 'item/commandExecution/requestApproval';
    const patch = message.method === 'item/fileChange/requestApproval';
    const params = message.params;
    const scoped = toolRun?.turn && params?.threadId === toolRun.thread && params?.turnId === toolRun.turn;
    let accepted = false, intentionalDenial = false;
    if (scoped && toolRun.coding) {
      const progress = codingApproval.completedProgress(events, toolRun, cwd, shells);
      (toolRun.approvalDiagnostics ??= []).push({
        phase: progress, command, patch,
        exactRead: approveFixtureCommand(params, cwd, shells, codingApproval.readCommand),
        exactTest: approveFixtureCommand(params, cwd, shells, toolRun.testCommand),
        testCommandDiagnostic: command ? fixtureCommandDiagnostic(params, cwd, toolRun.testCommand) : null,
        kindIsCommand: params.kind == null || params.kind === 'command',
        extraPermissions: params.additionalPermissions != null,
        explicitLocalEnvironment: params.environmentId === 'local',
        alternateEnvironment: params.environmentId != null && params.environmentId !== 'local',
        alternateApproval: params.approvalId != null,
        networkContext: params.networkApprovalContext != null,
        ordinaryAcceptOffered: params.availableDecisions == null || (Array.isArray(params.availableDecisions) && params.availableDecisions.includes('accept')),
      });
      if (progress >= 0 && progress < 4 && !toolRun.approvedSteps.has(progress)) {
        if (command && [0, 1, 3].includes(progress)) {
          accepted = approveFixtureCommand(params, cwd, shells, progress === 0 ? codingApproval.readCommand : toolRun.testCommand);
        } else if (patch && progress === 2) {
          const started = events.findLast(event => event.method === 'item/started' && event.params?.item?.id === params.itemId);
          const candidate = started?.params?.item;
          toolRun.patchDiagnostic = { rootGrant: params.grantRoot != null, itemPresent: !!candidate,
            itemInProgress: candidate?.status === 'inProgress',
            sameThread: started?.params?.threadId === params.threadId, sameTurn: started?.params?.turnId === params.turnId,
            admissibleSource: codingApproval.changedSource(candidate, cwd, toolRun.initial) !== null,
            changes: candidate?.changes?.map(change => ({
              update: change.kind?.type === 'update', move: change.kind?.move_path != null || change.kind?.movePath != null,
              targetMatches: typeof change.path === 'string' && resolve(cwd, change.path).toLowerCase() === resolve(cwd, 'solve.cjs').toLowerCase(),
              diffLines: typeof change.diff === 'string' ? change.diff.split('\n').length : null,
              boundedHunk: typeof change.diff === 'string' && /^@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@\n/.test(change.diff),
            })) ?? [] };
          // Only this synthetic fixture's own source proposal is retained for
          // local diagnosis. Never archive native events or account data.
          toolRun.sourceProposal = candidate?.changes?.length === 1
            && toolRun.patchDiagnostic.changes[0].targetMatches
            && typeof candidate.changes[0].diff === 'string' && candidate.changes[0].diff.length <= 4096
            ? candidate.changes[0].diff : undefined;
          accepted = codingApproval.approvePatch(params, started, cwd, toolRun.initial);
        }
        if (accepted) toolRun.approvedSteps.add(progress);
      }
    } else if (scoped && toolRun.repair) {
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
    if (namespaceRun && message.method === 'turn/started' && message.params?.threadId === namespaceRun.thread) {
      try { namespaceRun.bindTurn(message.params.turn?.id); } catch { failure = 'E_TURN_IDENTITY'; }
    }
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
if (deferred) { evidence.configurationOverride = true; evidence.overrideFields = ['features.tool_search', 'features.tool_search_always_defer_mcp_tools']; evidence.routingOverride = false; }
async function verifyText(selectedModel, effort) {
  const check = { model: selectedModel.id, effort, route: selectedModel.id.startsWith('webbridge/') ? 'web' : 'native', result: 'started' };
  (evidence.textChecks ??= []).push(check);
  console.log(JSON.stringify({ phase: 'text', route: check.route, model: check.model, effort }));
  const expected = `CXWEB_INSTALLED_${randomBytes(8).toString('hex')}` + (option === '--unicode' ? ' café 🦀 Ω "quoted" C:\\fixture\\input.txt' : '');
  check.unicode = option === '--unicode';
  check.expectedUtf8Sha256 = sha256(Buffer.from(expected, 'utf8'));
  const started = (await rpc('thread/start', { cwd, model: selectedModel.id, ephemeral: true, approvalPolicy: 'untrusted', sandbox: 'read-only' }));
  assert.equal(started.model, selectedModel.id, 'E_SELECTED_MODEL');
  assert.equal(started.modelProvider, 'openai', 'E_NATIVE_PROVIDER');
  const thread = started.thread.id;
  if (option === '--concurrent') check.threadSha256 = sha256(thread);
  const text = option === '--unicode'
    ? `Use no tools. Return a final answer containing exactly this JSON-decoded string: ${JSON.stringify(expected)}`
    : `Use no tools. Return a final answer with exactly this text: ${expected}`;
  const turn = (await rpc('turn/start', { threadId: thread, effort, input: [{ type: 'text', text, text_elements: [] }] })).turn.id;
  if (option === '--concurrent') check.startedAt = new Date().toISOString();
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
  if (option === '--concurrent') check.completedAt = new Date().toISOString();
}
async function verifyWebTools() {
  evidence.nativeMcpSearch = { result: 'started', harnessExecutedTool: false };
  const started = await rpc('thread/start', { cwd, model, ephemeral: true, experimentalRawEvents: true, approvalPolicy: 'untrusted', sandbox: 'read-only' });
  assert.equal(started.model, model, 'E_SELECTED_MODEL');
  const query = 'Rust programming language official website';
  const prompt = `Use the cxweb_web MCP search tool exactly once, with query exactly ${JSON.stringify(query)} and limit=3. Discover it using tool_search if needed. Wait for its real result. Then return exactly the first result title, a newline, and its link. Do not guess the result, use other tools, or change files.`;
  const turn = (await rpc('turn/start', { threadId: started.thread.id, effort: 'medium', summary: 'auto', input: [{ type: 'text', text: prompt, text_elements: [] }] })).turn.id;
  console.log(JSON.stringify({ phase: 'native MCP web search', model }));
  const deadline = Date.now() + 600000;
  let done;
  while (Date.now() < deadline) {
    assert.ok(!failure, failure);
    done = events.find(event => event.method === 'turn/completed' && event.params?.threadId === started.thread.id && event.params?.turn?.id === turn);
    if (done) break;
    assert.equal(child.exitCode, null, 'E_CLIENT_EXIT');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal(done?.params?.turn?.status, 'completed', 'E_TURN_FAILED');
  const completed = events.filter(event => event.method === 'item/completed' && event.params?.threadId === started.thread.id && event.params?.turnId === turn).map(event => event.params.item);
  const calls = completed.filter(item => item.type === 'mcpToolCall');
  assert.equal(calls.length, 1, 'E_MCP_CALL_COUNT');
  const call = calls[0];
  evidence.nativeMcpSearch.status = call.status;
  assert.equal(call.server, 'cxweb_web', 'E_MCP_SERVER');
  assert.equal(call.tool, 'search', 'E_MCP_TOOL');
  assert.deepEqual(call.arguments, { query, limit: 3 }, 'E_MCP_ARGUMENTS');
  assert.equal(call.status, 'completed', 'E_MCP_FAILED');
  assert.ok(!call.error, 'E_MCP_FAILED');
  const text = call.result.content.find(part => part.type === 'text')?.text;
  const result = JSON.parse(text);
  assert.equal(result.provider, 'Bing', 'E_SEARCH_PROVIDER');
  assert.ok(result.results.length > 0, 'E_SEARCH_EMPTY');
  const first = result.results[0];
  const answers = completed.filter(item => item.type === 'agentMessage');
  assert.equal(answers.length, 1, 'E_ANSWER_COUNT');
  assert.equal(answers[0].text, first.title + '\n' + first.link, 'E_MCP_RESULT_RECALL');
  const summaries = completed.filter(item => item.type === 'reasoning');
  evidence.nativeMcpSearch = { result: 'passed', harnessExecutedTool: false, actualNativeMcpCall: true,
    exactArguments: true, realPublicSearchResults: result.results.length, exactResultRecall: true,
    summaryItems: summaries.length, publicSummaryCharacters: summaries.reduce((n, item) => n + (item.summary ?? []).join('').length, 0),
    deferredToolDiscovery: completed.some(item => item.type === 'toolSearch') };
  assert.deepEqual(await readdir(cwd), [], 'E_UNEXPECTED_FILE');
}
async function verifyPublicReasoning() {
  const congruences = [[97n,42n],[101n,17n],[103n,89n],[107n,66n]];
  let expected = 0n, step = 1n;
  for (const [modulus, remainder] of congruences) { while (expected % modulus !== remainder) expected += step; step *= modulus; }
  const started = await rpc('thread/start', { cwd, model, ephemeral: true, experimentalRawEvents: true, approvalPolicy: 'untrusted', sandbox: 'read-only' });
  const prompt = 'Find the least nonnegative integer x such that x is congruent to 42 modulo 97, 17 modulo 101, 89 modulo 103, and 66 modulo 107. Verify all four congruences. Use no tools. Return only the decimal integer.';
  const turn = (await rpc('turn/start', { threadId: started.thread.id, effort: 'xhigh', summary: 'auto', input: [{ type:'text', text:prompt, text_elements:[] }] })).turn.id;
  console.log(JSON.stringify({ phase:'public reasoning trace', model }));
  const deadline = Date.now() + 600000;
  let done;
  while (Date.now() < deadline) {
    assert.ok(!failure, failure);
    done = events.find(event => event.method === 'turn/completed' && event.params?.threadId === started.thread.id && event.params?.turn?.id === turn);
    if (done) break;
    assert.equal(child.exitCode, null, 'E_CLIENT_EXIT');
    await new Promise(resolve => setTimeout(resolve,100));
  }
  assert.equal(done?.params?.turn?.status,'completed','E_TURN_FAILED');
  const items = events.filter(event => event.method === 'item/completed' && event.params?.threadId === started.thread.id && event.params?.turnId === turn).map(event => event.params.item);
  const answer = items.filter(item => item.type === 'agentMessage');
  assert.equal(answer.length,1,'E_ANSWER_COUNT');
  assert.equal(answer[0].text,expected.toString(),'E_ARITHMETIC_ANSWER');
  const chars = items.filter(item => item.type === 'reasoning').reduce((n,item) => n+(item.summary ?? []).join('').length,0);
  evidence.publicReasoning = { correctAnswer:true, summaryCharacters:chars, buffered:true };
  assert.ok(chars > 0,'E_PUBLIC_REASONING_NOT_OBSERVED');
}

async function verifyNamespaces(selectedModel) {
  evidence.nativeNamespaces = { result: 'started', fixtureValuesInPrompt: false, bridgeExecutesTools: false, requireSingleResponseBatch: option === '--batch' };
  const values = Object.fromEntries(namespaces.map(name => [name, `value-${randomBytes(12).toString('hex')}`]));
  const started = await rpc('thread/start', { cwd, model, ephemeral: true, approvalPolicy: 'untrusted', sandbox: 'read-only', dynamicTools: namespaceTools });
  assert.equal(started.model, model, 'E_SELECTED_MODEL');
  assert.equal(started.modelProvider, 'openai', 'E_NATIVE_PROVIDER');
  namespaceRun = new NamespaceFixture(started.thread.id, values);
  const parallel = option === '--batch' ? ' Emit both calls together in one response without waiting for the first result. Native Codex decides their execution order.' : '';
  const prompt = `This is a namespace isolation test. Invoke cxweb_alpha.read and cxweb_beta.read exactly once each with arguments ${JSON.stringify({ key: namespaceArgument })}.${parallel} These are two different tools with the same leaf name. Use no other tools. Read both actual results, then return exactly two lines in this order: cxweb_alpha=<its returned value> and cxweb_beta=<its returned value>. Do not guess either value. Do not modify files.`;
  const turn = (await rpc('turn/start', { threadId: namespaceRun.thread, effort: selectedModel.defaultReasoningEffort, input: [{ type: 'text', text: prompt, text_elements: [] }] })).turn.id;
  namespaceRun.bindTurn(turn);
  console.log(JSON.stringify({ phase: 'native namespace collision', model }));
  const deadline = Date.now() + 600000;
  let done;
  while (Date.now() < deadline) {
    assert.ok(!failure, failure);
    done = events.find(event => event.method === 'turn/completed' && event.params?.threadId === namespaceRun.thread && event.params?.turn?.id === turn);
    if (done) break;
    assert.equal(child.exitCode, null, 'E_CLIENT_EXIT');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal(done?.params?.turn?.status, 'completed', 'E_TURN_FAILED');
  assert.deepEqual(namespaceRun.calls.map(call => call.namespace).sort(), namespaces, 'E_NAMESPACE_CALLS');
  if (option === '--batch') assert.ok(namespaceRun.sameResponseBatch(), 'E_RESPONSE_BATCH');
  const completed = events.filter(event => event.method === 'item/completed' && event.params?.threadId === namespaceRun.thread && event.params?.turnId === turn).map(event => event.params.item);
  const tools = completed.filter(item => !['userMessage', 'agentMessage', 'reasoning'].includes(item.type));
  assert.deepEqual(tools.map(item => item.type), ['dynamicToolCall', 'dynamicToolCall'], 'E_TOOL_ORDER');
  assert.ok(tools.every(item => item.status === 'completed' && item.success === true), 'E_DYNAMIC_COMPLETION');
  const answers = completed.filter(item => item.type === 'agentMessage');
  assert.equal(answers.length, 1, 'E_ANSWER_COUNT');
  assert.equal(answers[0].text, namespaceRun.expected(), 'E_NAMESPACE_RESULT');
  assert.ok(tools.every(item => completed.indexOf(item) < completed.indexOf(answers[0])), 'E_RESULT_ORDER');
  assert.deepEqual(await readdir(cwd), [], 'E_UNEXPECTED_FILE');
  evidence.nativeNamespaces = { ...evidence.nativeNamespaces, result: 'passed', namespaces: namespaceRun.calls.map(call => call.namespace), exactUnicodeArguments: true, distinctCallIds: true, exactResultAssociation: true, finalAfterBothResults: true, noFileChanges: true,
    sameResponseBatch: namespaceRun.sameResponseBatch(), nativeExecutionPolicyUnchanged: true };
  namespaceRun = undefined;
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
async function verifyCoding(selectedModel) {
  const files = fixtureFiles(codingCase);
  files['tests.cjs'] = await readFile(new URL('./coding-fixture-runner.cjs', import.meta.url), 'utf8');
  for (const [name, contents] of Object.entries(files)) await writeFile(join(cwd, name), contents, { flag: 'wx' });
  const testCommand = codingApproval.testCommand(process.execPath);
  const before = codingRunner.check(files['solve.cjs'], codingCase.tests);
  assert.ok(before.failed > 0, 'E_FIXTURE_NOT_BROKEN');
  evidence.nativeCoding = { result: 'started', case: codingCase.id, corpusVersion: codingVersion,
    corpusSha256: codingFingerprint, testRunnerSha256: sha256(Buffer.from(files['tests.cjs'])),
    nodeVersion: process.version, nodeExecutableSha256: sha256(await readFile(process.execPath)),
    harnessExecutedNativeTools: false, actualNativeProcess: true, before };
  const started = await rpc('thread/start', { cwd, model, ephemeral: true, approvalPolicy: 'untrusted', sandbox: 'read-only' });
  assert.equal(started.model, model, 'E_SELECTED_MODEL');
  assert.equal(started.modelProvider, 'openai', 'E_NATIVE_PROVIDER');
  toolRun = { thread: started.thread.id, coding: true, initial: files['solve.cjs'], caseJson: files['cases.json'],
    testCommand, before, approvedSteps: new Set() };
  const prompt = `Repair the arithmetic function solve(a, b) in solve.cjs. ${codingCase.instruction} First use exec_command with cmd exactly ${JSON.stringify(codingApproval.readCommand)}, login=false and the current working directory. Next use exec_command with cmd exactly ${JSON.stringify(testCommand)}, login=false and the current working directory. Observe its actual failing result before editing. Use apply_patch once to replace only the return expression in solve.cjs, preserving the three-line wrapper. The expression may contain only a, b, integer literals, spaces, parentheses and arithmetic, comparison, logical or conditional operators; no other identifiers, calls, strings, property access, assignments, comments or additional statements. Do not change cases.json or tests.cjs. After the patch succeeds, run the identical test command and observe exit code 0 with all cases passed. Only then return exactly ${codingApproval.passedMessage}. Wait for every actual tool result. Do not run other commands, edit other files, request elevated permissions or access the network.`;
  const turn = (await rpc('turn/start', { threadId: toolRun.thread, effort: selectedModel.defaultReasoningEffort, input: [{ type: 'text', text: prompt, text_elements: [] }] })).turn.id;
  assert.ok(!toolRun.turn || toolRun.turn === turn, 'E_TURN_IDENTITY');
  toolRun.turn = turn;
  console.log(JSON.stringify({ phase: 'native arithmetic repair', case: codingCase.id, model }));
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
  assert.equal(codingApproval.completedProgress(events, toolRun, cwd, shells), 4, 'E_CODING_SEQUENCE');
  const completed = events.filter(event => event.method === 'item/completed' && event.params?.threadId === toolRun.thread && event.params?.turnId === turn);
  const answers = completed.filter(event => event.params.item.type === 'agentMessage');
  assert.equal(answers.length, 1, 'E_ANSWER_COUNT');
  assert.equal(answers[0].params.item.text, codingApproval.passedMessage, 'E_ANSWER_TEXT');
  assert.equal(codingApproval.completedProgress(completed.slice(0, completed.indexOf(answers[0])), toolRun, cwd, shells), 4, 'E_CODING_ANSWER_ORDER');
  const patch = completed.find(event => event.params.item.type === 'fileChange').params.item;
  const actualSource = await readFile(join(cwd, 'solve.cjs'), 'utf8');
  assert.equal(actualSource.replaceAll('\r\n', '\n'), codingApproval.changedSource(patch, cwd, toolRun.initial), 'E_CODING_FILE');
  for (const name of ['cases.json', 'tests.cjs']) assert.equal(await readFile(join(cwd, name), 'utf8'), files[name], 'E_TESTS_CHANGED');
  assert.deepEqual((await readdir(cwd)).sort(), Object.keys(files).sort(), 'E_UNEXPECTED_FILE');
  assert.equal(sha256(await readFile(process.execPath)), evidence.nativeCoding.nodeExecutableSha256, 'E_NODE_CHANGED');
  evidence.nativeCoding = { ...evidence.nativeCoding, result: 'passed', actualRead: true, realTestFailed: true,
    boundedPatch: true, realRetestPassed: true, finalAfterRetest: true, testInputsUnchanged: true,
    finalSourceSha256: sha256(Buffer.from(actualSource)), approvedSteps: [...toolRun.approvedSteps] };
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
  if (option === '--reasoning-trace') {
    await verifyPublicReasoning();
  } else if (['--web-tools', '--web-tools-deferred'].includes(option)) {
    await verifyWebTools();
  } else if (option === '--reasoning') {
    const pro = selectedModel.supportedReasoningEfforts.find(row => row.reasoningEffort === 'max')?.description;
    assert.ok(['Pro', '6 PRO'].includes(pro), 'E_REASONING_PRO_LABEL');
    const expected = [['low', 'Instant'], ['medium', 'Medium'], ['high', 'High'], ['xhigh', 'Extra High'], ['max', pro]];
    assert.deepEqual(selectedModel.supportedReasoningEfforts.map(row => [row.reasoningEffort, row.description]), expected, 'E_REASONING_CATALOG');
    evidence.text = 'started';
    for (const [effort] of expected) await verifyText(selectedModel, effort);
    evidence.text = 'passed';
  } else if (option === '--concurrent') {
    evidence.text = 'started';
    assert.ok(selectedModel.supportedReasoningEfforts.some(row => row.reasoningEffort === 'medium'), 'E_MODEL_EFFORT');
    const outcomes = await Promise.allSettled([
      verifyText(selectedModel, selectedModel.defaultReasoningEffort),
      verifyText(selectedModel, 'medium'),
    ]);
    const rejected = outcomes.find(outcome => outcome.status === 'rejected');
    if (rejected) throw rejected.reason;
    const [first, second] = evidence.textChecks;
    assert.notEqual(first.threadSha256, second.threadSha256, 'E_THREAD_ISOLATION');
    assert.notEqual(first.expectedUtf8Sha256, second.expectedUtf8Sha256, 'E_FIXTURE_COLLISION');
    assert.ok(Math.max(Date.parse(first.startedAt), Date.parse(second.startedAt)) < Math.min(Date.parse(first.completedAt), Date.parse(second.completedAt)), 'E_TASKS_DID_NOT_OVERLAP');
    evidence.concurrentTasks = { result: 'passed', oneNativeProcess: true, overlappingTurns: true, exactDistinctAnswers: true, distinctEfforts: first.effort !== second.effort };
    evidence.text = 'passed';
  } else if (['--namespaces', '--batch'].includes(option)) {
    await verifyNamespaces(selectedModel);
  } else if (codingCase) {
    await verifyCoding(selectedModel);
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
  if (namespaceRun) evidence.nativeNamespaces = { ...evidence.nativeNamespaces, result: 'failed', acceptedCalls: namespaceRun.calls.length };
  if (toolRun?.coding) {
    evidence.nativeCoding = { ...evidence.nativeCoding, result: 'failed',
      completedPrefix: codingApproval.completedProgress(events, toolRun, cwd, shells), approvedSteps: [...toolRun.approvedSteps],
      approvalDiagnostics: toolRun.approvalDiagnostics ?? [],
      patchDiagnostic: toolRun.patchDiagnostic ?? null,
      completedItems: events.filter(event => event.method === 'item/completed' && event.params?.threadId === toolRun.thread && event.params?.turnId === toolRun.turn)
        .map(event => event.params.item).filter(item => !['userMessage', 'agentMessage', 'reasoning'].includes(item.type))
        .map(item => ({ type: ['commandExecution', 'fileChange'].includes(item.type) ? item.type : 'other',
          status: ['completed', 'failed', 'declined'].includes(item.status) ? item.status : 'other',
          exitCode: Number.isInteger(item.exitCode) ? item.exitCode : null,
          exactRead: approveFixtureCommand(item, cwd, shells, codingApproval.readCommand),
          exactTest: approveFixtureCommand(item, cwd, shells, toolRun.testCommand),
          testResult: (() => {
            if (!approveFixtureCommand(item, cwd, shells, toolRun.testCommand)) return null;
            try {
              const value = JSON.parse(item.aggregatedOutput);
              return { total: Number.isInteger(value.total) ? value.total : null, passed: Number.isInteger(value.passed) ? value.passed : null,
                failed: Number.isInteger(value.failed) ? value.failed : null,
                error: ['E_FIXTURE_SOURCE', 'E_FIXTURE_CASES', 'E_FIXTURE_INPUT', 'E_FIXTURE_TIMEOUT', 'E_FIXTURE_EVALUATION'].includes(value.error) ? value.error : null };
            } catch { return null; }
          })(),
          exactReadOutput: typeof item.aggregatedOutput === 'string' && item.aggregatedOutput.replaceAll('\r\n', '\n').trim() === (toolRun.initial + toolRun.caseJson).trim(),
          outputLength: typeof item.aggregatedOutput === 'string' ? item.aggregatedOutput.length : null,
        })) };
  } else if (toolRun?.repair) {
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
  evidence.reasoningEvents = Object.fromEntries([...new Set(events.map(event => event.method).filter(method => typeof method === 'string' && /reasoning/i.test(method)))].map(method => [method, events.filter(event => event.method === method).length]));
  evidence.rawOutputKinds = Object.fromEntries([...new Set(events.filter(event => event.method === 'rawResponseItem/completed').map(event => event.params?.item?.type).filter(Boolean))].map(type => [type, events.filter(event => event.method === 'rawResponseItem/completed' && event.params?.item?.type === type).length]));
  if (evidence.nativeMcpSearch) evidence.nativeMcpSearch.deferredToolDiscovery = (evidence.rawOutputKinds.tool_search_call ?? 0) > 0;
  if (deferred && evidence.result === 'PASS' && !evidence.nativeMcpSearch?.deferredToolDiscovery) {
    evidence.result = 'FAIL'; evidence.error = 'E_DEFERRED_TOOL_DISCOVERY_NOT_OBSERVED'; process.exitCode = 1;
  }
  if (toolRun?.sourceProposal) await writeFile(cwd + '.patch.txt', toolRun.sourceProposal, { flag: 'wx' });
  if (!evidence.configUnchanged || !evidence.executableUnchanged) { evidence.result = 'FAIL'; process.exitCode = 1; }
  await writeFile(join(cwd, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n');
  console.log(JSON.stringify(evidence, null, 2));
}
