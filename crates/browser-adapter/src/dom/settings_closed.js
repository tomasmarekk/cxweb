function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0
    && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
  // Other ChatGPT dialogs can remain mounted/visible after Settings closes.
  // Check the UI we opened, without dismissing or claiming ownership of others.
  return ![...document.querySelectorAll('[role="dialog"]')].filter(visible).some(dialog => {
    const labels = [...dialog.querySelectorAll('[role="tab"]')].filter(visible)
      .map(node => node.textContent.replace(/\s+/g, ' ').trim());
    return labels.includes('General') && labels.includes('Account');
  });
}
