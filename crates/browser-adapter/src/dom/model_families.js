function () {
  // The alternate picker view retains the checked model while the effort view
  // is displayed. Read only that explicitly scoped model menu, never page text.
  const views = [...document.querySelectorAll('[data-testid="composer-model-picker-slider-advanced-view"]')];
  if (views.length !== 1) throw new Error('E_MODEL_FAMILY');
  const nodes = [...views[0].querySelectorAll('[role="menuitemradio"]')];
  if (!nodes.length || nodes.length > 16) throw new Error('E_MODEL_FAMILY');
  const families = nodes.map(node => {
    const label = (node.getAttribute('aria-label') || node.innerText || node.textContent || '').split('\n')[0].replace(/\s+/g, ' ').trim();
    if (!label || label.length > 80 || /[\u0000-\u001f\u007f]/.test(label)) throw new Error('E_MODEL_FAMILY');
    return { label, identity: label, selected: node.getAttribute('aria-checked') === 'true', disabled: node.getAttribute('aria-disabled') === 'true' || node.disabled === true };
  });
  if (new Set(families.map(item => item.identity)).size !== families.length || families.filter(item => item.selected).length !== 1 || families.find(item => item.selected).disabled) throw new Error('E_MODEL_FAMILY');
  return families;
}
