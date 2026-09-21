function () {
  const language = value => typeof value === 'string' && /^[a-z]{2,3}(?:-[a-z0-9]{2,8})*$/i.test(value) && value.length <= 32 ? value : null;
  const visible = selector => [...document.querySelectorAll(selector)].some(element => element instanceof HTMLElement && element.getClientRects().length > 0);
  const composer = visible('#prompt-textarea');
  const profile = visible('[data-testid="accounts-profile-button"]');
  const login = visible('[data-testid="login-button"], a[href*="/auth/login"]') ||
    [...document.querySelectorAll('button, a, [role="button"]')].some(element =>
      element instanceof HTMLElement && element.getClientRects().length > 0 &&
      !element.closest('[data-message-author-role], article') && /^(Log in|Sign in)$/i.test(element.textContent.trim()));
  const model = document.querySelector('[data-testid="model-switcher-dropdown-button"]');
  return {
    browser_language: language(navigator.language),
    page_language: language(document.documentElement.lang),
    verification_required: !!document.querySelector('#challenge-running, #challenge-stage, iframe[src^="https://challenges.cloudflare.com/"]') || document.title === 'Just a moment...',
    document_ready: document.readyState === 'complete',
    official_page: true,
    composer: !!composer,
    account_surface: !!profile,
    login_action: !!login,
    selected_label: model?.textContent.trim().slice(0, 120) ?? null
  };
}
