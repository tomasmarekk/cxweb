import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../ui/app.js', import.meta.url), 'utf8');
const flush = () => new Promise(resolve => setImmediate(resolve));
function panel(respond) {
  const element = () => ({
    children: [],
    value: '',
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
      requests.push({ command, refresh: params?.refresh, params });
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
  assert.deepEqual(ui.calls, ['status', 'qualify_text', 'status']);
  assert.equal(ui.requests.at(-1).refresh, false);
  assert.equal(ui.nodes.get('heading').textContent, 'Model candidates found');
  assert.match(ui.nodes.get('error').textContent, /not retried/);
  assert.equal(ui.nodes.get('test-text').disabled, false);
});

test('text success offers one explicit tool test and serializes every action', async () => {
  let finish;
  const ui = panel(async command => command === 'qualify_tools'
    ? new Promise(resolve => { finish = resolve; })
    : { phase: 'text_qualified', temporary_chat_available: true, text_qualified_model: 'webbridge/fixture' });
  await flush();
  assert.equal(ui.nodes.get('tool-qualification').hidden, false);
  assert.deepEqual(ui.calls, ['status']);
  ui.nodes.get('test-tools').click();
  ui.nodes.get('test-tools').click();
  ui.nodes.get('background').click();
  ui.nodes.get('connect').click();
  await flush();
  assert.deepEqual(ui.calls, ['status', 'qualify_tools']);
  for (const id of ['connect', 'test-text', 'test-tools', 'background']) assert.equal(ui.nodes.get(id).disabled, true);
  finish({ phase: 'tool_protocol_qualified', background_session: true });
  await flush();
  assert.equal(ui.nodes.get('codex').textContent, 'Awaiting integration');
  assert.equal(ui.nodes.get('tool-qualification').hidden, true);
  for (const id of ['connect', 'test-text', 'test-tools', 'background']) assert.equal(ui.nodes.get(id).disabled, false);
});

test('tool test requires text evidence and verified Temporary Chat', async () => {
  for (const status of [
    { phase: 'candidates_observed', temporary_chat_available: true },
    { phase: 'text_qualified', temporary_chat_available: true },
    { phase: 'text_qualified', text_qualified_model: 'webbridge/fixture', temporary_chat_available: false },
  ]) {
    const ui = panel(async () => status);
    await flush();
    assert.equal(ui.nodes.get('tool-qualification').hidden, true);
    await ui.nodes.get('test-tools').click();
    assert.deepEqual(ui.calls, ['status']);
  }
});

test('uncertain tool test is not retried and does not claim tool success', async () => {
  let failed = false;
  const ui = panel(async command => {
    if (command === 'qualify_tools') { failed = true; throw 'E_SUBMISSION_UNCERTAIN'; }
    if (failed) return { phase: 'awaiting_qualification' };
    return { phase: 'text_qualified', temporary_chat_available: true, text_qualified_model: 'webbridge/fixture' };
  });
  await flush();
  await ui.nodes.get('test-tools').click();
  assert.deepEqual(ui.calls, ['status', 'qualify_tools', 'status']);
  assert.equal(ui.requests.at(-1).refresh, false);
  assert.equal(ui.nodes.get('heading').textContent, 'Session awaiting verification');
  assert.equal(ui.nodes.get('codex').textContent, 'Awaiting verification');
  assert.equal(ui.nodes.get('tool-qualification').hidden, true);
  assert.match(ui.nodes.get('error').textContent, /not retried/);
  assert.equal(ui.nodes.get('test-tools').disabled, false);
});

test('background transition requires a detected session and an explicit action', async () => {
  const ui = panel(async command => command === 'background'
    ? { phase: 'authenticating', background_session: true }
    : { phase: 'awaiting_qualification' });
  await flush();
  assert.deepEqual(ui.calls, ['status']);
  assert.equal(ui.nodes.get('background-control').hidden, false);
  await ui.nodes.get('background').click();
  assert.deepEqual(ui.calls, ['status', 'background']);
  assert.equal(ui.nodes.get('heading').textContent, 'Restoring ChatGPT session');
  assert.equal(ui.nodes.get('background-control').hidden, true);
  assert.equal(ui.nodes.get('background-status').hidden, true, 'loading does not claim a working background session');
  assert.equal(ui.nodes.get('codex').textContent, 'Awaiting verification');
});

test('background action is unavailable before login and after transition', async () => {
  for (const status of [
    { phase: 'disconnected' },
    { phase: 'authenticating' },
    { phase: 'browser_unavailable' },
    { phase: 'awaiting_qualification', background_session: true },
  ]) {
    const ui = panel(async () => status);
    await flush();
    assert.equal(ui.nodes.get('background-control').hidden, true);
    await ui.nodes.get('background').click();
    assert.deepEqual(ui.calls, ['status']);
    assert.equal(ui.nodes.get('background-status').hidden, status.background_session !== true);
  }
});

test('a busy browser preserves its window and releases controls without retries', async () => {
  const ui = panel(async command => {
    if (command === 'background') throw 'E_BROWSER_BUSY';
    return { phase: 'awaiting_qualification' };
  });
  await flush();
  await ui.nodes.get('background').click();
  assert.deepEqual(ui.calls, ['status', 'background', 'status']);
  assert.equal(ui.requests.at(-1).refresh, false);
  assert.match(ui.nodes.get('error').textContent, /draft or an active response/);
  assert.equal(ui.nodes.get('background-status').hidden, true);
  assert.equal(ui.nodes.get('background').disabled, false);
});

test('other browser tabs prevent background transition without an automatic retry', async () => {
  const ui = panel(async command => {
    if (command === 'background') throw 'E_BROWSER_OTHER_PAGES';
    return { phase: 'awaiting_qualification' };
  });
  await flush();
  await ui.nodes.get('background').click();
  assert.deepEqual(ui.calls, ['status', 'background', 'status']);
  assert.equal(ui.requests.at(-1).refresh, false);
  assert.match(ui.nodes.get('error').textContent, /Other cxweb browser tabs.*left open/);
  assert.equal(ui.nodes.get('background-status').hidden, true);
  assert.equal(ui.nodes.get('background').disabled, false);
});

test('failed operation and unavailable receipt suppress old test actions', async () => {
  let failed = false;
  const ui = panel(async command => {
    if (command === 'qualify_tools') { failed = true; throw 'E_CONTROL_TIMEOUT'; }
    if (failed) throw 'E_CONTROL_UNAVAILABLE';
    return { phase: 'text_qualified', temporary_chat_available: true, text_qualified_model: 'webbridge/fixture' };
  });
  await flush();
  await ui.nodes.get('test-tools').click();
  assert.deepEqual(ui.calls, ['status', 'qualify_tools', 'status']);
  assert.equal(ui.nodes.get('heading').textContent, 'Connection status unavailable');
  assert.equal(ui.nodes.get('tool-qualification').hidden, true);
  assert.equal(ui.nodes.get('background-control').hidden, true);
  assert.equal(ui.nodes.get('connect').textContent, 'Check status');
  assert.equal(ui.nodes.get('diagnostic').textContent, 'E_CONTROL_TIMEOUT');
  await ui.nodes.get('test-tools').click();
  assert.equal(ui.calls.length, 3);
  await ui.nodes.get('connect').click();
  assert.equal(ui.calls.at(-1), 'status');
  assert.equal(ui.requests.at(-1).refresh, true);
});

test('all UI command names are registered and allowed only for the main local window', async () => {
  const capability = JSON.parse(await readFile(new URL('../src-tauri/capabilities/main.json', import.meta.url), 'utf8'));
  assert.deepEqual(capability.windows, ['main']);
  assert.equal(capability.remote, undefined);
  const rust = await readFile(new URL('../src-tauri/src/main.rs', import.meta.url), 'utf8');
  const build = await readFile(new URL('../src-tauri/build.rs', import.meta.url), 'utf8');
  for (const command of ['connect', 'status', 'qualify', 'qualify_text', 'qualify_tools', 'background', 'native_discover', 'native_preflight']) {
    assert.ok(capability.permissions.includes(`allow-${command.replaceAll('_', '-')}`));
    assert.ok(build.includes(`"${command}"`));
    assert.match(rust, new RegExp(`async fn ${command}\\(`));
    const permission = await readFile(new URL(`../src-tauri/permissions/autogenerated/${command}.toml`, import.meta.url), 'utf8');
    assert.ok(permission.includes(`commands.allow = ["${command}"]`));
  }
});

test('native discovery is explicit, deduplicates clicks and preserves browser status', async () => {
  let finish;
  const ui = panel(async command => command === 'native_discover'
    ? new Promise(resolve => { finish = resolve; })
    : { phase: 'awaiting_qualification' });
  await flush();
  assert.deepEqual(ui.calls, ['status']);
  ui.nodes.get('native-discover').click();
  ui.nodes.get('native-discover').click();
  await flush();
  assert.deepEqual(ui.calls, ['status', 'native_discover']);
  assert.equal(ui.nodes.get('native-discover').disabled, true);
  finish({ candidates: [
    { executable: 'C:\\fixture\\codex.exe', reviewed_build: '0.155.1', sources: ['npm_installation', 'path_executable'] },
    { executable: 'C:\\fixture\\<untrusted>\\codex.exe', reviewed_build: null, sources: ['desktop_backend_cache'] },
  ], diagnostics: ['E_DISCOVERY_TARGET_IDENTITY'] });
  await flush();
  assert.equal(ui.nodes.get('native-discover').disabled, false);
  assert.equal(ui.nodes.get('heading').textContent, 'Session awaiting verification');
  assert.equal(ui.nodes.get('codex').textContent, 'Awaiting verification');
  const results = ui.nodes.get('native-targets');
  assert.equal(results.hidden, false);
  const items = results.children[1].children;
  assert.match(items[0].textContent, /Reviewed backend 0.155.1 \(npm installation, PATH\)/);
  assert.match(items[1].textContent, /Unreviewed backend \(App backend cache\)/);
  assert.ok(items[1].textContent.includes('<untrusted>'));
  assert.match(results.children[2].textContent, /E_DISCOVERY_TARGET_IDENTITY/);
  assert.match(results.children[3].textContent, /does not identify the backend used/);
});

test('empty or failed discovery does not reconnect, retry or change native configuration', async () => {
  for (const fail of [false, true]) {
    const ui = panel(async command => {
      if (command !== 'native_discover') return { phase: 'disconnected' };
      if (fail) throw 'untrusted failure text';
      return { candidates: [], diagnostics: [] };
    });
    await flush();
    await ui.nodes.get('native-discover').click();
    assert.deepEqual(ui.calls, ['status', 'native_discover']);
    assert.equal(ui.nodes.get('heading').textContent, 'Sign in to ChatGPT');
    assert.equal(ui.nodes.get('native-discover').disabled, false);
    const results = ui.nodes.get('native-targets');
    if (fail) assert.match(results.textContent, /could not be inspected/);
    else assert.match(results.children[0].textContent, /custom installation may need an explicit path/);
  }
});

const selectedTarget = { client: 'C:\\fixture\\codex.exe', home: 'C:\\fixture\\home', cwd: 'C:\\fixture\\workspace' };
function fillTarget(ui) {
  for (const [id, key] of [['native-client', 'client'], ['native-home', 'home'], ['native-cwd', 'cwd']]) {
    ui.nodes.get(id).value = selectedTarget[key];
    ui.nodes.get(id).input();
  }
}
const submitTarget = ui => ui.nodes.get('native-preflight-form').submit({ preventDefault() {} });
function preflightReport(compatible = false) {
  return {
    client_build: '0.155.1', activation_eligible: false,
    assessment: { configuration_compatible: compatible, auth_mode: compatible ? 'subscription' : 'signed_out', active_layers: ['user'], conflicts: compatible ? [] : ['subscription_auth_required'] },
    remaining_checks: ['actual client picker and native coexistence'],
  };
}

test('selected target inspection requires all paths and an explicit submit', async () => {
  const ui = panel(async command => command === 'native_preflight' ? preflightReport() : { phase: 'awaiting_qualification' });
  await flush();
  await submitTarget(ui);
  assert.deepEqual(ui.calls, ['status']);
  assert.match(ui.nodes.get('native-preflight-result').textContent, /all three absolute paths/);
  fillTarget(ui);
  assert.deepEqual(ui.calls, ['status']);
  await submitTarget(ui);
  assert.deepEqual(ui.calls, ['status', 'native_preflight']);
  assert.deepEqual({ ...ui.requests.at(-1).params }, selectedTarget);
  const results = ui.nodes.get('native-preflight-result');
  assert.match(results.children[0].textContent, /requirements to resolve/);
  assert.match(results.children[1].textContent, /Native account mode: Signed out/);
  assert.match(results.children[2].children[0].textContent, /requires native Codex subscription sign-in/);
  assert.equal(ui.nodes.get('heading').textContent, 'Session awaiting verification');
});

test('compatible selected configuration never claims active integration or picker success', async () => {
  const ui = panel(async command => command === 'native_preflight' ? preflightReport(true) : { phase: 'text_qualified' });
  await flush();
  fillTarget(ui); await submitTarget(ui);
  const results = ui.nodes.get('native-preflight-result');
  assert.match(results.children[0].textContent, /No configuration conflict/);
  assert.match(results.children[0].textContent, /Integration is not active/);
  assert.match(results.children[3].textContent, /actual client picker and native coexistence/);
  assert.match(results.children[3].textContent, /does not certify an already-running Codex window/);
  assert.equal(ui.nodes.get('codex').textContent, 'Awaiting integration');
  ui.nodes.get('native-home').value = 'C:\\other-home';
  ui.nodes.get('native-home').input();
  assert.equal(results.hidden, true);
  assert.equal(results.children.length, 0);
});

test('pending preflight rejects duplicate submits and suppresses a result for an edited target', async () => {
  let finish;
  const ui = panel(async command => command === 'native_preflight'
    ? new Promise(resolve => { finish = resolve; }) : { phase: 'disconnected' });
  await flush(); fillTarget(ui);
  const pending = submitTarget(ui);
  await submitTarget(ui);
  await ui.nodes.get('native-discover').click();
  assert.deepEqual(ui.calls, ['status', 'native_preflight']);
  for (const id of ['native-client', 'native-home', 'native-cwd', 'native-choice', 'native-preflight', 'native-discover']) assert.equal(ui.nodes.get(id).disabled, true);
  ui.nodes.get('native-cwd').value = 'C:\\changed';
  ui.nodes.get('native-cwd').input();
  finish(preflightReport(true)); await pending;
  assert.equal(ui.nodes.get('native-preflight-result').hidden, true);
  assert.equal(ui.nodes.get('native-preflight-result').children.length, 0);
  assert.equal(ui.nodes.get('native-preflight').disabled, false);
});

test('discovered candidates require selection and unreviewed candidates cannot be selected', async () => {
  const ui = panel(async command => command === 'native_discover' ? { candidates: [
    { executable: selectedTarget.client, reviewed_build: '0.155.1', sources: ['npm_installation'] },
    { executable: 'C:\\unknown\\codex.exe', reviewed_build: null, sources: ['path_executable'] },
  ], diagnostics: [] } : { phase: 'disconnected' });
  await flush(); await ui.nodes.get('native-discover').click();
  const choice = ui.nodes.get('native-choice');
  assert.equal(ui.nodes.get('native-client').value, '');
  assert.equal(choice.children[0].value, '');
  assert.equal(choice.children[1].disabled, false);
  assert.equal(choice.children[2].disabled, true);
  choice.value = selectedTarget.client; choice.change();
  assert.equal(ui.nodes.get('native-client').value, selectedTarget.client);
  assert.deepEqual(ui.calls, ['status', 'native_discover']);
});

test('preflight failures stay local, are not retried and do not export arbitrary errors', async () => {
  for (const error of ['E_PREFLIGHT_CLIENT_UNQUALIFIED', 'PRIVATE_BACKEND_ERROR']) {
    const ui = panel(async command => {
      if (command === 'native_preflight') throw error;
      return { phase: 'awaiting_qualification' };
    });
    await flush(); fillTarget(ui); await submitTarget(ui);
    assert.deepEqual(ui.calls, ['status', 'native_preflight']);
    const text = ui.nodes.get('native-preflight-result').textContent;
    assert.ok(!text.includes('PRIVATE_'));
    if (error === 'E_PREFLIGHT_CLIENT_UNQUALIFIED') assert.match(text, /It was not started/);
    else assert.match(text, /could not be verified/);
    assert.equal(ui.nodes.get('native-preflight').disabled, false);
    assert.equal(ui.nodes.get('heading').textContent, 'Session awaiting verification');
  }
});
