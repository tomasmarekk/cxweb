function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0
    && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
  const dialogs = [...document.querySelectorAll('[role="dialog"]')].filter(visible);
  const diagnostic = { dialogs: dialogs.length, settings: false, close_candidates: 0, close_labels: [], hit: false };
  if (dialogs.length !== 1) return { diagnostic };
  const dialog = dialogs[0];
  const labels = [...dialog.querySelectorAll('[role="tab"]')].filter(visible)
    .map(node => node.textContent.replace(/\s+/g, ' ').trim());
  diagnostic.settings = labels.includes('Account') && labels.includes('General');
  const publicLabels = ['Close', 'Close settings', 'Close dialog', 'Done', 'Back'];
  const allButtons = [...dialog.querySelectorAll('button')].filter(visible);
  diagnostic.close_labels = [...new Set(allButtons.flatMap(node => [node.getAttribute('aria-label'), node.textContent?.trim()]).filter(label => publicLabels.includes(label)))];
  if (!diagnostic.settings) return { diagnostic };
  const buttons = allButtons.filter(node => visible(node)
    && !node.disabled && !node.closest('[inert]')
    && [node.getAttribute('aria-label'), node.textContent?.trim()].some(label => ['Close', 'Close settings'].includes(label)));
  diagnostic.close_candidates = buttons.length;
  if (buttons.length !== 1) return { diagnostic };
  const button = buttons[0];
  const box = button.getBoundingClientRect();
  const x = box.left + box.width / 2, y = box.top + box.height / 2;
  const hit = document.elementFromPoint(x, y);
  diagnostic.hit = !!(box.width > 0 && box.height > 0 && hit && button.contains(hit));
  return diagnostic.hit ? { x, y, diagnostic } : { diagnostic };
}
