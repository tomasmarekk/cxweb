function () {
  const stop = document.querySelector('[data-testid="stop-button"]');
  if (!stop || stop.disabled) return false;
  stop.click();
  return true;
}
