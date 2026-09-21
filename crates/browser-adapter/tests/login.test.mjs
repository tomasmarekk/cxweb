import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/login.js', import.meta.url), 'utf8');
function observe(challenge, { login = [], actions = [], hiddenSurface = false } = {}) {
  class Element {
    constructor(shown) { this.shown = shown; }
    getClientRects() { return this.shown ? [{}] : []; }
    closest() { return this.message ? {} : null; }
  }
  return vm.runInNewContext(`(${source})`, {
    HTMLElement: Element,
    navigator: { language: 'en-US' },
    document: {
      documentElement: { lang: 'en-US' }, readyState: 'complete', title: '',
      querySelector(selector) {
        if (selector.includes('#challenge-running')) return challenge ? {} : null;
        return null;
      },
      querySelectorAll(selector) {
        if (selector === 'button, a, [role="button"]') return actions.map(action => Object.assign(new Element(action.shown ?? true), { textContent: action.label, message: action.message }));
        if (selector.includes('login-button')) return login.map(shown => new Element(shown));
        if (selector === '#prompt-textarea' || selector === '[data-testid="accounts-profile-button"]') return [new Element(!hiddenSurface)];
        return [];
      }
    }
  })();
}

test('challenge detection is independent of a retained composer and reads no auth fields', () => {
  const ready = observe(false), challenged = observe(true);
  assert.equal(ready.verification_required, false);
  assert.equal(challenged.verification_required, true);
  assert.equal(challenged.composer, true);
  assert.equal(challenged.account_surface, true);
  assert.equal(challenged.browser_language, 'en-US');
});

test('hidden login links do not invalidate a visible signed-in surface', () => {
  assert.equal(observe(false, { login: [false] }).login_action, false);
  assert.equal(observe(false, { login: [false, true] }).login_action, true);
  const hidden = observe(false, { hiddenSurface: true });
  assert.equal(hidden.composer, false);
  assert.equal(hidden.account_surface, false);
});

test('current English sign-in buttons without legacy test IDs are detected without reading credentials', () => {
  for (const label of ['Log in', 'Sign in', ' Log in ']) {
    assert.equal(observe(false, { actions: [{ label }], hiddenSurface: true }).login_action, true);
  }
  for (const action of [{ label: 'Log in', shown: false }, { label: 'Log in', message: true }, { label: 'How to Log in' }]) {
    assert.equal(observe(false, { actions: [action] }).login_action, false);
  }
});
