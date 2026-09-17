function () {
  const composer = document.querySelector('#prompt-textarea');
  const profile = document.querySelector('[data-testid="accounts-profile-button"]');
  const login = document.querySelector('[data-testid="login-button"], a[href*="/auth/login"]');
  const model = document.querySelector('[data-testid="model-switcher-dropdown-button"]');
  return {
    official_page: true,
    composer: !!composer,
    account_surface: !!profile,
    login_action: !!login,
    selected_label: model?.textContent.trim().slice(0, 120) ?? null
  };
}
