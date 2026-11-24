'use strict';

function __loadDb() {
  const tries = ['../../database/db','../database/db','../../../database/db','./database/db'];
  for (const p of tries) { try { return require(p).db; } catch (e) { if (e.code !== 'MODULE_NOT_FOUND') throw e; } }
  throw new Error('Cannot find database/db');
}
const db = __loadDb();


/**
 * Persistent settings for the Lookup feature (Player / Alliance / State panels).
 * Admin configures one channel per guild where the public search panels live.
 */


function ensureTable() {
  db.exec(`
    CREATE TABLE IF NOT EXISTS lookup_settings (
      guild_id TEXT PRIMARY KEY,
      channel_id TEXT,
      player_panel_msg_id TEXT,
      alliance_panel_msg_id TEXT,
      state_panel_msg_id TEXT,
      updated_at INTEGER DEFAULT (strftime('%s','now'))
    )
  `);
}

ensureTable();

function getSettings(guildId) {
  const row = db.prepare('SELECT * FROM lookup_settings WHERE guild_id = ?').get(String(guildId));
  return row || {
    guild_id: String(guildId),
    channel_id: null,
    player_panel_msg_id: null,
    alliance_panel_msg_id: null,
    state_panel_msg_id: null,
  };
}

function setChannel(guildId, channelId) {
  const existing = getSettings(guildId);
  db.prepare(`
    INSERT INTO lookup_settings (guild_id, channel_id, updated_at)
    VALUES (?, ?, strftime('%s','now'))
    ON CONFLICT(guild_id) DO UPDATE SET
      channel_id = excluded.channel_id,
      updated_at = excluded.updated_at
  `).run(String(guildId), channelId ? String(channelId) : null);
  return getSettings(guildId);
}

function setPanelMessage(guildId, type, messageId) {
  const col = {
    player: 'player_panel_msg_id',
    alliance: 'alliance_panel_msg_id',
    state: 'state_panel_msg_id',
  }[type];
  if (!col) return;
  db.prepare(`
    INSERT INTO lookup_settings (guild_id, ${col}, updated_at)
    VALUES (?, ?, strftime('%s','now'))
    ON CONFLICT(guild_id) DO UPDATE SET
      ${col} = excluded.${col},
      updated_at = excluded.updated_at
  `).run(String(guildId), messageId ? String(messageId) : null);
}

module.exports = {
  getSettings,
  setChannel,
  setPanelMessage,
};
