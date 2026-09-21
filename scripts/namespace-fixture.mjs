// Native app-server dynamic tools with colliding leaf names. These fixture
// functions return test data only; no model-provided command is executed.
import assert from 'node:assert/strict';

export const namespaces = ['cxweb_alpha', 'cxweb_beta'];
export const argument = 'sample "quoted"\nline two café 🦀';
export const definitions = namespaces.map(name => ({
  type: 'namespace', name, description: `Independent ${name} test data.`,
  tools: [{ type: 'function', name: 'read', description: 'Return the requested fixture value.',
    inputSchema: { type: 'object', properties: { key: { type: 'string', const: argument } }, required: ['key'], additionalProperties: false } }],
}));

export class NamespaceFixture {
  constructor(thread, values) {
    this.thread = thread;
    this.turn = null;
    this.values = values;
    this.calls = [];
  }
  bindTurn(turn) {
    assert.ok(typeof turn === 'string' && turn.length > 0, 'E_TURN_IDENTITY');
    assert.ok(this.turn === null || this.turn === turn, 'E_TURN_IDENTITY');
    this.turn = turn;
  }
  answer(params) {
    assert.ok(this.turn && params?.threadId === this.thread && params.turnId === this.turn, 'E_DYNAMIC_SCOPE');
    assert.ok(namespaces.includes(params.namespace) && params.tool === 'read', 'E_DYNAMIC_TOOL');
    assert.deepEqual(params.arguments, { key: argument }, 'E_DYNAMIC_ARGUMENTS');
    assert.ok(typeof params.callId === 'string' && params.callId.length > 0, 'E_DYNAMIC_CALL_ID');
    assert.ok(!this.calls.some(call => call.callId === params.callId || call.namespace === params.namespace), 'E_DYNAMIC_DUPLICATE');
    const output = this.values[params.namespace];
    assert.equal(typeof output, 'string', 'E_DYNAMIC_FIXTURE');
    this.calls.push({ namespace: params.namespace, callId: params.callId });
    return { success: true, contentItems: [{ type: 'inputText', text: output }] };
  }
  expected() {
    return namespaces.map(name => `${name}=${this.values[name]}`).join('\n');
  }
  sameResponseBatch() {
    // Production wire encoding binds each call ID to its response and ordinal.
    // Native dispatch can be serial even when both calls came from one response.
    const ids = this.calls.map(call => /^resp_cxweb_([a-f0-9]{64})_call_([0-9]+)$/.exec(call.callId));
    return ids.length === 2 && ids.every(Boolean) && ids[0][1] === ids[1][1]
      && ids.map(id => id[2]).sort().join(',') === '0,1';
  }
}
