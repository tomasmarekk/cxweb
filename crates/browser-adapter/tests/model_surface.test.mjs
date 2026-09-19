import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/model_surface.js', import.meta.url), 'utf8');
const labelSource = await readFile(new URL('../src/dom/effort_label.js', import.meta.url), 'utf8');
test('an absent slider never fabricates a zero-valued reasoning candidate', () => {
  const observe = vm.runInNewContext(`(${source})`, {
    HTMLElement: class {},
    document: { querySelectorAll: () => [], querySelector: () => null }
  });
  const surface = observe();
  assert.equal(surface.candidates.length, 0);
  assert.equal(surface.temporary_chat, false);
});

function sliderSurface({ composerCount = 1, hasControl = true, announcement = 'Extra High, 4 of 5.' } = {}) {
  class Element {
    constructor(text = '') { this.textContent = text; }
    getClientRects() { return [{}]; }
    getAttribute(name) { return ({ 'aria-valuemin': '0', 'aria-valuemax': '4', 'aria-valuenow': '3' })[name] ?? null; }
    hasAttribute(name) { return this.getAttribute(name) !== null; }
  }
  const routeControl = new Element('Composer route');
  const unrelated = new Element('Unrelated global menu');
  const composer = new Element();
  composer.closest = () => ({ querySelectorAll: () => hasControl ? [routeControl] : [] });
  const slider = new Element();
  const container = new Element();
  const simpleView = Object.assign(new Element(), { innerText: announcement });
  container.querySelector = () => slider;
  container.querySelectorAll = () => [];
  return vm.runInNewContext(`(() => { const readEffortLabel = (${labelSource}); return (${source}); })()`, {
    HTMLElement: Element,
    document: {
      querySelector: () => unrelated,
      querySelectorAll(selector) {
        if (selector === '[data-model-reasoning-effort-slider]') return [container];
        if (selector === '[data-testid="composer-model-picker-slider-simple-view"]') return [simpleView];
        if (selector.includes('#prompt-textarea')) return Array(composerCount).fill(composer);
        return [];
      }
    }
  });
}

test('slider discovery labels come from the same composer control used for selection', () => {
  const surface = sliderSurface()();
  assert.equal(surface.candidates.length, 1);
  assert.equal(surface.candidates[0].label, 'Composer route · Extra High');
  assert.equal(surface.candidates[0].selected, true);
  assert.equal(surface.candidates[0].identity, 'reasoning-slider:0:4:3');
  assert.equal(surface.diagnostic.effort_range.max, 4);
});

test('effort discovery requires a hydrated label matching both position and range', () => {
  for (const announcement of ['', 'High, 3 of 5.', 'Extra High, 4 of 6.', 'Extra High', '4 of 5.']) {
    assert.throws(sliderSurface({ announcement }), /E_MODEL_EFFORT_LABEL/);
  }
  assert.equal(sliderSurface({ announcement: 'Extended, 4 of 5.\nUse arrow keys.' })().candidates[0].label, 'Composer route · Extended');
});

test('slider discovery refuses missing or ambiguous composer controls', () => {
  for (const options of [{ composerCount: 0 }, { composerCount: 2 }, { hasControl: false }]) {
    assert.throws(sliderSurface(options), /E_MODEL_MENU/);
  }
});
