import test from 'node:test';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { approveFixtureRead, approveFixturePatch, completedFixtureRead, completedFixtureDenial, fixtureReadCommand } from './probe-client-approval.mjs';

const cwd = resolve('.local/probes/approval-fixture/workspace');
test('denial evidence requires exactly one attributed declined command without execution output', () => {
  const event = { method: 'item/completed', params: { threadId: 'thread', turnId: 'turn', item: { type: 'commandExecution', command: fixtureReadCommand, cwd, status: 'declined', exitCode: null, aggregatedOutput: null, processId: null } } };
  const verify = events => completedFixtureDenial(events, 'thread', 'turn', cwd, 'private-marker');
  assert.equal(verify([event]), true);
  assert.equal(verify([]), false);
  assert.equal(verify([event, event]), false);
  for (const patch of [{ threadId: 'foreign' }, { turnId: 'foreign' }]) {
    assert.equal(verify([{ ...event, params: { ...event.params, ...patch } }]), false);
  }
  for (const patch of [{ exitCode: 0 }, { exitCode: undefined }, { status: 'completed' }, { status: 'failed' }, { command: 'other' }, { aggregatedOutput: 'private-marker' }, { aggregatedOutput: '' }, { aggregatedOutput: undefined }, { processId: '1' }, { cwd: resolve(cwd, '..') }]) {
    assert.equal(verify([{ ...event, params: { ...event.params, item: { ...event.params.item, ...patch } } }]), false);
  }
});
test('patch admission requires one successful exact read in the same native turn', () => {
  const event = { method: 'item/completed', params: { threadId: 'thread', turnId: 'turn', item: { type: 'commandExecution', command: fixtureReadCommand, cwd, status: 'completed', exitCode: 0, aggregatedOutput: 'marker\r\n' } } };
  const verify = events => completedFixtureRead(events, 'thread', 'turn', cwd, 'marker');
  assert.equal(verify([event]), true);
  assert.equal(verify([]), false);
  assert.equal(verify([event, event]), false);
  for (const patch of [{ threadId: 'foreign' }, { turnId: 'foreign' }]) {
    assert.equal(verify([{ ...event, params: { ...event.params, ...patch } }]), false);
  }
  for (const patch of [{ exitCode: 1 }, { status: 'failed' }, { command: 'other' }, { aggregatedOutput: 'prefix marker' }, { cwd: resolve(cwd, '..') }]) {
    assert.equal(verify([{ ...event, params: { ...event.params, item: { ...event.params.item, ...patch } } }]), false);
  }
});
test('only the exact fixture read in its assigned directory is approved', () => {
  for (const command of [fixtureReadCommand,
    `'C:\\Program Files\\PowerShell\\7\\pwsh.exe' -NoProfile -Command "${fixtureReadCommand}"`,
    `"C:\\\\Program Files\\\\PowerShell\\\\7\\\\pwsh.exe" -NoProfile -Command "${fixtureReadCommand}"`,
    `"C:/PROGRAM FILES/PowerShell/7/pwsh.exe" -Command "${fixtureReadCommand}"`,
    `'C:\\Program Files\\PowerShell\\7\\pwsh.exe' -Command '${fixtureReadCommand.replaceAll("'", "'\"'\"'")}'`,
  ]) assert.equal(approveFixtureRead({ command, cwd }, cwd), true);
  const bundled = 'C:\\fixture\\runtime\\pwsh.exe';
  const command = `'${bundled}' -Command "${fixtureReadCommand}"`;
  assert.equal(approveFixtureRead({ command, cwd }, cwd), false);
  assert.equal(approveFixtureRead({ command, cwd }, cwd, [bundled]), true);
});

test('a patch approval binds the event identity, exact new file and complete content', () => {
  const params = { itemId: 'item', threadId: 'thread', turnId: 'turn', grantRoot: null };
  const change = { path: resolve(cwd, 'probe-output.txt'), kind: { type: 'add' }, diff: 'marker\n' };
  const event = { params: { threadId: 'thread', turnId: 'turn', item: { id: 'item', type: 'fileChange', status: 'inProgress', changes: [change] } } };
  assert.equal(approveFixturePatch(params, event, cwd, 'marker'), true);
  assert.equal(approveFixturePatch({ ...params, turnId: 'other' }, event, cwd, 'marker'), false);
  assert.equal(approveFixturePatch({ ...params, grantRoot: cwd }, event, cwd, 'marker'), false);
  for (const changes of [[{ ...change, kind: { type: 'delete' } }], [{ ...change, path: '../probe-output.txt' }], [{ ...change, diff: 'other\n' }], [change, change]]) {
    const altered = structuredClone(event); altered.params.item.changes = changes;
    assert.equal(approveFixturePatch(params, altered, cwd, 'marker'), false);
  }
});

test('shell additions, changed targets, foreign directories and network approvals are refused', () => {
  for (const command of [
    `${fixtureReadCommand}; Remove-Item probe-input.txt`,
    `${fixtureReadCommand} | Invoke-Expression`,
    fixtureReadCommand.replace('probe-input', '../private'),
    `"C:\\untrusted\\pwsh.exe" -Command "${fixtureReadCommand}"`,
    `"C:\\Program Files\\PowerShell\\7\\pwsh.exe" -Command "${fixtureReadCommand}" -File other.ps1`,
    `'C:\\Program Files\\PowerShell\\7\\pwsh.exe' -Command "${fixtureReadCommand}`, // unclosed
    `'C:\\Program Files\\PowerShell\\7\\pwsh.exe' -Command "${fixtureReadCommand}; Write-Output injected"`,
    `'C:\\Program Files\\PowerShell\\7\\pwsh.exe' -Command "${fixtureReadCommand}" unexpected`,
  ]) assert.equal(approveFixtureRead({ command, cwd }, cwd), false);
  for (const override of [{ cwd: resolve(cwd, '..') }, { kind: 'stdin' }, { networkApprovalContext: {} }, { command: null }]) {
    assert.equal(approveFixtureRead({ command: fixtureReadCommand, cwd, ...override }, cwd), false);
  }
});
