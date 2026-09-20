'use strict';
// Installed-host attachment is independent of the login/test controller. A
// missing or unresponsive installed host must never launch a competing owner.
window.cxwebInstalled = (() => {
  const node = id => document.getElementById(id);
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  let targets = [], selected = '', snapshot = null, pending = false, confirmation = null;
  let timer = null, epoch = 0, attached = false;
  const stateText = { healthy: 'Verified', degraded: 'Degraded', unavailable: 'Unavailable', unknown: 'Unverified', not_installed: 'Not installed', restart_required: 'Restart required', auth_required: 'Sign-in required', incompatible: 'Incompatible', conflict: 'Conflict' };
  const wording = {
    disconnected: ['Connection inactive', 'The runtime is available. Web model routing is not active.'],
    preflight: ['Verifying connection', 'Some parts of the connection still need verification.'],
    authenticating: ['Sign in to ChatGPT', 'Complete sign-in in the managed sign-in window.'],
    discovering: ['Verifying models', 'The runtime is checking available web models.'],
    ready: ['Connected', 'ChatGPT Web models are available in Codex.'],
    busy: ['Task in progress', 'A web response is running. Client verification is shown below.'],
    restart_required: ['Restart Codex', 'Close and reopen Codex to load the connection.'],
    auth_required: ['ChatGPT sign-in required', 'Web requests are unavailable until the runtime verifies your sign-in again.'],
    rate_limited: ['ChatGPT limit reached', 'A new web request cannot be sent yet.'],
    offline: ['Connection unavailable', 'Check your internet connection.'],
    incompatible: ['Compatibility check required', 'Web models are unavailable with the currently verified configuration.'],
    config_conflict: ['Configuration changed', 'Your changes were preserved. Review the connection details.'],
    unavailable: ['ChatGPT unavailable', 'Web requests are currently unavailable. Check the connection details.'],
    disconnecting: ['Removing connection', 'Waiting for response cleanup and restoring configuration.'],
    removal_pending_restart: ['Finish removal', 'Close and reopen Codex. The runtime remains available for clients using the previous connection.'],
    disconnected_complete: ['Connection removed', 'The original Codex configuration has been restored.']
  };
  function controls() {
    node('installed-refresh').disabled = pending || Boolean(confirmation);
    node('installed-choice').disabled = pending || Boolean(confirmation);
    node('installed-remove').disabled = pending || Boolean(confirmation) || !snapshot || ['disconnecting', 'removal_pending_restart', 'disconnected_complete'].includes(snapshot.health.overall);
    const retryable = snapshot?.health.overall === 'unavailable' && snapshot.health.active_web_turns === 0 && snapshot.health.components.runtime?.state === 'healthy' &&
      ['E_ALREADY_RUNNING', 'E_BROWSER_RUNTIME_MISSING', 'E_BROWSER_START', 'E_BACKGROUND_NAVIGATION', 'E_BROWSER_OBSERVATION'].includes(snapshot.health.components.browser?.code);
    node('installed-retry').hidden = !retryable;
    node('installed-retry').disabled = !retryable || pending || Boolean(confirmation);
  }
  function error(code) {
    const messages = {
      E_INSTALLED_CHANGED: 'The runtime restarted. Check its current status before taking further action.',
      E_INSTALLED_RECOVERY: 'Background verification could not be restarted. Check status for the current sign-in or compatibility requirement.',
      E_INSTALLED_UNAVAILABLE: 'The installed runtime is not responding. No replacement process was started.',
      E_INSTALLED_REMOVE: 'Removal could not be completed. Check status for the configuration or cleanup error.',
      E_INSTALLED_TIMEOUT: 'The operation is still unconfirmed. Check status; it was not submitted again.',
      E_INSTALLED_UNCONFIRMED: 'The runtime did not confirm the request. Check status before trying again.',
      E_CONTROL_BUSY: 'Another operation is running. Check status before trying again.'
    };
    node('installed-error').textContent = messages[code] || 'The installed connection could not be verified. No browser or replacement runtime was started.';
    node('installed-error').hidden = false;
  }
  function unavailable() {
    snapshot = null;
    node('installed-heading').textContent = 'Connection status unavailable';
    node('installed-description').textContent = 'The current runtime state could not be verified. Check status before taking further action.';
    for (const id of ['chatgpt', 'app', 'cli']) node(`installed-${id}`).textContent = 'Unverified';
    node('installed-components').replaceChildren();
  }
  function render(value) {
    snapshot = value;
    const health = value.health;
    const words = wording[health.overall] || wording.unavailable;
    node('installed-heading').textContent = words[0];
    node('installed-description').textContent = words[1];
    for (const [id, dimension] of [['chatgpt', 'web_auth'], ['app', 'codex_app'], ['cli', 'codex_cli']]) {
      node(`installed-${id}`).textContent = stateText[health.components[dimension]?.state] || 'Unverified';
    }
    const details = node('installed-components'); details.replaceChildren();
    const labels = { runtime: 'Runtime', browser: 'Browser', web_auth: 'ChatGPT sign-in', web_models: 'Web models', native_upstream: 'Native Codex connection', codex_app: 'Codex App', codex_cli: 'Codex CLI', config: 'Configuration' };
    for (const [key, label] of Object.entries(labels)) {
      const component = health.components[key];
      const row = document.createElement('p');
      row.textContent = `${label}: ${stateText[component?.state] || 'Unverified'}${component?.observed_at ? `; observed ${component.observed_at}` : ''}${component?.code ? `; ${component.code}` : ''}`;
      details.append(row);
    }
    controls();
  }
  function clearTimer() { if (timer !== null) clearTimeout(timer); timer = null; }
  function schedule() {
    clearTimer();
    if (attached && selected && !document.hidden && !pending && !confirmation) timer = setTimeout(() => { timer = null; refresh(); }, 30000);
  }
  async function refresh() {
    if (pending || confirmation) return;
    if (!selected) { await attach(); return; }
    const current = ++epoch, installation = selected;
    pending = true; controls(); node('installed-error').hidden = true;
    try {
      const value = await invoke('installed_check', { installation });
      if (current === epoch && installation === selected) render(value);
    } catch (code) {
      if (current === epoch) { unavailable(); error(code); }
    } finally { pending = false; controls(); schedule(); }
  }
  async function attach() {
    if (pending) return attached;
    pending = true; clearTimer(); controls();
    try {
      const report = await invoke('installed_list');
      if (!report.targets.length && !report.diagnostics.length) {
        if (attached) { unavailable(); node('installed-description').textContent = 'The installed connection record is no longer available.'; return true; }
        attached = false; node('installed-view').hidden = true; return false;
      }
      attached = true; node('setup-view').hidden = true; node('installed-view').hidden = false;
      if (report.diagnostics.length) throw 'E_INSTALLED_JOURNAL';
      targets = report.targets;
      const choice = node('installed-choice'); choice.replaceChildren();
      const empty = document.createElement('option'); empty.value = ''; empty.textContent = 'Choose a connected Codex home'; choice.append(empty);
      for (const target of targets) {
        const option = document.createElement('option'); option.value = target.installation; option.textContent = target.home; choice.append(option);
      }
      if (!targets.some(t => t.installation === selected)) selected = targets.length === 1 ? targets[0].installation : '';
      choice.value = selected;
      node('installed-heading').textContent = selected ? 'Checking connection' : 'Choose a connection';
      node('installed-description').textContent = selected ? 'Reading the installed runtime state.' : 'Select the Codex home in Connection details.';
      snapshot = null;
    } catch (code) {
      attached = true; snapshot = null; selected = '';
      node('setup-view').hidden = true; node('installed-view').hidden = false;
      unavailable(); error(code);
    } finally { pending = false; controls(); }
    if (selected) await refresh();
    return attached;
  }
  function confirm(receipt) {
    confirmation = receipt; clearTimer(); controls();
    node('installed-confirm').showModal(); node('installed-keep').focus();
  }
  function closeDialog() {
    confirmation = null; node('installed-confirm').close(); controls();
    node('installed-remove').focus(); schedule();
  }
  async function remove(allowActive, receipt) {
    if (pending || !receipt) return;
    pending = true; clearTimer(); ++epoch; controls(); node('installed-error').hidden = true;
    let active = false;
    try {
      const value = await invoke('installed_disconnect', { ...receipt, allowActive });
      render(value);
    } catch (code) {
      if (code === 'E_WEB_ACTIVE' && !allowActive) active = true;
      else { unavailable(); error(code); }
    } finally { pending = false; controls(); }
    if (active) confirm(receipt); else schedule();
  }
  node('installed-choice').addEventListener('change', async () => {
    if (pending || confirmation) return;
    selected = targets.some(t => t.installation === node('installed-choice').value) ? node('installed-choice').value : '';
    snapshot = null; ++epoch; clearTimer(); controls();
    unavailable();
    if (selected) await refresh();
  });
  node('installed-refresh').addEventListener('click', refresh);
  node('installed-retry').addEventListener('click', async () => {
    if (pending || confirmation || !snapshot || node('installed-retry').disabled) return;
    const receipt = { installation: selected, instance: snapshot.instance };
    pending = true; clearTimer(); ++epoch; controls(); node('installed-error').hidden = true;
    node('installed-description').textContent = 'Verifying the saved session in the background. No message is sent and no sign-in window is opened.';
    try { render(await invoke('installed_retry_web', receipt)); }
    catch (code) { unavailable(); error(code); }
    finally { pending = false; controls(); schedule(); }
  });
  node('installed-remove').addEventListener('click', async () => {
    if (pending || confirmation || !snapshot) return;
    const receipt = { installation: selected, instance: snapshot.instance };
    if (snapshot.health.active_web_turns > 0) confirm(receipt);
    else await remove(false, receipt);
  });
  node('installed-keep').addEventListener('click', closeDialog);
  node('installed-confirm').addEventListener('cancel', event => { event.preventDefault(); closeDialog(); });
  node('installed-stop').addEventListener('click', async () => {
    const receipt = confirmation; closeDialog(); await remove(true, receipt);
  });
  document.addEventListener('visibilitychange', () => {
    clearTimer();
    if (!document.hidden && attached && selected && !pending && !confirmation) refresh();
  });
  return { attach };
})();
