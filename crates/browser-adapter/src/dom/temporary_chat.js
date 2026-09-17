function () {
  const params = [...new URLSearchParams(location.search).entries()];
  if (location.pathname !== '/' || params.length !== 1 || params[0][0] !== 'temporary-chat' || params[0][1] !== 'true') return false;
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const composers = [...document.querySelectorAll('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]')].filter(visible);
  const login = [...document.querySelectorAll('[data-testid="login-button"], a[href*="/auth/login"]')].some(visible);
  return composers.length === 1 && !login;
}
