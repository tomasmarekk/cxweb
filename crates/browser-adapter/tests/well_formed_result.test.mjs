import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/well_formed_result.js', import.meta.url), 'utf8');
const check = vm.runInNewContext(`(${source})`);

test('valid Unicode, literal JSON escapes and replacement characters stay unchanged', () => {
  const values = [null, true, 42, 'café 🦀 Ω', '\\ud83e', '\ufffd', {nested:['\r\n\t','🦀']}];
  for (const value of values) {
    const before = JSON.stringify(value);
    check(value);
    assert.equal(JSON.stringify(value), before);
  }
});

test('unpaired UTF-16 in nested values and keys never reaches transport serialization', () => {
  for (const value of ['\ud83e', '\udd80', 'x\ud83ey', ['\udd80'], {nested:['\ud83e']}, {['\ud83e']:'valid'}]) {
    assert.throws(() => check(value), {message:'E_BROWSER_UTF16'});
  }
  const cycle = {}; cycle.self = cycle;
  assert.throws(() => check(cycle), {message:'E_BROWSER_RESULT_DEPTH'});
});
