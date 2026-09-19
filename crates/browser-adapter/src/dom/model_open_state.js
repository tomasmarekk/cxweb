function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0;
  const composers = [...document.querySelectorAll('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]')].filter(visible);
  const form = composers.length === 1 ? composers[0].closest('form') : null;
  const controls = form ? [...form.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"], button[data-testid="model-switcher-dropdown-button"][aria-haspopup="menu"]')].filter(visible) : [];
  const button = controls.at(-1);
  const box = button?.getBoundingClientRect();
  const hit = box ? document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2) : null;
  return {
    composer_unique: composers.length === 1, form_present: !!form,
    control_present: !!button, control_unique: controls.length === 1,
    expanded: button?.getAttribute('aria-expanded') === 'true',
    button_visible: !!button?.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }),
    button_enabled: !!button && !button.disabled && button.getAttribute('aria-disabled') !== 'true',
    button_hit: !!hit && button.contains(hit), hit_present: !!hit,
    hit_is_body: hit === document.body,
    box_in_viewport: !!box && box.top >= 0 && box.left >= 0 && box.bottom <= innerHeight && box.right <= innerWidth,
    dialog_present: [...document.querySelectorAll('[role="dialog"]')].some(visible),
    document_focused: document.hasFocus(),
  };
}
