function () {
  const language = value => typeof value === 'string' && /^[a-z]{2,3}(?:-[a-z0-9]{2,8})*$/i.test(value) && value.length <= 32 ? value : null;
  const composer = document.querySelector('#prompt-textarea');
  const profile = document.querySelector('[data-testid="accounts-profile-button"]');
  const login = document.querySelector('[data-testid="login-button"], a[href*="/auth/login"]');
  const model = document.querySelector('[data-testid="model-switcher-dropdown-button"]');
  return {
    browser_language: language(navigator.language),
    page_language: language(document.documentElement.lang),
    official_page: true,
    composer: !!composer,
    account_surface: !!profile,
    login_action: !!login,
    selected_label: model?.textContent.trim().slice(0, 120) ?? null
  };
}
