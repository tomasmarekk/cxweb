function (expectedPrompt, expectedModel) {
  const composer = document.querySelector('#prompt-textarea');
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const form = composer?.closest('form');
  const model = form ? [...form.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"], button[data-testid="model-switcher-dropdown-button"][aria-haspopup="menu"]')].filter(visible).at(-1) : null;
  const send = document.querySelector('[data-testid="send-button"]');
  if (!composer || !model || !send) return 'E_SEND_SURFACE';
  if (send.disabled || document.querySelector('[data-testid="stop-button"]')) return 'E_SEND_DISABLED';
  if (model.textContent.trim() !== expectedModel) return 'E_MODEL_SELECTION';
  const expected = expectedPrompt.replace(/\r\n/g, '\n');
  let actual = (composer.value ?? composer.innerText).replace(/\r\n/g, '\n');
  if (actual !== expected && composer.isContentEditable) {
    // A paragraph boundary is one editor newline. Layout-derived innerText
    // may render it as two. Accept only plain paragraphs and inline text,
    // preserving every space, blank line, and literal Markdown delimiter.
    const inline = node => {
      if (node.nodeType === 3) return node.textContent;
      if (node.nodeType !== 1) return null;
      if (node.tagName === 'BR') return '\n';
      if (node.tagName !== 'SPAN') return null;
      const parts = [...node.childNodes].map(inline);
      return parts.includes(null) ? null : parts.join('');
    };
    const paragraphs = [...composer.childNodes];
    if (paragraphs.length && paragraphs.every(node => node.nodeType === 1 && node.tagName === 'P')) {
      const lines = paragraphs.map(paragraph => {
        const nodes = [...paragraph.childNodes];
        if (nodes.length === 1 && nodes[0].tagName === 'BR') return '';
        const parts = nodes.map(inline);
        return parts.includes(null) ? null : parts.join('');
      });
      if (!lines.includes(null)) actual = lines.join('\n').replace(/\r\n/g, '\n');
    }
  }
  if (actual !== expected) return 'E_COMPOSER_MISMATCH';
  send.click();
  return true;
}
