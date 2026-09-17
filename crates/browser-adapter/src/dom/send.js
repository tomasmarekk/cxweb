function (expectedPrompt, expectedModel) {
  const composer = document.querySelector('#prompt-textarea');
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const form = composer?.closest('form');
  const model = form ? [...form.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"], button[data-testid="model-switcher-dropdown-button"][aria-haspopup="menu"]')].filter(visible).at(-1) : null;
  const send = document.querySelector('[data-testid="send-button"]');
  if (!composer || !model || !send) return 'E_SEND_SURFACE';
  if (send.disabled || document.querySelector('[data-testid="stop-button"]')) return 'E_SEND_DISABLED';
  if (model.textContent.trim() !== expectedModel) return 'E_MODEL_SELECTION';
  if ((composer.value ?? composer.innerText).replace(/\r\n/g, '\n') !== expectedPrompt.replace(/\r\n/g, '\n')) return 'E_COMPOSER_MISMATCH';
  send.click();
  return true;
}
