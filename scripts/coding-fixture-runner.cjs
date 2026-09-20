// Trusted offline fixture tests. Only the bounded arithmetic expression in
// solve.cjs is editable. This is not a sandbox for arbitrary JavaScript.
'use strict';
const { readFileSync } = require('node:fs');
const { Script } = require('node:vm');

function expression(source) {
  if (typeof source !== 'string' || source.length > 1024) throw new Error('E_FIXTURE_SOURCE');
  const match = /^module\.exports = function solve\(a, b\) \{\n  return ([^\r\n]+);\n\};\n$/.exec(source.replaceAll('\r\n', '\n'));
  // No strings, property access, templates, identifiers other than primitive
  // parameters, statements, or function bodies can cross this boundary.
  // Reject comments and assignment/update/arrow operators as well.
  const value = match?.[1];
  if (!value || value.length > 240 || !/^[ab0-9 ()+*/%<>=!?:&|^-]+$/.test(value)
      || /[ab]{2}|\d[ab]|[ab]\d|\/[/\*]|\*\/|\+\+|--|=>|(?:<<|>>|>>>|[+*/%&|^-])=|(?<![=!<>])=(?!=)|(?<![=!])==(?!=)/.test(value)) {
    throw new Error('E_FIXTURE_SOURCE');
  }
  // Parsing alone cannot execute the expression. Numbers and a/b are the only
  // available values; code generation remains disabled during each evaluation.
  try { new Script(`(${value})`); } catch { throw new Error('E_FIXTURE_SOURCE'); }
  return value;
}

function check(source, cases) {
  const value = expression(source);
  if (!Array.isArray(cases) || cases.length < 2 || cases.length > 32
      || cases.some(row => !Array.isArray(row) || row.length !== 3
        || row.some(number => !Number.isSafeInteger(number) || Math.abs(number) > 1000000))) {
    throw new Error('E_FIXTURE_CASES');
  }
  const script = new Script(`(${value})`);
  let passed = 0;
  for (const [a, b, expected] of cases) {
    try {
      const actual = script.runInNewContext({ a, b }, {
        timeout: 25, contextCodeGeneration: { strings: false, wasm: false },
      });
      if (typeof actual === 'number' && Object.is(actual, expected)) passed++;
    } catch { /* A parseable but invalid arithmetic operation fails the test. */ }
  }
  return { total: cases.length, passed, failed: cases.length - passed };
}

module.exports = { expression, check };
if (require.main === module) {
  try {
    const source = readFileSync('./solve.cjs', 'utf8');
    const cases = JSON.parse(readFileSync('./cases.json', 'utf8'));
    const result = check(source, cases);
    process.stdout.write(JSON.stringify(result) + '\n');
    process.exitCode = result.failed === 0 ? 0 : 1;
  } catch (error) {
    process.stdout.write(JSON.stringify({ error: ['E_FIXTURE_SOURCE', 'E_FIXTURE_CASES'].includes(error.message)
      ? error.message : 'E_FIXTURE_INPUT' }) + '\n');
    process.exitCode = 2;
  }
}
