function () {
  return ![...document.querySelectorAll('[role="dialog"]')].some(node =>
    node instanceof HTMLElement && node.getClientRects().length > 0 && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }));
}
