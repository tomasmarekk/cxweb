function (expectedIdentity) {
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const match = /^reasoning-slider:(-?\d+):(-?\d+):(-?\d+)$/.exec(expectedIdentity);
  if (!match) throw new Error('E_MODEL_IDENTITY');
  const expectedMin = Number(match[1]);
  const expectedMax = Number(match[2]);
  const target = Number(match[3]);
  const containers = [...document.querySelectorAll('[data-model-reasoning-effort-slider]')].filter(visible);
  if (containers.length !== 1) throw new Error('E_MODEL_SLIDER');
  const sliders = [...containers[0].querySelectorAll('[role="slider"]')];
  if (sliders.length !== 1 || !(sliders[0] instanceof HTMLElement)) throw new Error('E_MODEL_SLIDER');
  const slider = sliders[0];
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
  const label = (controls.at(-1)?.textContent ?? 'ChatGPT route').replace(/\s+/g, ' ').trim().slice(0, 90) || 'ChatGPT route';
  return { min, max, current, target, label, candidate_label: `${label} · effort ${target - min + 1}`.slice(0, 120) };
}
