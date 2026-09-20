import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/temporary_chat.js', import.meta.url), 'utf8');
function observe({ pathname = '/', search = '?temporary-chat=true', challenge = false, login = false, composers = 1, loading = false, hidden = false } = {}) {
  class Element { getClientRects() { return hidden ? [] : [{}]; } }
  return vm.runInNewContext(`(${source})`, {
    URLSearchParams, HTMLElement: Element, location: { pathname, search },
    document: {
      title: '', readyState: loading ? 'loading' : 'complete',
      querySelector: () => challenge ? new Element() : null,
      querySelectorAll: selector => selector.includes('login-button')
        ? (login ? [new Element()] : []) : Array.from({ length: composers }, () => new Element()),
    },
  })();
}

test('temporary chat readiness still requires the exact route and one visible composer', () => {
  assert.equal(observe(), 'ready');
  assert.equal(observe({ hidden: true }), 'composer_missing');
  assert.equal(observe({ composers: 2 }), 'ambiguous');
  for (const changed of [{ pathname: '/c/private' }, { search: '' }, { search: '?temporary-chat=false' }, { search: '?temporary-chat=true&private=value' }, { search: '?temporary-chat=true&temporary-chat=true' }]) {
    assert.equal(observe(changed), 'route');
  }
});

test('preparation diagnostics distinguish loading, authentication and verification', () => {
  assert.equal(observe({ composers: 0 }), 'composer_missing');
  assert.equal(observe({ composers: 0, loading: true }), 'loading');
  assert.equal(observe({ login: true }), 'login');
  assert.equal(observe({ challenge: true, login: true }), 'verification');
});
