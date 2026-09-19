function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0
    && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
  const composers = [...document.querySelectorAll('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]')].filter(visible);
  if (composers.length !== 1) throw new Error('E_MODEL_MENU');
  const form = composers[0].closest('form');
  const controls = form ? [...form.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"], button[data-testid="model-switcher-dropdown-button"][aria-haspopup="menu"]')].filter(visible) : [];
  const button = controls.at(-1);
  if (!button || button.disabled || button.getAttribute('aria-disabled') === 'true' || button.closest('[inert]')) throw new Error('E_MODEL_MENU');
  if (button.getAttribute('aria-expanded') === 'true') return { expanded: true };
  button.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  button.focus({ preventScroll: true });
  if (document.activeElement !== button) throw new Error('E_MODEL_FOCUS');
  return { focused: true };
}
