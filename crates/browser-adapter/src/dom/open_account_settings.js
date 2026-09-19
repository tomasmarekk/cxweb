function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0;
  if ([...document.querySelectorAll('[role="dialog"]')].some(visible)) return false;
  const profiles = [...document.querySelectorAll('[data-testid="accounts-profile-button"][aria-expanded="true"]')].filter(visible);
  if (profiles.length !== 1) return false;
  const id = profiles[0].getAttribute('aria-controls');
  const controlled = id ? document.getElementById(id) : null;
  const menus = controlled && visible(controlled) ? [controlled] : [...document.querySelectorAll('[role="menu"]')].filter(visible);
  if (menus.length !== 1) return false;
  const settings = [...menus[0].querySelectorAll('[role="menuitem"]')].filter(node => visible(node) && node.textContent.replace(/\s+/g, ' ').trim() === 'Settings');
  if (settings.length !== 1) return false;
  settings[0].click();
  return true;
}
