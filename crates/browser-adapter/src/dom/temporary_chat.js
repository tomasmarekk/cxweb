function () {
  // Return fixed structural states only, never page text or account data.
  if (document.querySelector('#challenge-running, #challenge-stage, iframe[src^="https://challenges.cloudflare.com/"]') || document.title === 'Just a moment...') return 'verification';
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const login = [...document.querySelectorAll('[data-testid="login-button"], a[href*="/auth/login"]')].some(visible);
  if (login) return 'login';
  const params = [...new URLSearchParams(location.search).entries()];
  if (location.pathname !== '/' || params.length !== 1 || params[0][0] !== 'temporary-chat' || params[0][1] !== 'true') return 'route';
  const composers = [...document.querySelectorAll('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]')].filter(visible);
  if (composers.length === 1) {
    // Responsive layouts can contain multiple entry points. Their presence is
    // readiness only; account_scope independently verifies the selected identity.
    const profile = [...document.querySelectorAll('[data-testid="accounts-profile-button"]')]
      .some(node => visible(node) && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }));
    return profile ? 'ready' : 'account_loading';
  }
  if (composers.length > 1) return 'ambiguous';
  return document.readyState === 'complete' ? 'composer_missing' : 'loading';
}
