function (slider, min, max, value) {
  const normalize = text => text.replace(/\s+/g, ' ').trim();
  const accessible = slider.getAttribute('aria-valuetext');
  let label = accessible ? normalize(accessible) : null;
  if (!label) {
    const views = [...document.querySelectorAll('[data-testid="composer-model-picker-slider-simple-view"]')]
      .filter(node => node instanceof HTMLElement && node.getClientRects().length > 0);
    if (views.length !== 1) throw new Error('E_MODEL_EFFORT_LABEL');
    const firstLine = (views[0].innerText ?? '').split('\n')[0].trim();
    const match = /^([^,\n]+), (\d+) of (\d+)\.$/.exec(firstLine);
    if (!match || Number(match[2]) !== value - min + 1 || Number(match[3]) !== max - min + 1) {
      throw new Error('E_MODEL_EFFORT_LABEL');
    }
    label = normalize(match[1]);
  }
  if (!label || label.length > 80 || /[\u0000-\u001f\u007f]/.test(label)) throw new Error('E_MODEL_EFFORT_LABEL');
  return label;
}
