function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0;
  const dialogs = [...document.querySelectorAll('[role="dialog"]')].filter(visible);
  if (dialogs.length !== 1) return null;
  const dialog = dialogs[0];
  const known = ['General', 'Notifications', 'Personalization', 'Apps', 'Connected apps', 'Data controls', 'Security', 'Parental controls', 'Account'];
  const tabs = [...dialog.querySelectorAll('[role="tab"]')].filter(visible);
  const text = node => node.textContent.replace(/\s+/g, ' ').trim();
  const controls = known.filter(label => tabs.some(tab => text(tab) === label));
  const accounts = tabs.filter(tab => text(tab) === 'Account');
  const selected = accounts.length === 1 && accounts[0].getAttribute('aria-selected') === 'true';
  const controlled = selected ? accounts[0].getAttribute('aria-controls') : null;
  const panel = controlled ? document.getElementById(controlled) : null;
  const content = panel && dialog.contains(panel) && visible(panel) ? panel.innerText : '';
  if (content.length > 16000) throw new Error('E_ACCOUNT_SCOPE');
  const emails = [...new Set(content.match(/[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\.[A-Z]{2,}/gi) ?? [])];
  const knownFields = ['Email', 'Email address', 'Subscription', 'Plan', 'Manage', 'Workspace', 'Name'];
  return {
    account: emails.length === 1 ? emails[0].toLowerCase() : null,
    account_candidates: emails.length, controls, account_selected: selected, panel_present: !!panel && dialog.contains(panel) && visible(panel),
    fields: knownFields.filter(label => content.split('\n').some(line => line.trim() === label)),
    panel_loading: !!panel?.querySelector('[aria-busy="true"], [data-testid*="loading"], [data-testid*="skeleton"]')
  };
}
