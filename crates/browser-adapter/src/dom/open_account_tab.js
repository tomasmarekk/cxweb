function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0;
  const dialogs = [...document.querySelectorAll('[role="dialog"]')].filter(visible);
  if (dialogs.length !== 1) return null;
  const tabs = [...dialogs[0].querySelectorAll('[role="tab"]')].filter(node => visible(node) && node.textContent.replace(/\s+/g, ' ').trim() === 'Account');
  if (tabs.length !== 1 || tabs[0].getAttribute('aria-disabled') === 'true') return null;
  const tab = tabs[0];
  if (tab.getAttribute('aria-selected') === 'true') return { selected: true };
  tab.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  const box = tab.getBoundingClientRect();
  const x = box.left + box.width / 2, y = box.top + box.height / 2;
  if (box.width <= 0 || box.height <= 0 || !tab.contains(document.elementFromPoint(x, y))) return null;
  return { x, y };
}
