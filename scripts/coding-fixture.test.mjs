import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { Script } from 'node:vm';
import runner from './coding-fixture-runner.cjs';
import { cases, source, editableSource, fixtureFiles, version, fingerprint } from './coding-fixture.mjs';

test('live repair inputs retain their frozen version and fingerprint', () => {
  assert.equal(version, 'cxweb.arithmetic-repairs.v1');
  assert.equal(fingerprint, 'b073abc6cc040afaf32f3b72621c5aaf8ec73c07682a086ec774744e05291af5');
});

test('every repair starts with real failing cases and admits a passing reference', () => {
  for (const fixture of cases) {
    const before = runner.check(source(fixture.broken), fixture.tests);
    assert.ok(before.failed > 0, fixture.id);
    assert.deepEqual(runner.check(source(fixture.reference), fixture.tests), {
      total: fixture.tests.length, passed: fixture.tests.length, failed: 0,
    });
  }
  assert.equal(editableSource(source('b + a')), true);
  assert.equal(runner.check(source('b + a'), cases[0].tests).failed, 0);
  assert.equal(editableSource(source('a - b')), true); // Admissible is not correct.
});

test('source boundary rejects executable capabilities and malformed wrappers before evaluation', () => {
  for (const value of [
    'process.exit()', 'require("node:fs")', 'a.constructor', 'a["constructor"]',
    'globalThis', '(() => 1)()', 'a = b', 'a += b', 'a >>= b', 'a >>>= b', 'a <<= b', 'a++', '--a', 'a == b',
    'a /* comment */ + b', 'a // b', 'a; throw 1', '`template`', '\\u0061',
    '(function(){})()', 'a,b', 'a +\n b', 'a + "x"', 'a + b' + ' '.repeat(240),
  ]) assert.equal(editableSource(source(value)), false, value);
  for (const value of [source('a + b') + 'process.exit();', source('a + b').replace('solve', 'other'),
    source('a + b').replace('a, b', 'a = process, b'), source('a + b').replace('return', 'throw')]) {
    assert.equal(editableSource(value), false);
  }
  assert.equal(editableSource(source('a + b').replaceAll('\n', '\r\n')), true);
  for (const inputs of [[[{}, 1, 2], [1, 1, 2]], [[1, 2, Infinity], [1, 1, 2]], [], [[1, 2, 3]]]) {
    assert.throws(() => runner.check(source('a + b'), inputs), /E_FIXTURE_CASES/);
  }
});

test('actual offline test process reports failing, repaired and rejected-source exit codes', async () => {
  await mkdir(resolve('.local/probes'), { recursive: true });
  const directory = await mkdtemp(resolve('.local/probes/arithmetic-unit-'));
  const files = fixtureFiles(cases[0]);
  for (const [name, contents] of Object.entries(files)) await writeFile(join(directory, name), contents);
  await writeFile(join(directory, 'tests.cjs'), await readFile(new URL('./coding-fixture-runner.cjs', import.meta.url)));
  const run = () => spawnSync(process.execPath, ['tests.cjs'], { cwd: directory, windowsHide: true, encoding: 'utf8', timeout: 5000 });
  const before = run();
  assert.equal(before.status, 1);
  assert.ok(JSON.parse(before.stdout).failed > 0);
  await writeFile(join(directory, 'solve.cjs'), source('b + a'));
  const after = run();
  assert.equal(after.status, 0);
  assert.deepEqual(JSON.parse(after.stdout), { total: 4, passed: 4, failed: 0 });
  await writeFile(join(directory, 'solve.cjs'), source('process.exit(0)'));
  const rejected = run();
  assert.equal(rejected.status, 2);
  assert.deepEqual(JSON.parse(rejected.stdout), { error: 'E_FIXTURE_SOURCE' });
});

test('VM timeout and evaluation failures are not reported as incorrect arithmetic', () => {
  const original = Script.prototype.runInNewContext;
  try {
    Script.prototype.runInNewContext = () => { throw Object.assign(new Error('fixture timeout'), { code: 'ERR_SCRIPT_EXECUTION_TIMEOUT' }); };
    assert.throws(() => runner.check(source('a + b'), cases[0].tests), /E_FIXTURE_TIMEOUT/);
    Script.prototype.runInNewContext = () => { throw new Error('fixture VM failure'); };
    assert.throws(() => runner.check(source('a + b'), cases[0].tests), /E_FIXTURE_EVALUATION/);
  } finally { Script.prototype.runInNewContext = original; }
});
