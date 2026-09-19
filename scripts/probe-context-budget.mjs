// Synthetic native accounting experiment. No browser, real key or user home.
// Control usage below is deliberately fictional test data, never product usage.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { isAbsolute, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import { zstdDecompressSync } from 'node:zlib';

const executable = process.argv[2];
if (!executable || process.argv.length !== 3) throw new Error('Expected one reviewed absolute backend path');
assert.ok(isAbsolute(executable), 'backend path must be absolute');
const fingerprint = createHash('sha256').update(await readFile(executable)).digest('hex');
const builds = new Map([
  ['eba0f32c976667cb9298efafd98513e823eeda7b576a03ec658bb8be8d336316', '0.155.1'],
  ['bc45017e8239dc150258f69309ced9df6bbcdf5b8e4f346decf780ac0999e226', '0.155.0-alpha.9.2'],
]);
const build = builds.get(fingerprint);
assert.ok(build, 'only a reviewed native executable may run');
const root = resolve('.local/probes');
await mkdir(root, { recursive: true });
const work = await mkdtemp(join(root, 'context-budget-'));
const catalog = JSON.parse(execFileSync(resolve('target/release/cxweb.exe'), ['probe-catalog', '--client-build', build], { encoding: 'utf8', windowsHide: true }));
const localBudgetCatalog = JSON.parse(execFileSync(resolve('target/release/cxweb.exe'), ['probe-catalog', '--client-build', build, '--compaction'], { encoding: 'utf8', windowsHide: true }));
const model = catalog.models.find(model => model.slug === 'webbridge/diagnostic');
assert.ok(model);
// This deliberately small synthetic ceiling is scoped to the disposable home.
model.context_window = 12000;
model.auto_compact_token_limit = 3000;
const report = {
  schema: 'cxweb.native-context-budget-probe.v1', client: build, executable_sha256: fingerprint,
  synthetic: true, production_usage_changed: false, browser_used: false,
  synthetic_control_limits: { context_window: 12000, auto_compact_token_limit: 3000 },
  local_budget_limits: {
    context_window: localBudgetCatalog.models[0].context_window,
    max_context_window: localBudgetCatalog.models[0].max_context_window,
    auto_compact_token_limit: localBudgetCatalog.models[0].auto_compact_token_limit,
    source: 'Unmodified cxweb diagnostic compaction catalog; local estimate, not provider capacity',
  }, cases: [],
};

for (const mode of ['missing_usage', 'synthetic_usage_control', 'http_context_error_control', 'sse_context_error_control', 'sse_context_error_default_catalog', 'sse_context_error_local_budget']) {
  const home = join(work, mode, 'home');
  const cwd = join(work, mode, 'workspace');
  await mkdir(home, { recursive: true });
  await mkdir(cwd);
  const catalogPath = join(home, 'catalog.json');
  const localBudget = mode === 'sse_context_error_local_budget';
  const caseCatalog = structuredClone(localBudget ? localBudgetCatalog : catalog);
  const defaultCatalog = mode === 'sse_context_error_default_catalog';
  if (defaultCatalog) {
    for (const entry of caseCatalog.models) {
      delete entry.context_window;
      delete entry.auto_compact_token_limit;
    }
  }
  await writeFile(catalogPath, JSON.stringify(caseCatalog));
  const contextErrorMode = mode.includes('context_error');
  const sseContextError = mode.startsWith('sse_context_error');
  const capability = `/fixture-${randomUUID()}/backend-api/codex`;
  let responses = 0;
  let compactions = 0;
  let contextErrors = 0;
  let checkpointContinuations = 0;
  let upgrades = 0;
  let rejected = 0;
  const rejectionReasons = { method: 0, path: 0, authorization: 0 };
  let unauthenticatedRequests = 0;
  const inputSizes = [];
  const checkpoint = 'synthetic-context-budget-fixture-only';
  const answer = 'fixture '.repeat(4096);
  const server = createServer(async (request, response) => {
    try {
      // Match the established diagnostic gateway's explicit HTTP fallback.
      if (request.method === 'GET' && request.url === `${capability}/responses`) {
        upgrades++; response.writeHead(426); response.end(); return;
      }
      if (request.method !== 'POST' || request.url !== `${capability}/responses`
        || (request.headers.authorization !== undefined && request.headers.authorization !== 'Bearer cxweb-synthetic-context-probe')) {
        rejected++;
        if (request.method !== 'POST') rejectionReasons.method++;
        if (request.url !== `${capability}/responses`) rejectionReasons.path++;
        if (request.headers.authorization !== undefined && request.headers.authorization !== 'Bearer cxweb-synthetic-context-probe') rejectionReasons.authorization++;
        response.writeHead(403); response.end(); return;
      }
      if (!request.headers.authorization) unauthenticatedRequests++;
      const chunks = []; let length = 0;
      for await (const chunk of request) {
        length += chunk.length;
        if (length > 4 * 1024 * 1024) throw new Error('E_FIXTURE_LIMIT');
        chunks.push(chunk);
      }
      let body = Buffer.concat(chunks);
      if (request.headers['content-encoding'] === 'zstd') body = zstdDecompressSync(body, { maxOutputLength: 4 * 1024 * 1024 });
      else assert.ok(!request.headers['content-encoding']);
      const payload = JSON.parse(body);
      assert.equal(payload.model, 'webbridge/diagnostic');
      assert.ok(Array.isArray(payload.input));
      inputSizes.push(Buffer.byteLength(JSON.stringify(payload.input)));
      const compact = payload.input.at(-1)?.type === 'compaction_trigger';
      const restored = payload.input.some(item => item.type === 'compaction' && item.encrypted_content === checkpoint);
      if (!compact && restored) checkpointContinuations++;
      if (compact) compactions++; else responses++;
      if (contextErrorMode && !compact && responses === 2) {
        contextErrors++;
        if (sseContextError) {
          const failed = { type: 'response.failed', sequence_number: 0, response: { id: 'resp_fixture_context_error', object: 'response', status: 'failed', error: { code: 'context_length_exceeded', message: 'Synthetic context limit fixture' }, output: [] } };
          response.writeHead(200, { 'content-type': 'text/event-stream' });
          response.end(`event: response.failed\ndata: ${JSON.stringify(failed)}\n\n`);
          return;
        }
        response.writeHead(400, { 'content-type': 'application/json' });
        response.end(JSON.stringify({ error: { type: 'invalid_request_error', code: 'context_length_exceeded', message: 'Synthetic context limit fixture' } }));
        return;
      }
      const responseId = `resp_fixture_${responses}_${compactions}`;
      const item = compact
        ? { type: 'compaction', id: `cmp_fixture_${compactions}`, encrypted_content: checkpoint }
        : { type: 'message', id: `msg_fixture_${responses}`, role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: answer, annotations: [] }] };
      const completed = { id: responseId, object: 'response', status: 'completed', output: [item] };
      if (mode === 'synthetic_usage_control' && !compact) {
        completed.usage = { input_tokens: 1000, output_tokens: 5000, total_tokens: 6000 };
      }
      const events = [
        { type: 'response.created', response: { id: responseId, object: 'response', status: 'in_progress', output: [] } },
        { type: 'response.output_item.added', output_index: 0, item: compact ? { ...item, encrypted_content: '' } : { ...item, status: 'in_progress', content: [] } },
        ...(!compact ? [{ type: 'response.output_text.delta', item_id: item.id, output_index: 0, content_index: 0, delta: answer }] : []),
        { type: 'response.output_item.done', output_index: 0, item },
        { type: 'response.completed', response: completed },
      ];
      response.writeHead(200, { 'content-type': 'text/event-stream' });
      response.end(events.map((event, sequence_number) => `event: ${event.type}\ndata: ${JSON.stringify({ ...event, sequence_number })}\n\n`).join(''));
    } catch {
      response.writeHead(500, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ error: { code: 'E_FIXTURE_REQUEST' } }));
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const endpoint = `http://127.0.0.1:${server.address().port}${capability}`;
  await writeFile(join(home, 'config.toml'), `openai_base_url = ${JSON.stringify(endpoint)}\nmodel_catalog_json = ${JSON.stringify(catalogPath.replaceAll('\\', '/'))}\n`);
  const env = { ...process.env };
  for (const key of Object.keys(env)) if (/^(CODEX_|OPENAI_|CHATGPT_)/i.test(key)) delete env[key];
  env.CODEX_HOME = home;
  env.OPENAI_API_KEY = 'cxweb-synthetic-context-probe';
  const client = spawn(executable, ['app-server', '--listen', 'stdio://'], { env, cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
  const exited = new Promise(resolve => client.once('exit', resolve));
  const pending = new Map(); const notifications = []; let id = 0;
  createInterface({ input: client.stdout }).on('line', line => {
    let message; try { message = JSON.parse(line); } catch { return; }
    if (pending.has(message.id)) {
      const entry = pending.get(message.id); pending.delete(message.id); clearTimeout(entry.timer);
      if (message.error) entry.reject(new Error('E_FIXTURE_RPC')); else entry.resolve(message.result);
    } else if (message.method) {
      notifications.push(message);
      if (message.id !== undefined) client.stdin.write(JSON.stringify({ id: message.id, error: { code: -32601, message: 'No actions are authorized' } }) + '\n');
    }
  });
  const rpc = (method, params) => new Promise((resolve, reject) => {
    const key = ++id;
    const timer = setTimeout(() => { pending.delete(key); reject(new Error('E_FIXTURE_RPC_TIMEOUT')); }, 20000);
    pending.set(key, { resolve, reject, timer });
    client.stdin.write(JSON.stringify({ id: key, method, params }) + '\n');
  });
  const outcome = { mode, default_catalog_limits: defaultCatalog, local_budget_catalog: localBudget, result: 'INCOMPLETE', turns: [], context_error_recognized: false };
  try {
    await rpc('initialize', { clientInfo: { name: 'cxweb_context_probe', version: '0.1.0' }, capabilities: { experimentalApi: true } });
    client.stdin.write(JSON.stringify({ method: 'initialized', params: {} }) + '\n');
    const thread = await rpc('thread/start', { cwd, model: 'webbridge/diagnostic', ephemeral: true });
    for (let index = 0; index < 4; index++) {
      const turn = await rpc('turn/start', { threadId: thread.thread.id, input: [{ type: 'text', text: `Synthetic context accounting turn ${index}.`, text_elements: [] }] });
      let completed;
      for (let poll = 0; poll < 400; poll++) {
        completed = notifications.find(n => n.method === 'turn/completed' && n.params?.turn?.id === turn.turn.id);
        if (completed) break;
        await delay(50);
      }
      assert.ok(completed, 'turn reaches a terminal notification');
      const status = completed.params.turn.status;
      outcome.turns.push(status);
      if (completed.params.turn.error?.codexErrorInfo === 'contextWindowExceeded') outcome.context_error_recognized = true;
      assert.equal(status, contextErrorMode && index === 1 ? 'failed' : 'completed');
    }
    assert.equal(responses, 4, 'no duplicate generation');
    assert.ok(inputSizes.some(size => size > 30000), 'large assistant history was actually supplied');
    if (mode === 'missing_usage' || mode === 'http_context_error_control' || defaultCatalog) assert.equal(compactions, 0, 'missing accounting or catalog limits prevent native automatic compaction');
    else assert.ok(compactions > 0 && checkpointContinuations > 0, 'control causes native automatic compaction and continuation');
    assert.equal(outcome.context_error_recognized, sseContextError);
    outcome.result = 'PASS accounting behavior observed';
  } catch (error) {
    outcome.result = 'FAIL';
    outcome.error = error.code === 'ERR_ASSERTION' ? error.message.split('\n')[0] : 'E_CONTEXT_PROBE';
    process.exitCode = 1;
  } finally {
    for (const entry of pending.values()) clearTimeout(entry.timer);
    client.kill(); await exited;
    server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
    Object.assign(outcome, { responses, compactions, context_errors: contextErrors, checkpoint_continuations: checkpointContinuations, upgrade_probes: upgrades, rejected_requests: rejected, rejection_reasons: rejectionReasons, unauthenticated_requests: unauthenticatedRequests, input_bytes: inputSizes, synthetic_response_bytes: Buffer.byteLength(answer) });
    report.cases.push(outcome);
  }
}
report.executable_unchanged = createHash('sha256').update(await readFile(executable)).digest('hex') === fingerprint;
if (!report.executable_unchanged) process.exitCode = 1;
await writeFile(join(work, 'evidence.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report, null, 2));
console.log(`Evidence: ${join(work, 'evidence.json')}`);
