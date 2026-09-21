import { readFileSync, writeFileSync, appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

function versionParts(version) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error('Release versions must use major.minor.patch.');
  }
  const parts = version.split('.').map(Number);
  if (parts.some(part => part > 65535)) throw new Error('Version exceeds Windows installer limits.');
  return parts;
}

function compare(left, right) {
  const a = versionParts(left), b = versionParts(right);
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] - b[i];
  return 0;
}

export function selectVersion(source, requestedTag, existingTags) {
  versionParts(source);
  const existing = new Set(existingTags);
  const requested = requestedTag.trim();
  if (requested) {
    if (!requested.startsWith('v')) throw new Error('Explicit release tags must start with v.');
    const version = requested.slice(1);
    versionParts(version);
    if (compare(version, source) < 0) throw new Error('Release version cannot precede the source version.');
    if (existing.has(requested)) throw new Error(`Release tag ${requested} already exists. Leave the tag empty to select the next version automatically.`);
    return version;
  }
  let candidate = source;
  for (const tag of existing) {
    if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag)) continue;
    const version = tag.slice(1);
    if (compare(version, candidate) >= 0) {
      const parts = versionParts(version);
      parts[2]++;
      candidate = parts.join('.');
    }
  }
  versionParts(candidate);
  return candidate;
}

export function updateWorkspaceVersion(manifest, source, version) {
  versionParts(version);
  let changed = 0;
  const updated = manifest.replace(/(\[workspace\.package\][^]*?)(?=\r?\n\[|$)/, section =>
    section.replace(/^version\s*=\s*"([^"]+)"/m, (line, current) => {
      if (current !== source) throw new Error('Cargo and desktop source versions disagree.');
      changed++;
      return `version = "${version}"`;
    }));
  if (changed !== 1) throw new Error('Expected one workspace package version.');
  return updated;
}

export function externalLockEntries(lock) {
  return lock.replaceAll('\r\n', '\n').split('[[package]]')
    .filter(block => /^source = /m.test(block)).join('[[package]]');
}

function run(program, args) {
  return execFileSync(program, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}

function main() {
  const desktopPath = 'apps/desktop/src-tauri/tauri.conf.json';
  const desktop = JSON.parse(readFileSync(desktopPath, 'utf8'));
  const sourceVersion = desktop.version;
  const repository = process.env.GH_REPO;
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? '')) throw new Error('GH_REPO is required.');
  const tags = run('git', ['ls-remote', '--tags', 'origin']).split('\n')
    .map(line => line.split('\t')[1]?.replace(/^refs\/tags\//, '')).filter(Boolean);
  const releases = run('gh', ['api', '--paginate', `repos/${repository}/releases`, '--jq', '.[].tag_name']).split('\n');
  const version = selectVersion(sourceVersion, process.env.REQUESTED_TAG ?? '', [...tags, ...releases]);
  const manifest = updateWorkspaceVersion(readFileSync('Cargo.toml', 'utf8'), sourceVersion, version);
  const beforeLock = externalLockEntries(readFileSync('Cargo.lock', 'utf8'));
  writeFileSync('Cargo.toml', manifest);
  desktop.version = version;
  writeFileSync(desktopPath, JSON.stringify(desktop, null, 2) + '\n');
  run('cargo', ['update', '--workspace']);
  if (externalLockEntries(readFileSync('Cargo.lock', 'utf8')) !== beforeLock) {
    throw new Error('Release preparation unexpectedly changed external dependencies.');
  }
  const tag = `v${version}`;
  const installer = `cxweb_${version}_x64-setup.exe`;
  mkdirSync('.local', { recursive: true });
  writeFileSync('.local/release-provenance.json', JSON.stringify({
    source_commit: run('git', ['rev-parse', 'HEAD']), source_version: sourceVersion,
    release_version: version, release_tag: tag, installer,
    run_url: process.env.GITHUB_SERVER_URL && process.env.GITHUB_RUN_ID
      ? `${process.env.GITHUB_SERVER_URL}/${repository}/actions/runs/${process.env.GITHUB_RUN_ID}` : null,
  }, null, 2) + '\n');
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `tag=${tag}\nversion=${version}\ninstaller=${installer}\n`);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `Preparing **${tag}** from source version ${sourceVersion}.\n`);
  console.log(`Prepared ${tag}; external dependencies unchanged.`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
