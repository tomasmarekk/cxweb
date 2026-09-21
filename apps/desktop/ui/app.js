'use strict';
const $ = id => document.getElementById(id);
const invoke = window.__TAURI__?.core.invoke;
let phase = 'disconnected';
let pending = false;
let signInRequired = false;
let discoveryPending = false;
let preflightPending = false;
let targetRevision = 0;
let nativeRoute = null;
let nativeReady = false;
let resetReady = false;
let nativeOperation = null;
let nativeWaiting = false;
let nativeCancelling = false;
let nativeTimer = null;
let nativePolling = false;
let nativePollEpoch = 0;
function scheduleNativeStatus(delay = 500) {
  if (document.hidden || nativePolling || nativeTimer !== null || (!nativeWaiting && !nativeOperation)) return;
  const epoch = nativePollEpoch;
  nativeTimer = setTimeout(async () => {
    nativeTimer = null;
    if (document.hidden) return;
    nativePolling = true;
    let nextDelay = 500;
    try {
      const status = await invoke('status', { refresh: false });
      if (epoch !== nativePollEpoch) return;
      render(status);
    } catch (error) {
      if (epoch === nativePollEpoch) showError(error);
      nextDelay = 2000;
    } finally {
      nativePolling = false;
      scheduleNativeStatus(nextDelay);
    }
  }, delay);
}
document.addEventListener('visibilitychange', () => {
  nativePollEpoch += 1;
  if (nativeTimer !== null) { clearTimeout(nativeTimer); nativeTimer = null; }
  if (!document.hidden) scheduleNativeStatus(0);
});
const nativeControls = ['native-choice', 'native-client', 'native-home', 'native-cwd', 'native-preflight', 'native-discover'];
function updateNativeButton() {
  const running = nativeWaiting || Boolean(nativeOperation);
  $('native-text').disabled = pending || running || preflightPending || discoveryPending || !nativeReady;
  $('activate-codex').disabled = $('native-text').disabled;
  $('native-tools').disabled = $('native-text').disabled;
  $('native-repair').disabled = $('native-text').disabled;
  $('native-denial').disabled = $('native-text').disabled;
  $('reset-test').disabled = pending || running || preflightPending || discoveryPending || !resetReady;
  $('native-cancel').hidden = !running;
  $('native-cancel').disabled = !nativeOperation || nativeCancelling || nativeOperation.cancellation_requested === true;
  if (running) {
    for (const id of ['connect', 'test-text', 'test-tools', 'background', ...nativeControls]) $(id).disabled = true;
  } else if (!pending && !preflightPending && !discoveryPending) {
    for (const id of ['connect', 'test-text', 'test-tools', 'background', ...nativeControls]) $(id).disabled = false;
  }
}
function renderNative(status) {
  nativeOperation = status.native_operation || null;
  nativeRoute = status.tool_qualified_model || null;
  resetReady = ['tool_protocol_qualified', 'generation_ready'].includes(status.phase);
  $('reset-test').hidden = !resetReady;
  $('reset-test-help').hidden = !resetReady;
  nativeReady = status.background_session === true && Boolean(nativeRoute) && ['tool_protocol_qualified', 'generation_ready'].includes(status.phase);
  const result = $('native-text-result');
  const report = status.phase === 'generation_ready' ? status.native_text_report : null;
  result.hidden = !report && !status.native_text_error && !nativeOperation;
  if (nativeOperation) {
    result.textContent = nativeOperation.cancellation_requested ? 'Stopping the client test and waiting for cleanup...' : 'The Codex client test is running. You can close this window and return to its result later.';
  } else if (status.native_text_error) {
    const errors = {
      E_NATIVE_TEST_BACKGROUND: 'Complete the tool protocol test in background mode before testing Codex.',
      E_NATIVE_TEST_TARGET_CHANGED: 'This runtime already prepared a different Codex home or ChatGPT route. Continue with the original target.',
      E_NATIVE_TEST_PREPARE: 'The selected configuration could not be prepared. No configuration was changed.',
      E_NATIVE_PROBE_TARGET: 'Select existing original absolute paths for the executable, home and working directory.',
      E_NATIVE_PROBE_CLIENT_UNQUALIFIED: 'This backend version is not reviewed. It was not started.',
      E_NATIVE_PROBE_CANCELLED: 'The client test was cancelled. No automatic retry was made.',
      E_NATIVE_PROBE_TIMEOUT: 'The client test did not finish in time. No automatic retry was made.',
      E_NATIVE_PROBE_TEXT: 'The client did not return the exact expected text. No automatic retry was made.',
      E_NATIVE_PROBE_CONFIG: 'The isolated client configuration or account state could not be verified.',
      E_NATIVE_PROBE_DENIAL: 'The command denial could not be verified. No automatic retry was made.',
      E_NATIVE_PROBE_TEST: 'The fixture test sequence did not return the expected results. No automatic retry was made.',
      E_NATIVE_PROBE_ACTION: 'The client requested an operation outside this test. The test was stopped.',
      E_BROWSER_CLOSED: 'The background browser has stopped. Check status to restore the session.',
      E_BROWSER_OTHER_PAGES: 'Other cxweb browser tabs are open. Finish or close them before transferring the session.',
      E_BROWSER_BUSY: 'The browser contains a draft or active response. Finish it before testing the client.',
      E_MODEL_SELECTION: 'The selected ChatGPT route changed. Refresh and qualify it again.'
    };
    result.textContent = errors[status.native_text_error] || 'The native client test could not be verified. No automatic retry was made.';
  } else if (report) {
    result.textContent = report.exercise === 'denied_read'
      ? `Command denial verified through Codex ${report.client_build}. The read was declined and the final response acknowledged it. Full coding qualification, the actual picker and production activation still need verification.`
      : report.exercise === 'read_patch_test' && report.native_tools_executed === 3
      ? `Read, patch and test verified through Codex ${report.client_build}. The test command returned exit code 0 and the expected output. Full coding qualification, the actual picker and production activation still need verification.`
      : report.exercise === 'read_test_repair'
      ? (report.native_tools_executed === 4 && report.exact_text_received === true
        ? `Test failure and repair verified through Codex ${report.client_build}. The first test failed, Codex repaired the fixture file, and the second test passed. Full coding qualification, the actual picker and production activation still need verification.`
        : 'The failure and repair test result is incomplete. No repair qualification was recorded.')
      : report.native_tools_executed === 2
      ? `Read and patch verified through Codex ${report.client_build}. Full coding qualification, the actual picker and production activation still need verification.`
      : `Text transport verified through Codex ${report.client_build}. Coding support, the actual picker and production activation still need verification.`;
  } else result.textContent = '';
  updateNativeButton();
  scheduleNativeStatus();
}
function showError(code) {
  const messages = {
    E_ALREADY_RUNNING: 'cxweb is already running. Use the existing app window.',
    E_RUNTIME_MISSING: 'The cxweb runtime executable is missing. Build or reinstall the complete application.',
    E_RUNTIME_START: 'The runtime could not start. Close any older cxweb preview and reopen the app.',
    E_CONTROL_UNAVAILABLE: 'The runtime is not responding. Check status again shortly.',
    E_CONTROL_BUSY: 'Another cxweb window is processing a request. Try again shortly.',
    E_BROWSER_BUSY: 'The sign-in page contains a draft or an active response. Finish or clear it before closing the window.',
    E_BROWSER_OTHER_PAGES: 'Other cxweb browser tabs are still open. Finish or close them before changing browser mode. They have been left open.',
    E_BROWSER_IN_USE: 'The runtime is using this browser session. Check status instead of opening another sign-in or test session.',
    E_LOGIN_REQUIRED: 'Complete sign-in before continuing in the background.',
    E_BROWSER_RELEASE: 'The browser could not close safely. Check status before trying again.',
    E_BACKGROUND_NAVIGATION: 'The saved session could not load in the background. Check status for sign-in or verification requirements.',
    E_BACKGROUND_WINDOW: 'The background browser could not be hidden. The session was not confirmed.',
    E_STATE_PERMISSIONS: 'Local app data has unexpected security permissions. The connection was not changed.',
    E_BROWSER_RUNTIME_MISSING: 'No supported browser was found for development verification.',
    E_CONTROL_TIMEOUT: 'The browser is not responding yet. Check status again shortly.',
    E_MODEL_DISCOVERY: 'The ChatGPT model menu could not be verified. Refresh model candidates.',
    E_MODEL_OPEN: 'Model verification stopped while opening the ChatGPT model menu.',
    E_MODEL_READ: 'Model verification stopped while reading the visible ChatGPT model menu.',
    E_MODEL_CLOSE: 'Model verification stopped while closing the ChatGPT model menu.',
    E_MODEL_PARSE: 'The visible ChatGPT model menu returned an unsupported structure.',
    E_MODEL_RESULT: 'The visible ChatGPT model menu exceeded the safe discovery limits.',
    E_MODEL_RESTORE: 'Model discovery could not restore the original thinking effort. No route was qualified. Refresh model candidates.',
    E_MODEL_FAMILY: 'The selected model family could not be verified. Refresh model candidates.',
    E_MODEL_SELECTION: 'The selected ChatGPT route changed or could not be verified. Refresh model candidates.',
    E_SESSION_SCOPE: 'The ChatGPT account or workspace changed or could not be verified. Refresh model candidates. No automatic retry was made.',
    E_BROWSER_RATE_LIMITED: 'ChatGPT temporarily limited access because requests were too frequent. Wait a few minutes before trying again. No automatic retry was made; signing in again is not required.',
    E_MODEL_SELECT: 'The requested thinking effort could not be selected. Refresh model candidates.',
    E_MODEL_LABEL: 'The model label changed during verification. Refresh model candidates.',
    E_SUBMISSION_UNCERTAIN: 'The test may have been submitted. It was not retried automatically.',
    E_QUALIFICATION_TIMEOUT: 'The test did not complete within five minutes. No automatic retry was made.',
    E_TURN_AMBIGUOUS: 'Multiple conversation turns were observed. No automatic retry was made.',
    E_TURN_ATTRIBUTION: 'The conversation identity changed. No automatic retry was made.',
    E_USER_MESSAGE_MISMATCH: 'The submitted message could not be matched to the test. No automatic retry was made.',
    E_MODEL_FIDELITY: 'The model control changed after submission. No automatic retry was made.',
    E_INVALID_TOOL_ENVELOPE: 'The response did not follow the required tool format. No automatic retry was made.',
    E_TOOL_ENVELOPE_FENCED: 'The response used an unsupported code block. No automatic retry was made.',
    E_QUALIFICATION_PROTOCOL: 'ChatGPT responded, but the test response did not match the required protocol.',
    E_LIVE_QUALIFICATION: 'The live text test could not be verified. No automatic retry was made.',
    E_TEMPORARY_CHAT: 'Temporary Chat could not be verified. No test message was sent.',
    E_QUALIFICATION_SELECT: 'The test stopped while selecting its route in Temporary Chat.',
    E_QUALIFICATION_BASELINE: 'The test could not establish the initial conversation state.',
    E_QUALIFICATION_INSERT: 'The test could not insert its message. Send was not clicked.',
    E_QUALIFICATION_OBSERVE: 'The test response could not be attributed. No automatic retry was made.',
    E_SEND_SURFACE: 'The send controls were unavailable. Send was not clicked.',
    E_SEND_DISABLED: 'The send button was not ready. Send was not clicked.',
    E_COMPOSER_MISMATCH: 'The composer did not contain the exact test message. Send was not clicked.'
  };
  $('error').textContent = messages[code] || 'Verification failed. Check the connection status for details.';
  $('error').hidden = false; $('diagnostic').textContent = String(code).slice(0, 80);
}
function render(status) {
  renderNative(status);
  if (status.installation) {
    nativeReady = false;
    resetReady = false;
    $('reset-test').hidden = true;
    $('reset-test-help').hidden = true;
    updateNativeButton();
  }
  const activation = $('activation-result');
  activation.hidden = !status.routing_installed && !status.activation_error;
  const activationErrors = {
    E_ACTIVATION_CONFIG_CONFLICT: 'The selected Codex configuration has a conflict. Check the selected target to see its details.',
    E_ACTIVATION_TARGET_CHANGED: 'This runtime already owns another target. Use its original Codex home and route.',
    E_ACTIVATION_TARGET_PERMISSIONS: 'The selected target has unsupported permissions. The connection was not activated.',
    E_PREFLIGHT_MODELS: 'The native model catalog could not be verified. The connection was not activated.',
    E_CATALOG_CACHE: 'The old Codex model cache could not be refreshed safely. The connection was not activated.',
    E_ACTIVATION_INSTALL: 'The background runtime could not be installed.',
    E_ACTIVATION_SUPERVISION: 'Background startup could not be registered. Check installed connections before retrying.',
    E_CONFIG_APPLY: 'The configuration transaction did not finish. Check installed connections for its recovery status.'
  };
  activation.textContent = status.activation_error
    ? (activationErrors[status.activation_error] || `Codex connection could not be activated (${status.activation_error}).`)
    : status.routing_installed ? 'Connection installed. Restart Codex CLI and Codex App to refresh their model lists. Select the ChatGPT Web model in each client and send a request. Client verification is still pending.' : '';
  phase = status.phase;
  signInRequired = phase === 'authenticating' && status.background_session === true && (status.observation?.login_action === true || status.observation?.verification_required === true);
  $('error').hidden = true;
  $('light').classList.toggle('pending', phase !== 'disconnected');
  $('runtime').textContent = status.browser_version ? `Browser: ${status.browser_version}. Private connection over a Windows pipe.` : 'The browser has not started yet.';
  $('models').hidden = true; $('models').replaceChildren();
  $('qualification').hidden = true;
  $('tool-qualification').hidden = true;
  const sessionDetected = ['awaiting_qualification', 'candidates_observed', 'text_qualified', 'tool_protocol_qualified', 'discovery_failed', 'generation_ready'].includes(phase);
  $('background-control').hidden = status.background_session === true || !sessionDetected;
  $('background-status').hidden = status.background_session !== true || !sessionDetected;
  $('codex').textContent = 'Awaiting verification';
  if (phase === 'disconnected') {
    $('heading').textContent = 'Sign in to ChatGPT';
    $('description').textContent = "Open the official sign-in page in the app's dedicated browser profile.";
    $('chatgpt').textContent = 'Signed out'; $('connect').textContent = 'Connect ChatGPT';
  } else if (phase === 'awaiting_qualification') {
    $('heading').textContent = 'Session awaiting verification';
    $('description').textContent = 'The ChatGPT interface is available. The account and available models still need verification.';
    $('chatgpt').textContent = 'Session detected'; $('connect').textContent = 'Verify available models';
  } else if (phase === 'candidates_observed') {
    $('qualification').hidden = status.temporary_chat_available !== true;
    $('heading').textContent = 'Model candidates found';
    $('description').textContent = 'The visible ChatGPT routes were observed. Coding and Codex picker qualification is still required.';
    $('chatgpt').textContent = 'Session detected'; $('connect').textContent = 'Refresh model candidates';
    $('codex').textContent = 'Candidates observed';
    const heading = document.createElement('strong'); heading.textContent = 'Observed ChatGPT models';
    const list = document.createElement('ul');
    for (const model of status.candidate_models || []) {
      const item = document.createElement('li');
      item.textContent = model.label + (model.selected ? ' · selected' : ''); list.append(item);
    }
    const privacy = document.createElement('p');
    privacy.textContent = status.temporary_chat_available ? 'Temporary Chat verified.' : 'Temporary Chat was not verified; private generation remains disabled.';
    $('models').append(heading, list, privacy); $('models').hidden = false;
  } else if (phase === 'text_qualified') {
    $('tool-qualification').hidden = status.temporary_chat_available !== true || !status.text_qualified_model;
    $('heading').textContent = 'Text test passed';
    $('description').textContent = 'ChatGPT returned the expected test response. Tool support and Codex integration still need verification.';
    $('chatgpt').textContent = 'Text verified';
    $('codex').textContent = 'Awaiting integration';
    $('connect').textContent = 'Refresh model candidates';
  } else if (phase === 'tool_protocol_qualified') {
    $('heading').textContent = 'Tool protocol test passed';
    $('description').textContent = 'ChatGPT returned valid function and custom tool requests. Execution through Codex still needs verification.';
    $('chatgpt').textContent = 'Tool requests verified';
    $('codex').textContent = 'Awaiting integration';
    $('connect').textContent = 'Refresh model candidates';
  } else if (phase === 'generation_ready') {
    $('heading').textContent = 'Background session ready';
    $('description').textContent = 'The runtime owns the verified browser session. Codex integration still needs activation and client verification.';
    $('chatgpt').textContent = 'Session ready';
    $('codex').textContent = 'Awaiting integration';
    $('connect').textContent = 'Check status';
  } else if (phase === 'discovery_failed') {
    $('heading').textContent = 'Model menu changed';
    $('description').textContent = 'The signed-in page is available, but its current model menu structure is not recognized yet.';
    $('chatgpt').textContent = 'Session detected'; $('connect').textContent = 'Retry model discovery';
    const diagnostic = status.model_discovery_diagnostic || {};
    const details = document.createElement('p');
    details.className = 'selectable';
    details.textContent = `Expanded: ${String(diagnostic.switcher_expanded)}; roots: ${diagnostic.visible_roots ?? 0}; candidates: ${diagnostic.candidate_nodes ?? 0}; model test IDs: ${(diagnostic.model_testids || []).join(', ') || 'none'}; roles: ${(diagnostic.visible_roles || []).join(', ') || 'none'}.`;
    $('models').append(details); $('models').hidden = false;
  } else if (phase === 'authenticating') {
    const verification = status.background_session === true && status.observation?.verification_required === true;
    const restoring = status.background_session === true && !signInRequired;
    $('heading').textContent = verification ? 'ChatGPT verification required' : restoring ? 'Restoring ChatGPT session' : 'Sign in to ChatGPT';
    $('description').textContent = verification
      ? 'ChatGPT requested additional verification in background mode. Background requests are unavailable. Open the sign-in window to check your session.'
      : restoring
      ? 'The saved session is loading in the background. Check status again shortly.'
      : signInRequired ? 'Your saved session needs sign-in. Open the sign-in window to continue.'
      : 'Complete sign-in in the browser window, including MFA or any other verification. Close all cxweb Chrome windows, then select Check status.';
    $('chatgpt').textContent = restoring ? 'Connecting' : 'Waiting for sign-in';
    $('connect').textContent = signInRequired ? 'Open sign-in window' : 'Check status';
  } else if (phase === 'browser_unavailable') {
    $('heading').textContent = 'Browser unavailable';
    $('description').textContent = 'You can reopen the sign-in window. Saved sign-in data stays in the dedicated cxweb profile.';
    $('chatgpt').textContent = 'Unverified'; $('connect').textContent = 'Reopen browser';
  } else if (phase === 'status_unavailable') {
    $('heading').textContent = 'Connection status unavailable';
    $('description').textContent = 'The last operation did not return a verified status. Check status before starting another test.';
    $('chatgpt').textContent = 'Unverified'; $('connect').textContent = 'Check status';
  }
  if (status.routing_installed) {
    $('heading').textContent = 'Codex connection installed';
    $('description').textContent = 'Restart Codex CLI and Codex App, then select a ChatGPT Web model. Use Installed connections to check or disconnect the background runtime.';
    $('codex').textContent = 'Restart clients to verify';
  }
}
const actionButtons = ['connect', 'test-text', 'test-tools', 'background', 'native-text', 'native-tools', 'native-repair', 'native-denial', 'reset-test', 'activate-codex'];
async function runAction(command, params = {}) {
  if (pending || (nativeOperation && command !== 'status')) return;
  pending = true;
  if (command === 'native_text') { nativeWaiting = true; scheduleNativeStatus(); }
  if (command === 'native_text') for (const id of nativeControls) $(id).disabled = true;
  for (const id of actionButtons) $(id).disabled = true;
  try {
    const status = await invoke(command, params);
    render(status);
    if (status.routing_installed && window.cxwebInstalled) await window.cxwebInstalled.attach();
  }
  catch (error) {
    // A failed operation invalidates qualification in the runtime. Read its
    // cached receipt without repeating browser work or retaining stale success.
    if (command !== 'status') {
      try { render(await invoke('status', { refresh: false })); }
      catch { render({ phase: 'status_unavailable' }); }
    } else { render({ phase: 'status_unavailable' }); }
    showError(error);
  }
  finally {
    pending = false;
    if (command === 'native_text') {
      nativeWaiting = false;
      nativePollEpoch += 1;
      if (nativeTimer !== null) { clearTimeout(nativeTimer); nativeTimer = null; }
    }
    for (const id of actionButtons) $(id).disabled = false;
    if (command === 'native_text') for (const id of nativeControls) $(id).disabled = false;
    updateNativeButton();
    scheduleNativeStatus();
  }
}
async function check(connect = false, refresh = true) {
  await runAction(connect ? 'connect' : 'status', connect ? {} : { refresh });
}
async function act() {
  if (phase === 'awaiting_qualification' || phase === 'candidates_observed' || phase === 'discovery_failed' || phase === 'text_qualified' || phase === 'tool_protocol_qualified') {
    await runAction('qualify');
    return;
  }
  await check(phase === 'disconnected' || phase === 'browser_unavailable' || signInRequired);
}
$('connect').addEventListener('click', act);
$('test-text').addEventListener('click', async () => {
  if (pending || phase !== 'candidates_observed' || $('qualification').hidden) return;
  $('test-text').textContent = 'Waiting for the test response…';
  try { await runAction('qualify_text'); }
  finally {
    $('test-text').textContent = 'Send text test';
  }
});
$('test-tools').addEventListener('click', async () => {
  if (pending || phase !== 'text_qualified' || $('tool-qualification').hidden) return;
  $('test-tools').textContent = 'Waiting for the tool test response…';
  try { await runAction('qualify_tools'); }
  finally { $('test-tools').textContent = 'Send tool protocol test'; }
});
$('background').addEventListener('click', async () => {
  if (pending || $('background-control').hidden) return;
  await runAction('background');
});
$('native-discover').addEventListener('click', async () => {
  if (pending || nativeOperation || discoveryPending || preflightPending) return;
  discoveryPending = true; $('native-discover').disabled = true; updateNativeButton();
  const results = $('native-targets');
  results.hidden = false; results.textContent = 'Inspecting local executable files…';
  try {
    const report = await invoke('native_discover');
    const choice = $('native-choice');
    const placeholder = document.createElement('option');
    placeholder.value = ''; placeholder.textContent = 'Choose an installation or enter a path below';
    choice.replaceChildren(placeholder);
    results.replaceChildren();
    const description = document.createElement('p');
    description.textContent = report.candidates.length
      ? 'Executable candidates found. Selecting a target and checking its configuration are still required.'
      : 'No supported executable locations were found. A custom installation may need an explicit path.';
    results.append(description);
    const list = document.createElement('ul');
    for (const candidate of report.candidates) {
      const item = document.createElement('li');
      const sources = candidate.sources.map(source => ({path_executable:'PATH', npm_installation:'npm installation', desktop_backend_cache:'App backend cache'})[source] || 'Other location').join(', ');
      item.textContent = `${candidate.reviewed_build ? `Reviewed backend ${candidate.reviewed_build}` : 'Unreviewed backend'} (${sources}): ${candidate.executable}`;
      list.append(item);
      const option = document.createElement('option');
      option.value = candidate.executable; option.textContent = item.textContent;
      option.disabled = !candidate.reviewed_build;
      choice.append(option);
    }
    results.append(list);
    if (report.diagnostics.length) {
      const note = document.createElement('p');
      note.textContent = `Some locations could not be inspected: ${report.diagnostics.join(', ')}.`;
      results.append(note);
    }
    const scope = document.createElement('p');
    scope.textContent = 'This inspection does not launch Codex, read credentials or change configuration. A cached App backend does not identify the backend used by a running Codex window.';
    results.append(scope);
  } catch {
    results.textContent = 'Codex installations could not be inspected. Your connection has not been changed.';
  } finally {
    discoveryPending = false; $('native-discover').disabled = false; updateNativeButton();
  }
});
function targetChanged() {
  targetRevision += 1;
  $('native-text-result').hidden = true;
  $('native-preflight-result').hidden = true;
  $('native-preflight-result').replaceChildren();
}
for (const id of ['native-client', 'native-home', 'native-cwd']) $(id).addEventListener('input', targetChanged);
$('native-choice').addEventListener('change', () => {
  if ($('native-choice').value) $('native-client').value = $('native-choice').value;
  targetChanged();
});
$('native-preflight-form').addEventListener('submit', async event => {
  event.preventDefault();
  if (pending || nativeOperation || preflightPending || discoveryPending) return;
  const target = { client: $('native-client').value.trim(), home: $('native-home').value.trim(), cwd: $('native-cwd').value.trim() };
  const results = $('native-preflight-result');
  results.hidden = false; results.replaceChildren();
  if (!target.client || !target.home || !target.cwd) {
    results.textContent = 'Enter all three absolute paths before checking the target.';
    return;
  }
  const revision = targetRevision;
  preflightPending = true;
  updateNativeButton();
  const controls = ['native-choice', 'native-client', 'native-home', 'native-cwd', 'native-preflight', 'native-discover'];
  for (const id of controls) $(id).disabled = true;
  results.textContent = 'Inspecting the selected Codex configuration…';
  try {
    const report = await invoke('native_preflight', target);
    if (revision !== targetRevision) return;
    results.replaceChildren();
    const summary = document.createElement('p');
    summary.textContent = report.assessment.configuration_compatible
      ? 'No configuration conflict was found for this target. Integration is not active.'
      : 'This target has configuration or authentication requirements to resolve. Integration is not active.';
    results.append(summary);
    const details = document.createElement('p');
    const auth = {subscription:'ChatGPT subscription', api_key:'API key', bedrock:'Amazon Bedrock', signed_out:'Signed out', unknown:'Unknown'}[report.assessment.auth_mode] || 'Unknown';
    details.textContent = `Backend: ${report.client_build}. Native account mode: ${auth}. Active configuration layers: ${report.assessment.active_layers.join(', ') || 'none'}.`;
    results.append(details);
    const list = document.createElement('ul');
    const descriptions = {
      target_permissions:'The Codex home has unsupported owner or access permissions in its path. Activation requires a qualified location. No permissions were changed.',
      subscription_auth_required:'This integration requires native Codex subscription sign-in.',
      environment_auth:'An environment variable overrides native authentication.',
      environment_route:'An environment variable overrides native routing.',
      openai_base_url:'An existing OpenAI route is configured.',
      chatgpt_base_url:'An existing ChatGPT route is configured.',
      model_catalog_json:'An existing model catalog is configured.',
      model_provider:'A different model provider is selected.',
      reserved_provider_override:'The built-in OpenAI provider is overridden.',
      profile:'A profile is selected.', profiles:'Configured profiles need separate target qualification.',
      selected_profile:'A selected profile needs separate target qualification.',
      managed_routing:'Managed routing or residency requirements need separate qualification.',
      unknown_config_layer:'An unknown configuration layer is active.',
      ambiguous_user_config:'The effective user configuration could not be uniquely identified.'
    };
    for (const conflict of report.assessment.conflicts) {
      const item = document.createElement('li'); item.textContent = descriptions[conflict] || `Configuration conflict: ${conflict}`; list.append(item);
    }
    results.append(list);
    const remaining = document.createElement('p');
    remaining.textContent = `Still required: ${report.remaining_checks.join('; ')}. This inspection covers the selected paths and this app's environment, without profile or command-line overrides. It does not certify an already-running Codex window.`;
    results.append(remaining);
  } catch (error) {
    if (revision !== targetRevision) return;
    const messages = {
      E_PREFLIGHT_TARGET:'Use existing absolute paths for the executable, Codex home and working directory.',
      E_PREFLIGHT_TARGET_IDENTITY:'A selected path is unsupported, inaccessible or contains a file link. Select the original local path.',
      E_PREFLIGHT_CLIENT_UNQUALIFIED:'This executable version has not been qualified. It was not started.',
      E_PREFLIGHT_CONFIG_PERMISSIONS:'The selected configuration or its directory has unsupported permissions. No permissions were changed.',
      E_PREFLIGHT_CONFIG_PARSE:'The selected configuration could not be parsed.',
      E_PREFLIGHT_CONFIG_CHANGED:'The configuration changed during inspection. Check the target again after the edit finishes.',
      E_PREFLIGHT_TARGET_CHANGED:'A selected path changed during inspection.',
      E_PREFLIGHT_HOME_MISMATCH:'The backend reported a different configuration from the selected home.',
      E_PREFLIGHT_TIMEOUT:'The backend did not finish its status checks in time. No automatic retry was made.'
    };
    results.textContent = messages[error] || 'The selected target could not be verified. No integration was installed or test retried.';
  } finally {
    preflightPending = false;
    updateNativeButton();
    for (const id of controls) $(id).disabled = false;
  }
});
async function runNativeTest(exercise = 'text') {
  if (pending || nativeOperation || preflightPending || discoveryPending || !nativeReady) return;
  const target = { client: $('native-client').value.trim(), home: $('native-home').value.trim(), cwd: $('native-cwd').value.trim(), route: nativeRoute };
  if (exercise !== 'text') target.exercise = exercise;
  if (!target.client || !target.home || !target.cwd) {
    $('native-text-result').hidden = false;
    $('native-text-result').textContent = 'Enter the executable, Codex home and working directory before testing the selected client.';
    return;
  }
  const [id, label] = {text:['native-text', 'Test selected client'], read_patch_test:['native-tools', 'Test coding tools'], read_test_repair:['native-repair', 'Test failure and repair'], denied_read:['native-denial', 'Test command denial']}[exercise];
  const button = $(id);
  button.textContent = 'Waiting for the Codex test...';
  try { await runAction('native_text', target); }
  finally { button.textContent = label; }
}
$('native-text').addEventListener('click', () => runNativeTest());
$('native-tools').addEventListener('click', () => runNativeTest('read_patch_test'));
$('native-repair').addEventListener('click', () => runNativeTest('read_test_repair'));
$('native-denial').addEventListener('click', () => runNativeTest('denied_read'));
$('activate-codex').addEventListener('click', async () => {
  if (pending || nativeOperation || preflightPending || discoveryPending || !nativeReady) return;
  const target = { client: $('native-client').value.trim(), home: $('native-home').value.trim(), cwd: $('native-cwd').value.trim(), route: nativeRoute };
  if (!target.client || !target.home || !target.cwd) {
    $('activation-result').hidden = false;
    $('activation-result').textContent = 'Select the Codex executable, home and working directory before connecting.';
    return;
  }
  await runAction('activate_codex', target);
});
$('native-cancel').addEventListener('click', async () => {
  if (!nativeOperation || nativeCancelling || nativeOperation.cancellation_requested) return;
  const receipt = { instance: nativeOperation.instance, operation: nativeOperation.operation };
  nativeCancelling = true;
  updateNativeButton();
  const epoch = nativePollEpoch;
  try {
    await invoke('native_cancel', receipt);
    const status = await invoke('status', { refresh: false });
    if (epoch === nativePollEpoch) render(status);
  } catch (error) { showError(error); }
  finally { nativeCancelling = false; updateNativeButton(); scheduleNativeStatus(); }
});
$('reset-test').addEventListener('click', async () => {
  if (pending || nativeOperation || preflightPending || discoveryPending || !resetReady) return;
  await runAction('reset_test');
});
async function start() {
  if (invoke && window.cxwebInstalled && await window.cxwebInstalled.attach()) return;
  $('setup-view').hidden = false;
  if (invoke) await check(false, false); else showError('E_DESKTOP_IPC');
}
start();
