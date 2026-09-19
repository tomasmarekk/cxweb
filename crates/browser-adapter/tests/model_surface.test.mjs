import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/model_surface.js', import.meta.url), 'utf8');
test('an absent slider never fabricates a zero-valued reasoning candidate', () => {
  const observe = vm.runInNewContext(`(${source})`, {
    HTMLElement: class {},
    document: { querySelectorAll: () => [], querySelector: () => null }
  });
  const surface = observe();
  assert.equal(surface.candidates.length, 0);
  assert.equal(surface.temporary_chat, false);
});

function sliderSurface({ composerCount = 1, hasControl = true } = {}) {
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
  container.querySelector = () => slider;
  return vm.runInNewContext(`(${source})`, {
    HTMLElement: Element,
    document: {
      querySelector: () => unrelated,
      querySelectorAll(selector) {
        if (selector === '[data-model-reasoning-effort-slider]') return [container];
        if (selector.includes('#prompt-textarea')) return Array(composerCount).fill(composer);
        return [];
      }
    }
  });
}

test('slider discovery labels come from the same composer control used for selection', () => {
  const surface = sliderSurface()();
  assert.equal(surface.candidates.length, 5);
  assert.equal(surface.candidates[3].label, 'Composer route · effort 4');
  assert.equal(surface.candidates[3].selected, true);
  assert.equal(surface.candidates[3].identity, 'reasoning-slider:0:4:3');
});

test('slider discovery refuses missing or ambiguous composer controls', () => {
  for (const options of [{ composerCount: 0 }, { composerCount: 2 }, { hasControl: false }]) {
    assert.throws(sliderSurface(options), /E_MODEL_MENU/);
  }
});
