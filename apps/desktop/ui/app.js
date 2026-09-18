'use strict';
const $ = id => document.getElementById(id);
const invoke = window.__TAURI__?.core.invoke;
let phase = 'disconnected';
let pending = false;
function showError(code) {
  const messages = {
    E_ALREADY_RUNNING: 'cxweb is already running. Use the existing app window.',
    E_RUNTIME_MISSING: 'The cxweb runtime executable is missing. Build or reinstall the complete application.',
    E_RUNTIME_START: 'The runtime could not start. Close any older cxweb preview and reopen the app.',
    E_CONTROL_UNAVAILABLE: 'The runtime is not responding. Try again without closing the browser.',
    E_CONTROL_BUSY: 'Another cxweb window is processing a request. Try again shortly.',
    E_STATE_PERMISSIONS: 'Local app data has unexpected security permissions. The connection was not changed.',
    E_BROWSER_RUNTIME_MISSING: 'No supported browser was found for development verification.',
    E_CONTROL_TIMEOUT: 'The browser is not responding yet. Check its window and try checking the status again.',
    E_MODEL_DISCOVERY: 'The ChatGPT model menu could not be verified. Leave the ChatGPT page open and try again.',
    E_MODEL_OPEN: 'Model verification stopped while opening the ChatGPT model menu.',
    E_MODEL_READ: 'Model verification stopped while reading the visible ChatGPT model menu.',
    E_MODEL_CLOSE: 'Model verification stopped while closing the ChatGPT model menu.',
    E_MODEL_PARSE: 'The visible ChatGPT model menu returned an unsupported structure.',
    E_MODEL_RESULT: 'The visible ChatGPT model menu exceeded the safe discovery limits.',
    E_MODEL_SELECTION: 'The selected ChatGPT route changed or could not be verified. Refresh model candidates.',
    E_SUBMISSION_UNCERTAIN: 'The test may have been submitted. It was not retried automatically.',
    E_QUALIFICATION_TIMEOUT: 'The test did not complete within five minutes. Its browser tab is available for inspection.',
    E_TURN_AMBIGUOUS: 'Multiple conversation turns were observed. The test tab is available for inspection.',
    E_TURN_ATTRIBUTION: 'The conversation identity changed. No automatic retry was made.',
    E_USER_MESSAGE_MISMATCH: 'The submitted message could not be matched to the test. The test tab is available for inspection.',
    E_MODEL_FIDELITY: 'The model control changed after submission. No automatic retry was made.',
    E_INVALID_TOOL_ENVELOPE: 'The response used an unsupported code block. The test tab is available for inspection.',
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
  $('error').textContent = messages[code] || 'Verification failed. Check the browser window and try again.';
  $('error').hidden = false; $('diagnostic').textContent = String(code).slice(0, 80);
}
function render(status) {
  phase = status.phase;
  $('error').hidden = true;
  $('light').classList.toggle('pending', phase !== 'disconnected');
  $('runtime').textContent = status.browser_version ? `Browser: ${status.browser_version}. Private connection over a Windows pipe.` : 'The browser has not started yet.';
  $('models').hidden = true; $('models').replaceChildren();
  $('qualification').hidden = true;
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
    $('heading').textContent = 'Text test passed';
    $('description').textContent = 'ChatGPT returned the expected test response. Tool support and Codex integration still need verification.';
    $('chatgpt').textContent = 'Text verified';
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
    $('heading').textContent = 'Sign in to ChatGPT';
    $('description').textContent = 'Complete sign-in in the browser window, including MFA or any other verification.';
    $('chatgpt').textContent = 'Waiting for sign-in'; $('connect').textContent = 'Check status';
  } else if (phase === 'browser_unavailable') {
    $('heading').textContent = 'Browser unavailable';
    $('description').textContent = 'You can reopen the sign-in window. Saved sign-in data stays in the dedicated cxweb profile.';
    $('chatgpt').textContent = 'Unverified'; $('connect').textContent = 'Reopen browser';
  }
}
async function check(connect = false, refresh = true) {
  if (pending) return;
  pending = true; $('connect').disabled = true;
  try { render(await invoke(connect ? 'connect' : 'status', connect ? {} : { refresh })); } catch (error) { showError(error); }
  finally {
    pending = false; $('connect').disabled = false;
  }
}
async function act() {
  if (phase === 'awaiting_qualification' || phase === 'candidates_observed' || phase === 'discovery_failed' || phase === 'text_qualified') {
    if (pending) return;
    pending = true; $('connect').disabled = true;
    try { render(await invoke('qualify')); } catch (error) { showError(error); }
    finally { pending = false; $('connect').disabled = false; }
    return;
  }
  await check(phase === 'disconnected' || phase === 'browser_unavailable');
}
$('connect').addEventListener('click', act);
$('test-text').addEventListener('click', async () => {
  if (pending || phase !== 'candidates_observed' || $('qualification').hidden) return;
  pending = true; $('connect').disabled = true; $('test-text').disabled = true;
  $('test-text').textContent = 'Waiting for the test response…';
  try { render(await invoke('qualify_text')); } catch (error) { showError(error); }
  finally {
    pending = false; $('connect').disabled = false; $('test-text').disabled = false;
    $('test-text').textContent = 'Send text test';
  }
});
if (invoke) check(false, false); else showError('E_DESKTOP_IPC');
