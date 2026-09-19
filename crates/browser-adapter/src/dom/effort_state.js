function (expectedIdentity) {
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const route = JSON.parse(expectedIdentity);
  if (!Array.isArray(route) || route.length !== 5 || route[0] !== 'reasoning-slider-v2') throw new Error('E_MODEL_IDENTITY');
  const [, expectedFamily, expectedMin, expectedMax, target] = route;
  const family = readModelFamilies().find(item => item.selected);
  if (family.identity !== expectedFamily) throw new Error('E_MODEL_FAMILY');
  const containers = [...document.querySelectorAll('[data-model-reasoning-effort-slider]')].filter(visible);
  if (containers.length !== 1) throw new Error('E_MODEL_SLIDER');
  const sliders = [...containers[0].querySelectorAll('[role="slider"]')];
  if (sliders.length !== 1 || !(sliders[0] instanceof HTMLElement)) throw new Error('E_MODEL_SLIDER');
  const slider = sliders[0];
  if (!['aria-valuemin', 'aria-valuemax', 'aria-valuenow'].every(name => slider.hasAttribute(name))) throw new Error('E_MODEL_SLIDER');
  const min = Number(slider.getAttribute('aria-valuemin'));
  const max = Number(slider.getAttribute('aria-valuemax'));
  const current = Number(slider.getAttribute('aria-valuenow'));
  if (![min, max, current, target].every(Number.isSafeInteger)
      || min !== expectedMin || max !== expectedMax || min > current || current > max
      || min > target || target > max || max - min >= 5) throw new Error('E_MODEL_SLIDER');
  slider.focus();
  if (document.activeElement !== slider) throw new Error('E_MODEL_FOCUS');
  const composer = document.querySelector('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]');
  const form = composer?.closest('form');
  const controls = form ? [...form.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"], button[data-testid="model-switcher-dropdown-button"][aria-haspopup="menu"]')].filter(visible) : [];
  if (!controls.length) throw new Error('E_MODEL_MENU');
  const label = family.label;
  let effortLabel = null;
  try { effortLabel = readEffortLabel(slider, min, max, current); } catch { /* The label may hydrate after the numeric value. */ }
  return { min, max, current, target, label, candidate_label: effortLabel ? `${label} · ${effortLabel}` : null };
}
