function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0
    && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
  const profiles = [...document.querySelectorAll('[data-testid="accounts-profile-button"]')].filter(node => visible(node) && node.getAttribute('aria-expanded') === 'true');
  if (profiles.length !== 1) return null;
  const root = document.getElementById(profiles[0].getAttribute('aria-controls') ?? '');
  if (!root || !visible(root)) return null;
  const first = [...root.querySelectorAll('[role="menuitem"]')].filter(visible)[0];
  if (!first) return null;
  const box = first.getBoundingClientRect();
  const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
  const point = hit && first.contains(hit) ? { x: box.left + box.width / 2, y: box.top + box.height / 2 } : null;
  const expanded = first.getAttribute('aria-expanded') === 'true' || first.getAttribute('data-state') === 'open';
  const controlled = document.getElementById(first.getAttribute('aria-controls') ?? '');
  const others = [...document.querySelectorAll('[role="menu"]')].filter(node => node !== root && visible(node));
  const submenu = expanded ? (controlled && visible(controlled) ? controlled : others.length === 1 ? others[0] : null) : null;
  const text = submenu?.innerText ?? '';
  if (text.length > 16000) throw new Error('E_ACCOUNT_SCOPE');
  const publicLabels = ['Personal', 'Personal account', 'Personal workspace', 'Workspaces', 'Switch account', 'Add account', 'Add workspace', 'Manage workspace', 'Workspace settings', 'Pro', 'Plus', 'Free', 'Business', 'Enterprise'];
  const labels = text.split('\n').map(line => line.trim());
  const selected = '[aria-checked="true"], [aria-selected="true"], [data-state="checked"]';
  const controls = submenu ? [...submenu.querySelectorAll('[role="menuitem"], [role="menuitemradio"], [role="option"], button')].filter(visible) : [];
  const selectedAccounts = controls.filter(node => node.getAttribute('role') === 'menuitemradio' && node.matches(selected));
  const emailCount = text => new Set((text ?? '').match(/[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\.[A-Z]{2,}/gi) ?? []).size;
  const selectedEmails = selectedAccounts.length === 1 ? [...new Set((selectedAccounts[0].innerText ?? '').match(/[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\.[A-Z]{2,}/gi) ?? [])] : [];
  // The observed single-account layout places its email in the heading before
  // the checked row. Corroboration against Settings happens in Rust; this alone
  // does not assert either the active account or a provider workspace ID.
  const headingLayout = controls.length === 3 && controls[0].getAttribute('role') === 'menuitem'
    && !controls[0].matches(selected) && controls[1] === selectedAccounts[0]
    && controls[2].getAttribute('role') === 'menuitem' && controls[2].innerText.trim() === 'Add account';
  const headingEmails = headingLayout && selectedEmails.length === 0
    ? [...new Set((controls[0].innerText ?? '').match(/[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\.[A-Z]{2,}/gi) ?? [])] : [];
  const accountSource = selectedEmails.length === 1 ? 'selected_account' : headingEmails.length === 1 ? 'account_heading' : null;
  const account = accountSource === 'selected_account' ? selectedEmails[0] : accountSource === 'account_heading' ? headingEmails[0] : null;
  return {
    point,
    account: account && account.length <= 320 ? account.toLowerCase() : null,
    diagnostic: {
      available: first.getAttribute('aria-haspopup') === 'menu',
      expanded, submenu_present: !!submenu,
      account_candidates: new Set(text.match(/[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\.[A-Z]{2,}/gi) ?? []).size,
      workspace_candidates: submenu?.querySelectorAll('[data-workspace-id], [data-testid*="workspace"]').length ?? 0,
      selected_items: submenu?.querySelectorAll(selected).length ?? 0,
      selected_account_candidates: selectedEmails.length,
      account_source: accountSource,
      selected_email_sources: selectedAccounts.length === 1 ? [selectedAccounts[0].innerText, selectedAccounts[0].textContent, selectedAccounts[0].getAttribute('aria-label')].map(emailCount) : [],
      email_controls: controls.filter(node => emailCount(node.innerText) > 0).map(node => {
        const role = node.getAttribute('role');
        return `${['menuitem', 'menuitemradio', 'option'].includes(role) ? role : 'button'}:${node.matches(selected) ? 'selected' : 'unselected'}`;
      }),
      public_labels: publicLabels.filter(label => labels.includes(label)),
      controls: controls.slice(0, 32).map(node => {
        const role = node.getAttribute('role');
        const kind = ['menuitem', 'menuitemradio', 'option'].includes(role) ? role : 'button';
        const label = (node.textContent ?? '').replace(/\s+/g, ' ').trim();
        return `${kind}:${publicLabels.includes(label) ? label : 'other'}:${node.matches(selected) ? 'selected' : 'unselected'}`;
      })
    }
  };
}
