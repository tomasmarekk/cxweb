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
