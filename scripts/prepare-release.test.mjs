import test from 'node:test';
import assert from 'node:assert/strict';
import { selectVersion, updateWorkspaceVersion, externalLockEntries } from './prepare-release.mjs';

test('automatic releases advance past published versions without changing old releases', () => {
  assert.equal(selectVersion('0.1.1', '', []), '0.1.1');
  assert.equal(selectVersion('0.1.1', '', ['v0.1.0', 'v0.1.1']), '0.1.2');
  assert.equal(selectVersion('0.1.1', '', ['v0.1.4', 'v0.1.2', 'v0.1.1']), '0.1.5');
  assert.equal(selectVersion('0.2.0', '', ['v0.1.9', 'unrelated-tag']), '0.2.0');
  assert.equal(selectVersion('0.1.1', '', ['v0.1.9', 'v0.1.10']), '0.1.11');
});

test('explicit versions are validated and collisions fail before building', () => {
  assert.equal(selectVersion('0.1.1', 'v0.2.0', ['v0.1.1']), '0.2.0');
  for (const tag of ['v0.1.1', 'v0.0.9', '0.1.2', 'v01.2.3', 'v1.2.3\ninjected=x', 'v1.2.65536']) {
    assert.throws(() => selectVersion('0.1.1', tag, ['v0.1.1']));
  }
  assert.throws(() => selectVersion('0.1.1', '', ['v0.1.65535']));
});

test('staging only changes the workspace package version and detects mismatched sources', () => {
  const source = '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "0.1.1"\nedition = "2024"\n\n[workspace.dependencies]\nexample = { version = "0.1.1" }\n';
  const updated = updateWorkspaceVersion(source, '0.1.1', '0.1.2');
  assert.equal(updated, source.replace('version = "0.1.1"', 'version = "0.1.2"'));
  assert.throws(() => updateWorkspaceVersion(source, '0.0.1', '0.1.2'));
  assert.throws(() => updateWorkspaceVersion('[workspace]\nmembers = []', '0.1.1', '0.1.2'));
});

test('lock comparison accepts workspace version changes but detects dependency changes', () => {
  const source = 'version = 4\n[[package]]\nname = "cxweb"\nversion = "0.1.1"\n[[package]]\nname = "external"\nversion = "1.0.0"\nsource = "registry+example"\nchecksum = "original"\n';
  assert.equal(externalLockEntries(source), externalLockEntries(source.replace('0.1.1', '0.1.2')));
  assert.equal(externalLockEntries(source), externalLockEntries(source.replaceAll('\n', '\r\n')));
  assert.notEqual(externalLockEntries(source), externalLockEntries(source.replace('original', 'changed')));
});
