#!/usr/bin/env node
/**
 * Publish the npm packages for a released version from your own machine,
 * using the local `npm login` session (2FA prompts work as usual).
 *
 * Binaries are downloaded from the GitHub Release that release.yml creates
 * when a `v*` tag is pushed, so no npm token has to live in CI.
 *
 * Usage:
 *   node npm/scripts/publish-from-release.js <version> [--dry-run] [--otp <code>]
 *                                            [--force] [--repo owner/name]
 *
 *   --dry-run   run `npm publish --dry-run` (nothing is uploaded)
 *   --otp       one-time 2FA code to pass to every `npm publish`
 *   --force     skip the Cargo.toml version consistency check
 *   --repo      GitHub repo to download assets from (default: from npm/main/package.json)
 */
const { spawnSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const https = require('https');

const ROOT = path.join(__dirname, '..', '..');
const PUBLISH_DIR = path.join(ROOT, 'npm-publish');
const DOWNLOAD_DIR = path.join(PUBLISH_DIR, 'downloads');
const MAIN_PACKAGE = JSON.parse(fs.readFileSync(path.join(ROOT, 'npm', 'main', 'package.json'), 'utf8'));

// Must match the archive names produced by .github/workflows/release.yml
const PLATFORMS = {
  'darwin-x64': { asset: 'ccline-macos-x64.tar.gz', binary: 'ccline' },
  'darwin-arm64': { asset: 'ccline-macos-arm64.tar.gz', binary: 'ccline' },
  'linux-x64': { asset: 'ccline-linux-x64.tar.gz', binary: 'ccline' },
  'linux-x64-musl': { asset: 'ccline-linux-x64-static.tar.gz', binary: 'ccline' },
  'linux-arm64': { asset: 'ccline-linux-arm64.tar.gz', binary: 'ccline' },
  'linux-arm64-musl': { asset: 'ccline-linux-arm64-static.tar.gz', binary: 'ccline' },
  'win32-x64': { asset: 'ccline-windows-x64.zip', binary: 'ccline.exe' },
};

function parseArgs(argv) {
  const opts = { version: null, dryRun: false, otp: null, force: false, repo: null };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--dry-run') opts.dryRun = true;
    else if (arg === '--force') opts.force = true;
    else if (arg === '--otp') opts.otp = argv[++i];
    else if (arg === '--repo') opts.repo = argv[++i];
    else if (arg.startsWith('--')) fail(`Unknown option: ${arg}`);
    else if (!opts.version) opts.version = arg.replace(/^v/, '');
    else fail(`Unexpected argument: ${arg}`);
  }
  if (!opts.version) {
    fail('Usage: node npm/scripts/publish-from-release.js <version> [--dry-run] [--otp <code>] [--force] [--repo owner/name]');
  }
  if (!opts.repo) {
    const match = /github\.com\/([^/]+\/[^/.]+)/.exec(MAIN_PACKAGE.repository?.url || '');
    if (!match) fail('Cannot determine GitHub repo; pass --repo owner/name');
    opts.repo = match[1];
  }
  return opts;
}

function fail(message) {
  console.error(`❌ ${message}`);
  process.exit(1);
}

function run(cmd, args, options = {}) {
  const result = spawnSync(cmd, args, {
    stdio: 'inherit',
    shell: process.platform === 'win32',
    ...options,
  });
  if (result.status !== 0) {
    throw new Error(`${cmd} ${args.join(' ')} failed (exit code ${result.status})`);
  }
}

function capture(cmd, args) {
  const result = spawnSync(cmd, args, { encoding: 'utf8', shell: process.platform === 'win32' });
  return result.status === 0 ? result.stdout.trim() : null;
}

function download(url, dest, redirectsLeft = 5) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { 'User-Agent': 'ccline-publish-script' } }, (res) => {
        if ([301, 302, 303, 307, 308].includes(res.statusCode) && res.headers.location) {
          res.resume();
          if (redirectsLeft === 0) return reject(new Error(`Too many redirects for ${url}`));
          return resolve(download(res.headers.location, dest, redirectsLeft - 1));
        }
        if (res.statusCode !== 200) {
          res.resume();
          return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
        }
        const file = fs.createWriteStream(dest);
        res.pipe(file);
        file.on('finish', () => file.close(resolve));
        file.on('error', reject);
      })
      .on('error', reject);
  });
}

function cargoVersion() {
  const cargo = fs.readFileSync(path.join(ROOT, 'Cargo.toml'), 'utf8');
  const match = /^version\s*=\s*"([^"]+)"/m.exec(cargo);
  return match ? match[1] : null;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  const { version, dryRun, otp, force, repo } = opts;
  const publishArgs = ['publish', '--access', 'public'];
  if (dryRun) publishArgs.push('--dry-run');
  if (otp) publishArgs.push('--otp', otp);

  // 1. Preconditions
  const user = capture('npm', ['whoami']);
  if (!user) fail('Not logged in to npm. Run `npm login` first.');
  console.log(`👤 npm user: ${user}`);

  const crateVersion = cargoVersion();
  if (crateVersion !== version && !force) {
    fail(`Cargo.toml version (${crateVersion}) != ${version}; the binary's --version and update checker rely on them matching. Use --force to override.`);
  }

  // 2. Download release assets
  fs.mkdirSync(DOWNLOAD_DIR, { recursive: true });
  for (const [platform, { asset }] of Object.entries(PLATFORMS)) {
    const url = `https://github.com/${repo}/releases/download/v${version}/${asset}`;
    const dest = path.join(DOWNLOAD_DIR, asset);
    console.log(`⬇️  ${platform}: ${url}`);
    await download(url, dest);
  }

  // 3. Prepare package.json files (writes npm-publish/<platform> and npm-publish/main)
  run('node', [path.join(__dirname, 'prepare-packages.js'), version]);

  // 4. Extract binaries into the platform packages
  for (const [platform, { asset, binary }] of Object.entries(PLATFORMS)) {
    const extractDir = path.join(DOWNLOAD_DIR, platform);
    fs.rmSync(extractDir, { recursive: true, force: true });
    fs.mkdirSync(extractDir, { recursive: true });
    // bsdtar (macOS, Windows 10+) and GNU tar both handle .tar.gz; bsdtar also handles .zip
    run('tar', ['-xf', path.join(DOWNLOAD_DIR, asset), '-C', extractDir]);

    const target = path.join(PUBLISH_DIR, platform, binary);
    fs.copyFileSync(path.join(extractDir, binary), target);
    if (!binary.endsWith('.exe')) fs.chmodSync(target, 0o755);
    console.log(`📦 ${platform}: ${target}`);
  }

  // 5. Publish platform packages first, then the main package
  const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
  for (const platform of Object.keys(PLATFORMS)) {
    const cwd = path.join(PUBLISH_DIR, platform);
    const name = JSON.parse(fs.readFileSync(path.join(cwd, 'package.json'), 'utf8')).name;
    console.log(`\n🚀 Publishing ${name}@${version}${dryRun ? ' (dry run)' : ''}`);
    run(npm, publishArgs, { cwd });
  }

  const mainDir = path.join(PUBLISH_DIR, 'main');
  console.log(`\n🚀 Publishing ${MAIN_PACKAGE.name}@${version}${dryRun ? ' (dry run)' : ''}`);
  run(npm, publishArgs, { cwd: mainDir });

  console.log(`\n🎉 Done. Install with: npm install -g ${MAIN_PACKAGE.name}`);
}

main().catch((error) => fail(error.message));
