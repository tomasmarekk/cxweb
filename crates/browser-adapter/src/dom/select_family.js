function (identity, opened, clicked) {
  const views = [...document.querySelectorAll('[data-testid="composer-model-picker-slider-simple-view"], [data-testid="composer-model-picker-slider-advanced-view"]')];
  if (!views.some(node => node instanceof HTMLElement && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }))) return null;
  const families = readModelFamilies();
  const index = families.findIndex(item => item.identity === identity && !item.disabled);
  if (index < 0) throw new Error('E_MODEL_FAMILY');
  if (families[index].selected) return { selected: true };
  if (clicked) return null;
  const view = document.querySelector('[data-testid="composer-model-picker-slider-advanced-view"]');
  const option = [...view.querySelectorAll('[role="menuitemradio"]')][index];
  const point = node => {
    if (!(node instanceof HTMLElement) || !node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) return null;
    const box = node.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0) return null;
    const x = box.left + box.width / 2, y = box.top + box.height / 2;
    const hit = document.elementFromPoint(x, y);
    return hit && node.contains(hit) ? { x, y } : null;
  };
  const optionPoint = point(option);
  if (optionPoint) return { ...optionPoint, action: 'select' };
  if (opened) return null;
  const triggers = [...document.querySelectorAll('[aria-label="Select model"]')].filter(node => point(node));
  return triggers.length === 1 ? { ...point(triggers[0]), action: 'open' } : null;
}
