import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../ui/app.js', import.meta.url), 'utf8');
const flush = () => new Promise(resolve => setImmediate(resolve));
function panel(respond) {
  const nodes = new Map();
  const calls = [];
  const requests = [];
  const timers = new Map();
  const document = {
    hidden: false,
    addEventListener() {},
    getElementById(id) {
      if (!nodes.has(id)) nodes.set(id, {
        classList: { toggle() {} },
        addEventListener(event, action) { this[event] = action; }
      });
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
