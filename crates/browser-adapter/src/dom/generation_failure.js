function (user, assistant) {
  if (!user) return false;
  const main = document.querySelector('main');
  if (!main) return false;
  // A partial answer may already have Copy even though ChatGPT has failed.
  // Accept only a visible terminal status or its explicit error control, never
  // a quoted prompt or a historical assistant card.
  return [...main.querySelectorAll('*')].some(node =>
    node instanceof HTMLElement &&
    node.getClientRects().length > 0 &&
    !user.contains(node) &&
    !node.closest('[data-turn="user"], [data-message-author-role="user"], nav, aside') &&
    (!node.closest('[data-turn-id-container]') || !!assistant?.contains(node)) &&
    (node.matches('[data-testid="regenerate-thread-error-button"]') ||
      (node.textContent.trim() === 'Thinking failed' &&
        ![...node.children].some(child => child.textContent.trim() === 'Thinking failed')))
  );
}
