// Exercise native configuration inspection with disposable homes and no account.
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile, access, readdir, symlink, unlink } from 'node:fs/promises';
import { resolve, join, isAbsolute, dirname, basename } from 'node:path';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

const client = process.argv[2];
if (!client || !isAbsolute(client) || process.argv.length > 4) {
  throw new Error('Usage: node scripts/probe-preflight.mjs <selected absolute native backend> [cxweb executable]');
}
const bridge = resolve(process.argv[3] ?? 'target/debug/cxweb.exe');
const root = resolve('.local/probes');
await mkdir(root, { recursive: true });
const work = await mkdtemp(join(root, 'preflight-'));
const protectFixture = (path, foreign = false) => execFileSync('powershell.exe', [
  '-NoProfile', '-NonInteractive', '-File', resolve('scripts/protect-preflight-fixture.ps1'),
  '-Path', path, ...(foreign ? ['-ForeignRead'] : []),
], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
protectFixture(work);
const env = { ...process.env };
for (const key of Object.keys(env)) if (/^(CODEX_|OPENAI_|CHATGPT_)/i.test(key)) delete env[key];
const cases = [
  { name: 'signed-out', expected: ['subscription_auth_required'], mode: 'signed_out' },
  { name: 'synthetic-api-key', login: true, expected: ['subscription_auth_required'], mode: 'api_key' },
  { name: 'environment-auth', env: { OPENAI_API_KEY: 'cxweb-synthetic-not-a-real-key' }, expected: ['environment_auth', 'subscription_auth_required'] },
  { name: 'unowned-route', config: 'openai_base_url = "http://127.0.0.1:1/PRIVATE_ROUTE"\n', expected: ['openai_base_url', 'subscription_auth_required'] },
  { name: 'environment-route', env: { OPENAI_BASE_URL: 'http://127.0.0.1:1/PRIVATE_ROUTE' }, expected: ['environment_route', 'subscription_auth_required'] },
  { name: 'custom-provider', config: 'model_provider = "fixture"\n[model_providers.fixture]\nname = "Fixture"\nbase_url = "http://127.0.0.1:1"\nwire_api = "responses"\n', expected: ['model_provider', 'subscription_auth_required'] },
  // These Windows builds no longer load the legacy file from CODEX_HOME.
  // Actual system/cloud policy is not modified by this isolated harness.
  { name: 'ignored-legacy-home-policy', managed: 'chatgpt_base_url = "http://127.0.0.1:1/PRIVATE_ROUTE"\n', expected: ['subscription_auth_required'] },
  { name: 'static-catalog', catalog: true, expected: ['model_catalog_json', 'subscription_auth_required'] },
  { name: 'disabled-project', project: 'openai_base_url = "http://127.0.0.1:1/PRIVATE_ROUTE"\n', expected: ['subscription_auth_required'] },
];
const evidence = { schema: 'cxweb.preflight-probe.v1', client: execFileSync(client, ['--version'], { encoding: 'utf8', windowsHide: true }).trim(), authentication: 'signed-out and synthetic API-key fixtures only', managedRoutingDetection: 'unit RPC fixtures only; real system/cloud policy was not modified', modelRequests: 0, cases: [] };
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
try {
  for (const fixture of cases) {
    const home = join(work, fixture.name, 'home'), cwd = join(work, fixture.name, 'workspace');
    await mkdir(home, { recursive: true }); await mkdir(cwd);
    const files = new Map();
    if (fixture.catalog) {
      const catalog = execFileSync(bridge, ['probe-catalog'], { windowsHide: true });
      const file = join(home, 'catalog.json');
      files.set(file, catalog);
      fixture.config = `model_catalog_json = ${JSON.stringify(file.replaceAll('\\', '/'))}\n`;
    }
    if (fixture.config) files.set(join(home, 'config.toml'), fixture.config);
    if (fixture.managed) files.set(join(home, 'managed_config.toml'), fixture.managed);
    if (fixture.project) {
      await mkdir(join(cwd, '.codex'));
      files.set(join(cwd, '.codex', 'config.toml'), fixture.project);
    }
    for (const [file, content] of files) await writeFile(file, content);
    if (fixture.login) execFileSync(client, ['login', '--with-api-key'], { env: { ...env, CODEX_HOME: home }, input: 'cxweb-synthetic-not-a-real-key\n', windowsHide: true, stdio: ['pipe', 'ignore', 'ignore'] });
    const result = spawnSync(bridge, ['native-preflight', '--client', client, '--home', home, '--cwd', cwd], { env: { ...env, ...fixture.env }, encoding: 'utf8', windowsHide: true, timeout: 60000 });
    assert.equal(result.status, 0, `${fixture.name}: ${result.stderr}`);
    const report = JSON.parse(result.stdout);
    assert.deepEqual(report.assessment.conflicts, fixture.expected.toSorted(), fixture.name);
    if (fixture.mode) assert.equal(report.assessment.auth_mode, fixture.mode, fixture.name);
    if (fixture.project) assert.ok(report.assessment.disabled_layer_count > 0, 'untrusted project is disabled by native config resolution');
    assert.equal(report.activation_eligible, false);
    assert.equal(report.model_requests, 0);
    assert.equal(report.user_config_unchanged, true);
    assert.equal(report.selected_config_and_parent_access_verified, true);
    assert.equal(report.target_path_identity_verified, true);
    assert.equal(report.executable_unchanged, true);
    assert.ok(!JSON.stringify(report).includes('PRIVATE_'));
    for (const [file, content] of files) assert.equal(digest(await readFile(file)), digest(content), 'fixture bytes preserved');
    if (!fixture.config) {
      await assert.rejects(access(join(home, 'config.toml')), 'native inspection does not create user config');
    }
    evidence.cases.push({ name: fixture.name, result: 'PASS', report });
  }
  const unknown = join(work, 'unreviewed.exe');
  await writeFile(unknown, 'Synthetic fixture; this file must never execute');
  const result = spawnSync(bridge, ['native-preflight', '--client', unknown, '--home', join(work, 'signed-out', 'home'), '--cwd', join(work, 'signed-out', 'workspace')], { env, encoding: 'utf8', windowsHide: true, timeout: 10000 });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /E_PREFLIGHT_EXECUTABLE/);
  evidence.cases.push({ name: 'non-codex-executable', result: 'PASS refused before execution' });
  const foreign = await mkdtemp(join(root, 'preflight-'));
  protectFixture(foreign, true);
  const denied = spawnSync(bridge, ['native-preflight', '--client', client, '--home', foreign, '--cwd', foreign], { env, encoding: 'utf8', windowsHide: true, timeout: 60000 });
  assert.notEqual(denied.status, 0);
  assert.match(denied.stderr, /E_PREFLIGHT_CONFIG_PERMISSIONS/);
  assert.deepEqual(await readdir(foreign), [], 'native backend must not start in exposed home');
  evidence.cases.push({ name: 'foreign-read-grant', result: 'PASS refused before backend launch; home unchanged' });
  // Junctions are deliberately tested in the original input, before any
  // canonicalization. Each link is ours; its target is never modified/deleted.
  const targetFixture = join(work, 'target-identity');
  await mkdir(targetFixture);
  const targetHome = join(targetFixture, 'home'), targetCwd = join(targetFixture, 'workspace');
  await mkdir(targetHome); await mkdir(targetCwd);
  for (const kind of ['home', 'workspace', 'client']) {
    const link = join(targetFixture, `${kind}-junction`);
    const target = kind === 'home' ? targetHome : kind === 'workspace' ? targetCwd : dirname(client);
    await symlink(target, link, 'junction');
    try {
      const denied = spawnSync(bridge, ['native-preflight',
        '--client', kind === 'client' ? join(link, basename(client)) : client,
        '--home', kind === 'home' ? link : targetHome,
        '--cwd', kind === 'workspace' ? link : targetCwd,
      ], { env, encoding: 'utf8', windowsHide: true, timeout: 10000 });
      assert.notEqual(denied.status, 0);
      assert.match(denied.stderr, /E_PREFLIGHT_TARGET_IDENTITY/);
      assert.deepEqual(await readdir(targetHome), [], 'reparse target rejected before backend launch');
      assert.deepEqual(await readdir(targetCwd), [], 'workspace remains empty');
      evidence.cases.push({ name: `${kind}-junction`, result: 'PASS refused before backend launch; fixture unchanged' });
    } finally {
      await unlink(link);
    }
  }
  evidence.result = 'PASS';
} catch (error) {
  evidence.result = 'FAIL';
  evidence.error = String(error.message);
  process.exitCode = 1;
} finally {
  await writeFile(join(work, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n');
  console.log(JSON.stringify(evidence, null, 2));
  console.log(`Evidence: ${join(work, 'evidence.json')}`);
}
