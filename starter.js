'use strict';

/**
 * OmniCore (Rust) — panel startup file
 *
 * Use this when your host only lets you set a startup file / command and
 * does not give you an interactive console.
 *
 * Panel examples:
 *   Startup command:  node starter.js
 *   JS FILE:          starter.js
 *   Start script:     node starter.js
 *
 * What it does:
 *   1. Loads .env into the process environment (token, OWNER_ID, etc.)
 *   2. Finds the omnicore binary (see BINARY PATHS below)
 *   3. Runs it as a child process
 *   4. If the bot exits with code 42 (Owner → Restart), starts it again
 *   5. On any other exit code, exits so the panel can mark the server stopped
 *
 * Layout expected next to this file:
 *   starter.js
 *   .env                 ← DISCORD_BOT_TOKEN + OWNER_ID
 *   omnicore             ← Linux x86_64 binary (chmod +x)
 *   assets/              ← fonts + emoji-icons (required)
 *   data/                ← created automatically for SQLite
 *
 * BINARY PATHS (first match wins):
 *   OMNICORE_BIN env var
 *   ./omnicore
 *   ./omnicore-linux-x86_64
 *   ./bin/omnicore
 *   ./target/release/omnicore
 */

const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const zlib = require('zlib');
const https = require('https');
const { Readable, Transform } = require('stream');
const { pipeline } = require('stream/promises');
const { spawn, execFile } = require('child_process');
const { promisify } = require('util');
const execFileAsync = promisify(execFile);

const ROOT = __dirname;
const RESTART_CODE = 42;
const RETRY_MS = 1000;
const CRASH_RETRY_MS = 5000;

// ── load .env (stdlib only — no dotenv package required) ──────────────
function loadEnvFile(filePath) {
  if (!fs.existsSync(filePath)) return;
  const text = fs.readFileSync(filePath, 'utf8');
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eq = trimmed.indexOf('=');
    if (eq <= 0) continue;
    const key = trimmed.slice(0, eq).trim();
    let val = trimmed.slice(eq + 1).trim();
    if (
      (val.startsWith('"') && val.endsWith('"')) ||
      (val.startsWith("'") && val.endsWith("'"))
    ) {
      val = val.slice(1, -1);
    }
    if (process.env[key] === undefined) process.env[key] = val;
  }
}

loadEnvFile(path.join(ROOT, '.env'));

const AUTO_UPDATE = process.env.OMNICORE_AUTO_UPDATE === '1';
const GITHUB_REPOSITORY = process.env.GITHUB_REPOSITORY || 'necromancerelux-ux/OmniCore_Rust';
const GITHUB_TOKEN = process.env.GITHUB_TOKEN || '';
const BACKUP_TOKEN = process.env.GITHUB_BACKUP_TOKEN || '';
const BACKUP_ENABLED = process.env.OMNICORE_BACKUP_ENABLED === '1';
const BACKUP_INTERVAL_MS = Math.max(0.1, Number.parseFloat(process.env.OMNICORE_BACKUP_INTERVAL_HOURS || process.env.GITHUB_BACKUP_INTERVAL_HOURS || '6') || 6) * 60 * 60 * 1000;
const BACKUP_RETENTION_MS = Math.max(1, Number.parseInt(process.env.OMNICORE_BACKUP_RETENTION_DAYS || '14', 10) || 14) * 24 * 60 * 60 * 1000;
const UPDATE_INTERVAL_MS = Math.max(
  1,
  Number.parseInt(process.env.OMNICORE_UPDATE_INTERVAL_MINUTES || '5', 10) || 5
) * 60 * 1000;

// Sensible defaults for panel hosts
if (!process.env.DATABASE_PATH) {
  process.env.DATABASE_PATH = path.join(ROOT, 'data', 'bot.sqlite');
}
if (!process.env.RUST_LOG) {
  process.env.RUST_LOG = 'info';
}

// Ensure data dir exists for SQLite
try {
  fs.mkdirSync(path.dirname(process.env.DATABASE_PATH), { recursive: true });
} catch {
  /* ignore */
}

// ── resolve binary ────────────────────────────────────────────────────
function resolveBinary() {
  const candidates = [
    process.env.OMNICORE_BIN,
    path.join(ROOT, 'omnicore'),
    path.join(ROOT, 'omnicore-linux-x86_64'),
    path.join(ROOT, 'bin', 'omnicore'),
    path.join(ROOT, 'target', 'release', 'omnicore'),
  ].filter(Boolean);

  for (const candidate of candidates) {
    try {
      if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
        // Ensure executable bit on Unix
        try {
          fs.accessSync(candidate, fs.constants.X_OK);
        } catch {
          try {
            fs.chmodSync(candidate, 0o755);
          } catch {
            /* panel may not allow chmod — try running anyway */
          }
        }
        return candidate;
      }
    } catch {
      /* next */
    }
  }
  return null;
}

let binary = resolveBinary();
if (!binary && AUTO_UPDATE && GITHUB_TOKEN && process.platform === 'linux' && ['x64', 'arm64'].includes(process.arch)) {
  binary = process.env.OMNICORE_BIN || path.join(ROOT, 'omnicore');
}
if (!binary) {
  console.error('[starter] No omnicore binary found.');
  console.error('[starter] Place a Linux x86_64 binary next to starter.js as:');
  console.error('[starter]   ./omnicore');
  console.error('[starter] or set OMNICORE_BIN to the full path.');
  console.error('[starter] Build with: cargo build --release --no-default-features');
  console.error('[starter] Or download the GitHub Actions artifact omnicore-linux-x86_64');
  process.exit(1);
}

if (!process.env.DISCORD_BOT_TOKEN && !process.env.TOKEN && !process.env.BOT_TOKEN) {
  console.error('[starter] Missing DISCORD_BOT_TOKEN in .env');
  process.exit(1);
}

console.log(`[starter] binary: ${binary}`);
console.log(`[starter] database: ${process.env.DATABASE_PATH}`);
console.log(`[starter] cwd: ${ROOT}`);

// ── supervisor ────────────────────────────────────────────────────────
let child = null;
let stopping = false;

function start() {
  console.log(`[starter] launching at ${new Date().toISOString()}`);
  child = spawn(binary, [], {
    cwd: ROOT,
    stdio: 'inherit',
    env: process.env,
  });

  child.on('error', (err) => {
    console.error('[starter] failed to spawn binary:', err.message);
    if (err.code === 'ENOENT') {
      console.error('[starter] binary path not found or not executable');
    }
    if (err.code === 'EACCES') {
      console.error('[starter] permission denied — chmod +x the binary');
    }
    process.exit(1);
  });

  child.on('exit', (code, signal) => {
    child = null;
    if (stopping) {
      process.exit(code ?? (signal ? 1 : 0));
      return;
    }
    if (code === RESTART_CODE) {
      console.log(`[starter] restart requested (exit ${RESTART_CODE}) — relaunching in ${RETRY_MS}ms`);
      setTimeout(start, RETRY_MS);
      return;
    }
    // Unexpected crash: retry a few times so a transient failure does not
    // leave the panel "online" with a dead process forever. After that, exit
    // so the panel can mark the server stopped / auto-restart if configured.
    const delay = CRASH_RETRY_MS;
    console.error(
      `[starter] process exited code=${code} signal=${signal} — retry in ${delay}ms`
    );
    setTimeout(start, delay);
  });
}

for (const sig of ['SIGINT', 'SIGTERM']) {
  process.on(sig, () => {
    stopping = true;
    console.log(`[starter] received ${sig}`);
    if (child) {
      try {
        child.kill(sig);
      } catch {
        process.exit(0);
      }
    } else {
      process.exit(0);
    }
  });
}

async function fetchGitHub(url, token, maxBytes, accept = 'application/vnd.github+json') {
  if (typeof fetch !== 'function') {
    throw new Error('Automatic updates require Node.js 18 or newer (built-in fetch).');
  }
  const headers = { 'User-Agent': 'OmniCore-updater', Accept: accept };
  if (token) headers.Authorization = `Bearer ${token}`;
  let response = await fetch(url, { headers, redirect: 'manual' });
  if ([301, 302, 303, 307, 308].includes(response.status)) {
    const location = response.headers.get('location');
    if (!location) throw new Error(`GitHub returned ${response.status} without a download location.`);
    // Private release downloads redirect to a signed URL; do not forward the token.
    response = await fetch(location, { headers: { 'User-Agent': 'OmniCore-updater' } });
  }
  if (!response.ok) throw new Error(`GitHub request failed (${response.status}).`);
  const declared = Number(response.headers.get('content-length') || 0);
  if (declared && declared > maxBytes) throw new Error('Release asset exceeds the configured size limit.');
  const data = Buffer.from(await response.arrayBuffer());
  if (data.length > maxBytes) throw new Error('Release asset exceeds the configured size limit.');
  return data;
}

async function installLatestRelease() {
  if (!AUTO_UPDATE) return false;
  if (!GITHUB_TOKEN) throw new Error('GITHUB_TOKEN is missing; automatic updates are disabled.');
  if (process.platform !== 'linux' || !['x64', 'arm64'].includes(process.arch)) {
    throw new Error(`No automatic release asset is configured for ${process.platform}/${process.arch}.`);
  }

  const [owner, repo] = GITHUB_REPOSITORY.split('/');
  if (!owner || !repo || GITHUB_REPOSITORY.split('/').length !== 2) {
    throw new Error('GITHUB_REPOSITORY must be owner/repository.');
  }
  const apiRoot = `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}`;
  const releaseData = await fetchGitHub(`${apiRoot}/releases/latest`, GITHUB_TOKEN, 2 * 1024 * 1024);
  const release = JSON.parse(releaseData.toString('utf8'));
  if (!release.tag_name || !Array.isArray(release.assets)) throw new Error('Latest GitHub release is malformed.');

  const binaryName = process.arch === 'x64' ? 'omnicore-linux-x86_64' : 'omnicore-linux-aarch64';
  const binaryAsset = release.assets.find((asset) => asset.name === binaryName);
  const checksumAsset = release.assets.find((asset) => asset.name === 'SHA256SUMS.txt');
  if (!binaryAsset || !checksumAsset) throw new Error('Latest release is missing its binary or checksum asset.');
  const assetApiPrefix = `${apiRoot}/releases/assets/`;
  if (!binaryAsset.url.startsWith(assetApiPrefix) || !checksumAsset.url.startsWith(assetApiPrefix)) {
    throw new Error('Latest release contains an unexpected asset URL.');
  }

  const versionPath = path.join(ROOT, 'omnicore.version');
  let installedVersion = '';
  try { installedVersion = fs.readFileSync(versionPath, 'utf8').trim(); } catch { /* first install */ }
  if (installedVersion === release.tag_name) return false;

  const checksums = await fetchGitHub(checksumAsset.url, GITHUB_TOKEN, 64 * 1024, 'application/octet-stream');
  const checksumLine = checksums.toString('utf8').split(/\r?\n/)
    .find((line) => line.trim().endsWith(` ${binaryName}`) || line.trim().endsWith(` *${binaryName}`));
  const expectedHash = checksumLine && checksumLine.trim().split(/\s+/)[0];
  if (!expectedHash || !/^[a-f0-9]{64}$/i.test(expectedHash)) throw new Error('Release checksum is missing or malformed.');

  const binaryData = await fetchGitHub(binaryAsset.url, GITHUB_TOKEN, 150 * 1024 * 1024, 'application/octet-stream');
  const actualHash = crypto.createHash('sha256').update(binaryData).digest('hex');
  if (actualHash.toLowerCase() !== expectedHash.toLowerCase()) throw new Error('Downloaded binary checksum does not match.');

  const resolvedBinary = path.resolve(binary);
  const resolvedRoot = path.resolve(ROOT) + path.sep;
  if (!resolvedBinary.startsWith(resolvedRoot)) {
    throw new Error('Refusing to replace a binary outside the starter.js directory.');
  }
  const tempPath = `${resolvedBinary}.update-${process.pid}`;
  const backupPath = `${resolvedBinary}.previous`;
  await fs.promises.mkdir(path.dirname(resolvedBinary), { recursive: true });
  await fs.promises.writeFile(tempPath, binaryData, { mode: 0o755 });
  await fs.promises.chmod(tempPath, 0o755);
  try {
    if (fs.existsSync(resolvedBinary)) {
      await fs.promises.rm(backupPath, { force: true });
      await fs.promises.rename(resolvedBinary, backupPath);
    }
    await fs.promises.rename(tempPath, resolvedBinary);
    await fs.promises.writeFile(versionPath, `${release.tag_name}\n`, { mode: 0o600 });
  } catch (error) {
    await fs.promises.rm(tempPath, { force: true }).catch(() => {});
    if (fs.existsSync(backupPath) && !fs.existsSync(resolvedBinary)) {
      await fs.promises.rename(backupPath, resolvedBinary).catch(() => {});
    }
    throw error;
  }
  console.log(`[starter] installed release ${release.tag_name}`);
  return true;
}

function backupEncryptionKey() {
  const raw = process.env.OMNICORE_BACKUP_KEY || '';
  let key;
  if (/^[a-f0-9]{64}$/i.test(raw)) key = Buffer.from(raw, 'hex');
  else key = Buffer.from(raw, 'base64');
  if (key.length !== 32) throw new Error('OMNICORE_BACKUP_KEY must encode exactly 32 bytes (64 hex characters or base64).');
  return key;
}

function githubApiRoot() {
  const [owner, repo] = GITHUB_REPOSITORY.split('/');
  if (!owner || !repo || GITHUB_REPOSITORY.split('/').length !== 2) {
    throw new Error('GITHUB_REPOSITORY must be owner/repository.');
  }
  return `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}`;
}

async function githubJson(url, token, method = 'GET', body) {
  const response = await fetch(url, {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28',
      'User-Agent': 'OmniCore-backup',
      ...(body ? { 'Content-Type': 'application/json' } : {}),
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  const text = await response.text();
  let data = {};
  try { data = text ? JSON.parse(text) : {}; } catch { /* report status below */ }
  if (!response.ok) {
    const error = new Error(`GitHub API ${method} failed (${response.status}).`);
    error.status = response.status;
    throw error;
  }
  return data;
}

async function ensureBackupRelease(token) {
  const root = githubApiRoot();
  const tag = 'database-backups';
  try {
    const existing = await githubJson(`${root}/releases/tags/${tag}`, token);
    if (!existing.prerelease) {
      await githubJson(`${root}/releases/${existing.id}`, token, 'PATCH', { prerelease: true });
      existing.prerelease = true;
    }
    return existing;
  } catch (error) {
    if (error.status !== 404) throw error;
  }
  return githubJson(`${root}/releases`, token, 'POST', {
    tag_name: tag,
    target_commitish: 'main',
    name: 'Encrypted database backups',
    body: 'Encrypted SQLite snapshots. The decryption key is managed outside GitHub.',
    draft: false,
    prerelease: true,
  });
}

async function downloadGitHubAssetToFile(url, token, destination, maxBytes = 2 * 1024 * 1024 * 1024) {
  const headers = {
    Authorization: `Bearer ${token}`,
    Accept: 'application/octet-stream',
    'User-Agent': 'OmniCore-backup',
  };
  let response = await fetch(url, { headers, redirect: 'manual' });
  if ([301, 302, 303, 307, 308].includes(response.status)) {
    const location = response.headers.get('location');
    if (!location) throw new Error('GitHub asset redirect has no location.');
    const redirected = new URL(location);
    if (redirected.protocol !== 'https:' || !(redirected.hostname === 'github.com' || redirected.hostname.endsWith('.githubusercontent.com'))) {
      throw new Error('Refusing an unexpected encrypted-backup download host.');
    }
    response = await fetch(redirected, { headers: { 'User-Agent': 'OmniCore-backup' } });
  }
  if (!response.ok || !response.body) throw new Error(`GitHub backup download failed (${response.status}).`);
  const declared = Number(response.headers.get('content-length') || 0);
  if (declared && declared > maxBytes) throw new Error('Encrypted backup exceeds the download size limit.');
  let total = 0;
  const limit = new Transform({
    transform(chunk, _encoding, callback) {
      total += chunk.length;
      if (total > maxBytes) callback(new Error('Encrypted backup exceeds the download size limit.'));
      else callback(null, chunk);
    },
  });
  await pipeline(
    Readable.fromWeb(response.body),
    limit,
    fs.createWriteStream(destination, { flags: 'wx', mode: 0o600 }),
  );
  return total;
}

function uploadReleaseAsset(uploadUrl, token, filePath, assetName) {
  return new Promise((resolve, reject) => {
    const url = new URL(uploadUrl);
    if (url.protocol !== 'https:' || url.hostname !== 'uploads.github.com') {
      reject(new Error('Refusing to upload backup to an unexpected host.'));
      return;
    }
    const stat = fs.statSync(filePath);
    url.searchParams.set('name', assetName);
    const request = https.request(url, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: 'application/vnd.github+json',
        'Content-Type': 'application/octet-stream',
        'Content-Length': stat.size,
        'User-Agent': 'OmniCore-backup',
        'X-GitHub-Api-Version': '2022-11-28',
      },
    }, (response) => {
      let text = '';
      response.setEncoding('utf8');
      response.on('data', (chunk) => { if (text.length < 8192) text += chunk; });
      response.on('end', () => {
        if (response.statusCode !== 201) {
          reject(new Error(`GitHub release asset upload failed (${response.statusCode}).`));
          return;
        }
        try { resolve(text ? JSON.parse(text) : {}); }
        catch { resolve({}); }
      });
    });
    request.on('error', reject);
    fs.createReadStream(filePath).on('error', reject).pipe(request);
  });
}

async function deleteOldBackupAssets(release, token) {
  const root = githubApiRoot();
  for (const asset of release.assets || []) {
    if (!/^omnicore-db-\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}Z\.sqlite\.gz\.enc$/.test(asset.name)) continue;
    const createdAt = Date.parse(asset.created_at || '');
    if (!Number.isFinite(createdAt) || Date.now() - createdAt <= BACKUP_RETENTION_MS) continue;
    await githubJson(`${root}/releases/assets/${asset.id}`, token, 'DELETE');
    console.log(`[starter] removed expired encrypted backup ${asset.name}`);
  }
}

async function uploadEncryptedBackup(filePath, token, release) {
  const name = path.basename(filePath);
  const current = await githubJson(`${githubApiRoot()}/releases/${release.id}`, token);
  for (const asset of current.assets || []) {
    if (asset.name === name) await githubJson(`${githubApiRoot()}/releases/assets/${asset.id}`, token, 'DELETE');
  }
  const uploadTemplate = release.upload_url || '';
  if (!uploadTemplate.startsWith('https://uploads.github.com/')) {
    throw new Error('Backup release returned an invalid upload URL.');
  }
  const uploadUrl = uploadTemplate.replace(/\{\?.*\}$/, '');
  await uploadReleaseAsset(uploadUrl, token, filePath, name);
  const updatedRelease = await githubJson(`${githubApiRoot()}/releases/${release.id}`, token);
  await deleteOldBackupAssets(updatedRelease, token);
}

async function encryptDatabaseSnapshot(snapshotPath, encryptedPath, key) {
  const iv = crypto.randomBytes(12);
  const magic = Buffer.from('OMNICORE-BACKUP-V1\n', 'ascii');
  const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
  await fs.promises.writeFile(encryptedPath, Buffer.concat([magic, iv]), { mode: 0o600, flag: 'wx' });
  await pipeline(
    fs.createReadStream(snapshotPath),
    zlib.createGzip(),
    cipher,
    fs.createWriteStream(encryptedPath, { flags: 'a', mode: 0o600 }),
  );
  await fs.promises.appendFile(encryptedPath, cipher.getAuthTag());
}

async function fileSha256(filePath) {
  const hash = crypto.createHash('sha256');
  for await (const chunk of fs.createReadStream(filePath)) hash.update(chunk);
  return hash.digest('hex');
}

async function decryptBackupToFile(encryptedPath, destinationPath, key) {
  const magic = Buffer.from('OMNICORE-BACKUP-V1\n', 'ascii');
  const stat = await fs.promises.stat(encryptedPath);
  const ciphertextOffset = magic.length + 12;
  if (stat.size <= ciphertextOffset + 16) throw new Error('Encrypted backup is truncated.');
  const handle = await fs.promises.open(encryptedPath, 'r');
  const prefix = Buffer.alloc(ciphertextOffset);
  const tag = Buffer.alloc(16);
  try {
    await handle.read(prefix, 0, prefix.length, 0);
    await handle.read(tag, 0, tag.length, stat.size - tag.length);
  } finally {
    await handle.close();
  }
  if (!prefix.subarray(0, magic.length).equals(magic)) throw new Error('Backup format is not recognized.');
  const decipher = crypto.createDecipheriv('aes-256-gcm', key, prefix.subarray(magic.length));
  decipher.setAuthTag(tag);
  await pipeline(
    fs.createReadStream(encryptedPath, { start: ciphertextOffset, end: stat.size - tag.length - 1 }),
    decipher,
    zlib.createGunzip(),
    fs.createWriteStream(destinationPath, { flags: 'wx', mode: 0o600 }),
  );
  const file = await fs.promises.open(destinationPath, 'r');
  try {
    const signature = Buffer.alloc(15);
    const { bytesRead } = await file.read(signature, 0, signature.length, 0);
    if (bytesRead !== signature.length || signature.toString('ascii') !== 'SQLite format 3') {
      throw new Error('Decrypted backup is not a SQLite database.');
    }
  } finally {
    await file.close();
  }
  await fs.promises.chmod(destinationPath, 0o600);
}

let backupUploadsBlocked = false;
async function restoreDatabaseIfMissing() {
  const databasePath = process.env.DATABASE_PATH || path.join(ROOT, 'data', 'bot.sqlite');
  if (!BACKUP_ENABLED || fs.existsSync(databasePath)) return false;
  const backupDir = path.join(path.dirname(databasePath), 'backups');
  const markerPath = path.join(backupDir, 'restore-failed.block');
  const encryptedPath = path.join(backupDir, `restore-${process.pid}.sqlite.gz.enc`);
  const restoredPath = path.join(backupDir, `restore-${process.pid}.sqlite`);
  await fs.promises.mkdir(backupDir, { recursive: true, mode: 0o700 });
  const blockUploads = async (reason) => {
    backupUploadsBlocked = true;
    await fs.promises.writeFile(markerPath, `${new Date().toISOString()} ${reason}\n`, { mode: 0o600 }).catch(() => {});
    console.error(`[starter] database restore failed; automatic uploads are blocked: ${reason}`);
  };

  const token = BACKUP_TOKEN || GITHUB_TOKEN;
  if (!token || !process.env.OMNICORE_BACKUP_KEY) {
    await blockUploads('backup credentials are missing');
    return false;
  }
  let release;
  try {
    release = await githubJson(`${githubApiRoot()}/releases/tags/database-backups`, token);
  } catch (error) {
    if (error.status === 404) {
      console.log('[starter] no GitHub database backup exists; starting with a new database');
      return false;
    }
    await blockUploads(error.message);
    return false;
  }

  const candidates = (release.assets || [])
    .filter((asset) => /^omnicore-db-\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}Z\.sqlite\.gz\.enc$/.test(asset.name))
    .sort((a, b) => Date.parse(b.created_at || 0) - Date.parse(a.created_at || 0));
  if (!candidates.length) {
    console.log('[starter] no encrypted SQLite backup asset exists; starting with a new database');
    return false;
  }

  try {
    const asset = candidates[0];
    if (!asset.url.startsWith(`${githubApiRoot()}/releases/assets/`)) {
      throw new Error('Backup release contains an unexpected asset URL.');
    }
    const bytes = await downloadGitHubAssetToFile(asset.url, token, encryptedPath);
    await decryptBackupToFile(encryptedPath, restoredPath, backupEncryptionKey());
    await fs.promises.mkdir(path.dirname(databasePath), { recursive: true });
    for (const suffix of ['-wal', '-shm']) await fs.promises.rm(`${databasePath}${suffix}`, { force: true }).catch(() => {});
    await fs.promises.rename(restoredPath, databasePath);
    const hash = await fileSha256(databasePath);
    const statePath = path.join(backupDir, 'github-backup-state.json');
    await fs.promises.writeFile(statePath, JSON.stringify({ hash, repo: GITHUB_REPOSITORY, at: new Date().toISOString() }), { mode: 0o600 });
    await fs.promises.rm(markerPath, { force: true });
    backupUploadsBlocked = false;
    console.log(`[starter] restored the database from encrypted GitHub backup (${(bytes / 1048576).toFixed(1)} MB)`);
    return true;
  } catch (error) {
    await blockUploads(error.message);
    return false;
  } finally {
    await fs.promises.rm(encryptedPath, { force: true }).catch(() => {});
    await fs.promises.rm(restoredPath, { force: true }).catch(() => {});
  }
}

let backupInProgress = false;
async function createAndUploadDatabaseBackup() {
  if (!BACKUP_ENABLED || backupInProgress) return;
  backupInProgress = true;
  const token = BACKUP_TOKEN || GITHUB_TOKEN;
  const databasePath = process.env.DATABASE_PATH || path.join(ROOT, 'data', 'bot.sqlite');
  const backupDir = path.join(path.dirname(databasePath), 'backups');
  const markerPath = path.join(backupDir, 'restore-failed.block');
  const statePath = path.join(backupDir, 'github-backup-state.json');
  const stamp = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z').replace(/:/g, '-');
  const snapshotPath = path.join(backupDir, `omnicore-db-${stamp}.sqlite`);
  const encryptedPath = `${snapshotPath}.gz.enc`;
  try {
    if (backupUploadsBlocked || fs.existsSync(markerPath)) {
      console.error('[starter] database upload blocked after a failed restore; verify data and remove data/backups/restore-failed.block only when safe');
      return;
    }
    if (!token) throw new Error('Set GITHUB_BACKUP_TOKEN (Contents read/write) to upload backups.');
    const key = backupEncryptionKey();
    if (!fs.existsSync(databasePath)) {
      console.log('[starter] database backup skipped; database file does not exist yet');
      return;
    }
    await fs.promises.mkdir(backupDir, { recursive: true, mode: 0o700 });
    await fs.promises.chmod(backupDir, 0o700).catch(() => {});
    await execFileAsync(binary, ['--snapshot-database', snapshotPath], {
      cwd: ROOT,
      env: process.env,
      timeout: 10 * 60 * 1000,
      maxBuffer: 1024 * 1024,
    });
    await fs.promises.chmod(snapshotPath, 0o600);
    const hash = await fileSha256(snapshotPath);
    const pending = (await fs.promises.readdir(backupDir))
      .filter((name) => /^omnicore-db-\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}Z\.sqlite\.gz\.enc$/.test(name))
      .sort();
    let priorState = {};
    try { priorState = JSON.parse(await fs.promises.readFile(statePath, 'utf8')); } catch { /* first backup */ }
    if (priorState.hash !== hash) {
      await encryptDatabaseSnapshot(snapshotPath, encryptedPath, key);
      pending.push(path.basename(encryptedPath));
    }
    await fs.promises.rm(snapshotPath, { force: true });
    if (!pending.length) {
      console.log('[starter] database unchanged since last backup; upload skipped');
      return;
    }
    const release = await ensureBackupRelease(token);
    for (const name of pending) {
      const pendingPath = path.join(backupDir, name);
      await uploadEncryptedBackup(pendingPath, token, release);
      await fs.promises.rm(pendingPath, { force: true });
      console.log(`[starter] uploaded encrypted SQLite backup ${name}`);
    }
    await fs.promises.writeFile(statePath, JSON.stringify({ hash, repo: GITHUB_REPOSITORY, at: new Date().toISOString() }), { mode: 0o600 });
  } catch (error) {
    console.error('[starter] encrypted database backup failed:', error.message);
    if (fs.existsSync(encryptedPath)) {
      console.error(`[starter] encrypted backup retained locally at ${encryptedPath} for retry`);
    }
  } finally {
    await fs.promises.rm(snapshotPath, { force: true }).catch(() => {});
    backupInProgress = false;
  }
}

function beginDatabaseBackups() {
  if (!BACKUP_ENABLED) {
    console.log('[starter] encrypted GitHub database backups are disabled');
    return;
  }
  if (!process.env.OMNICORE_BACKUP_KEY || !(BACKUP_TOKEN || GITHUB_TOKEN)) {
    console.error('[starter] backups enabled but OMNICORE_BACKUP_KEY or GitHub backup token is missing');
    return;
  }
  const initial = setTimeout(createAndUploadDatabaseBackup, 2 * 60 * 1000);
  initial.unref();
  const timer = setInterval(createAndUploadDatabaseBackup, BACKUP_INTERVAL_MS);
  timer.unref();
}

async function beginAutoUpdates() {
  if (!AUTO_UPDATE) {
    console.log('[starter] automatic release updates are disabled');
    await launchBot();
    return;
  }
  if (!GITHUB_TOKEN) {
    console.error('[starter] OMNICORE_AUTO_UPDATE=1 but GITHUB_TOKEN is missing; using the installed binary');
    await launchBot();
    return;
  }
  const updateAndRestart = async () => {
    if (stopping || updateInProgress) return;
    updateInProgress = true;
    try {
      const changed = await installLatestRelease();
      if (changed && child) {
        console.log('[starter] release updated; gracefully restarting the bot');
        child.kill('SIGTERM');
      }
    } catch (error) {
      console.error('[starter] release update failed:', error.message);
    } finally {
      updateInProgress = false;
    }
  };
  try {
    await installLatestRelease();
  } catch (error) {
    console.error('[starter] initial release check failed; using the installed binary:', error.message);
  }
  await launchBot();
  const timer = setInterval(updateAndRestart, UPDATE_INTERVAL_MS);
  timer.unref();
}

let updateInProgress = false;
async function launchBot() {
  if (BACKUP_ENABLED) await restoreDatabaseIfMissing();
  start();
  beginDatabaseBackups();
}
beginAutoUpdates();
