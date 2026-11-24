'use strict';

function __loadDb() {
  const tries = ['../../database/db','../database/db','../../../database/db','./database/db'];
  for (const p of tries) { try { return require(p).db; } catch (e) { if (e.code !== 'MODULE_NOT_FOUND') throw e; } }
  throw new Error('Cannot find database/db');
}
const db = __loadDb();


/**
 * Persistent storage for Whiteout Stats API key (bot-owner only).
 * Single global row — one key for the whole bot.
 */


function ensureTable() {
  db.exec(`
    CREATE TABLE IF NOT EXISTS wos_stats_credentials (
      id INTEGER PRIMARY KEY CHECK (id = 1),
      api_key TEXT NOT NULL,
      updated_at INTEGER NOT NULL,
      updated_by TEXT,
      last_test_at INTEGER,
      last_test_ok INTEGER,
      last_test_message TEXT
    )
  `);
}

ensureTable();

function get() {
  ensureTable();
  return db.prepare('SELECT * FROM wos_stats_credentials WHERE id = 1').get() || null;
}

function isConfigured() {
  const row = get();
  return Boolean(row && row.api_key);
}

/**
 * @param {{ apiKey: string, updatedBy?: string|null }} p
 */
function set({ apiKey, updatedBy = null }) {
  ensureTable();
  const now = Date.now();
  db.prepare(`
    INSERT INTO wos_stats_credentials (id, api_key, updated_at, updated_by)
    VALUES (1, @api_key, @updated_at, @updated_by)
    ON CONFLICT(id) DO UPDATE SET
      api_key = excluded.api_key,
      updated_at = excluded.updated_at,
      updated_by = excluded.updated_by
  `).run({
    api_key: String(apiKey).trim(),
    updated_at: now,
    updated_by: updatedBy ? String(updatedBy) : null,
  });
  return get();
}

function clear() {
  ensureTable();
  db.prepare('DELETE FROM wos_stats_credentials WHERE id = 1').run();
}

function recordTest({ ok, message }) {
  ensureTable();
  const row = get();
  if (!row) return null;
  db.prepare(`
    UPDATE wos_stats_credentials
    SET last_test_at = ?, last_test_ok = ?, last_test_message = ?
    WHERE id = 1
  `).run(Date.now(), ok ? 1 : 0, message ? String(message).slice(0, 500) : null);
  return get();
}

/**
 * Parse pasted API key from:
 * - bare key: wos_xxxx
 * - "Authorization: Bearer wos_xxxx"
 * - curl -H "Authorization: Bearer wos_xxxx"
 */
function parseApiKeyPaste(raw) {
  if (!raw || typeof raw !== 'string') return null;
  const text = raw.trim();

  // Bearer token
  const bearer =
    text.match(/Authorization:\s*Bearer\s+(wos_[A-Za-z0-9_\-]+)/i) ||
    text.match(/Bearer\s+(wos_[A-Za-z0-9_\-]+)/i) ||
    text.match(/-H\s+['"]Authorization:\s*Bearer\s+(wos_[A-Za-z0-9_\-]+)['"]/i);
  if (bearer) return bearer[1].trim();

  // X-Api-Key header
  const xKey =
    text.match(/X-Api-Key:\s*(wos_[A-Za-z0-9_\-]+)/i) ||
    text.match(/-H\s+['"]X-Api-Key:\s*(wos_[A-Za-z0-9_\-]+)['"]/i);
  if (xKey) return xKey[1].trim();

  // Bare key
  const bare = text.match(/^(wos_[A-Za-z0-9_\-]+)$/i);
  if (bare) return bare[1].trim();

  // Any wos_ token in the paste
  const any = text.match(/(wos_[A-Za-z0-9_\-]{10,})/i);
  if (any) return any[1].trim();

  return null;
}

module.exports = {
  ensureTable,
  get,
  isConfigured,
  set,
  clear,
  recordTest,
  parseApiKeyPaste,
};
