function () {
  const controls = [...document.querySelectorAll('[data-testid="accounts-profile-button"]')]
    .filter(node => node instanceof HTMLElement && node.getClientRects().length
      && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }));
  if (controls.length === 0) return { failure: 'E_ACCOUNT_MISSING' };
  // Responsive layouts can expose more than one entry point for this menu.
  // Opening an entry point is not proof of account identity.
  const expanded = controls.filter(control => control.getAttribute('aria-expanded') === 'true');
  if (expanded.length > 1) return { failure: 'E_ACCOUNT_AMBIGUOUS' };
  if (expanded.length === 1) return true;
  for (const control of controls) {
    control.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    const box = control.getBoundingClientRect();
    const x = box.left + box.width / 2, y = box.top + box.height / 2;
    const hit = document.elementFromPoint(x, y);
    if (box.width > 0 && box.height > 0 && hit && control.contains(hit)) return { x, y };
  }
  return {
    failure: 'E_ACCOUNT_OPEN',
    control_tags: controls.slice(0, 8).map(node => `${node.tagName}:${node.tabIndex}`)
  };
}
