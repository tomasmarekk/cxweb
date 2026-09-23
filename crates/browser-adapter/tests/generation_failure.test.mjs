import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/generation_failure.js', import.meta.url), 'utf8');
class Element {
  constructor(text = '', parent = null, selector = '', shown = true) {
    this.textContent = text;
    this.parent = parent;
    this.selector = selector;
    this.shown = shown;
    this.children = [];
    parent?.children.push(this);
  }
  getClientRects() { return this.shown ? [{}] : []; }
  contains(node) { return node === this || this.children.some(child => child.contains(node)); }
  closest(selector) {
    return this.selector && selector.includes(this.selector) ? this : this.parent?.closest(selector) ?? null;
  }
  querySelectorAll() { return this.children.flatMap(child => [child, ...child.querySelectorAll()]); }
}
const main = new Element();
const detect = vm.runInNewContext(`(${source})`, {
  HTMLElement: Element,
  document: { querySelector: selector => selector === 'main' ? main : null },
});
function reset() { main.children.length = 0; }

test('visible current thinking failure is terminal even without an assistant card', () => {
  reset();
  const user = new Element('request', main, '[data-turn="user"]');
  new Element('Thinking failed', main);
  assert.equal(detect(user, null, false, false), true);
  assert.equal(detect(user, null, true, false), false);
  assert.equal(detect(user, null, false, true), false);
});

test('quoted, hidden, and historical messages cannot claim a current failure', () => {
  reset();
  const user = new Element('Thinking failed', main, '[data-turn="user"]');
  new Element('Thinking failed', user);
  new Element('Thinking failed', main, '', false);
  const oldAssistant = new Element('', main, '[data-turn-id-container]');
  new Element('Thinking failed', oldAssistant);
  const assistant = new Element('', main, '[data-turn-id-container]');
  assert.equal(detect(user, assistant, false, false), false);
  new Element('Thinking failed', assistant);
  assert.equal(detect(user, assistant, false, false), true);
});
