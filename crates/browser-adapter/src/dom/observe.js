function (baselineIds, expectedPrompt) {
  const model = document.querySelector('[data-testid="model-switcher-dropdown-button"]');
  if (!model) throw new Error('E_BROWSER_ADAPTER');
  const messages = [...document.querySelectorAll('[data-message-id][data-message-author-role]')];
  if (messages.length > 2000) throw new Error('E_CONTEXT_BUDGET');
  const old = new Set(baselineIds);
  const fresh = messages.filter(node => !old.has(node.getAttribute('data-message-id')));
  const users = fresh.filter(node => node.getAttribute('data-message-author-role') === 'user');
  const user = users.length === 1 ? users[0] : null;
  const userIndex = user ? messages.indexOf(user) : -1;
  const afterUser = user ? messages.slice(userIndex + 1) : [];
  const assistants = afterUser.filter(node => node.getAttribute('data-message-author-role') === 'assistant');
  const assistant = assistants.length === 1 ? assistants[0] : null;
  const content = assistant?.querySelector('.markdown');
  const turn = assistant?.closest('[data-testid^="conversation-turn-"]');
  const text = content?.innerText ?? '';
  if (text.length > 4 * 1024 * 1024) throw new Error('E_PAYLOAD_LIMIT');
  return {
    user_id: user?.getAttribute('data-message-id') ?? null,
    user_matches: !!user && user.innerText.replace(/\r\n/g, '\n') === expectedPrompt.replace(/\r\n/g, '\n'),
    assistant_id: assistant?.getAttribute('data-message-id') ?? null,
    text,
    generating: !!document.querySelector('[data-testid="stop-button"]'),
    completion_control: !!turn?.querySelector('[data-testid="copy-turn-action-button"]'),
    fenced_output: !!content?.querySelector('pre'),
    selected_model: model.textContent.trim(),
    ambiguous: users.length > 1 || assistants.length > 1 || afterUser.some(node => node.getAttribute('data-message-author-role') === 'user')
  };
}
