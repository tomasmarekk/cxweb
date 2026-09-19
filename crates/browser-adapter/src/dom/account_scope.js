function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0;
  const profiles = [...document.querySelectorAll('[data-testid="accounts-profile-button"]')].filter(visible);
  const expanded = profiles.filter(profile => profile.getAttribute('aria-expanded') === 'true');
  if (expanded.length !== 1) return null;
  const profile = expanded[0];
  const controlled = profile?.getAttribute('aria-controls');
  const root = controlled ? document.getElementById(controlled) : null;
  const menus = root && visible(root) ? [root] : [...document.querySelectorAll('[role="menu"]')].filter(visible);
  if (menus.length !== 1) return null;
  const menu = menus[0];
  const text = menu.innerText;
  if (text.length > 16000) throw new Error('E_ACCOUNT_SCOPE');
  const emails = [...new Set(text.match(/[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\.[A-Z]{2,}/gi) ?? [])];
  const selected = '[aria-checked="true"], [aria-selected="true"], [data-state="checked"]';
  const workspaces = [...menu.querySelectorAll('[data-workspace-id], [data-testid*="workspace"]')].filter(visible);
  const selectedWorkspaces = workspaces.filter(node => node.matches(selected));
  const workspace = selectedWorkspaces.length === 1 ? selectedWorkspaces[0].getAttribute('data-workspace-id') : null;
  return {
    account: emails.length === 1 ? emails[0].toLowerCase() : null,
    workspace: workspace && workspace.length <= 240 ? workspace : null,
    diagnostic: {
      menu_present: true,
      account_candidates: emails.length,
      workspace_candidates: workspaces.length,
      selected_workspace_candidates: selectedWorkspaces.length,
      menu_items: menu.querySelectorAll('[role="menuitem"], [role="menuitemradio"]').length,
      selected_items: menu.querySelectorAll(selected).length,
      has_account_id: !!menu.querySelector('[data-account-id]'),
      has_workspace_id: !!menu.querySelector('[data-workspace-id]')
    }
  };
}
