// Test-client approval only. The product never makes native approval decisions.
import { resolve } from 'node:path';

export const fixtureReadCommand = "Get-Content -LiteralPath './probe-input.txt'";
export const fixtureTestPassed = 'cxweb fixture tests passed';
export const fixtureTestFailed = 'cxweb fixture tests failed';
export const fixtureBrokenOutput = 'incorrect fixture output';
export const fixtureTestCommand = "if ((Get-Content -LiteralPath './probe-output.txt' -Raw -ErrorAction Stop) -ceq (Get-Content -LiteralPath './probe-input.txt' -Raw -ErrorAction Stop)) { Write-Output 'cxweb fixture tests passed'; exit 0 } else { Write-Output 'cxweb fixture tests failed'; exit 1 }";

// Codex renders the original argv with shlex::try_join. This bounded inverse
// reads quoted words only; it does not interpret or execute shell syntax.
function words(value) {
  if (value.length > 8192 || value.includes('\0')) return null;
  const result = [];
  let quote = '', word = '', started = false;
  for (let index = 0; index < value.length; index++) {
    const character = value[index];
    if (quote === "'") {
      if (character === "'") quote = ''; else word += character;
    } else if (character === '\\') {
      const next = value[++index];
      if (next === undefined) return null;
      if (quote === '"' && !['"', '\\', '$', '`', '\n'].includes(next)) word += '\\';
      if (next !== '\n') word += next;
      started = true;
    } else if (quote === '"') {
      if (character === '"') quote = ''; else word += character;
    } else if (character === "'" || character === '"') {
      quote = character; started = true;
    } else if (/\s/.test(character)) {
      if (started) { result.push(word); word = ''; started = false; }
    } else {
      word += character; started = true;
    }
  }
  if (quote) return null;
  if (started) result.push(word);
  return result;
}

export function approveFixtureRead(params, cwd, hostShellExecutables = []) {
  return approveCommand(params, cwd, hostShellExecutables, fixtureReadCommand);
}
export function approveFixtureTest(params, cwd, hostShellExecutables = []) {
  return approveCommand(params, cwd, hostShellExecutables, fixtureTestCommand);
}
function approveCommand(params, cwd, hostShellExecutables, expected) {
  if (typeof params?.command !== 'string' || typeof params.cwd !== 'string'
      || resolve(params.cwd).toLowerCase() !== resolve(cwd).toLowerCase()
      || (params.kind != null && params.kind !== 'command')
      || params.networkApprovalContext != null
      || params.additionalPermissions != null
      || params.environmentId != null
      || params.approvalId != null
      || (params.availableDecisions != null && (!Array.isArray(params.availableDecisions)
        || !params.availableDecisions.includes('accept')))) return false;
  // These fixtures admit only ordinary commands in the default environment,
  // without a permission overlay or subcommand/stdin approval. The caller uses
  // one-command "accept"; policy amendment proposals are never applied.
  if (params.command === expected) return true;
  const argv = words(params.command);
  if (!argv || argv.length < 3 || argv.at(-1) !== expected || argv.at(-2) !== '-Command') return false;
  const shells = [
    ...hostShellExecutables,
    'C:\\Program Files\\PowerShell\\7\\pwsh.exe',
    'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe',
  ].map(path => resolve(path).toLowerCase());
  if (!shells.includes(resolve(argv[0]).toLowerCase())) return false;
  return ['', '-NoProfile', '-NoLogo -NoProfile', '-NoProfile -NonInteractive'].includes(argv.slice(1, -2).join(' '));
}

function exactRepair(item, cwd, marker) {
  const changes = item?.changes;
  if (item?.type !== 'fileChange' || !Array.isArray(changes) || changes.length !== 1) return false;
  const change = changes[0];
  return change.kind?.type === 'update' && change.kind.movePath == null
    && typeof change.path === 'string'
    && resolve(cwd, change.path).toLowerCase() === resolve(cwd, 'probe-output.txt').toLowerCase()
    && change.diff === `@@ -1 +1 @@\n-${fixtureBrokenOutput}\n+${marker}\n`;
}
export function approveFixtureRepair(params, itemEvent, cwd, marker) {
  const item = itemEvent?.params?.item;
  return Boolean(params && params.grantRoot == null && item?.status === 'inProgress'
    && params.itemId === item.id && params.threadId === itemEvent.params.threadId
    && params.turnId === itemEvent.params.turnId && exactRepair(item, cwd, marker));
}

// Return the exact completed prefix length, or -1 for an invalid sequence.
// Both admission and final verification use actual native results, never prose.
export function fixtureRepairProgress(events, threadId, turnId, cwd, marker, shells = []) {
  const items = events.filter(event => event.method === 'item/completed'
    && event.params?.threadId === threadId && event.params?.turnId === turnId)
    .map(event => event.params.item)
    .filter(item => !['userMessage', 'agentMessage', 'reasoning'].includes(item?.type));
  if (items.length > 4) return -1;
  const validators = [
    item => item?.type === 'commandExecution' && item.status === 'completed' && item.exitCode === 0
      && approveFixtureRead(item, cwd, shells) && item.aggregatedOutput?.trim() === marker,
    item => item?.type === 'commandExecution' && ['completed', 'failed'].includes(item.status) && item.exitCode === 1
      && approveFixtureTest(item, cwd, shells) && item.aggregatedOutput?.trim() === fixtureTestFailed,
    item => item?.status === 'completed' && exactRepair(item, cwd, marker),
    item => item?.type === 'commandExecution' && item.status === 'completed' && item.exitCode === 0
      && approveFixtureTest(item, cwd, shells) && item.aggregatedOutput?.trim() === fixtureTestPassed,
  ];
  return items.every((item, index) => validators[index](item)) ? items.length : -1;
}

export function approveFixturePatch(params, itemEvent, cwd, marker) {
  const item = itemEvent?.params?.item;
  const changes = item?.changes;
  if (!params || params.grantRoot != null || item?.type !== 'fileChange'
      || item.status !== 'inProgress' || params.itemId !== item.id
      || params.threadId !== itemEvent.params.threadId || params.turnId !== itemEvent.params.turnId
      || !Array.isArray(changes) || changes.length !== 1) return false;
  const change = changes[0];
  return change.kind?.type === 'add' && typeof change.path === 'string'
    && resolve(cwd, change.path).toLowerCase() === resolve(cwd, 'probe-output.txt').toLowerCase()
    && change.diff === marker + '\n';
}

// Evidence must belong to one native turn. A marker in unrelated output, a
// failed read, or a patch started before that read cannot authorize a write.
export function completedFixtureRead(events, threadId, turnId, cwd, marker, shells = []) {
  const commands = events.filter(event => event.method === 'item/completed'
    && event.params?.threadId === threadId && event.params?.turnId === turnId
    && event.params?.item?.type === 'commandExecution');
  if (commands.length !== 1) return false;
  const item = commands[0].params.item;
  return item.status === 'completed' && item.exitCode === 0
    && approveFixtureRead(item, cwd, shells) && item.aggregatedOutput?.trim() === marker;
}

export function completedFixtureDenial(events, threadId, turnId, cwd, marker, shells = []) {
  const commands = events.filter(event => event.method === 'item/completed'
    && event.params?.threadId === threadId && event.params?.turnId === turnId
    && event.params?.item?.type === 'commandExecution');
  if (commands.length !== 1) return false;
  const item = commands[0].params.item;
  // Reviewed app-server builds emit null output/exit code for a declined item.
  // A refusal is not a command process result and must not contain file output.
  return item.status === 'declined' && item.exitCode === null && item.processId == null
    && approveFixtureRead(item, cwd, shells)
    && item.aggregatedOutput === null && !JSON.stringify(item).includes(marker);
}
