import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/send.js', import.meta.url), 'utf8');
const text = value => ({ nodeType: 3, textContent: value });
const element = (tagName, ...childNodes) => ({ nodeType: 1, tagName, childNodes });

function attempt(children, rendered, expected) {
  class Visible { getClientRects() { return [1]; } }
  const model = Object.assign(new Visible(), { textContent: 'Selected mode' });
  const composer = {
    isContentEditable: true, innerText: rendered, childNodes: children,
    closest: () => ({ querySelectorAll: () => [model] })
  };
  let clicks = 0;
  const send = { disabled: false, click: () => clicks++ };
  const run = vm.runInNewContext(`(${source})`, {
    HTMLElement: Visible,
    document: { querySelector: selector => ({
      '#prompt-textarea': composer, '[data-testid="send-button"]': send
    })[selector] ?? null }
  });
  return { result: run(expected, 'Selected mode'), clicks };
}

test('plain editor paragraphs preserve line boundaries without layout spacing', () => {
  assert.deepEqual(attempt([
    element('P', element('SPAN', text('literal `code`'))),
    element('P', element('BR')),
    element('P', text('  {"key": "🦀"}  ')),
    element('P', element('BR'))
  ], 'literal `code`\n\n\n\n  {"key": "🦀"}  \n\n',
  'literal `code`\n\n  {"key": "🦀"}  \n'), { result: true, clicks: 1 });
});

test('real whitespace changes, removed delimiters and missing lines never send', () => {
  for (const changed of ['A B', 'A  B\n', 'A\tB', 'A\u00a0 B', 'A  C']) {
    assert.deepEqual(attempt([element('P', text(changed))], changed, 'A  B'),
      { result: 'E_COMPOSER_MISMATCH', clicks: 0 });
  }
  assert.deepEqual(attempt([element('P', text('code'))], 'code', '`code`'),
    { result: 'E_COMPOSER_MISMATCH', clicks: 0 });
});

test('unknown rich nodes cannot satisfy the paragraph fallback', () => {
  assert.deepEqual(attempt([element('P', element('CODE', text('A'))), element('P', text('B'))],
    'A\n\nB', 'A\nB'), { result: 'E_COMPOSER_MISMATCH', clicks: 0 });
});

test('explicit line breaks preserve an additional blank line', () => {
  assert.deepEqual(attempt([element('P', text('A'), element('BR'), element('BR'), text('B'))],
    'layout differs', 'A\n\nB'), { result: true, clicks: 1 });
});
