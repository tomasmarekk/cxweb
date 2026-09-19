function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0
    && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
  const menus = [...document.querySelectorAll('[role="menu"], [role="listbox"], [data-testid="composer-intelligence-picker-content"], [data-model-reasoning-effort-slider]')];
  const expanded = [...document.querySelectorAll('form button[aria-haspopup="menu"][aria-expanded="true"][data-tone="neutral"], form button[data-testid="model-switcher-dropdown-button"][aria-expanded="true"], [data-testid="accounts-profile-button"][aria-expanded="true"]')];
  return !menus.some(visible) && !expanded.some(visible);
}
