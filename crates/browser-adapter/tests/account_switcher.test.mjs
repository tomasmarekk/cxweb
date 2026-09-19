import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/account_switcher.js', import.meta.url), 'utf8');
function inspect({ emails = ['fixture@example.invalid'], selected = true, expanded = true, extraMenus = 0, linked = true, heading = false } = {}) {
  class Node {
    constructor(text = '', attrs = {}) { this.innerText = this.textContent = text; this.attrs = attrs; }
    getClientRects() { return [1]; }
    checkVisibility() { return true; }
    getAttribute(name) { return this.attrs[name] ?? null; }
    matches() { return this.attrs['aria-checked'] === 'true'; }
    getBoundingClientRect() { return { left: 0, top: 0, width: 30, height: 20 }; }
    contains(node) { return node === this; }
  }
  const profile = new Node('', { 'aria-expanded': 'true', 'aria-controls': 'root' });
  const first = new Node('Private profile name', { role: 'menuitem', 'aria-haspopup': 'menu', 'aria-expanded': String(expanded), 'aria-controls': linked ? 'submenu' : '' });
  const root = new Node(); root.querySelectorAll = () => [first];
  const radio = new Node(heading ? 'Private account name' : emails.join('\n'), { role: 'menuitemradio', 'aria-checked': String(selected) });
  const controls = [new Node(heading ? emails.join('\n') : 'Private heading', { role: 'menuitem' }), radio, new Node('Add account', { role: 'menuitem' })];
  const submenu = new Node(controls.map(n => n.innerText).join('\n'));
  submenu.querySelectorAll = selector => selector.startsWith('[data-workspace-id]') ? []
    : selector.startsWith('[aria-checked') ? controls.filter(n => n.matches()) : controls;
  return vm.runInNewContext(`(${source})`, {
    HTMLElement: Node,
    document: {
      querySelectorAll: selector => selector.includes('accounts-profile-button') ? [profile] : [root, submenu, ...Array.from({ length: extraMenus }, () => new Node())],
      getElementById: id => id === 'root' ? root : id === 'submenu' ? submenu : null,
      elementFromPoint: () => first
    }
  })();
}
test('selected account evidence is private and diagnostics contain only structure', () => {
  const result = inspect({ emails: ['Fixture@Example.invalid'] });
  assert.equal(result.account, 'fixture@example.invalid');
  assert.equal(result.diagnostic.selected_account_candidates, 1);
  assert.deepEqual(Array.from(result.diagnostic.controls), ['menuitem:other:unselected', 'menuitemradio:other:selected', 'menuitem:Add account:unselected']);
  assert.doesNotMatch(JSON.stringify(result.diagnostic), /fixture|example|Private/i);
});
test('unselected accounts, multiple identities and collapsed or ambiguous submenus supply no identity', () => {
  assert.equal(inspect({ selected: false }).account, null);
  assert.equal(inspect({ emails: ['a@example.invalid', 'b@example.invalid'] }).account, null);
  assert.equal(inspect({ expanded: false }).account, null);
  assert.equal(inspect({ linked: false, extraMenus: 1 }).account, null);
});
test('the known account-heading layout yields a corroboration candidate without inventing a selected email', () => {
  const result = inspect({ heading: true });
  assert.equal(result.account, 'fixture@example.invalid');
  assert.equal(result.diagnostic.account_source, 'account_heading');
  assert.equal(result.diagnostic.selected_account_candidates, 0);
  assert.equal(inspect({ heading: true, selected: false }).account, null);
  assert.equal(inspect({ heading: true, emails: ['a@example.invalid', 'b@example.invalid'] }).account, null);
  assert.doesNotMatch(JSON.stringify(result.diagnostic), /fixture|example|Private/i);
});
