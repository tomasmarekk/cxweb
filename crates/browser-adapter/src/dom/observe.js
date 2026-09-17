function (baselineIds, expectedPrompt) {
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const composers = [...document.querySelectorAll('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]')].filter(visible);
  const form = composers.length === 1 ? composers[0].closest('form') : null;
  const controls = form ? [...form.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"], button[data-testid="model-switcher-dropdown-button"][aria-haspopup="menu"]')].filter(visible) : [];
  const model = controls.at(-1);
  if (!model) throw new Error('E_BROWSER_ADAPTER');
  const containers = [...document.querySelectorAll('[data-turn-id-container]')].filter(node =>
    node.parentElement?.closest('[data-turn-id-container]')?.getAttribute('data-turn-id-container') !== node.getAttribute('data-turn-id-container'));
  if (containers.length > 2000) throw new Error('E_CONTEXT_BUDGET');
  const identities = containers.map(node => node.getAttribute('data-turn-id-container'));
  if (identities.some(id => !id || id.length > 240) || new Set(identities).size !== identities.length) throw new Error('E_TURN_IDENTITY');
  const old = new Set(baselineIds);
  const fresh = containers.filter(node => !old.has(node.getAttribute('data-turn-id-container')));
  const userSelector = '[data-turn="user"], [data-message-author-role="user"]';
  const assistantSelector = '[data-turn="assistant"], [data-message-author-role="assistant"]';
  const users = fresh.filter(node => node.matches(userSelector) || node.querySelector(userSelector));
  const user = users.length === 1 ? users[0] : null;
  const userIndex = user ? containers.indexOf(user) : -1;
  const afterUser = user ? containers.slice(userIndex + 1).filter(node => !old.has(node.getAttribute('data-turn-id-container'))) : [];
  const assistants = afterUser.filter(node => node.matches(assistantSelector) || node.querySelector(assistantSelector));
  const assistant = assistants.length === 1 ? assistants[0] : null;
  const content = assistant?.querySelector('.markdown');
  const userContent = user?.querySelector('[data-message-author-role="user"]') ?? user;
  const text = content?.innerText ?? '';
  if (text.length > 4 * 1024 * 1024) throw new Error('E_PAYLOAD_LIMIT');
  return {
    user_id: user?.getAttribute('data-turn-id-container') ?? null,
    user_matches: !!userContent && userContent.innerText.replace(/\r\n/g, '\n').trim() === expectedPrompt.replace(/\r\n/g, '\n').trim(),
    assistant_id: assistant?.getAttribute('data-turn-id-container') ?? null,
    text,
    generating: !!document.querySelector('[data-testid="stop-button"]'),
    completion_control: !!assistant?.querySelector('[data-testid="copy-turn-action-button"]'),
    fenced_output: !!content?.querySelector('pre'),
    selected_model: model.textContent.trim().slice(0, 120),
    ambiguous: users.length > 1 || assistants.length > 1 || afterUser.some(node => node !== user && (node.matches(userSelector) || node.querySelector(userSelector)))
  };
}
