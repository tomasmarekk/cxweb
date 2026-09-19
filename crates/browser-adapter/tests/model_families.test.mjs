import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/model_families.js', import.meta.url), 'utf8');
const effortSource = await readFile(new URL('../src/dom/effort_state.js', import.meta.url), 'utf8');
function observe(items, views = 1) {
  const nodes = items.map(({ label, selected = false, disabled = false }) => ({
    innerText: label,
    getAttribute: key => ({ 'aria-checked': String(selected), 'aria-disabled': String(disabled) })[key] ?? null
  }));
  return vm.runInNewContext(`(${source})`, { document: {
    querySelectorAll: selector => {
      assert.equal(selector, '[data-testid="composer-model-picker-slider-advanced-view"]');
      return Array.from({ length: views }, () => ({ querySelectorAll: selector => {
        assert.equal(selector, '[role="menuitemradio"]'); return nodes;
      } }));
    }
  } })();
}

test('family identity comes from the scoped checked model, excluding secondary descriptions', () => {
  const families = observe([{ label: 'Latest', selected: true }, { label: 'Fixture model\nAvailability note' }]);
  assert.equal(families[0].selected, true);
  assert.equal(families[1].label, 'Fixture model');
  assert.equal(families[1].identity, 'Fixture model');
});

test('missing, duplicate, disabled or ambiguous family evidence is rejected', () => {
  for (const items of [[], [{ label: 'A' }], [{ label: 'A', selected: true, disabled: true }],
    [{ label: 'A', selected: true }, { label: 'B', selected: true }],
    [{ label: 'A', selected: true }, { label: 'A' }], [{ label: '', selected: true }]]) {
    assert.throws(() => observe(items), /E_MODEL_FAMILY/);
  }
  for (const views of [0, 2]) assert.throws(() => observe([{ label: 'A', selected: true }], views), /E_MODEL_FAMILY/);
});

test('effort selection rejects the wrong model family before touching the slider', () => {
  const select = vm.runInNewContext(`(${effortSource})`, {
    readModelFamilies: () => [{ identity: 'Other family', selected: true }],
    document: { querySelectorAll: () => assert.fail('wrong family must not access the slider') }
  });
  assert.throws(() => select('["reasoning-slider-v2","Expected family",0,4,3]'), /E_MODEL_FAMILY/);
  assert.throws(() => select('reasoning-slider:0:4:3'));
});
