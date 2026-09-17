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
    E_MODEL_SELECTION: 'The selected ChatGPT route could not be verified without changing it.'
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
  if (phase === 'awaiting_qualification' || phase === 'candidates_observed' || phase === 'discovery_failed') {
    if (pending) return;
    pending = true; $('connect').disabled = true;
    try { render(await invoke('qualify')); } catch (error) { showError(error); }
    finally { pending = false; $('connect').disabled = false; }
    return;
  }
  await check(phase === 'disconnected' || phase === 'browser_unavailable');
}
$('connect').addEventListener('click', act);
if (invoke) check(false, false); else showError('E_DESKTOP_IPC');
