import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/account_settings.js', import.meta.url), 'utf8');
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
