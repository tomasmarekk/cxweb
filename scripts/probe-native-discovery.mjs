// Check the read-only inventory against the two reviewed local installations.
// Persist portable evidence without user paths or environment values.
import { execFileSync } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, isAbsolute } from 'node:path';
import assert from 'node:assert/strict';

assert.ok(process.argv.length <= 3, 'Usage: node scripts/probe-native-discovery.mjs [cxweb executable]');
const bridge = resolve(process.argv[2] ?? 'target/release/cxweb.exe');
const started = performance.now();
const report = JSON.parse(execFileSync(bridge, ['native-discover'], {
  windowsHide: true, encoding: 'utf8', timeout: 60000, maxBuffer: 1024 * 1024,
}));
assert.equal(report.activation_eligible, false);
assert.equal(report.target_selection_required, true);
assert.equal(report.executed_processes, 0);
assert.ok(report.candidates.length <= 32);
assert.equal(new Set(report.candidates.map(candidate => candidate.executable)).size, report.candidates.length);
for (const candidate of report.candidates) {
  assert.ok(isAbsolute(candidate.executable));
  const contents = await readFile(candidate.executable);
  assert.equal(createHash('sha256').update(contents).digest('hex'), candidate.executable_sha256);
  assert.ok(candidate.sources.every(source => ['path_executable', 'npm_installation', 'desktop_backend_cache'].includes(source)));
}
assert.ok(report.candidates.some(candidate => candidate.reviewed_build === '0.155.1' && candidate.sources.includes('npm_installation')));
assert.ok(report.candidates.some(candidate => candidate.reviewed_build === '0.155.0-alpha.9.2' && candidate.sources.includes('desktop_backend_cache')));
const evidence = {
  schema: 'cxweb.native-discovery-probe.v1',
  result: 'PASS',
  scope: 'Read-only local executable inventory; not running App identity or picker qualification',
  elapsed_ms: Math.round(performance.now() - started),
  target_selection_required: report.target_selection_required,
  activation_eligible: report.activation_eligible,
  candidates: report.candidates.map(({ executable, ...candidate }) => ({ ...candidate, fingerprint_independently_verified: true })),
  diagnostics: report.diagnostics,
};
await writeFile('integration-tests/compatibility/native-discovery-windows.json', JSON.stringify(evidence, null, 2) + '\n');
console.log(JSON.stringify({ result: evidence.result, candidates: evidence.candidates.length, builds: evidence.candidates.map(candidate => candidate.reviewed_build), diagnostics: evidence.diagnostics }));
