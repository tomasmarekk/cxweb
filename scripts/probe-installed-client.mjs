// Verify an explicitly selected native client's real installed routing.
// Uses its existing subscription and consumes allowance when --text is supplied.
// No auth files, routing overrides, model catalogs or client binaries are changed.
import { spawn } from 'node:child_process';
import { readFile, mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createHash, randomBytes } from 'node:crypto';
import { isAbsolute, resolve, join } from 'node:path';
import { createInterface } from 'node:readline';
import assert from 'node:assert/strict';

const [client, home, model, option] = process.argv.slice(2);
assert.ok(client && home && model?.startsWith('webbridge/') && isAbsolute(client) && isAbsolute(home));
assert.ok(process.argv.length <= 6 && (!option || option === '--text'));
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
const child = spawn(client, ['app-server'], { cwd, env: { ...process.env, CODEX_HOME: home }, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
const pending = new Map();
const events = [];
let failure, bytes = 0, id = 0;
const send = value => child.stdin.write(JSON.stringify(value) + '\n');
const lines = createInterface({ input: child.stdout });
lines.on('line', line => {
  bytes += Buffer.byteLength(line);
  if (bytes > 64 * 1024 * 1024 || events.length > 4096) { failure = 'E_OUTPUT_LIMIT'; child.kill(); return; }
  let message;
  try { message = JSON.parse(line); } catch { failure = 'E_PROTOCOL'; child.kill(); return; }
  if (message.method && message.id !== undefined) {
    // Text-only verification never approves tools or other client-side actions.
    send({ id: message.id, error: { code: -32601, message: 'No actions approved by this text test' } });
    failure = 'E_UNEXPECTED_ACTION';
  } else if (message.id !== undefined) {
    const callback = pending.get(message.id);
    pending.delete(message.id);
    callback?.(message);
  } else { events.push(message); }
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
const evidence = { schema: 'cxweb.installed-client.v1', build: builds.get(hash), executableSha256: hash, synthetic: false, configurationOverride: false, actualGuiPicker: 'not observed', text: 'not requested' };
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
  assert.ok(models.some(row => !row.id.startsWith('webbridge/')), 'E_NATIVE_MODELS_MISSING');
  evidence.ownedAndNativeCatalog = true;
  if (option === '--text') {
    evidence.text = 'started';
    const expected = `CXWEB_INSTALLED_${randomBytes(8).toString('hex')}`;
    const thread = (await rpc('thread/start', { cwd, model, ephemeral: true, approvalPolicy: 'untrusted', sandbox: 'read-only' })).thread.id;
    const turn = (await rpc('turn/start', { threadId: thread, effort: selectedModel.defaultReasoningEffort, input: [{ type: 'text', text: `Use no tools. Return a final answer with exactly this text: ${expected}`, text_elements: [] }] })).turn.id;
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
  process.exitCode = 1;
} finally {
  lines.close();
  child.stdin.end();
  if (child.exitCode === null) child.kill();
  evidence.configUnchanged = sha256(await readFile(configPath)) === configBefore;
  evidence.executableUnchanged = sha256(await readFile(client)) === hash;
  if (!evidence.configUnchanged || !evidence.executableUnchanged) { evidence.result = 'FAIL'; process.exitCode = 1; }
  await writeFile(join(cwd, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n');
  console.log(JSON.stringify(evidence, null, 2));
}
