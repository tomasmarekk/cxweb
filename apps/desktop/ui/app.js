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
    E_CONTROL_TIMEOUT: 'The browser is not responding yet. Check its window and try checking the status again.'
  };
  $('error').textContent = messages[code] || 'Verification failed. Check the browser window and try again.';
  $('error').hidden = false; $('diagnostic').textContent = String(code).slice(0, 80);
}
function render(status) {
  phase = status.phase;
  $('error').hidden = true;
  $('light').classList.toggle('pending', phase !== 'disconnected');
  $('runtime').textContent = status.browser_version ? `Browser: ${status.browser_version}. Private connection over a Windows pipe.` : 'The browser has not started yet.';
  if (phase === 'disconnected') {
    $('heading').textContent = 'Sign in to ChatGPT';
    $('description').textContent = "Open the official sign-in page in the app's dedicated browser profile.";
    $('chatgpt').textContent = 'Signed out'; $('connect').textContent = 'Connect ChatGPT';
  } else if (phase === 'awaiting_qualification') {
    $('heading').textContent = 'Session awaiting verification';
    $('description').textContent = 'The ChatGPT interface is available. The account and available models still need verification.';
    $('chatgpt').textContent = 'Session detected'; $('connect').textContent = 'Check status';
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
$('connect').addEventListener('click', () => check(phase === 'disconnected' || phase === 'browser_unavailable'));
if (invoke) check(false, false); else showError('E_DESKTOP_IPC');
