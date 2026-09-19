function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0
    && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
  const menus = [...document.querySelectorAll('[role="menu"], [role="listbox"], [data-testid="composer-intelligence-picker-content"], [data-model-reasoning-effort-slider]')];
  return !menus.some(visible);
}
