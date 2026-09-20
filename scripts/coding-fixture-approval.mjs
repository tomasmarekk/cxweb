// Approval policy for the arithmetic diagnostic, never installed on web routes.
import { resolve } from 'node:path';
import { approveFixtureCommand } from './probe-client-approval.mjs';
import { editableSource } from './coding-fixture.mjs';

export const readCommand = "Get-Content -LiteralPath './solve.cjs', './cases.json'";
export const passedMessage = 'CXWEB_ARITHMETIC_PASSED';
export function testCommand(node) {
  if (!/^[A-Za-z]:[\\/][A-Za-z0-9_ ./\\-]+\.exe$/.test(node)) throw new Error('E_FIXTURE_NODE');
  return `& '${node}' './tests.cjs'`;
}
const normalize = text => text.replaceAll('\r\n', '\n');

// Apply one bounded unified hunk to known fixture bytes. File names and moves
// are checked separately. No fuzzy matching or arbitrary patch execution.
export function patchedSource(diff, initial) {
  if (typeof diff !== 'string' || diff.length > 4096 || !editableSource(initial)) return null;
  const lines = normalize(diff).split('\n');
  if (lines.pop() !== '') return null;
  const header = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@$/.exec(lines.shift() ?? '');
  if (!header) return null;
  const start = Number(header[1]) - 1, removed = Number(header[2] ?? 1);
  const newStart = Number(header[3]) - 1, added = Number(header[4] ?? 1);
  const original = normalize(initial).split('\n'); original.pop();
  if (start < 0 || start !== newStart || start + removed > original.length || removed < 1 || added < 1) return null;
  let consumed = 0;
  const replacement = [];
  for (const line of lines) {
    if (![' ', '+', '-'].includes(line[0])) return null;
    const content = line.slice(1);
    if (line[0] !== '+') {
      if (original[start + consumed] !== content) return null;
      consumed++;
    }
    if (line[0] !== '-') replacement.push(content);
  }
  if (consumed !== removed || replacement.length !== added) return null;
  const result = [...original.slice(0, start), ...replacement, ...original.slice(start + removed)].join('\n') + '\n';
  return editableSource(result) ? result : null;
}

export function changedSource(item, cwd, initial) {
  const changes = item?.changes;
  if (item?.type !== 'fileChange' || !Array.isArray(changes) || changes.length !== 1) return null;
  const change = changes[0];
  if (change.kind?.type !== 'update' || change.kind.movePath != null || change.kind.move_path != null || typeof change.path !== 'string'
      || resolve(cwd, change.path).toLowerCase() !== resolve(cwd, 'solve.cjs').toLowerCase()) return null;
  return patchedSource(change.diff, initial);
}

export function approvePatch(params, event, cwd, initial) {
  const item = event?.params?.item;
  return Boolean(params && params.grantRoot == null && item?.status === 'inProgress'
    && item.id === params.itemId && event.params.threadId === params.threadId
    && event.params.turnId === params.turnId && changedSource(item, cwd, initial));
}

export function completedProgress(events, run, cwd, shells) {
  const items = events.filter(event => event.method === 'item/completed'
    && event.params?.threadId === run.thread && event.params?.turnId === run.turn)
    .map(event => event.params.item).filter(item => !['userMessage', 'agentMessage', 'reasoning'].includes(item?.type));
  if (items.length > 4) return -1;
  const command = (item, expected, exit, output) => item?.type === 'commandExecution'
    && ['completed', ...(exit ? ['failed'] : [])].includes(item.status) && item.exitCode === exit
    && approveFixtureCommand(item, cwd, shells, expected)
    && typeof item.aggregatedOutput === 'string' && normalize(item.aggregatedOutput).trim() === output;
  const validators = [
    item => command(item, readCommand, 0, (run.initial + run.caseJson).trim()),
    item => command(item, run.testCommand, 1, JSON.stringify(run.before)),
    item => item?.status === 'completed' && changedSource(item, cwd, run.initial) !== null,
    item => command(item, run.testCommand, 0, JSON.stringify({ total: run.before.total, passed: run.before.total, failed: 0 })),
  ];
  return items.every((item, index) => validators[index](item)) ? items.length : -1;
}
