function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0
    && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
  const dialogs = [...document.querySelectorAll('[role="dialog"]')].filter(visible);
  if (dialogs.length !== 1) return null;
  const tabs = [...dialogs[0].querySelectorAll('[role="tab"]')].filter(node => visible(node) && node.textContent.replace(/\s+/g, ' ').trim() === 'Account');
  if (tabs.length !== 1 || tabs[0].getAttribute('aria-disabled') === 'true' || tabs[0].disabled || tabs[0].closest('[inert]')) return null;
  const tab = tabs[0];
  if (tab.getAttribute('aria-selected') === 'true') return { selected: true };
  tab.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  // The dialog animates after mounting. Focusing the actual tab avoids using
  // coordinates captured before its final position; native Enter follows.
  tab.focus({ preventScroll: true });
  return document.activeElement === tab ? { focused: true } : null;
}
