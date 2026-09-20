import test from 'node:test';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { cases, source, fixtureFiles } from './coding-fixture.mjs';
import runner from './coding-fixture-runner.cjs';
import { readCommand, testCommand, patchedSource, changedSource, approvePatch, completedProgress } from './coding-fixture-approval.mjs';

const cwd = resolve('.local/probes/arithmetic-fixture/workspace');
const initial = source(cases[0].broken);
const diff = '@@ -1,3 +1,3 @@\n module.exports = function solve(a, b) {\n-  return a - b;\n+  return b + a;\n };\n';
const item = { id: 'patch', type: 'fileChange', status: 'inProgress',
  changes: [{ path: resolve(cwd, 'solve.cjs'), kind: { type: 'update', movePath: null }, diff }] };
const params = { itemId: 'patch', threadId: 'thread', turnId: 'turn' };
const event = { method: 'item/started', params: { threadId: 'thread', turnId: 'turn', item } };

test('bounded unified patch binds exact original bytes, counts and safe resulting source', () => {
  assert.equal(patchedSource(diff, initial), source('b + a'));
  assert.equal(patchedSource('@@ -2 +2 @@\n-  return a - b;\n+  return a + b;\n', initial), source('a + b'));
  assert.equal(patchedSource(diff.replaceAll('\n', '\r\n'), initial.replaceAll('\n', '\r\n')), source('b + a'));
  for (const invalid of [
    diff.replace('-1,3', '-2,3'), diff.replace('+1,3', '+2,3'), diff.replace('-1,3', '-1,2'),
    diff.replace('+1,3', '+1,4'), diff.replace('return a - b', 'return a + b'),
    diff.replace('return b + a', 'return process.exit(0)'),
    diff.replace('solve(a, b)', 'other(a, b)'), diff + diff, diff.trimEnd(),
    '--- a/solve.cjs\n+++ b/solve.cjs\n' + diff, diff.replace('@@ -1,3', '@@ -0,3'),
    diff.replace('@@ -1,3', '@@ -999999999999999999999,3'),
  ]) assert.equal(patchedSource(invalid, initial), null, invalid);
});

test('native patch approval admits only the attributed solve file without wider grants', () => {
  assert.equal(approvePatch(params, event, cwd, initial), true);
  for (const override of [{ grantRoot: cwd }, { itemId: 'foreign' }, { threadId: 'foreign' }, { turnId: 'foreign' }]) {
    assert.equal(approvePatch({ ...params, ...override }, event, cwd, initial), false);
  }
  for (const override of [{ path: '../solve.cjs' }, { path: 'tests.cjs' }, { kind: { type: 'add' } },
    { kind: { type: 'update', movePath: 'other.cjs' } }, { diff: diff.replace('b + a', 'require(1)') }]) {
    const altered = structuredClone(item); Object.assign(altered.changes[0], override);
    assert.equal(changedSource(altered, cwd, initial), null);
  }
  assert.equal(changedSource({ ...item, changes: [...item.changes, ...item.changes] }, cwd, initial), null);
  assert.equal(approvePatch(params, { ...event, params: { ...event.params, item: { ...item, status: 'completed' } } }, cwd, initial), false);
});

test('coding evidence requires the actual ordered read, failed test, patch and passed retest', () => {
  const caseJson = fixtureFiles(cases[0])['cases.json'];
  const before = runner.check(initial, cases[0].tests);
  const command = testCommand('C:\\node\\node.exe');
  const run = { thread: 'thread', turn: 'turn', initial, caseJson, before, testCommand: command };
  const items = [
    { type: 'commandExecution', command: readCommand, cwd, status: 'completed', exitCode: 0, aggregatedOutput: initial + caseJson },
    { type: 'commandExecution', command, cwd, status: 'failed', exitCode: 1, aggregatedOutput: JSON.stringify(before) },
    { ...item, status: 'completed' },
    { type: 'commandExecution', command, cwd, status: 'completed', exitCode: 0, aggregatedOutput: JSON.stringify({ total: 4, passed: 4, failed: 0 }) },
  ];
  const events = items.map(item => ({ method: 'item/completed', params: { threadId: 'thread', turnId: 'turn', item } }));
  const progress = events => completedProgress(events, run, cwd, []);
  for (let end = 0; end <= 4; end++) assert.equal(progress(events.slice(0, end)), end);
  assert.equal(progress([events[0], events[2], events[1], events[3]]), -1);
  assert.equal(progress([...events, events[3]]), -1);
  for (const [index, override] of [[0, { exitCode: 1 }], [0, { aggregatedOutput: 'invented' }],
    [1, { status: 'declined' }], [1, { exitCode: 0 }], [2, { status: 'failed' }],
    [3, { exitCode: 1 }], [3, { command: command + '; exit 0' }],
    [3, { aggregatedOutput: JSON.stringify({ total: 4, passed: 3, failed: 1 }) }],
    [3, { additionalPermissions: {} }], [3, { environmentId: 'remote' }]]) {
    const changed = structuredClone(events); Object.assign(changed[index].params.item, override);
    assert.equal(progress(changed), -1);
  }
  const foreign = structuredClone(events); foreign[0].params.turnId = 'foreign';
  assert.equal(progress(foreign), -1);
  for (const node of ["C:\\node\\node.exe'; exit 0; '", 'node', '../node.exe']) assert.throws(() => testCommand(node), /E_FIXTURE_NODE/);
});
