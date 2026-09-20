function () {
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const composers = [...document.querySelectorAll('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]')].filter(visible);
  if (composers.length !== 1) throw new Error('E_BROWSER_BASELINE_COMPOSER');
  const form = composers[0].closest('form');
  const controls = form ? [...form.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"], button[data-testid="model-switcher-dropdown-button"][aria-haspopup="menu"]')].filter(visible) : [];
  const model = controls.at(-1);
  if (!model) throw new Error('E_BROWSER_BASELINE_MODEL');
  const containers = [...document.querySelectorAll('[data-turn-id-container]')].filter(node =>
    node.parentElement?.closest('[data-turn-id-container]')?.getAttribute('data-turn-id-container') !== node.getAttribute('data-turn-id-container'));
  if (containers.length > 2000) throw new Error('E_CONTEXT_BUDGET');
  const ids = containers.map(node => node.getAttribute('data-turn-id-container'));
  if (ids.some(id => !id || id.length > 240) || new Set(ids).size !== ids.length) throw new Error('E_TURN_IDENTITY');
  return {
    ids,
    selected_model: model.textContent.trim().slice(0, 120),
    composer_empty: (composers[0].value ?? composers[0].textContent).trim() === '',
    generating: !!document.querySelector('[data-testid="stop-button"]')
  };
}
