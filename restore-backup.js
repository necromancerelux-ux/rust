'use strict';

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const zlib = require('zlib');
const { pipeline } = require('stream/promises');

const MAGIC = Buffer.from('OMNICORE-BACKUP-V1\n', 'ascii');

function keyFromEnvironment() {
  const raw = process.env.OMNICORE_BACKUP_KEY || '';
  const key = /^[a-f0-9]{64}$/i.test(raw) ? Buffer.from(raw, 'hex') : Buffer.from(raw, 'base64');
  if (key.length !== 32) throw new Error('OMNICORE_BACKUP_KEY must encode exactly 32 bytes.');
  return key;
}

async function main() {
  const [encryptedPath, destinationPath] = process.argv.slice(2);
  if (!encryptedPath || !destinationPath) {
    throw new Error('Usage: node restore-backup.js <encrypted-backup> <new-sqlite-path>');
  }
  const source = path.resolve(encryptedPath);
  const destination = path.resolve(destinationPath);
  if (fs.existsSync(destination)) throw new Error('Refusing to overwrite the destination database.');

  const stat = await fs.promises.stat(source);
  const ivOffset = MAGIC.length;
  const ciphertextOffset = ivOffset + 12;
  if (stat.size <= ciphertextOffset + 16) throw new Error('Encrypted backup is truncated.');
  const handle = await fs.promises.open(source, 'r');
  const prefix = Buffer.alloc(ciphertextOffset);
  const tag = Buffer.alloc(16);
  try {
    await handle.read(prefix, 0, prefix.length, 0);
    await handle.read(tag, 0, tag.length, stat.size - tag.length);
  } finally {
    await handle.close();
  }
  if (!prefix.subarray(0, MAGIC.length).equals(MAGIC)) throw new Error('Backup format is not recognized.');

  const decipher = crypto.createDecipheriv('aes-256-gcm', keyFromEnvironment(), prefix.subarray(ivOffset));
  decipher.setAuthTag(tag);
  await fs.promises.mkdir(path.dirname(destination), { recursive: true });
  try {
    await pipeline(
      fs.createReadStream(source, { start: ciphertextOffset, end: stat.size - tag.length - 1 }),
      decipher,
      zlib.createGunzip(),
      fs.createWriteStream(destination, { flags: 'wx', mode: 0o600 }),
    );
    await fs.promises.chmod(destination, 0o600);
    console.log(`Restored SQLite snapshot to ${destination}`);
  } catch (error) {
    await fs.promises.rm(destination, { force: true }).catch(() => {});
    throw error;
  }
}

main().catch((error) => {
  console.error('Backup restore failed:', error.message);
  process.exitCode = 1;
});
