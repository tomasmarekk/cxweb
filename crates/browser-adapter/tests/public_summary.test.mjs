import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

class Element {
  constructor(text, children = [], visible = true) { this.innerText = text; this.children = children; this.visible = visible; }
  getClientRects() { return this.visible ? [{}] : []; }
  contains(other) { return this === other || this.children.some(child => child.contains(other)); }
  querySelectorAll() { return this.children.flatMap(child => [child, ...child.querySelectorAll()]); }
}
const source = await readFile(new URL('../src/dom/public_summary.js', import.meta.url), 'utf8');
const read = vm.runInNewContext(`(${source})`, { HTMLElement: Element });
const summary = (turn, answer) => Array.from(read(turn, answer));

test('public status without item anchors is retained, nested copies are not duplicated', () => {
  const child = new Element('Checking results');
  const parent = new Element('Checking results', [child]);
  assert.deepEqual(summary(new Element('', [parent])), ['Checking results']);
});
test('answer wrappers cannot erase adjacent public status, and answers never enter the summary', () => {
  const answer = new Element('Private final output');
  const status = new Element('Comparing two options');
  const wrapper = new Element('Comparing two options Private final output', [status, answer]);
  assert.deepEqual(summary(new Element('', [wrapper]), answer), ['Comparing two options']);
});
test('hidden content, absent turns and action controls do not fabricate reasoning', () => {
  assert.deepEqual(summary(null), []);
  assert.deepEqual(summary(new Element('', [new Element('hidden', [], false), new Element('Answer now')])), []);
});

test('a rendered Thinking status is retained without inventing a detailed explanation', () => {
  assert.deepEqual(summary(new Element('', [new Element('Thinking')])), ['Thinking']);
});
