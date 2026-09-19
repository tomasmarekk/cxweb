import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../ui/app.js', import.meta.url), 'utf8');
const flush = () => new Promise(resolve => setImmediate(resolve));
function panel(respond) {
  const element = () => ({
    children: [],
    replaceChildren(...nodes) { this.children = nodes; },
    append(...nodes) { this.children.push(...nodes); },
    classList: { toggle() {} },
    addEventListener(event, action) { this[event] = action; }
  });
  const nodes = new Map();
  const calls = [];
  const requests = [];
  const timers = new Map();
  const document = {
    hidden: false,
    addEventListener() {},
    createElement: element,
    getElementById(id) {
      if (!nodes.has(id)) nodes.set(id, element());
      return nodes.get(id);
    }
  };
  vm.runInNewContext(source, {
    document,
    window: { __TAURI__: { core: { invoke(command, params) {
      calls.push(command);
      requests.push({ command, refresh: params?.refresh });
      return respond(command, params);
    } } } },
    setTimeout(action) { const token = {}; timers.set(token, action); return token; },
    clearTimeout(token) { timers.delete(token); }
  });
  return { nodes, calls, requests, timers, document };
}

test('tool protocol evidence is distinguished from actual Codex execution', async () => {
  const ui = panel(async () => ({ phase: 'tool_protocol_qualified', tool_qualified_model: 'webbridge/fixture' }));
  await flush();
  assert.equal(ui.nodes.get('heading').textContent, 'Tool protocol test passed');
  assert.equal(ui.nodes.get('codex').textContent, 'Awaiting integration');
  assert.match(ui.nodes.get('description').textContent, /Execution through Codex still needs verification/);
  assert.deepEqual(ui.calls, ['status']);
  await ui.nodes.get('connect').click();
  assert.deepEqual(ui.calls, ['status', 'qualify']);
});

test('login starts only after explicit action and failed browser can reconnect', async () => {
  let phase = 'disconnected';
  const ui = panel(async () => ({ phase }));
  await flush();
  assert.deepEqual(ui.calls, ['status']);
  assert.equal(ui.requests[0].refresh, false);
  phase = 'authenticating';
  ui.nodes.get('connect').click();
  await flush();
  assert.deepEqual(ui.calls, ['status', 'connect']);
  assert.equal(ui.timers.size, 0);
  phase = 'browser_unavailable';
  ui.nodes.get('connect').click();
  await flush();
  assert.equal(ui.requests.at(-1).refresh, true);
  assert.equal(ui.timers.size, 0);
  ui.nodes.get('connect').click();
  await flush();
  assert.equal(ui.calls.at(-1), 'connect');
});

test('an observed session stays unqualified and stops automatic polling', async () => {
  const ui = panel(async () => ({ phase: 'awaiting_qualification' }));
  await flush();
  assert.equal(ui.nodes.get('heading').textContent, 'Session awaiting verification');
  assert.equal(ui.timers.size, 0);
});

test('background loading checks status without opening a sign-in window', async () => {
  const ui = panel(async () => ({ phase: 'authenticating', background_session: true }));
  await flush();
  assert.equal(ui.nodes.get('heading').textContent, 'Restoring ChatGPT session');
  await ui.nodes.get('connect').click();
  assert.deepEqual(ui.calls, ['status', 'status']);
});

test('expired background session opens sign-in only on explicit action', async () => {
  const ui = panel(async () => ({ phase: 'authenticating', background_session: true, observation: { login_action: true } }));
  await flush();
  assert.deepEqual(ui.calls, ['status']);
  assert.equal(ui.nodes.get('connect').textContent, 'Open sign-in window');
  await ui.nodes.get('connect').click();
  assert.deepEqual(ui.calls, ['status', 'connect']);
});

test('a verification challenge offers the visible verification window without retrying', async () => {
  const ui = panel(async () => ({ phase: 'authenticating', background_session: true, observation: { verification_required: true } }));
  await flush();
  assert.equal(ui.nodes.get('connect').textContent, 'Open sign-in window');
  assert.equal(ui.nodes.get('heading').textContent, 'ChatGPT verification required');
  assert.match(ui.nodes.get('description').textContent, /Background requests are unavailable/);
  assert.equal(ui.timers.size, 0);
  assert.deepEqual(ui.calls, ['status']);
});

test('IPC failure is visible and releases the action button', async () => {
  const ui = panel(async () => { throw 'E_ALREADY_RUNNING'; });
  await flush();
  assert.equal(ui.nodes.get('error').hidden, false);
  assert.match(ui.nodes.get('error').textContent, /cxweb is already running/);
  assert.equal(ui.nodes.get('connect').disabled, false);
  assert.equal(ui.timers.size, 0);
});

test('a restarted runtime resets the former session display', async () => {
  let phase = 'awaiting_qualification';
  const ui = panel(async () => ({ phase }));
  await flush();
  phase = 'disconnected';
  ui.nodes.get('connect').click();
  await flush();
  assert.equal(ui.nodes.get('chatgpt').textContent, 'Signed out');
  assert.equal(ui.nodes.get('connect').textContent, 'Connect ChatGPT');
});

test('text test requires an explicit click and duplicate clicks do not send twice', async () => {
  let finish;
  const ui = panel(async command => command === 'qualify_text'
    ? new Promise(resolve => { finish = resolve; })
    : { phase: 'candidates_observed', temporary_chat_available: true, candidate_models: [] });
  await flush();
  assert.deepEqual(ui.calls, ['status']);
  assert.equal(ui.nodes.get('qualification').hidden, false);
  ui.nodes.get('test-text').click();
  ui.nodes.get('test-text').click();
  await flush();
  assert.deepEqual(ui.calls, ['status', 'qualify_text']);
  assert.equal(ui.nodes.get('test-text').disabled, true);
  finish({ phase: 'text_qualified' });
  await flush();
  assert.equal(ui.nodes.get('heading').textContent, 'Text test passed');
  assert.equal(ui.nodes.get('codex').textContent, 'Awaiting integration');
});

test('unverified Temporary Chat cannot run a text test', async () => {
  const ui = panel(async () => ({ phase: 'candidates_observed', temporary_chat_available: false }));
  await flush();
  ui.nodes.get('test-text').click();
  await flush();
  assert.deepEqual(ui.calls, ['status']);
});

test('failed text qualification is never retried or displayed as passed', async () => {
  const ui = panel(async command => {
    if (command === 'qualify_text') throw 'E_SUBMISSION_UNCERTAIN';
    return { phase: 'candidates_observed', temporary_chat_available: true };
  });
  await flush();
  await ui.nodes.get('test-text').click();
  assert.deepEqual(ui.calls, ['status', 'qualify_text']);
  assert.equal(ui.nodes.get('heading').textContent, 'Model candidates found');
  assert.match(ui.nodes.get('error').textContent, /not retried/);
  assert.equal(ui.nodes.get('test-text').disabled, false);
});
