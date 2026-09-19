import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/account_scope.js', import.meta.url), 'utf8');
const openSource = await readFile(new URL('../src/dom/open_account.js', import.meta.url), 'utf8');

function openProfiles({ expanded = 0, obscured = [true, false] } = {}) {
  class Control {
    constructor(index) { this.index = index; }
    getClientRects() { return [1]; }
    checkVisibility() { return true; }
    getAttribute() { return this.index < expanded ? 'true' : 'false'; }
    getBoundingClientRect() { return { left: this.index * 100, top: 0, width: 20, height: 20 }; }
    contains(node) { return node === this; }
    scrollIntoView() {}
  }
  const controls = obscured.map((_, index) => new Control(index));
  return vm.runInNewContext(`(${openSource})`, {
    HTMLElement: Control,
    document: {
      querySelectorAll: () => controls,
      elementFromPoint: x => { const index = Math.floor(x / 100); return obscured[index] ? null : controls[index]; }
    }
  })();
}

test('profile opening chooses an actionable entry point and refuses obscured or ambiguous menus', () => {
  assert.equal(openProfiles().x, 110);
  assert.equal(openProfiles({ obscured: [true, true] }).failure, 'E_ACCOUNT_OPEN');
  assert.equal(openProfiles({ expanded: 1 }), true);
  assert.equal(openProfiles({ expanded: 2 }).failure, 'E_ACCOUNT_AMBIGUOUS');
});
function scope(text, workspaces = [], expandedProfiles = 1) {
  class Visible { getClientRects() { return [1]; } }
  const workspaceNodes = workspaces.map(({ id, selected }) => Object.assign(new Visible(), {
    matches: () => selected, getAttribute: () => id
  }));
  const menu = Object.assign(new Visible(), {
    innerText: text,
    querySelector: selector => selector === '[data-workspace-id]' ? workspaceNodes[0] : null,
    querySelectorAll: selector => selector.startsWith('[data-workspace-id]') ? workspaceNodes : []
  });
  const profiles = Array.from({ length: expandedProfiles }, () => Object.assign(new Visible(), {
    getAttribute: name => name === 'aria-expanded' ? 'true' : 'account-menu'
  }));
  const inspect = vm.runInNewContext(`(${source})`, {
    HTMLElement: Visible,
    document: {
      querySelector: () => ({ getAttribute: () => 'account-menu' }),
      getElementById: () => menu,
      querySelectorAll: selector => selector.includes('accounts-profile-button') ? profiles : [menu]
    }
  });
  return inspect();
}

test('account and workspace require distinct positive evidence', () => {
  const result = scope('fixture@example.invalid', [{ id: 'workspace-a', selected: true }]);
  assert.equal(result.account, 'fixture@example.invalid');
  assert.equal(result.workspace, 'workspace-a');
  assert.equal(JSON.stringify(result.diagnostic).includes('fixture@example'), false);
  assert.equal(JSON.stringify(result.diagnostic).includes('workspace-a'), false);
});

test('the expanded account entry point owns the menu and multiple expanded entries are refused', () => {
  assert.equal(scope('unrelated-menu@example.invalid', [], 0), null);
  assert.equal(scope('fixture@example.invalid', [], 1).account, 'fixture@example.invalid');
  assert.equal(scope('fixture@example.invalid', [], 2), null);
});

test('missing workspace is never silently labeled personal', () => {
  const result = scope('fixture@example.invalid');
  assert.equal(result.account, 'fixture@example.invalid');
  assert.equal(result.workspace, null);
});

test('multiple account or workspace candidates stay unqualified', () => {
  const result = scope('a@example.invalid b@example.invalid', [
    { id: 'workspace-a', selected: true }, { id: 'workspace-b', selected: true }
  ]);
  assert.equal(result.account, null);
  assert.equal(result.workspace, null);
});

test('unselected workspace and display name are not identity evidence', () => {
  const result = scope('Fixture user', [{ id: 'workspace-a', selected: false }]);
  assert.equal(result.account, null);
  assert.equal(result.workspace, null);
});
