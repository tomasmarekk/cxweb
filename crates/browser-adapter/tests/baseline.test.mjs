import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/baseline.js', import.meta.url), 'utf8');
function observe({ composers = 1, model = true, hidden = false } = {}) {
  class Element {
    textContent = '';
    getClientRects() { return hidden ? [] : [{}]; }
    closest() { return { querySelectorAll: () => model ? [new Element()] : [] }; }
  }
  return vm.runInNewContext(`(${source})`, {
    HTMLElement: Element,
    document: {
      querySelector: () => null,
      querySelectorAll: selector => selector === '[data-turn-id-container]' ? [] : Array.from({ length: composers }, () => new Element()),
    },
  })();
}

test('baseline requires one visible composer and a scoped model control', () => {
  assert.equal(observe().composer_empty, true);
  for (const options of [{ composers: 0 }, { composers: 2 }, { hidden: true }]) {
    assert.throws(() => observe(options), /E_BROWSER_BASELINE_COMPOSER/);
  }
  assert.throws(() => observe({ model: false }), /E_BROWSER_BASELINE_MODEL/);
});
