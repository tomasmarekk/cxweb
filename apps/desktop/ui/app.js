'use strict';
const $ = id => document.getElementById(id);
const invoke = window.__TAURI__?.core.invoke;
let phase = 'disconnected';
let pending = false;
let signInRequired = false;
function showError(code) {
  const messages = {
    E_ALREADY_RUNNING: 'cxweb is already running. Use the existing app window.',
    E_RUNTIME_MISSING: 'The cxweb runtime executable is missing. Build or reinstall the complete application.',
    E_RUNTIME_START: 'The runtime could not start. Close any older cxweb preview and reopen the app.',
    E_CONTROL_UNAVAILABLE: 'The runtime is not responding. Check status again shortly.',
    E_CONTROL_BUSY: 'Another cxweb window is processing a request. Try again shortly.',
    E_BROWSER_BUSY: 'The sign-in page contains a draft or an active response. Finish or clear it before closing the window.',
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
    E_MODEL_SELECT: 'The requested thinking effort could not be selected. Refresh model candidates.',
    E_MODEL_LABEL: 'The model label changed during verification. Refresh model candidates.',
    E_SUBMISSION_UNCERTAIN: 'The test may have been submitted. It was not retried automatically.',
    E_QUALIFICATION_TIMEOUT: 'The test did not complete within five minutes. No automatic retry was made.',
    E_TURN_AMBIGUOUS: 'Multiple conversation turns were observed. No automatic retry was made.',
    E_TURN_ATTRIBUTION: 'The conversation identity changed. No automatic retry was made.',
    E_USER_MESSAGE_MISMATCH: 'The submitted message could not be matched to the test. No automatic retry was made.',
    E_MODEL_FIDELITY: 'The model control changed after submission. No automatic retry was made.',
    E_INVALID_TOOL_ENVELOPE: 'The response used an unsupported code block. No automatic retry was made.',
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
  phase = status.phase;
  signInRequired = phase === 'authenticating' && status.background_session === true && (status.observation?.login_action === true || status.observation?.verification_required === true);
  $('error').hidden = true;
  $('light').classList.toggle('pending', phase !== 'disconnected');
  $('runtime').textContent = status.browser_version ? `Browser: ${status.browser_version}. Private connection over a Windows pipe.` : 'The browser has not started yet.';
  $('models').hidden = true; $('models').replaceChildren();
  $('qualification').hidden = true;
  $('tool-qualification').hidden = true;
  const sessionDetected = ['awaiting_qualification', 'candidates_observed', 'text_qualified', 'tool_protocol_qualified', 'discovery_failed'].includes(phase);
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
      : 'Complete sign-in in the browser window, including MFA or any other verification. You can close the window afterward.';
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
}
const actionButtons = ['connect', 'test-text', 'test-tools', 'background'];
async function runAction(command, params = {}) {
  if (pending) return;
  pending = true;
  for (const id of actionButtons) $(id).disabled = true;
  try { render(await invoke(command, params)); }
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
    for (const id of actionButtons) $(id).disabled = false;
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
if (invoke) check(false, false); else showError('E_DESKTOP_IPC');
