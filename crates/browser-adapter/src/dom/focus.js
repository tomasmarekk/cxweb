function () {
  const composer = document.querySelector('#prompt-textarea');
  if (!composer || (composer.value ?? composer.textContent).trim() !== '' || document.querySelector('[data-testid="stop-button"]')) throw new Error('E_BROWSER_BUSY');
  composer.focus();
  return document.activeElement === composer;
}
