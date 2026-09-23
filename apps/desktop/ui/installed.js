'use strict';
// Installed-host attachment is independent of the login/test controller. A
// missing or unresponsive installed host must never launch a competing owner.
window.cxwebInstalled = (() => {
  const node = id => document.getElementById(id);
  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  let targets = [], selected = '', snapshot = null, pending = false, confirmation = null;
  let timer = null, epoch = 0, attached = false;
  const stateText = { healthy: 'Verified', degraded: 'Degraded', unavailable: 'Unavailable', unknown: 'Unverified', not_installed: 'Not installed', restart_required: 'Restart required', auth_required: 'Sign-in required', incompatible: 'Incompatible', conflict: 'Conflict' };
  const reasoningHelp = 'Verify the remaining reasoning choices with eight fixed test messages. This uses your ChatGPT allowance. No coding tools run and no project files change. Verification continues if you close this window.';
  function canQualifyReasoning() {
    const health = snapshot?.health;
    return snapshot?.reasoning?.length === 1 && snapshot.reasoning[0].levels.length === 1 &&
      ['preflight', 'ready', 'restart_required'].includes(health.overall) && health.active_web_turns === 0 &&
      ['runtime', 'web_auth', 'web_models'].every(key => health.components[key]?.state === 'healthy');
  }
  const wording = {
    disconnected: ['Connection inactive', 'The runtime is available. Web model routing is not active.'],
    preflight: ['Verifying connection', 'Some parts of the connection still need verification.'],
    authenticating: ['Sign in to ChatGPT', 'Complete sign-in in the managed sign-in window.'],
    discovering: ['Verifying models', 'The runtime is checking available web models.'],
    ready: ['Connected', 'ChatGPT Web models are available in Codex.'],
    busy: ['Task in progress', 'A web response is running. Client verification is shown below.'],
    restart_required: ['Restart Codex', 'Close and reopen Codex to load the connection.'],
    auth_required: ['ChatGPT sign-in required', 'Web requests are unavailable until the runtime verifies your sign-in again.'],
    rate_limited: ['ChatGPT limit reached', 'ChatGPT temporarily limited access because requests were too frequent. Wait a few minutes before trying again. No automatic retry was made; signing in again is not required.'],
    offline: ['Connection unavailable', 'Check your internet connection.'],
    incompatible: ['Compatibility check required', 'Web models are unavailable with the currently verified configuration.'],
    config_conflict: ['Configuration changed', 'Your changes were preserved. Review the connection details.'],
    unavailable: ['ChatGPT unavailable', 'Web requests are currently unavailable. Check the connection details.'],
    disconnecting: ['Removing connection', 'Waiting for response cleanup and restoring configuration.'],
    removal_pending_restart: ['Finish removal', 'Close and reopen Codex. The runtime remains available for clients using the previous connection.'],
    disconnected_complete: ['Connection removed', 'The original Codex configuration has been restored.']
  };
  function connectionWords(health) {
    const components = health.components;
    if (health.overall === 'auth_required' && components.web_auth?.code === 'E_LOGIN_WINDOW_OPEN') {
      return ['Complete ChatGPT sign-in', 'Finish sign-in or verification in the browser, close its window, then select Verify sign-in. Use the same account and workspace as this connection.'];
    }
    if (health.overall === 'unavailable') {
      if (components.runtime?.state !== 'healthy') return ['Runtime needs attention', 'Response cleanup or runtime availability could not be confirmed. Review the connection details.'];
      if (components.config?.state === 'unavailable') return ['Configuration unavailable', 'The Codex configuration could not be checked. Review the connection details.'];
    }
    // A concrete browser failure takes precedence over native service failures.
    // Unknown browser observations do not hide an already observed native error.
    if (['browser', 'web_auth', 'web_models'].every(key => ['healthy', 'unknown'].includes(components[key]?.state))) {
      const native = components.native_upstream;
      if (health.overall === 'auth_required' && native?.code === 'E_NATIVE_AUTH_REQUIRED') {
        return ['Codex sign-in required', 'The native Codex service rejected its sign-in. Sign in again from Codex, then check status. ChatGPT web sign-in is separate.'];
      }
      if (health.overall === 'rate_limited' && native?.code === 'E_NATIVE_RATE_LIMITED') {
        return ['Codex limit reached', 'The native Codex service limited access. Check its limit information in Codex before trying again. No automatic retry was made.'];
      }
      if (health.overall === 'unavailable' && ['E_NATIVE_FORBIDDEN', 'E_NATIVE_SERVER', 'E_NATIVE_REQUEST_REJECTED', 'E_NATIVE_UNAVAILABLE', 'E_NATIVE_REDIRECT', 'E_NATIVE_STREAM'].includes(native?.code)) {
        return ['Codex connection unavailable', 'The native Codex connection failed. Review the connection details and check status after resolving the service or connection problem.'];
      }
    }
    return wording[health.overall] || wording.unavailable;
  }
  function controls() {
    node('installed-compaction-choice').disabled = pending || Boolean(confirmation) || !snapshot;
    for (const id of ['installed-copy-diagnostics', 'installed-export-diagnostics']) node(id).disabled = pending || Boolean(confirmation) || !snapshot;
    node('installed-refresh').disabled = pending || Boolean(confirmation);
    node('installed-choice').disabled = pending || Boolean(confirmation);
    node('installed-remove').disabled = pending || Boolean(confirmation) || !snapshot || ['disconnecting', 'removal_pending_restart', 'disconnected_complete'].includes(snapshot.health.overall);
    const removed = snapshot && ['removal_pending_restart', 'disconnected_complete'].includes(snapshot.health.overall);
    node('installed-clear-session').hidden = !removed;
    node('installed-clear-session').disabled = !removed || pending || Boolean(confirmation);
    const retryable = snapshot?.health.overall === 'unavailable' && snapshot.health.active_web_turns === 0 && snapshot.health.components.runtime?.state === 'healthy' &&
      ['E_ALREADY_RUNNING', 'E_BROWSER_RUNTIME_MISSING', 'E_BROWSER_START', 'E_BACKGROUND_NAVIGATION', 'E_BROWSER_TEMPORARY_NAVIGATION', 'E_BROWSER_OBSERVATION', 'E_BROWSER_BASELINE_MODEL', 'E_BROWSER_BASELINE_COMPOSER'].includes(snapshot.health.components.browser?.code);
    node('installed-retry').hidden = !retryable;
    node('installed-retry').disabled = !retryable || pending || Boolean(confirmation);
    const canQualify = canQualifyReasoning();
    const login = snapshot?.health.overall === 'auth_required' && snapshot.health.components.runtime?.state === 'healthy' &&
      ['E_LOGIN_REQUIRED', 'E_BROWSER_VERIFICATION_REQUIRED', 'E_SESSION_SCOPE', 'E_LOGIN_WINDOW_OPEN'].includes(snapshot.health.components.web_auth?.code);
    node('installed-login').hidden = !login;
    node('installed-login').disabled = !login || pending || Boolean(confirmation) || snapshot.health.active_web_turns !== 0;
    node('installed-login').textContent = snapshot?.health.components.web_auth?.code === 'E_LOGIN_WINDOW_OPEN' ? 'Verify sign-in' : 'Open ChatGPT sign-in';
    node('installed-qualify-reasoning').hidden = !canQualify;
    node('installed-qualify-reasoning').disabled = !canQualify || pending || Boolean(confirmation);
    node('installed-reasoning-help').hidden = !canQualify;
  }
  function error(code) {
    const messages = {
      E_INSTALLED_CHANGED: 'The runtime restarted. Check its current status before taking further action.',
      E_INSTALLED_RECOVERY: 'Background verification could not be restarted. Check status for the current sign-in or compatibility requirement.',
      E_INSTALLED_LOGIN: 'Sign-in could not be completed. Finish signing in, close the managed browser window and any extra tabs, then check status before trying again.',
      E_INSTALLED_UNAVAILABLE: 'The installed runtime is not responding. No replacement process was started.',
      E_INSTALLED_REMOVE: 'Removal could not be completed. Check status for the configuration or cleanup error.',
      E_INSTALLED_TIMEOUT: 'The operation is still unconfirmed. Check status; it was not submitted again.',
      E_INSTALLED_UNCONFIRMED: 'The runtime did not confirm the request. Check status before trying again.',
      E_CONTROL_BUSY: 'Another operation is running. Check status before trying again.',
      E_COMPACTION_MODEL_UNAVAILABLE: 'This compaction model or reasoning level is no longer available. Choose a currently verified option.',
      E_COMPACTION_SETTINGS: 'The compaction preference could not be read or saved. Select an option again to repair it.'
    };
    const reasoningErrors = {
      E_REASONING_DISCOVERY: 'The expected reasoning choices could not be verified.',
      E_QUALIFICATION_PROTOCOL: 'A reasoning test returned an invalid response.',
      E_QUALIFICATION_TIMEOUT: 'A reasoning test did not finish within its time limit.',
      E_SESSION_SCOPE: 'The signed-in account or workspace could not be verified.',
      E_BROWSER_RATE_LIMITED: 'ChatGPT temporarily limited access because requests were too frequent. Wait a few minutes before trying again. Signing in again is not required.',
      E_WEB_CLEANUP_UNCONFIRMED: 'Browser cleanup could not be confirmed. New web requests are blocked.',
      E_CANCELLED: 'Reasoning verification was cancelled.',
      E_WEB_RECOVERY_RECEIPT: 'The verified choices could not be published.',
      E_REASONING_QUALIFICATION: 'Reasoning verification could not be completed.',
      E_WEB_ACTIVE: 'Another web operation is running.'
    };
    node('installed-error').textContent = messages[code] || (reasoningErrors[code] ? `${reasoningErrors[code]} No automatic retry was made. Check status before continuing.` : 'The installed connection could not be verified. No browser or replacement runtime was started.');
    node('installed-error').hidden = false;
  }
  function unavailable() {
    snapshot = null;
    node('installed-heading').textContent = 'Connection status unavailable';
    node('installed-description').textContent = 'The current runtime state could not be verified. Check status before taking further action.';
    for (const id of ['chatgpt', 'app', 'cli']) node(`installed-${id}`).textContent = 'Unverified';
    node('installed-components').replaceChildren();
    node('installed-reasoning').hidden = true;
    node('installed-reasoning-list').replaceChildren();
    node('installed-compaction').hidden = true;
  }
  function render(value) {
    snapshot = value;
    const health = value.health;
    const words = connectionWords(health);
    node('installed-heading').textContent = words[0];
    node('installed-description').textContent = words[1];
    for (const [id, dimension] of [['chatgpt', 'web_auth'], ['app', 'codex_app'], ['cli', 'codex_cli']]) {
      const component = health.components[dimension];
      node(`installed-${id}`).textContent = component?.state === 'unknown' && component.code === 'E_CLIENT_AWAITING_LAUNCH'
        ? 'Awaiting launch' : component?.state === 'healthy' && component.evidence === 'request_success'
        ? 'Request verified' : component?.state === 'healthy' && component.evidence === 'client_handshake'
          ? 'Catalog available' : stateText[component?.state] || 'Unverified';
    }
    const details = node('installed-components'); details.replaceChildren();
    const labels = { runtime: 'Runtime', browser: 'Browser', web_auth: 'ChatGPT sign-in', web_models: 'Web models', native_upstream: 'Native Codex connection', codex_app: 'Codex App', codex_cli: 'Codex CLI', config: 'Configuration' };
    for (const [key, label] of Object.entries(labels)) {
      const component = health.components[key];
      const row = document.createElement('p');
      row.textContent = `${label}: ${stateText[component?.state] || 'Unverified'}${component?.observed_at ? `; observed ${component.observed_at}` : ''}${component?.code ? `; ${component.code}` : ''}`;
      if (component?.evidence === 'request_success') row.textContent += '; successful web request from a reviewed client build; picker verification is separate';
      if (['codex_app', 'codex_cli'].includes(key) && component?.evidence === 'client_handshake') {
        row.textContent += component.state === 'healthy' ? '; qualified catalog offered to the reported client build; no generation test'
          : component.code === 'E_CLIENT_CATALOG_CHANGED' ? '; catalog changed since the client last requested it'
            : '; catalog response could not be verified';
      }
      if (component?.code === 'E_CLIENT_AWAITING_LAUNCH') row.textContent += '; installation found; open this client to load the connection';
      if (component?.code === 'E_CLIENT_DISCOVERY') row.textContent += '; installation could not be determined; no absence or compatibility claim';
      if (component?.code === 'E_CLIENT_NOT_FOUND') row.textContent += key === 'codex_app'
        ? '; official Windows package is not registered for this user'
        : '; not found in the runtime PATH or supported npm locations; a terminal can use different locations';
      details.append(row);
    }
    const families = value.reasoning || [];
    node('installed-compaction').hidden = !families.length;
    const compaction = node('installed-compaction-choice'); compaction.replaceChildren();
    const inherited = document.createElement('option'); inherited.value = ''; inherited.textContent = 'Same as task (Pro can take longer)'; compaction.append(inherited);
    for (const family of families) for (const level of family.levels) {
      const option = document.createElement('option');
      option.value = JSON.stringify({ model: family.model, effort: level.effort });
      option.textContent = `${family.name} · ${level.description}`; compaction.append(option);
    }
    const savedCompaction = value.compaction ? JSON.stringify({ model: value.compaction.model, effort: value.compaction.effort }) : '';
    if (savedCompaction && !Array.from(compaction.options).some(option => option.value === savedCompaction)) {
      const missing = document.createElement('option'); missing.value = savedCompaction; missing.textContent = 'Saved choice is unavailable — select another option'; missing.disabled = true; compaction.append(missing);
    }
    compaction.value = savedCompaction;
    node('installed-compaction-status').textContent = value.compaction_error ? 'The saved preference could not be read. Select an option to repair it.' : value.compaction ? 'Saved for new compactions.' : 'Summaries currently use the task model. Select Instant or Medium for faster summaries when available.';
    node('installed-reasoning').hidden = !families.length;
    const reasoning = node('installed-reasoning-list'); reasoning.replaceChildren();
    node('installed-reasoning-help').textContent = reasoningHelp;
    for (const family of families) {
      const name = document.createElement('p'); name.textContent = family.name; reasoning.append(name);
      const list = document.createElement('ul');
      for (const level of family.levels) {
        const item = document.createElement('li');
        const alias = level.effort === 'low' && level.description === 'Instant' ? ' — Low in CLI, Light in App'
          : level.effort === 'max' && ['Pro', '6 PRO'].includes(level.description) ? ' — Max in Codex' : '';
        item.textContent = `${level.description}${alias}`; list.append(item);
      }
      reasoning.append(list);
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
      const connected = report.targets.filter(target => target.connection_removed !== true);
      if (!connected.length && !report.diagnostics.length) {
        if (attached) { unavailable(); node('installed-description').textContent = 'The installed connection record is no longer available.'; return true; }
        attached = false; node('installed-view').hidden = true; return false;
      }
      attached = true; node('setup-view').hidden = true; node('installed-view').hidden = false;
      if (report.diagnostics.length) throw 'E_INSTALLED_JOURNAL';
      targets = connected;
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
  node('installed-compaction-choice').addEventListener('change', async () => {
    if (pending || confirmation || !snapshot) return;
    const choice = node('installed-compaction-choice').value;
    const receipt = { installation: selected, instance: snapshot.instance, choice: choice ? JSON.parse(choice) : null };
    pending = true; clearTimer(); ++epoch; controls(); node('installed-error').hidden = true;
    try { render(await invoke('installed_set_compaction', receipt)); }
    catch (code) { render(snapshot); error(code); }
    finally { pending = false; controls(); schedule(); }
  });
  async function diagnostics(save) {
    if (pending || confirmation || !snapshot) return;
    const receipt = { installation: selected, instance: snapshot.instance };
    pending = true; clearTimer(); ++epoch; controls();
    const result = node('installed-diagnostics-result');
    result.textContent = save ? 'Choose a new file for the diagnostics.' : 'Preparing safe diagnostics.';
    try {
      if (save) {
        const written = await invoke('installed_export_diagnostics', receipt);
        result.textContent = written ? 'Diagnostics saved. Nothing was uploaded.' : 'Export cancelled. No file was written.';
      } else {
        const report = await invoke('installed_diagnostics', receipt);
        await navigator.clipboard.writeText(report);
        result.textContent = 'Safe diagnostics copied. Nothing was uploaded.';
      }
    } catch (code) {
      result.textContent = code === 'E_DIAGNOSTICS_EXISTS'
        ? 'That file already exists and was preserved. Choose a new filename.'
        : code === 'E_INSTALLED_CHANGED'
        ? 'The runtime restarted. Check status before collecting diagnostics again.'
        : save ? 'Diagnostics could not be saved. Choose another new file and try again.'
        : 'Diagnostics could not be copied. Try exporting them to a new file.';
    } finally { pending = false; controls(); schedule(); }
  }
  node('installed-copy-diagnostics').addEventListener('click', () => diagnostics(false));
  node('installed-export-diagnostics').addEventListener('click', () => diagnostics(true));
  node('installed-login').addEventListener('click', async () => {
    if (pending || confirmation || !snapshot || node('installed-login').disabled) return;
    const receipt = { installation: selected, instance: snapshot.instance, finish: snapshot.health.components.web_auth?.code === 'E_LOGIN_WINDOW_OPEN' };
    pending = true; clearTimer(); ++epoch; controls(); node('installed-error').hidden = true;
    node('installed-description').textContent = receipt.finish ? 'Verifying the saved account, workspace and models. No message is sent.' : 'Opening the official ChatGPT sign-in window.';
    try { render(await invoke('installed_web_login', receipt)); }
    catch (code) { unavailable(); error(code); }
    finally { pending = false; controls(); schedule(); }
  });
  node('installed-qualify-reasoning').addEventListener('click', async () => {
    if (pending || confirmation || !canQualifyReasoning()) return;
    const receipt = { installation: selected, instance: snapshot.instance };
    pending = true; clearTimer(); ++epoch; controls(); node('installed-error').hidden = true;
    node('installed-description').textContent = 'Verifying reasoning choices in the background. This can take several minutes. No test is retried automatically.';
    try {
      render(await invoke('installed_qualify_reasoning', receipt));
      node('installed-description').textContent = 'Reasoning verification finished. The verified choices are shown below. Fully restart Codex to refresh its picker.';
    } catch (code) { unavailable(); error(code); }
    finally { pending = false; controls(); schedule(); }
  });
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
  function closeSessionDialog() {
    confirmation = null;
    node('installed-session-confirm').close();
    controls(); node('installed-clear-session').focus(); schedule();
  }
  node('installed-clear-session').addEventListener('click', () => {
    if (pending || confirmation || node('installed-clear-session').disabled) return;
    confirmation = { localSession: true };
    clearTimer(); controls();
    node('installed-session-confirm').showModal();
    node('installed-session-keep').focus();
  });
  node('installed-session-keep').addEventListener('click', closeSessionDialog);
  node('installed-session-confirm').addEventListener('cancel', event => { event.preventDefault(); closeSessionDialog(); });
  node('installed-session-clear').addEventListener('click', async () => {
    if (pending || !confirmation?.localSession) return;
    confirmation = null; pending = true; ++epoch; clearTimer(); controls();
    node('installed-session-confirm').close();
    const result = node('installed-session-result');
    result.textContent = 'Clearing the dedicated local browser profile.';
    try {
      await invoke('clear_local_session');
      result.textContent = 'The local ChatGPT session was cleared. Your Codex sign-in was preserved. Remote logout and server-side deletion were not performed.';
    } catch (code) {
      result.textContent = code === 'E_SESSION_CONNECTED'
        ? 'Remove every cxweb connection before clearing the shared local ChatGPT session. Nothing was deleted.'
        : code === 'E_SESSION_IN_USE'
        ? 'The dedicated profile is still in use. Close cxweb sign-in windows and finish disconnecting before trying again. Nothing was deleted.'
        : code === 'E_INSTALLED_JOURNAL'
        ? 'Connection ownership could not be verified. Nothing was deleted.'
        : 'The local profile could not be fully cleared. Some local data may remain. No automatic retry was made.';
    } finally { pending = false; controls(); node('installed-clear-session').focus(); schedule(); }
  });
  document.addEventListener('visibilitychange', () => {
    clearTimer();
    if (!document.hidden && attached && selected && !pending && !confirmation) refresh();
  });
  return { attach };
})();
