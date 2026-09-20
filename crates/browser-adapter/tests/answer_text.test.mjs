import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src/dom/answer_text.js', import.meta.url), 'utf8');
const project = vm.runInNewContext(`(${source})`, { getComputedStyle: node => node.style ?? { display: 'block', visibility: 'visible' } });
const text = textContent => ({ nodeType: 3, textContent });
const element = (tagName, childNodes = [], attrs = {}) => ({
  nodeType: 1, tagName, childNodes, hidden: !!attrs.hidden, style: attrs.style,
  getAttribute: name => attrs[name] ?? null,
  matches: selector => {
    assert.equal(selector, 'button, [role="button"], script, style');
    return ['BUTTON', 'SCRIPT', 'STYLE'].includes(tagName) || attrs.role === 'button';
  },
});

test('text nodes retain exact indentation, escapes, nonbreaking spaces and Unicode', () => {
  const payload = '{"input":"+  return a + b;  four:    tab:\t NBSP:\u00a0 café 🦀 C:\\\\fixture"}';
  assert.equal(project(element('DIV', [element('P', [text(payload)])])), payload);
  assert.equal(project(null), '');
  assert.equal(project(element('DIV')), '');
});

test('paragraph and explicit break boundaries are retained without CSS spacing', () => {
  const root = element('DIV', [element('P', [text('{')]), element('P', [
    text('  "text": "two  '), element('SPAN', [text('spaces')]), text('"'),
  ]), element('P', [text('}')])]);
  assert.equal(project(root), '{\n  "text": "two  spaces"\n}');
  assert.equal(project(element('DIV', [text('a'), element('BR'), element('BR'), text('b')])), 'a\n\nb');
});

test('hidden content and UI controls cannot enter the answer projection', () => {
  const hidden = [
    element('DIV', [text('hidden')], { hidden: true }),
    element('SPAN', [text('aria hidden')], { 'aria-hidden': 'true' }),
    element('DIV', [text('display hidden')], { style: { display: 'none', visibility: 'visible' } }),
    element('DIV', [text('visibility hidden')], { style: { display: 'block', visibility: 'hidden' } }),
    element('BUTTON', [text('Copy')]), element('SPAN', [text('Copy')], { role: 'button' }),
    element('SCRIPT', [text('script')]), element('STYLE', [text('css')]), { nodeType: 8 },
  ];
  assert.equal(project(element('DIV', [element('P', [text('first')]), ...hidden, element('P', [text('second')])])), 'first\nsecond');
});

test('oversized, over-deep and over-wide answers fail before an unbounded join', () => {
  assert.throws(() => project(element('DIV', [text('x'.repeat(4 * 1024 * 1024 + 1))])), /E_PAYLOAD_LIMIT/);
  let deep = text('x'); for (let n = 0; n < 65; n++) deep = element('SPAN', [deep]);
  assert.throws(() => project(deep), /E_PAYLOAD_LIMIT/);
  assert.throws(() => project(element('DIV', Array.from({ length: 65536 }, () => text('')))), /E_PAYLOAD_LIMIT/);
});
