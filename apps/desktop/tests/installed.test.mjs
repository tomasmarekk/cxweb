import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const installed = await readFile(new URL('../ui/installed.js', import.meta.url), 'utf8');
const app = await readFile(new URL('../ui/app.js', import.meta.url), 'utf8');
const flush = () => new Promise(resolve => setImmediate(resolve));
const target = { installation: 'a'.repeat(32), home: 'C:\\fixture\\home' };
function health(overall = 'preflight', turns = 0) {
  const components = {};
  for (const name of ['runtime', 'browser', 'web_auth', 'web_models', 'native_upstream', 'codex_app', 'codex_cli', 'config']) components[name] = { state: 'unknown', observed_at: null, evidence: 'none', code: null };
  components.runtime.state = 'healthy';
  components.web_auth.state = 'healthy';
  return { instance: 'b'.repeat(32), health: { overall, active_web_turns: turns, components } };
}
function panel(respond) {
  const nodes = new Map(), calls = [], timers = new Map(), events = new Map();
  let focus = null;
  const element = id => ({
    value: '', children: [], hidden: false, disabled: false, open: false,
    replaceChildren(...nodes) { this.children = nodes; }, append(...nodes) { this.children.push(...nodes); },
    classList: { toggle() {} }, addEventListener(event, action) { this[event] = action; },
    focus() { focus = id; }, showModal() { this.open = true; }, close() { this.open = false; }
  });
  const document = {
    hidden: false,
    addEventListener(event, action) { if (!events.has(event)) events.set(event, []); events.get(event).push(action); },
    createElement: () => element('created'),
    getElementById(id) { if (!nodes.has(id)) nodes.set(id, element(id)); return nodes.get(id); }
  };
  const window = { __TAURI__: { core: { invoke(command, params) { calls.push({ command, params }); return respond(command, params); } } } };
  const context = vm.createContext({ document, window,
    setTimeout(action, delay) { const token = {}; timers.set(token, { action, delay }); return token; },
    clearTimeout(token) { timers.delete(token); }
  });
  vm.runInContext(installed, context); vm.runInContext(app, context);
  return { nodes, calls, timers, document, get focus() { return focus; },
    visibility(hidden) { document.hidden = hidden; for (const action of events.get('visibilitychange') || []) action(); }
  };
}
const list = (targets = [target], diagnostics = []) => ({ targets, diagnostics });

function transient(code = 'E_ALREADY_RUNNING') {
  const value = health('unavailable');
  value.health.components.browser = { state: 'unavailable', code };
  return value;
}

test('background retry is explicit, instance-bound and single while status checks stay passive', async () => {
  let finish;
  const ui = panel(async command => command === 'installed_list' ? list() : command === 'installed_retry_web'
    ? new Promise(resolve => { finish = resolve; }) : transient());
  await flush();
  const button = ui.nodes.get('installed-retry');
  assert.equal(button.hidden, false); assert.equal(button.disabled, false);
  assert.deepEqual(ui.calls.map(c => c.command), ['installed_list', 'installed_check']);
  const pending = button.click(); await button.click();
  assert.equal(ui.calls.filter(c => c.command === 'installed_retry_web').length, 1);
  assert.equal(ui.calls.at(-1).params.instance, 'b'.repeat(32));
  assert.equal(ui.calls.at(-1).params.installation, target.installation);
  assert.match(ui.nodes.get('installed-description').textContent, /No message is sent/);
  assert.equal(ui.nodes.get('installed-remove').disabled, true);
  finish(health()); await pending;
  assert.equal(button.hidden, true);
  assert.equal(ui.nodes.get('installed-app').textContent, 'Unverified');
});

test('login, compatibility, cleanup and active work do not offer background retry', async () => {
  for (const code of ['E_LOGIN_REQUIRED', 'E_SESSION_SCOPE', 'E_BROWSER_VERSION_CHANGED', 'E_BROWSER_LANGUAGE', 'E_WEB_RECOVERY_CLEANUP']) {
    const ui = panel(async command => command === 'installed_list' ? list() : transient(code));
    await flush(); await ui.nodes.get('installed-retry').click();
    assert.equal(ui.nodes.get('installed-retry').hidden, true);
    assert.ok(ui.calls.every(c => c.command !== 'installed_retry_web'));
  }
  for (const active of [false, true]) {
    const value = transient();
    if (active) value.health.active_web_turns = 1;
    else value.health.components.runtime.state = 'degraded';
    const ui = panel(async command => command === 'installed_list' ? list() : value);
    await flush(); assert.equal(ui.nodes.get('installed-retry').hidden, true);
  }
});

test('failed retry clears stale status and never targets a restarted runtime automatically', async () => {
  const ui = panel(async command => {
    if (command === 'installed_list') return list();
    if (command === 'installed_retry_web') throw 'E_INSTALLED_CHANGED';
    return transient();
  });
  await flush(); await ui.nodes.get('installed-retry').click();
  assert.match(ui.nodes.get('installed-error').textContent, /runtime restarted/);
  assert.equal(ui.nodes.get('installed-retry').hidden, true);
  assert.equal(ui.nodes.get('installed-chatgpt').textContent, 'Unverified');
  assert.deepEqual(ui.calls.map(c => c.command), ['installed_list', 'installed_check', 'installed_retry_web']);
});

test('startup attaches an installed host without starting login or generation', async () => {
  const ui = panel(async command => command === 'installed_list' ? list() : health());
  await flush();
  assert.deepEqual(ui.calls.map(c => c.command), ['installed_list', 'installed_check']);
  assert.equal(ui.nodes.get('setup-view').hidden, true);
  assert.equal(ui.nodes.get('installed-view').hidden, false);
  assert.equal(ui.nodes.get('installed-heading').textContent, 'Verifying connection');
  assert.equal(ui.nodes.get('installed-app').textContent, 'Unverified');
  assert.equal(ui.nodes.get('installed-cli').textContent, 'Unverified');
  assert.equal(ui.nodes.get('installed-chatgpt').textContent, 'Verified');
});
test('only a complete empty inventory falls through to the login controller', async () => {
  const ui = panel(async command => command === 'installed_list' ? list([]) : { phase: 'disconnected' });
  await flush();
  assert.deepEqual(ui.calls.map(c => c.command), ['installed_list', 'status']);
  assert.equal(ui.nodes.get('setup-view').hidden, false);
  assert.equal(ui.nodes.get('installed-view').hidden, true);
  assert.equal(ui.calls[1].params.refresh, false);
});
test('unreadable inventory or stopped runtime never starts a competing login process', async () => {
  for (const failure of ['inventory', 'runtime']) {
    const ui = panel(async command => {
      if (command === 'installed_list') return failure === 'inventory' ? list([], ['E_INSTALLED_JOURNAL']) : list();
      throw 'E_INSTALLED_UNAVAILABLE';
    });
    await flush();
    assert.equal(ui.nodes.get('setup-view').hidden, true);
    assert.equal(ui.nodes.get('installed-remove').disabled, true);
    assert.equal(ui.nodes.get('installed-heading').textContent, 'Connection status unavailable');
    assert.ok(ui.calls.every(c => c.command.startsWith('installed_')));
  }
});
test('multiple installed homes require selection and send only that identity', async () => {
  const other = { installation: 'c'.repeat(32), home: 'C:\\fixture\\second' };
  const ui = panel(async command => command === 'installed_list' ? list([target, other]) : health());
  await flush();
  assert.deepEqual(ui.calls.map(c => c.command), ['installed_list']);
  const choice = ui.nodes.get('installed-choice'); choice.value = other.installation;
  await choice.change();
  assert.equal(ui.calls.at(-1).params.installation, other.installation);
});
test('idle removal is single, instance-bound and leaves restart requirements visible', async () => {
  let finish;
  const ui = panel(async command => command === 'installed_list' ? list() : command === 'installed_disconnect'
    ? new Promise(resolve => { finish = resolve; }) : health());
  await flush();
  const removal = ui.nodes.get('installed-remove').click();
  await ui.nodes.get('installed-remove').click();
  assert.equal(ui.calls.filter(c => c.command === 'installed_disconnect').length, 1);
  const request = ui.calls.at(-1).params;
  assert.equal(request.installation, target.installation);
  assert.equal(request.instance, 'b'.repeat(32));
  assert.equal(request.allowActive, false);
  finish(health('removal_pending_restart')); await removal;
  assert.equal(ui.nodes.get('installed-heading').textContent, 'Finish removal');
  assert.equal(ui.nodes.get('installed-remove').disabled, true);
  assert.ok(!ui.calls.some(c => ['connect', 'background', 'status'].includes(c.command)));
});
test('busy removal needs a dialog and Escape keeps the connection', async () => {
  const ui = panel(async command => command === 'installed_list' ? list() : health('busy', 1));
  await flush(); await ui.nodes.get('installed-remove').click();
  assert.equal(ui.nodes.get('installed-confirm').open, true);
  assert.equal(ui.focus, 'installed-keep');
  assert.equal(ui.nodes.get('installed-choice').disabled, true);
  let prevented = false;
  ui.nodes.get('installed-confirm').cancel({ preventDefault() { prevented = true; } });
  assert.equal(prevented, true);
  assert.equal(ui.nodes.get('installed-confirm').open, false);
  assert.equal(ui.focus, 'installed-remove');
  assert.ok(!ui.calls.some(c => c.command === 'installed_disconnect'));
});
test('a new turn after the idle snapshot prompts instead of retrying with cancellation', async () => {
  const ui = panel(async (command, params) => {
    if (command === 'installed_list') return list();
    if (command === 'installed_disconnect') {
      if (!params.allowActive) throw 'E_WEB_ACTIVE';
      return health('removal_pending_restart');
    }
    return health();
  });
  await flush(); await ui.nodes.get('installed-remove').click();
  assert.equal(ui.nodes.get('installed-confirm').open, true);
  assert.equal(ui.calls.filter(c => c.command === 'installed_disconnect').length, 1);
  await ui.nodes.get('installed-stop').click();
  const removals = ui.calls.filter(c => c.command === 'installed_disconnect');
  assert.deepEqual(removals.map(c => c.params.allowActive), [false, true]);
  assert.equal(removals[0].params.instance, removals[1].params.instance);
  assert.equal(ui.nodes.get('installed-heading').textContent, 'Finish removal');
});
test('replacement runtime refusal does not retarget or resubmit a confirmed removal', async () => {
  const ui = panel(async command => {
    if (command === 'installed_list') return list();
    if (command === 'installed_disconnect') throw 'E_INSTALLED_CHANGED';
    return health('busy', 1);
  });
  await flush(); await ui.nodes.get('installed-remove').click(); await ui.nodes.get('installed-stop').click();
  assert.equal(ui.calls.filter(c => c.command === 'installed_disconnect').length, 1);
  assert.equal(ui.nodes.get('installed-remove').disabled, true);
  assert.match(ui.nodes.get('installed-error').textContent, /runtime restarted/);
});
test('idle polling is passive, spaced 30 seconds apart and suspended while hidden', async () => {
  const ui = panel(async command => command === 'installed_list' ? list() : health());
  await flush();
  assert.equal(ui.timers.size, 1);
  assert.equal([...ui.timers.values()][0].delay, 30000);
  ui.visibility(true); assert.equal(ui.timers.size, 0);
  const count = ui.calls.length;
  ui.visibility(false); await flush();
  assert.equal(ui.calls.length, count + 1);
  assert.equal(ui.calls.at(-1).command, 'installed_check');
  assert.equal([...ui.timers.values()][0].delay, 30000);
});
test('a failed refresh clears earlier verified rows without retrying or starting a browser', async () => {
  let fail = false;
  const ui = panel(async command => {
    if (command === 'installed_list') return list();
    if (fail) throw 'E_INSTALLED_UNAVAILABLE';
    return health();
  });
  await flush(); fail = true; await ui.nodes.get('installed-refresh').click();
  assert.equal(ui.nodes.get('installed-chatgpt').textContent, 'Unverified');
  assert.equal(ui.nodes.get('installed-remove').disabled, true);
  assert.equal(ui.calls.length, 3);
});
