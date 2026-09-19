import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/account_settings.js', import.meta.url), 'utf8');
const openSource = await readFile(new URL('../src/dom/open_account_tab.js', import.meta.url), 'utf8');
function fixture({ selected = true, email = 'fixture@example.invalid', dialogCount = 1, foreignPanel = false } = {}) {
  class Element {
    getClientRects() { return [1]; }
    getAttribute(name) { return this.attrs?.[name] ?? null; }
  }
  let clicks = 0;
  const account = Object.assign(new Element(), {
    textContent: 'Account', attrs: { 'aria-selected': String(selected), 'aria-controls': 'account-panel' }, click() { clicks++; }
  });
  const general = Object.assign(new Element(), { textContent: 'General' });
  const panel = Object.assign(new Element(), { innerText: email, querySelector: () => null });
  const dialog = Object.assign(new Element(), { querySelectorAll: () => [general, account], contains: node => !foreignPanel && node === panel });
  const read = vm.runInNewContext(`(${source})`, {
    HTMLElement: Element,
    document: { querySelectorAll: () => Array(dialogCount).fill(dialog), getElementById: () => panel }
  });
  return { read, clicks: () => clicks };
}

test('only the selected Account panel can supply one account identity', () => {
  const result = fixture().read();
  assert.equal(result.account, 'fixture@example.invalid');
  assert.equal(result.account_candidates, 1);
  assert.equal(fixture({ selected: false }).read().account, null);
  assert.equal(fixture({ foreignPanel: true }).read().account, null);
  assert.equal(fixture({ email: 'a@example.invalid b@example.invalid' }).read().account, null);
});

test('the account observer never clicks and refuses ambiguous dialogs', () => {
  const page = fixture({ selected: false });
  page.read();
  assert.equal(page.clicks(), 0);
  page.read(true);
  assert.equal(page.clicks(), 0);
  assert.equal(fixture({ dialogCount: 2 }).read(true), null);
});

test('Account activation focuses the actual enabled tab without a stale pointer or synthetic click', () => {
  for (const mode of ['enabled', 'selected', 'disabled', 'inert', 'unfocusable', 'ambiguous']) {
    class Element {
      getClientRects() { return [1]; }
      checkVisibility() { return true; }
      getAttribute(name) { return name === 'aria-disabled' && mode === 'disabled' || name === 'aria-selected' && mode === 'selected' ? 'true' : null; }
      closest() { return mode === 'inert' ? {} : null; }
      scrollIntoView() {}
      focus() { if (mode !== 'unfocusable') document.activeElement = this; }
      click() { assert.fail('The adapter must not synthesize a click'); }
      getBoundingClientRect() { assert.fail('Dialog animation invalidates captured coordinates'); }
    }
    const tab = Object.assign(new Element(), { textContent: 'Account' });
    const dialog = Object.assign(new Element(), { querySelectorAll: () => mode === 'ambiguous' ? [tab, tab] : [tab] });
    const document = { querySelectorAll: () => [dialog], activeElement: null };
    const result = vm.runInNewContext(`(${openSource})`, { HTMLElement: Element, document })();
    if (mode === 'enabled') { assert.equal(result.focused, true); assert.equal(document.activeElement, tab); }
    else if (mode === 'selected') assert.equal(result.selected, true);
    else assert.equal(result, null);
  }
});
