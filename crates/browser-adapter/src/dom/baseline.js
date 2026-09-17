function () {
  const composer = document.querySelector('#prompt-textarea');
  const model = document.querySelector('[data-testid="model-switcher-dropdown-button"]');
  if (!composer || !model) throw new Error('E_BROWSER_ADAPTER');
  const messages = [...document.querySelectorAll('[data-message-id][data-message-author-role]')];
  if (messages.length > 2000) throw new Error('E_CONTEXT_BUDGET');
  return {
    ids: messages.map(node => node.getAttribute('data-message-id')),
    selected_model: model.textContent.trim(),
    composer_empty: (composer.value ?? composer.textContent).trim() === '',
    generating: !!document.querySelector('[data-testid="stop-button"]')
  };
}
