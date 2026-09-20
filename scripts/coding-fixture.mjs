// Deterministic arithmetic repairs, separate from protocol-format samples.
import { createHash } from 'node:crypto';
import runner from './coding-fixture-runner.cjs';

export const version = 'cxweb.arithmetic-repairs.v1';
export const source = value => `module.exports = function solve(a, b) {\n  return ${value};\n};\n`;
export const cases = [
  { id: 'sum', instruction: 'Return the sum of the two integers.', broken: 'a - b',
    tests: [[3, 4, 7], [-5, 2, -3], [0, 0, 0], [9, -4, 5]], reference: 'a + b' },
  { id: 'inclusive-count', instruction: 'Count the integers in the inclusive interval from a to b. Return zero when a is greater than b.',
    broken: 'b - a', tests: [[2, 5, 4], [7, 7, 1], [5, 2, 0], [-3, 1, 5]], reference: 'a > b ? 0 : b - a + 1' },
  { id: 'absolute-distance', instruction: 'Return the nonnegative absolute distance between the two integers.',
    broken: 'a - b', tests: [[7, 2, 5], [2, 7, 5], [-5, -1, 4], [3, 3, 0]], reference: 'a >= b ? a - b : b - a' },
  { id: 'ceiling-pages', instruction: 'Return the number of pages needed for a items at b items per page. Inputs satisfy a >= 0 and b > 0.',
    broken: '(a / b) | 0', tests: [[0, 4, 0], [8, 4, 2], [9, 4, 3], [1, 8, 1], [99, 10, 10]], reference: '((a + b - 1) / b) | 0' },
];
export const fingerprint = createHash('sha256').update(JSON.stringify({ version, cases })).digest('hex');

export function editableSource(value) {
  try { runner.expression(value); return true; } catch { return false; }
}

// A proposed source must be admissible before a native patch/test is approved.
// Correctness is evaluated by actual test output, never by matching reference
// source. Different correct arithmetic expressions remain admissible.
export function fixtureFiles(fixture) {
  if (!cases.includes(fixture)) throw new Error('E_FIXTURE_CASE');
  return { 'solve.cjs': source(fixture.broken), 'cases.json': JSON.stringify(fixture.tests, null, 2) + '\n' };
}
