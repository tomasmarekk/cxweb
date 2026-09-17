function (expectedPrompt, expectedModel) {
  const composer = document.querySelector('#prompt-textarea');
  const model = document.querySelector('[data-testid="model-switcher-dropdown-button"]');
  const send = document.querySelector('[data-testid="send-button"]');
  if (!composer || !model || !send || send.disabled || document.querySelector('[data-testid="stop-button"]') || model.textContent.trim() !== expectedModel) throw new Error('E_BROWSER_ADAPTER');
  if ((composer.value ?? composer.innerText).replace(/\r\n/g, '\n') !== expectedPrompt.replace(/\r\n/g, '\n')) throw new Error('E_COMPOSER_MISMATCH');
  send.click();
  return true;
}
