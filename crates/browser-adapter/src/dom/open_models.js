function () {
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const composers = [...document.querySelectorAll('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]')].filter(visible);
  if (composers.length !== 1) throw new Error('E_MODEL_MENU');
  const form = composers[0].closest('form');
  if (!form) throw new Error('E_MODEL_MENU');
  const controls = [...form.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"], button[data-testid="model-switcher-dropdown-button"][aria-haspopup="menu"]')].filter(visible);
  const button = controls.at(-1);
  if (!(button instanceof HTMLElement) || button.getClientRects().length === 0) throw new Error('E_MODEL_MENU');
  const box = button.getBoundingClientRect();
  if (box.width <= 0 || box.height <= 0) throw new Error('E_MODEL_MENU');
  return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
}
