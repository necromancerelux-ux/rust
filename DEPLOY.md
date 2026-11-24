# OmniCore deploy

## Panel startup (HidenCloud / Node-only start command)

1. Upload into the same folder:
   - `omnicore` (Linux x86_64 binary, chmod +x)
   - `package.json` (launcher metadata; it declares no npm dependencies)
   - `starter.js`
   - `.env` (from `.env.example`)
   - `assets/` (fonts + emoji-icons)

2. Set the host **startup file / command** to:

```text
node starter.js
```

`starter.js` loads `.env`, runs the Rust binary, and auto-restarts on exit code 42 (Owner → Restart). The launcher uses Node built-in modules only; no `npm install` or third-party Node packages are needed. The panel can start it with `node starter.js` or `npm start`.

## Required .env

```env
DISCORD_BOT_TOKEN=...
OWNER_ID=...
```

## Automatic GitHub releases

The `Automatic Linux release` workflow publishes a private GitHub Release for
each successful push to `main`. Each release contains Linux x86_64 and ARM64
binaries, `.tar.gz` packages, and `SHA256SUMS.txt`. The release is created only
after both binary builds succeed. The host can download assets using a
fine-grained GitHub token with read-only Contents access; never put that token
in the repo.

To enable automatic installation on a Linux x86_64 or ARM64 Node panel, set these
variables in the host's environment (or its untracked `.env` file):

```env
OMNICORE_AUTO_UPDATE=1
GITHUB_TOKEN=<fine-grained token with read-only Contents access>
GITHUB_REPOSITORY=necromancerelux-ux/OmniCore_Rust
OMNICORE_UPDATE_INTERVAL_MINUTES=5
```

`starter.js` detects x86_64 versus ARM64, checks the latest private release before starting, then polls at
the configured interval. It downloads the binary and checksum over HTTPS,
verifies SHA-256, atomically replaces the local binary, and gracefully restarts
the bot. Keep `starter.js`, `.env`, and `assets/` on the host; the updater only
replaces the binary beside `starter.js`. Automatic updates are disabled unless
`OMNICORE_AUTO_UPDATE=1` is set. The updater requires Node.js 18+ and a writable
startup directory.

To enable encrypted database backups, add these host-only settings:

```env
OMNICORE_BACKUP_ENABLED=1
GITHUB_BACKUP_TOKEN=<fine-grained token with Contents read/write>
OMNICORE_BACKUP_KEY=<32-byte key as 64 hex characters or base64>
OMNICORE_BACKUP_INTERVAL_HOURS=6
OMNICORE_BACKUP_RETENTION_DAYS=14
```

The helper invokes the binary's `--snapshot-database` mode, which uses SQLite
`VACUUM INTO` to capture a consistent snapshot including committed WAL data.
By default it checks every 6 hours, hashes the snapshot, and skips upload if
the database has not changed. It compresses and encrypts changed snapshots with
AES-256-GCM before upload. Backups are stored as assets on a separate prerelease
named `database-backups`, so they do not interfere with the bot's latest
release. Old backup assets are removed after the configured retention period;
failed uploads leave only encrypted files under `data/backups/` for retry. No
plaintext database is uploaded.

If the local database file is missing, `starter.js` checks the backup prerelease
and restores the newest encrypted snapshot before starting the bot. If a
restore fails, uploads are blocked and a `restore-failed.block` marker is left
beside the database so a fresh/empty database cannot overwrite the remote
backup. Verify the restored data before removing that marker manually.

Keep the encryption key outside GitHub and retain a separate secure copy: the
backup cannot be restored without it. The backup format is the ASCII header
`OMNICORE-BACKUP-V1\n`, followed by a 12-byte IV, AES-GCM ciphertext of the
gzip-compressed SQLite snapshot, and the 16-byte authentication tag. The GitHub
backup token needs write permission to create and prune release assets; keep it
only in the host environment. Backups are disabled unless explicitly enabled.

To restore an asset manually, download it from the private `database-backups`
prerelease, set `OMNICORE_BACKUP_KEY` in the local environment, and run:

```bash
node restore-backup.js omnicore-db-YYYY-MM-DDTHH-MM-SSZ.sqlite.gz.enc /safe/path/restored.sqlite
```

The restore utility refuses to overwrite an existing destination and removes a
partial output if authentication/decompression fails. Verify the restored copy
before pointing the bot at it.

## Build the binary locally (on any PC with Rust 1.85+)

```bash
cargo build --release --no-default-features
# → target/release/omnicore
```

Or download the latest private release asset from the GitHub Releases page.

## Docker

```bash
docker compose up -d --build
```
