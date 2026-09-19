import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/login.js', import.meta.url), 'utf8');
function observe(challenge) {
  return vm.runInNewContext(`(${source})`, {
    navigator: { language: 'en-US' },
    document: {
      documentElement: { lang: 'en-US' }, readyState: 'complete', title: '',
      querySelector(selector) {
        if (selector.includes('#challenge-running')) return challenge ? {} : null;
        if (selector === '#prompt-textarea' || selector === '[data-testid="accounts-profile-button"]') return {};
        return null;
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
