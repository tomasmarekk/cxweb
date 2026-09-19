import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/focus_models.js', import.meta.url), 'utf8');
function fixture({ count = 1, disabled = false, inert = false, expanded = false, refusesFocus = false } = {}) {
  let focused = 0;
  const document = { activeElement: null };
  class Element {
    getClientRects() { return [{}]; }
    checkVisibility() { return true; }
  }
  const button = Object.assign(new Element(), {
    disabled,
    getAttribute: name => name === 'aria-expanded' ? String(expanded) : null,
    closest: () => inert ? {} : null,
    scrollIntoView() {},
    focus() { focused++; if (!refusesFocus) document.activeElement = this; },
    click() { assert.fail('the DOM helper must not click'); }
  });
  const composer = Object.assign(new Element(), { closest: () => ({ querySelectorAll: () => [button] }) });
  document.querySelectorAll = () => Array(count).fill(composer);
  const focus = vm.runInNewContext(`(${source})`, { document, HTMLElement: Element });
  return { focus, get focused() { return focused; }, document, button };
}

test('keyboard preparation focuses the model control and leaves an expanded menu alone', () => {
  const closed = fixture();
  assert.equal(closed.focus().focused, true);
  assert.equal(closed.document.activeElement, closed.button);
  const open = fixture({ expanded: true });
  assert.equal(open.focus().expanded, true);
  assert.equal(open.focused, 0);
});

test('missing, ambiguous, inert, disabled and unfocusable controls cannot be activated', () => {
  for (const options of [{ count: 0 }, { count: 2 }, { disabled: true }, { inert: true }, { refusesFocus: true }]) {
    assert.throws(() => fixture(options).focus(), /E_MODEL_MENU|E_MODEL_FOCUS/);
  }
});
