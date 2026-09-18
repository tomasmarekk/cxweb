function (prompt) {
  const composer = document.querySelector('#prompt-textarea');
  if (!composer || document.activeElement !== composer || !composer.isContentEditable) return false;
  if (composer.textContent !== '' || document.querySelector('[data-testid="stop-button"]')) return false;
  // Place a collapsed caret inside the empty editor. The browser editing command
  // preserves literal input while notifying the editor through its input event.
  const caret = document.createRange();
  caret.selectNodeContents(composer);
  caret.collapse(true);
  const selection = window.getSelection();
  if (!selection) return false;
  selection.removeAllRanges();
  selection.addRange(caret);
  return document.execCommand('insertText', false, prompt);
}
