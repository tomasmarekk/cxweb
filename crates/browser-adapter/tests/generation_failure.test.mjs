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
  matches(selector) { return this.selector === selector; }
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
  assert.equal(detect(user, null), true);
});

test('quoted, hidden, and historical messages cannot claim a current failure', () => {
  reset();
  const user = new Element('Thinking failed', main, '[data-turn="user"]');
  new Element('Thinking failed', user);
  new Element('Thinking failed', main, '', false);
  const oldAssistant = new Element('', main, '[data-turn-id-container]');
  new Element('Thinking failed', oldAssistant);
  const assistant = new Element('', main, '[data-turn-id-container]');
  assert.equal(detect(user, null), false);
  assert.equal(detect(user, assistant), false);
  new Element('Thinking failed', assistant);
  assert.equal(detect(user, assistant), true);
});

test('a current response error remains terminal with Copy or a retry control', () => {
  reset();
  const user = new Element('request', main, '[data-turn="user"]');
  const assistant = new Element('', main, '[data-turn-id-container]');
  new Element('Thinking failed', assistant);
  assert.equal(detect(user, assistant), true);
  reset();
  const currentUser = new Element('request', main, '[data-turn="user"]');
  const currentAssistant = new Element('', main, '[data-turn-id-container]');
  new Element('Try again', currentAssistant, '[data-testid="regenerate-thread-error-button"]');
  assert.equal(detect(currentUser, currentAssistant), true);
});

test('hidden and historical retry controls do not claim the new turn', () => {
  reset();
  const user = new Element('request', main, '[data-turn="user"]');
  const historical = new Element('', main, '[data-turn-id-container]');
  new Element('Try again', historical, '[data-testid="regenerate-thread-error-button"]');
  const current = new Element('', main, '[data-turn-id-container]');
  new Element('Try again', current, '[data-testid="regenerate-thread-error-button"]', false);
  assert.equal(detect(user, current), false);
  assert.equal(detect(user, null), false);
});
