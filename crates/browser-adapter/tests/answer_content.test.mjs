import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/answer_content.js', import.meta.url), 'utf8');
const select = vm.runInNewContext(`(${source})`);
const block = ({ intermediate = false, nested = false } = {}) => ({
  closest: selector => {
    assert.equal(selector, '[data-streaming-response-status], [data-testid^="cot-v5"]');
    return intermediate ? {} : null;
  },
  parentElement: { closest: selector => {
    assert.equal(selector, '.markdown');
    return nested ? {} : null;
  } }
});
const turn = (...blocks) => ({ querySelectorAll: selector => {
  assert.equal(selector, '[data-message-author-role="assistant"] .markdown');
  return blocks;
} });

test('intermediate blocks cannot replace the unique assistant answer', () => {
  const final = block();
  const selected = select(turn(block({ intermediate: true }), final, block({ intermediate: true })));
  assert.equal(selected.content, final);
  assert.equal(selected.candidates, 1);
  assert.equal(selected.intermediate, 2);
});

test('missing and ambiguous final blocks are never selected by position', () => {
  for (const input of [null, turn(), turn(block({ intermediate: true })), turn(block(), block())]) {
    assert.equal(select(input).content, null);
  }
});

test('a nested renderer is not counted as a second answer', () => {
  const final = block();
  const selected = select(turn(final, block({ nested: true })));
  assert.equal(selected.content, final);
  assert.equal(selected.candidates, 1);
});
