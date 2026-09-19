function () {
  const composers = [...document.querySelectorAll('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]')];
  if (document.querySelector('[data-testid="stop-button"]')) return 'busy';
  if (composers.length > 1) return 'unknown';
  if (composers.length === 1) {
    return (composers[0].value ?? composers[0].textContent).trim() === '' ? 'idle' : 'busy';
  }
  // An expired session or verification page has no composer. Recognize those
  // surfaces without reading credentials, account data or challenge contents.
  if (document.querySelector('[data-turn-id-container], [data-message-author-role], input[type="password"], [contenteditable="true"], textarea')) return 'unknown';
  return document.querySelector('[data-testid="login-button"], a[href*="/auth/login"], #challenge-running, #challenge-stage, iframe[src^="https://challenges.cloudflare.com/"]') ? 'idle' : 'unknown';
}
