import test from 'node:test';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { approveFixtureRead, approveFixturePatch, completedFixtureRead, completedFixtureDenial, fixtureReadCommand } from './probe-client-approval.mjs';
import { approveFixtureCommand, fixtureCommandDiagnostic } from './probe-client-approval.mjs';
import { approveFixtureTest, approveFixtureRepair, fixtureRepairProgress, fixtureTestCommand, fixtureTestPassed, fixtureTestFailed, fixtureBrokenOutput } from './probe-client-approval.mjs';

test('command diagnostics distinguish metadata and payload without granting similar commands or retaining content', () => {
  const directory = resolve('.local/diagnostic');
  const expected = "& 'C:\\fixture\\node.exe' './tests.cjs'";
  for (const [command, difference] of [
    [expected, 'exact'], [expected + ' ', 'outer-whitespace'],
    [expected.replaceAll('\\', '/'), 'path-separators'],
    [expected.replaceAll('\\', '\\\\'), 'doubled-backslashes'], ['private-unexpected-command', 'other'],
  ]) {
    const params = { cwd: directory, command };
    const diagnosis = fixtureCommandDiagnostic(params, directory, expected);
    assert.equal(diagnosis.payloadDifference, difference);
    assert.equal(diagnosis.cwdMatches, true);
    assert.equal(approveFixtureCommand(params, directory, [], expected), difference === 'exact');
    assert.equal(JSON.stringify(diagnosis).includes('private-unexpected-command'), false);
  }
  const command = `'C:\\Program Files\\PowerShell\\7\\pwsh.exe' -NoProfile -Command '${expected.replaceAll("'", "'\"'\"'")}'`;
  const diagnosis = fixtureCommandDiagnostic({ cwd: resolve(directory, '..'), command }, directory, expected);
  assert.equal(diagnosis.cwdMatches, false);
  assert.equal(diagnosis.shellWrapped, true);
  assert.equal(diagnosis.payloadDifference, 'exact');
});

const cwd = resolve('.local/probes/approval-fixture/workspace');
function repairEvents() {
  return [
    { type: 'commandExecution', command: fixtureReadCommand, cwd, status: 'completed', exitCode: 0, aggregatedOutput: 'marker\r\n' },
    { type: 'commandExecution', command: fixtureTestCommand, cwd, status: 'failed', exitCode: 1, aggregatedOutput: fixtureTestFailed + '\r\n' },
    { id: 'patch', type: 'fileChange', status: 'completed', changes: [{ path: resolve(cwd, 'probe-output.txt'), kind: { type: 'update', movePath: null }, diff: `@@ -1 +1 @@\n-${fixtureBrokenOutput}\n+marker\n` }] },
    { type: 'commandExecution', command: fixtureTestCommand, cwd, status: 'completed', exitCode: 0, aggregatedOutput: fixtureTestPassed + '\r\n' },
  ].map(item => ({ method: 'item/completed', params: { threadId: 'thread', turnId: 'turn', item } }));
}
test('repair progression requires the exact attributed read, failure, patch and successful retest in order', () => {
  const events = repairEvents();
  const verify = events => fixtureRepairProgress(events, 'thread', 'turn', cwd, 'marker');
  for (let count = 0; count <= 4; count++) assert.equal(verify(events.slice(0, count)), count);
  assert.equal(verify([...events, events[3]]), -1);
  assert.equal(verify([events[0], events[2], events[1], events[3]]), -1);
  assert.equal(verify([events[0], events[0]]), -1);
  assert.equal(verify([...events.slice(0, 1), { ...events[1], params: { ...events[1].params, turnId: 'foreign' } }, ...events.slice(2)]), -1);
  for (const [step, override] of [[0, { aggregatedOutput: 'invented' }], [1, { exitCode: 0 }], [1, { aggregatedOutput: fixtureTestPassed }], [2, { status: 'failed' }], [3, { exitCode: 1 }], [3, { status: 'failed' }], [3, { aggregatedOutput: fixtureTestFailed }], [3, { command: 'other' }], [3, { type: 'mcpToolCall' }]]) {
    const altered = structuredClone(events); Object.assign(altered[step].params.item, override);
    assert.equal(verify(altered), -1);
  }
});
test('repair admits only the exact file update and denies moves, expanded scope or modified test commands', () => {
  const event = repairEvents()[2]; event.method = 'item/started'; event.params.item.status = 'inProgress';
  const params = { itemId: 'patch', threadId: 'thread', turnId: 'turn' };
  assert.equal(approveFixtureRepair(params, event, cwd, 'marker'), true);
  assert.equal(approveFixtureRepair({ ...params, grantRoot: cwd }, event, cwd, 'marker'), false);
  assert.equal(approveFixtureRepair({ ...params, turnId: 'foreign' }, event, cwd, 'marker'), false);
  const moved = structuredClone(event); moved.params.item.changes[0].kind.move_path = resolve(cwd, 'other.txt');
  assert.equal(approveFixtureRepair(params, moved, cwd, 'marker'), false);
  for (const override of [{ kind: { type: 'add' } }, { kind: { type: 'update', movePath: resolve(cwd, 'other.txt') } }, { path: '../probe-output.txt' }, { diff: '+marker\n' }]) {
    const altered = structuredClone(event); Object.assign(altered.params.item.changes[0], override);
    assert.equal(approveFixtureRepair(params, altered, cwd, 'marker'), false);
  }
  assert.equal(approveFixtureTest({ command: fixtureTestCommand, cwd }, cwd), true);
  assert.equal(approveFixtureTest({ command: `'C:\\Program Files\\PowerShell\\7\\pwsh.exe' -NoProfile -Command '${fixtureTestCommand.replaceAll("'", "'\"'\"'")}'`, cwd }, cwd), true);
  for (const command of [fixtureReadCommand, fixtureTestCommand + '; exit 0', fixtureTestCommand.replace('exit 1', 'exit 0')]) assert.equal(approveFixtureTest({ command, cwd }, cwd), false);
});
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

test('fixture commands refuse extra permissions and non-default execution contexts', () => {
  for (const [approve, command] of [[approveFixtureRead, fixtureReadCommand], [approveFixtureTest, fixtureTestCommand]]) {
    for (const override of [
      { additionalPermissions: {} },
      { additionalPermissions: { network: { enabled: true } } },
      { additionalPermissions: { fileSystem: { write: [resolve(cwd, '..')] } } },
      { additionalPermissions: { fileSystem: { read: [resolve(cwd, '..')] } } },
      { environmentId: 'foreign-environment' },
      { environmentId: 'LOCAL' },
      { environmentId: ' local' },
      { environmentId: '' },
      { approvalId: 'subcommand-or-stdin-approval' },
      { approvalId: '' },
      { availableDecisions: ['acceptForSession', 'decline'] },
      { availableDecisions: [{ acceptWithExecpolicyAmendment: { execpolicy_amendment: ['pwsh'] } }] },
      { availableDecisions: [] },
      { availableDecisions: 'accept' },
    ]) assert.equal(approve({ command, cwd, ...override }, cwd), false, JSON.stringify(override));
  }
});

test('ordinary one-command acceptance remains valid with null scope and display metadata', () => {
  for (const [approve, command] of [[approveFixtureRead, fixtureReadCommand], [approveFixtureTest, fixtureTestCommand]]) {
    const params = { command, cwd, kind: 'command', additionalPermissions: null,
      environmentId: null, approvalId: null, availableDecisions: null, networkApprovalContext: null };
    assert.equal(approve(params, cwd), true);
    assert.equal(approve({ ...params, environmentId: 'local' }, cwd), true);
    assert.equal(approve({ ...params, availableDecisions: ['accept', 'decline', 'cancel'],
      reason: 'Run the exact fixture command', commandActions: [], startedAtMs: 123,
      proposedExecpolicyAmendment: ['pwsh'],
      proposedNetworkPolicyAmendments: [{ action: 'allow', host: 'example.invalid' }],
    }, cwd), true); // The caller sends only "accept", never a persistent amendment.
  }
});
