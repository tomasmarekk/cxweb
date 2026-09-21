import test from 'node:test';
import assert from 'node:assert/strict';
import { NamespaceFixture, argument } from './namespace-fixture.mjs';

const values = { cxweb_alpha: 'alpha value', cxweb_beta: 'beta value' };
const call = (namespace = 'cxweb_alpha', callId = namespace) => ({
  threadId: 'thread', turnId: 'turn', namespace, tool: 'read', callId, arguments: { key: argument },
});
const fixture = () => { const f = new NamespaceFixture('thread', values); f.bindTurn('turn'); return f; };

test('colliding leaf names resolve by namespace regardless of call order', () => {
  const f = fixture();
  assert.equal(f.answer(call('cxweb_beta')).contentItems[0].text, 'beta value');
  assert.equal(f.answer(call()).contentItems[0].text, 'alpha value');
  assert.equal(f.expected(), 'cxweb_alpha=alpha value\ncxweb_beta=beta value');
});
test('wrong scope, namespace, arguments and ambiguous aliases never reveal fixture values', () => {
  const mutations = [
    { threadId: 'other' }, { turnId: 'other' }, { namespace: null }, { namespace: 'unknown' },
    { tool: 'cxweb_alpha.read' }, { arguments: { key: argument, extra: true } },
    { arguments: JSON.stringify({ key: argument }) }, { arguments: { key: 'coerced' } }, { callId: '' },
  ];
  for (const change of mutations) {
    const f = fixture();
    assert.throws(() => f.answer({ ...call(), ...change }), /E_DYNAMIC/);
    assert.equal(f.calls.length, 0);
  }
});
test('duplicate calls and changed turn identity cannot execute a second time', () => {
  const f = fixture(); f.answer(call());
  assert.throws(() => f.answer(call('cxweb_alpha', 'new-id')), /E_DYNAMIC_DUPLICATE/);
  assert.throws(() => f.answer(call('cxweb_beta', 'cxweb_alpha')), /E_DYNAMIC_DUPLICATE/);
  assert.throws(() => f.bindTurn('other'), /E_TURN_IDENTITY/);
  assert.equal(f.calls.length, 1);
});
test('batch evidence requires two distinct ordered calls from the same response', () => {
  const response = 'a'.repeat(64);
  const f = fixture();
  f.answer(call('cxweb_beta', `resp_cxweb_${response}_call_1`));
  f.answer(call('cxweb_alpha', `resp_cxweb_${response}_call_0`));
  assert.equal(f.sameResponseBatch(), true);
  for (const id of [`resp_cxweb_${'b'.repeat(64)}_call_1`, `resp_cxweb_${response}_call_3`, 'arbitrary']) {
    const other = fixture();
    other.answer(call('cxweb_alpha', `resp_cxweb_${response}_call_0`));
    other.answer(call('cxweb_beta', id));
    assert.equal(other.sameResponseBatch(), false);
  }
});
