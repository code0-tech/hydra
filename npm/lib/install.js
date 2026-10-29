'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const https = require('https');
const crypto = require('crypto');

const { extractTarGz, extractZip } = require('./extract.js');

const pkg = require('../package.json');

const REPO = 'code0-tech/hydra';

// Mirrors the `asset` names produced by .github/workflows/release.yml.
function releaseAsset() {
  const platform = os.platform();
  const arch = os.arch();

  if (platform === 'linux' && arch === 'x64') return { name: 'codezero-linux-x86_64', ext: 'tar.gz' };
  if (platform === 'darwin' && arch === 'x64') return { name: 'codezero-macos-x86_64', ext: 'tar.gz' };
  if (platform === 'darwin' && arch === 'arm64') return { name: 'codezero-macos-arm64', ext: 'tar.gz' };
  if (platform === 'win32' && arch === 'x64') return { name: 'codezero-windows-x86_64', ext: 'zip' };

  throw new Error(
    `codezero: no prebuilt binary for platform "${platform}" (${arch}). ` +
      'Install from source instead: https://github.com/code0-tech/hydra#install'
  );
}

function binaryName() {
  return os.platform() === 'win32' ? 'codezero.exe' : 'codezero';
}

function binaryDir() {
  return path.join(__dirname, '..', 'bin');
}

function binaryPath() {
  return path.join(binaryDir(), binaryName());
}

function get(url, redirects = 5) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { 'User-Agent': 'codezero-npm-installer' } }, (res) => {
        if (
          res.statusCode >= 300 &&
          res.statusCode < 400 &&
          res.headers.location &&
          redirects > 0
        ) {
          res.resume();
          resolve(get(res.headers.location, redirects - 1));
          return;
        }
        if (res.statusCode !== 200) {
          res.resume();
          reject(new Error(`request to ${url} failed with status ${res.statusCode}`));
          return;
        }
        const chunks = [];
        res.on('data', (c) => chunks.push(c));
        res.on('end', () => resolve(Buffer.concat(chunks)));
        res.on('error', reject);
      })
      .on('error', reject);
  });
}

async function verifyChecksum(tag, assetName, data) {
  try {
    const checksums = (
      await get(`https://github.com/${REPO}/releases/download/${tag}/checksums.txt`)
    ).toString('utf8');
    const line = checksums.split('\n').find((l) => l.trim().endsWith(assetName));
    if (!line) return;
    const expected = line.trim().split(/\s+/)[0];
    const actual = crypto.createHash('sha256').update(data).digest('hex');
    if (expected !== actual) {
      throw new Error(`checksum mismatch for ${assetName}: expected ${expected}, got ${actual}`);
    }
  } catch (err) {
    if (err.message.startsWith('checksum mismatch')) throw err;
    console.warn(`codezero: could not verify checksum (${err.message}), continuing anyway`);
  }
}

async function ensureBinary() {
  const dest = binaryPath();
  if (fs.existsSync(dest)) return dest;

  const { name, ext } = releaseAsset();
  const tag = `v${pkg.version}`;
  const assetName = `${name}.${ext}`;
  const url = `https://github.com/${REPO}/releases/download/${tag}/${assetName}`;

  console.log(`codezero: downloading ${assetName} from release ${tag}...`);
  const archive = await get(url);
  await verifyChecksum(tag, assetName, archive);

  const data = ext === 'zip' ? extractZip(archive, binaryName()) : extractTarGz(archive, binaryName());

  fs.mkdirSync(binaryDir(), { recursive: true });
  const tmp = `${dest}.download`;
  fs.writeFileSync(tmp, data);
  if (os.platform() !== 'win32') fs.chmodSync(tmp, 0o755);
  fs.renameSync(tmp, dest);

  return dest;
}

module.exports = { ensureBinary, binaryPath };

if (require.main === module) {
  ensureBinary()
    .then(() => console.log('codezero: binary ready'))
    .catch((err) => {
      console.warn(`codezero: postinstall download failed (${err.message}).`);
      console.warn('codezero: it will be downloaded on first run instead.');
    });
}
