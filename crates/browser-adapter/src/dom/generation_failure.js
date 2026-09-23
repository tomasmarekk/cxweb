function (user, assistant, generating, complete) {
  if (!user || generating || complete) return false;
  const main = document.querySelector('main');
  if (!main) return false;
  // Match the visible status itself, never a quoted prompt or historical
  // message. Temporary Chat contains one submitted request per page.
  return [...main.querySelectorAll('*')].some(node =>
    node instanceof HTMLElement &&
    node.getClientRects().length > 0 &&
    node.textContent.trim() === 'Thinking failed' &&
    ![...node.children].some(child => child.textContent.trim() === 'Thinking failed') &&
    !user.contains(node) &&
    !node.closest('[data-turn="user"], [data-message-author-role="user"], nav, aside') &&
    (!assistant || assistant.contains(node) || !node.closest('[data-turn-id-container]'))
  );
}
