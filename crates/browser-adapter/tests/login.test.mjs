import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/login.js', import.meta.url), 'utf8');
function observe(challenge, { login = [], hiddenSurface = false } = {}) {
  class Element {
    constructor(shown) { this.shown = shown; }
    getClientRects() { return this.shown ? [{}] : []; }
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
