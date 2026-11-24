'use strict';

/**
 * Unified player / alliance / state data client.
 *
 * Primary: public whiteoutstats.com website API  (NO key required)
 * Optional: official API key for higher limits / Frosty features
 * Silent fallback: existing Oracle client
 * Never expose which source answered.
 *
 * Player hybrid (see getPlayer):
 *   - Arena heroes + hero gear / images → whiteoutstats
 *   - Chief gear + charms               → WoS Oracle (when available)
 */

const fetch = require('node-fetch');
const { normalizeTroopLevels } = require('./troopLevels');
function __load_wosCredentials() {
  const tries = [
    '../database/wosCredentials',
    '../lookup/database/wosCredentials',
    './database/wosCredentials',
    '../../lookup/database/wosCredentials',
  ];
  for (const p of tries) { try { return require(p); } catch (e) { if (e.code !== 'MODULE_NOT_FOUND') throw e; } }
  throw new Error('Cannot find wosCredentials. Tried: ' + tries.join(', '));
}
const wosCredentials = __load_wosCredentials();

const SITE_BASE = (process.env.WHITEOUTSTATS_SITE_URL || 'https://www.whiteoutstats.com').replace(/\/$/, '');
const SITE_API = `${SITE_BASE}/api`;
const OFFICIAL_API = (process.env.WHITEOUTSTATS_BASE_URL || 'https://api.whiteoutstats.com/v1').replace(/\/$/, '');

// The public site sits behind Cloudflare; a browser-like UA/locale keeps the
// plain JSON endpoints from being answered with a JS challenge page.
const SITE_UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36';

let oracleClient = null;
function getOracle() {
  if (!oracleClient) {
    try {
      oracleClient = require('../../giftcode/services/oracleClient');
    } catch {
      oracleClient = null;
    }
  }
  return oracleClient;
}

function resolveWosKey() {
  try {
    const row = wosCredentials.get();
    if (row && row.api_key) return row.api_key;
  } catch { /* ignore */ }
  return process.env.WHITEOUTSTATS_API_KEY || process.env.WOS_STATS_KEY || '';
}

function isWosConfigured() {
  return true; // public site always available
}

function isOfficialKeyConnected() {
  return Boolean(resolveWosKey());
}

async function siteFetch(path, options = {}) {
  const url = `${SITE_API}${path.startsWith('/') ? path : `/${path}`}`;
  try {
    const res = await fetch(url, {
      ...options,
      headers: {
        Accept: 'application/json',
        'User-Agent': SITE_UA,
        'Accept-Language': 'en-US,en;q=0.9',
        Referer: `${SITE_BASE}/`,
        ...(options.headers || {}),
      },
      timeout: options.timeout || 15000,
      size: 50 * 1024 * 1024, // refuse absurdly large bodies instead of buffering them
    });
    if (!res.ok) return null;
    const data = await res.json();
    if (data && data.detail && !data.ok && !data.uid && !data.fid && !data.player) return null;
    return data;
  } catch {
    return null;
  }
}

/**
 * Like siteFetch, but keeps HTTP status + error body instead of hiding them.
 * Used by the map-update endpoints, which answer 4xx with a JSON reason
 * (`detail`, `error: "cooldown" | "ip_cooldown"`) we want to show the user.
 */
async function siteJson(path, options = {}) {
  const url = `${SITE_API}${path.startsWith('/') ? path : `/${path}`}`;
  try {
    const res = await fetch(url, {
      ...options,
      headers: {
        Accept: 'application/json',
        'User-Agent': SITE_UA,
        'Accept-Language': 'en-US,en;q=0.9',
        Referer: `${SITE_BASE}/map`,
        ...(options.headers || {}),
      },
      timeout: options.timeout || 20000,
      size: 50 * 1024 * 1024,
    });
    let data = null;
    try { data = await res.json(); } catch { data = null; }
    return { ok: res.ok, status: res.status, data };
  } catch (e) {
    return { ok: false, status: 0, data: null, error: e && e.message };
  }
}

async function officialFetch(path, options = {}) {
  const key = resolveWosKey();
  if (!key) return null;
  const url = `${OFFICIAL_API}${path.startsWith('/') ? path : `/${path}`}`;
  try {
    const res = await fetch(url, {
      ...options,
      headers: {
        Authorization: `Bearer ${key}`,
        'X-Api-Key': key,
        Accept: 'application/json',
        ...(options.headers || {}),
      },
      timeout: 15000,
      size: 50 * 1024 * 1024,
    });
    if (!res.ok) return null;
    const data = await res.json();
    if (data && data.ok === false) return null;
    return data;
  } catch {
    return null;
  }
}

async function wosFetch(path, options = {}) {
  const site = await siteFetch(path, options);
  if (site) return site;
  return officialFetch(path, options);
}

async function testWosConnection() {
  try {
    const data = await siteFetch('/leaderboards/home');
    if (data && (data.ok || data.players)) {
      return { ok: true, message: 'Connected (public site · no key needed)' };
    }
  } catch { /* fall through */ }

  const key = resolveWosKey();
  if (!key) {
    return { ok: false, message: 'Public site unreachable and no API key stored.' };
  }
  try {
    const res = await fetch(`${OFFICIAL_API}/usage`, {
      headers: {
        Authorization: `Bearer ${key}`,
        'X-Api-Key': key,
        Accept: 'application/json',
      },
      timeout: 10000,
    });
    if (!res.ok) {
      const text = await res.text().catch(() => '');
      return { ok: false, message: `HTTP ${res.status}${text ? `: ${text.slice(0, 120)}` : ''}` };
    }
    const data = await res.json().catch(() => ({}));
    const remaining = data?.daily?.remaining ?? data?.minute?.remaining;
    return {
      ok: true,
      message: remaining != null ? `Official key OK · ${remaining} remaining today` : 'Official key OK',
      data,
    };
  } catch (e) {
    return { ok: false, message: e.message || 'Connection failed' };
  }
}

function absUrl(pathOrUrl) {
  if (!pathOrUrl) return null;
  if (/^https?:\/\//i.test(pathOrUrl)) return pathOrUrl;
  if (pathOrUrl.startsWith('/')) return `${SITE_BASE}${pathOrUrl}`;
  return `${SITE_BASE}/${pathOrUrl}`;
}

function safeJson(str) {
  if (str == null || str === '') return null;
  if (typeof str !== 'string') return str;
  try { return JSON.parse(str); } catch { return null; }
}

/** First value that is neither null/undefined/'' nor an empty array/object. */
function firstNonEmpty(...values) {
  for (const v of values) {
    if (v == null || v === '') continue;
    if (Array.isArray(v)) { if (v.length) return v; continue; }
    if (typeof v === 'object') { if (Object.keys(v).length) return v; continue; }
    return v;
  }
  return null;
}

/** Normalize the public `expedition_troops` array into per-kind entries. */
function normalizeTroops(p) {
  const raw = Array.isArray(p) ? p : (p && Array.isArray(p.expedition_troops) ? p.expedition_troops : []);
  const best = new Map();
  const label = { inf: 'Infantry', lan: 'Lancer', mm: 'Marksman' };
  for (const t of raw) {
    if (!t || t.tier == null) continue;
    const key = String(t.kind || '').toLowerCase();
    const tier = Number(t.tier);
    const star = Number(t.star) || 0;
    const prev = best.get(key);
    if (prev && prev.tier >= tier) continue;
    best.set(key, { kind: key, tier, star, fc: star, count: Number(t.count) || 0 });
  }
  const out = [];
  for (const [k, e] of best) out.push({ kind: k, name: label[k] || k, tier: e.tier, star: e.star, fc: e.fc, count: e.count });
  return out;
}

function normalizeSitePlayer(p) {
  if (!p) return null;

  // Prefer parsed objects over JSON strings when both exist. `[]` is truthy,
  // so plain `a || b` would wrongly pick an empty array over a populated one.
  const heroes = firstNonEmpty(
    safeJson(p.heroes), safeJson(p.arena_heroes), safeJson(p.arena_heroes_json)
  );
  const chiefGear = firstNonEmpty(
    safeJson(p.lords_gear), safeJson(p.lords_equipments), safeJson(p.gear),
    safeJson(p.lords_equipments_json)
  );
  const gearPrivacy = !!(p.lords_gear_privacy || p.chief_gear_privacy);
  const gearPrivacyMessage =
    p.lords_gear_message ||
    p.chief_gear_message ||
    (gearPrivacy ? 'Chief Gear privacy is active — this chief is hiding their equipment.' : null);
  const trails = firstNonEmpty(
    safeJson(p.trails_list), safeJson(p.trails), safeJson(p.trails_json)
  );
  const ranks = firstNonEmpty(
    safeJson(p.leaderboards), safeJson(p.ranks), safeJson(p.ranks_json)
  );
  const glory = firstNonEmpty(
    safeJson(p.glory), safeJson(p.glory_json)
  );
  const allianceObj = p.alliance && typeof p.alliance === 'object' ? p.alliance : null;

  return {
    chiefId: String(p.fid || p.chief_id || ''),
    uid: p.uid != null ? String(p.uid) : null,
    name: p.nick_name || p.name || 'Unknown',
    avatarUrl: absUrl(p.avatar_url) || (p.upload_image
      ? (String(p.upload_image).startsWith('http') ? String(p.upload_image)
        : 'https://gof-formal-avatar.akamaized.net/avatar/' + String(p.upload_image).replace(/^\//, ''))
      : null),
    stateId: p.kid != null ? Number(p.kid) : null,
    power: p.power != null ? Number(p.power) : null,
    kills: p.kills != null ? Number(p.kills) : null,
    furnace: p.stove_lv != null ? Number(p.stove_lv) : (p.furnace_level != null ? Number(p.furnace_level) : null),
    townCenter: p.town_center_level != null ? Number(p.town_center_level) : null,
    x: p.x != null ? Number(p.x) : null,
    y: p.y != null ? Number(p.y) : null,
    vip: p.vip != null ? Number(p.vip) : (p.vip_level != null ? Number(p.vip_level) : null),
    vipActive: p.is_vip_active != null ? Boolean(p.is_vip_active) : null,
    online: p.online != null ? Boolean(p.online) : null,
    lastLogin: p.last_login || null,
    inactive90d: p.inactive_90d != null ? Boolean(p.inactive_90d) : null,
    language: p.language || null,
    office: p.office != null ? Number(p.office) : null,
    officeLabel: p.office_label || p.appoint_label || null,
    tgLevel: p.tg_level != null ? Number(p.tg_level) : (p.tg != null ? Number(p.tg) : null),
    tgLabel: p.tg_label || null,
    levelLabel: p.level_label || null,
    migrantScore: p.migrant_score != null ? Number(p.migrant_score) : null,
    migrantRank: p.migrant_rank != null ? Number(p.migrant_rank) : null,
    powerRank: p.power_rank != null ? Number(p.power_rank) : null,
    killsRank: p.kills_rank != null ? Number(p.kills_rank) : null,
    stoveRank: p.stove_rank != null ? Number(p.stove_rank) : null,
    mysticRank: p.mystic_rank != null ? Number(p.mystic_rank) : null,
    mysticTrial: p.mystic_trial != null ? Number(p.mystic_trial) : null,
    shield: {
      active: p.shield_active != null ? Boolean(p.shield_active) : null,
      remaining: p.shield_remaining,
      label: p.shield_label || null,
      endtime: p.shield_endtime,
    },
    burn: {
      active: p.burn_active != null ? Boolean(p.burn_active) : null,
      remaining: p.burn_remaining,
      label: p.burn_label || null,
      endtime: p.burn_endtime,
    },
    alliance: {
      id: (allianceObj?.aid != null ? String(allianceObj.aid) : null) || (p.aid != null ? String(p.aid) : null),
      tag: allianceObj?.abbr || p.alliance_abbr || p.abbr || null,
      name: allianceObj?.name || p.alliance_name || null,
      rank: p.alliance_rank != null ? Number(p.alliance_rank) : (allianceObj?.rank != null ? Number(allianceObj.rank) : null),
      rankLabel: p.alliance_rank_label || allianceObj?.rank_label || null,
      power: allianceObj?.power != null ? Number(allianceObj.power) : null,
      members: p.alliance_member_count != null ? Number(p.alliance_member_count) : (allianceObj?.count != null ? Number(allianceObj.count) : null),
      leader: allianceObj?.leader_name || null,
      flagUrl: absUrl(p.alliance_flag_url || allianceObj?.flag_url) || null,
    },
    ranks,
    heroes,
    chiefGear,
    troop: normalizeTroops(p),
    gearPrivacy,
    gearPrivacyMessage,
    trails,
    glory,
    skins: p.skins || p.equipedskin || null,
    citySkin: p.city_skin || null,
    offices: p.offices || null,
    battleRecord: p.battle_record || null,
    crowns: glory?.glories ? { count: Object.keys(glory.glories || {}).length, raw: glory } : null,
    troopLevels: normalizeTroopLevels(p),
    updatedAt: {
      map: p.map_updated_at,
      stats: p.stats_updated_at,
      detail: p.detail_updated_at,
      arena: p.arena_updated_at,
      lastSeen: p.last_seen_at || p.last_active_at,
      firstSeen: p.first_seen_at,
    },
    raw: p,
  };
}

function normalizeOfficialPlayer(raw) {
  if (!raw || !raw.player) return null;
  const p = raw.player;
  const alliance = p.alliance || {};
  return {
    chiefId: String(p.chief_id || p.fid || ''),
    uid: p.uid != null ? String(p.uid) : null,
    name: p.nick_name || 'Unknown',
    avatarUrl: p.avatar_url || null,
    stateId: p.kid != null ? Number(p.kid) : null,
    power: p.power != null ? Number(p.power) : null,
    kills: p.kills != null ? Number(p.kills) : null,
    furnace: p.furnace_level != null ? Number(p.furnace_level) : null,
    x: p.x != null ? Number(p.x) : null,
    y: p.y != null ? Number(p.y) : null,
    vip: p.vip != null ? Number(p.vip) : null,
    alliance: {
      id: alliance.aid != null ? String(alliance.aid) : null,
      tag: alliance.abbr || null,
      name: alliance.name || null,
      rank: alliance.rank != null ? Number(alliance.rank) : null,
      rankLabel: alliance.rank_label || null,
      power: alliance.power != null ? Number(alliance.power) : null,
      members: alliance.count != null ? Number(alliance.count) : null,
      leader: alliance.leader_name || null,
      flagUrl: alliance.flag_url || null,
    },
    ranks: raw.ranks || null,
    heroes: raw.heroes || null,
    chiefGear: raw.chief_gear || null,
    crowns: null,
    troopLevels: normalizeTroopLevels(raw.player || raw),
    raw,
  };
}

function normalizeOraclePlayer(raw) {
  if (!raw) return null;
  const p = raw.player || raw;
  const troopSrc = firstNonEmpty(
    raw.expedition_troops, raw.troops, raw.troop_levels, raw.troopLevels,
    p.expedition_troops, p.troops, p.troop_levels, p.troopLevels
  );
  return {
    chiefId: String(p.fid || p.chief_id || p.id || ''),
    uid: p.uid != null ? String(p.uid) : null,
    name: p.nick || p.nick_name || p.name || 'Unknown',
    avatarUrl: p.avatar || p.avatar_url || null,
    stateId: p.kid != null ? Number(p.kid) : (p.state != null ? Number(p.state) : null),
    power: p.power != null ? Number(p.power) : null,
    kills: p.kills != null ? Number(p.kills) : (p.kill_count != null ? Number(p.kill_count) : null),
    furnace: p.stove_lv != null ? Number(p.stove_lv) : (p.furnace_level != null ? Number(p.furnace_level) : null),
    x: p.x != null ? Number(p.x) : null,
    y: p.y != null ? Number(p.y) : null,
    vip: p.vip != null ? Number(p.vip) : null,
    alliance: {
      id: p.aid != null ? String(p.aid) : null,
      tag: p.alliance_tag || p.abbr || null,
      name: p.alliance_name || p.alliance || null,
      rank: p.alliance_rank != null ? Number(p.alliance_rank) : null,
      rankLabel: null,
      power: null,
      members: null,
      leader: null,
      flagUrl: null,
    },
    ranks: raw.ranks || raw.standings || null,
    heroes: raw.heroes || raw.arena || null,
    chiefGear: raw.chief_gear || raw.gear || null,
    crowns: raw.crowns || null,
    troop: normalizeTroops(Array.isArray(troopSrc) ? troopSrc : (troopSrc || raw)),
    troopLevels: normalizeTroopLevels({ expedition_troops: Array.isArray(troopSrc) ? troopSrc : undefined, troop_levels: troopSrc && !Array.isArray(troopSrc) ? troopSrc : undefined, ...raw, ...p }),
    raw,
  };
}

/**
 * Hybrid player fetch (sources never shown to users):
 * - Whiteout Stats (public site / official key): primary for base profile +
 *   arena heroes, their gear details, and images.
 * - WoS Oracle: preferred for chief gear and charms (and silent full fallback).
 */
function hasChiefGearPayload(gear) {
  if (!gear) return false;
  if (Array.isArray(gear)) return gear.length > 0;
  if (typeof gear === 'object') {
    if (Array.isArray(gear.pieces) && gear.pieces.length) return true;
    if (Array.isArray(gear.slots) && gear.slots.length) return true;
    if (Array.isArray(gear.equipment) && gear.equipment.length) return true;
    if (Array.isArray(gear.charms) && gear.charms.length) return true;
    if (gear.weapon || gear.helmet || gear.gloves || gear.chest || gear.boots) return true;
    return Object.keys(gear).length > 0;
  }
  return false;
}

/**
 * Prefer Oracle /loadout for chief gear + charms.
 * Returns { pieces: [...], loadout, profile } when possible.
 */
function extractOracleChiefGear(raw, loadout) {
  // Prefer dedicated loadout payload (has lord_gear.pieces + gems/charms)
  let pieces = null;
  try {
    const cardRender = require('../../giftcode/services/lookupCardRender');
    if (typeof cardRender.findLordGearPieces === 'function') {
      pieces = cardRender.findLordGearPieces(loadout || raw);
      if ((!pieces || !pieces.length) && raw) {
        pieces = cardRender.findLordGearPieces(raw);
      }
    }
  } catch { /* optional */ }

  if (Array.isArray(pieces) && pieces.length) {
    return { pieces, loadout: loadout || null };
  }

  if (!raw && !loadout) return null;
  const src = loadout || raw;
  const p = src.player || src;
  const gear =
    src.chief_gear ||
    src.lord_gear ||
    src.lords_gear ||
    src.gear ||
    p.chief_gear ||
    p.lord_gear ||
    p.lords_gear ||
    null;
  if (!gear) return null;
  if (Array.isArray(gear)) return { pieces: gear, loadout: loadout || null };
  if (Array.isArray(gear.pieces)) return { pieces: gear.pieces, loadout: loadout || null };
  return { ...gear, loadout: loadout || null };
}

async function fetchOracleRaw(idStr) {
  const oracle = getOracle();
  if (!oracle) return { profile: null, loadout: null };
  let profile = null;
  let loadout = null;
  try {
    // Loadout is the important call for chief gear + charms
    if (typeof oracle.fetchPlayerLoadout === 'function') {
      const res = await oracle.fetchPlayerLoadout(idStr);
      if (res && res.ok !== false) {
        loadout = res.loadout || res.data || res;
      }
    }
  } catch { /* swallow */ }
  try {
    if (typeof oracle.getPlayer === 'function') {
      profile = await oracle.getPlayer(idStr);
    } else if (typeof oracle.fetchPlayer === 'function') {
      const res = await oracle.fetchPlayer(idStr);
      profile = res?.data || res?.player || res;
    }
  } catch { /* swallow */ }
  return { profile, loadout };
}

/** Public site name/ID search. Returns the raw result rows (max ~40). */
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function searchPlayers(query) {
  const q = String(query == null ? '' : query).trim();
  if (!q) return [];
  const d = await siteFetch(`/search?q=${encodeURIComponent(q)}`);
  return d && Array.isArray(d.results) ? d.results : [];
}

/**
 * Ask the public site to (re)build a player's profile. Needed because the
 * expedition/troop system is populated lazily and may be missing until refreshed.
 * Tries POST /players/{id}/refresh then GET /players/{id}?refresh=1.
 */
async function triggerPlayerSiteRefresh(id) {
  const pid = String(id == null ? '' : id).trim();
  if (!pid) return false;
  try {
    const res = await siteJson(`/players/${encodeURIComponent(pid)}/refresh`, { method: 'POST', timeout: 20000 });
    if (res && res.ok) return true;
  } catch { /* ignore */ }
  try {
    const res = await siteJson(`/players/${encodeURIComponent(pid)}?refresh=1`, { timeout: 20000 });
    if (res && res.ok) return true;
  } catch { /* ignore */ }
  return false;
}

/**
 * After triggering a refresh, poll profile fetches until troop data appears.
 * @param {string[]} candidateIds - uid/fid ids to try fetching
 * @param {{ attempts?: number, delayMs?: number }} [opts]
 * @returns {object|null} normalized site player with troopLevels, or best seen
 */
async function fetchPlayerWithTroopRefresh(candidateIds, opts = {}) {
  const ids = [...new Set((candidateIds || []).filter(Boolean).map(String))];
  if (!ids.length) return null;
  const attempts = Math.max(1, Number(opts.attempts) || 4);
  const delayMs = Math.max(500, Number(opts.delayMs) || 2500);

  let best = null;
  const fetchOnce = async () => {
    for (const tid of ids) {
      try {
        const direct = await siteFetch(`/players/${encodeURIComponent(tid)}`, { timeout: 20000 });
        if (direct && (direct.uid || direct.fid)) {
          const n = normalizeSitePlayer(direct);
          if (n) {
            if (!best) best = n;
            else if (n.troopLevels && !best.troopLevels) best = n;
            else if (n.troopLevels && best.troopLevels) {
              // prefer richer troop payload
              const as = Array.isArray(n.troop) ? n.troop.length : 0;
              const bs = Array.isArray(best.troop) ? best.troop.length : 0;
              if (as >= bs) best = n;
            }
            if (best && best.troopLevels) return best;
          }
        }
      } catch { /* ignore */ }
    }
    return best;
  };

  // initial fetch
  await fetchOnce();
  if (best && best.troopLevels) return best;

  // trigger refresh on all plausible ids (uid-like first if present)
  for (const tid of ids) {
    await triggerPlayerSiteRefresh(tid);
  }

  for (let i = 0; i < attempts; i++) {
    await sleep(delayMs);
    await fetchOnce();
    if (best && best.troopLevels) return best;
  }
  return best;
}

async function getPlayer(id, options = {}) {
  const idStr = String(id == null ? '' : id).trim();
  if (!idStr) return null;
  const wantTroops = options.wantTroops === true;

  const scoreProfile = (p) => {
    if (!p) return -1;
    let s = 0;
    if (p.troopLevels) s += 100;
    if (Array.isArray(p.troop) && p.troop.length) s += 20;
    if (p.chiefId || p.uid) s += 5;
    if (hasChiefGearPayload(p.chiefGear)) s += 2;
    if (p.heroes) s += 1;
    return s;
  };
  const pickBetter = (a, b) => (scoreProfile(b) > scoreProfile(a) ? b : a);

  let sitePlayer = null;

  // Collect all search candidates for the given id (and try to be exhaustive)
  let searchResults = [];
  try { searchResults = await searchPlayers(idStr); } catch { searchResults = []; }

  // 1. Resolve best site profile: try full profiles for all relevant candidates
  try {
    const candidates = Array.isArray(searchResults) ? [...searchResults] : [];
    // Build fetch targets (uids/fids) from search results, prioritizing matches
    const targets = [];
    const seenT = new Set();
    const pushTarget = (tid, priority = 0) => {
      const k = String(tid == null ? '' : tid).trim();
      if (!k || seenT.has(k)) return;
      seenT.add(k);
      targets.push({ id: k, priority });
    };
    // Prioritize exact matches to requested id
    for (const r of candidates) {
      const isExactUid = String(r.uid) === idStr;
      const isExactFid = String(r.fid) === idStr;
      const pr = (isExactUid || isExactFid) ? 3 : (options.idType === 'uid' && isExactUid ? 3 : options.idType === 'fid' && isExactFid ? 3 : 1);
      if (r.uid != null) pushTarget(r.uid, pr + 1);
      if (r.fid != null) pushTarget(r.fid, pr);
    }
    // Always try the id itself
    pushTarget(idStr, 2);
    targets.sort((a, b) => b.priority - a.priority);

    for (const t of targets) {
      try {
        const full = await siteFetch(`/players/${encodeURIComponent(t.id)}`);
        if (full && (full.uid || full.fid)) {
          const n = normalizeSitePlayer(full);
          sitePlayer = pickBetter(sitePlayer, n);
          if (sitePlayer && sitePlayer.troopLevels) break; // found what we need
        }
      } catch { /* continue */ }
    }

    // If no full profile gave troops, accept best search-row normalization as fallback
    if (!sitePlayer || (!sitePlayer.troopLevels && wantTroops)) {
      let bestRow = null;
      for (const r of candidates) {
        const n = normalizeSitePlayer(r);
        bestRow = pickBetter(bestRow, n);
      }
      sitePlayer = pickBetter(sitePlayer, bestRow);
    }
  } catch { /* continue */ }

  // 1b. Extra direct fetch attempts if still missing troop data
  if (!sitePlayer || (!sitePlayer.troopLevels && wantTroops)) {
    const tryIds = new Set([idStr, sitePlayer?.uid, sitePlayer?.chiefId].filter(Boolean).map(String));
    // If we have search results (from earlier or fresh), mine their ids
    let sr = searchResults;
    if (!sr || !sr.length) { try { sr = await searchPlayers(idStr); } catch { sr = []; } }
    for (const r of (sr || [])) {
      if (r && r.uid != null) tryIds.add(String(r.uid));
      if (r && r.fid != null) tryIds.add(String(r.fid));
    }
    const tryIdArr = [...tryIds];

    // Normal fast fetch pass
    for (const tid of tryIdArr) {
      try {
        const direct = await siteFetch(`/players/${encodeURIComponent(tid)}`);
        if (direct && (direct.uid || direct.fid)) {
          const n = normalizeSitePlayer(direct);
          const prev = sitePlayer;
          sitePlayer = pickBetter(sitePlayer, n);
          // also merge troop fields into existing best profile
          if (prev && sitePlayer && !prev.troopLevels && sitePlayer.troopLevels) { /* picked better */ }
          else if (prev && n && n.troopLevels && !prev.troopLevels) {
            prev.troopLevels = n.troopLevels;
            if (!prev.troop || !prev.troop.length) prev.troop = n.troop;
            sitePlayer = prev;
          }
          if (sitePlayer && sitePlayer.troopLevels) break;
        }
      } catch { /* ignore */ }
    }

    // If troop data is explicitly requested and still missing, trigger a site-side
    // player refresh and poll until expedition_troops becomes available.
    if (wantTroops && (!sitePlayer || !sitePlayer.troopLevels)) {
      // Prefer uid for refresh endpoints; include fid/id as fallback.
      const refreshIds = [];
      if (sitePlayer?.uid) refreshIds.push(String(sitePlayer.uid));
      for (const r of (sr || [])) { if (r && r.uid != null) refreshIds.push(String(r.uid)); }
      for (const tid of tryIdArr) refreshIds.push(tid);
      const refreshed = await fetchPlayerWithTroopRefresh(refreshIds, {
        attempts: options.troopRefreshAttempts || 4,
        delayMs: options.troopRefreshDelayMs || 2500,
      });
      if (refreshed) {
        const prev = sitePlayer;
        sitePlayer = pickBetter(sitePlayer, refreshed);
        if (prev && refreshed.troopLevels && !prev.troopLevels) {
          prev.troopLevels = refreshed.troopLevels;
          if (!prev.troop || !prev.troop.length) prev.troop = refreshed.troop;
          sitePlayer = pickBetter(prev, sitePlayer);
        }
      }
    }
  }

  // 2. Official API if key connected (still treated as whiteoutstats family)
  if (!sitePlayer) {
    const include = options.include || 'base,heroes,ranks,chief_gear';
    const qs = new URLSearchParams({ include });
    if (options.idType === 'uid') qs.set('id_type', 'uid');
    const official = await officialFetch(`/players/${encodeURIComponent(idStr)}?${qs}`);
    if (official) {
      sitePlayer = normalizeOfficialPlayer(official);
    }
  }

  // 3. Secondary source (Oracle) — fetch when missing gear, or when troop data
  //    is still unavailable and we need every possible source.
  const needsOracleGear = !sitePlayer
    || (!hasChiefGearPayload(sitePlayer.chiefGear) && !sitePlayer.gearPrivacy && options.preferOracleChiefGear !== false);
  const needsOracleTroops = wantTroops
    ? (!sitePlayer || !sitePlayer.troopLevels)
    : (!sitePlayer || (!sitePlayer.troopLevels && options.skipOracleTroops !== true));
  let oracleProfile = null;
  let oracleLoadout = null;
  let oracleChiefGear = null;
  const oracleIds = [idStr, sitePlayer?.chiefId, sitePlayer?.uid].filter(Boolean).map(String);
  const uniqOracleIds = [...new Set(oracleIds)];
  if ((needsOracleGear || needsOracleTroops) && options.skipOracle !== true) {
    for (const oid of uniqOracleIds) {
      const raw = await fetchOracleRaw(oid);
      if (!oracleProfile && raw.profile) oracleProfile = raw.profile;
      if (!oracleLoadout && raw.loadout) oracleLoadout = raw.loadout;
      if (!oracleChiefGear) oracleChiefGear = extractOracleChiefGear(raw.profile, raw.loadout);
      if ((oracleProfile || oracleLoadout) && (!wantTroops || normalizeTroopLevels(oracleProfile || oracleLoadout || {}))) break;
    }
  }

  if (sitePlayer) {
    // Fill chief gear & charms when the site had none; never replace real site gear.
    if (oracleChiefGear && options.preferOracleChiefGear !== false) {
      if (hasChiefGearPayload(oracleChiefGear) && !hasChiefGearPayload(sitePlayer.chiefGear)) {
        sitePlayer.chiefGear = oracleChiefGear;
      }
    }
    // If site had no heroes but Oracle loadout/profile did, use those as last resort only
    if (!sitePlayer.heroes) {
      const oh =
        oracleLoadout?.heroes ||
        oracleProfile?.heroes ||
        oracleProfile?.arena ||
        (oracleProfile?.player && (oracleProfile.player.heroes || oracleProfile.player.arena));
      if (oh) sitePlayer.heroes = oh;
    }
    // Merge troop data from any available source when the site answer lacks it.
    if (!sitePlayer.troopLevels || !Array.isArray(sitePlayer.troop) || !sitePlayer.troop.length) {
      const troopCandidates = [oracleProfile, oracleLoadout, sitePlayer.raw, sitePlayer].filter(Boolean);
      for (const c of troopCandidates) {
        const tl = normalizeTroopLevels(c);
        if (tl) {
          if (!sitePlayer.troopLevels) sitePlayer.troopLevels = tl;
          const tr = normalizeTroops(c.expedition_troops || c.troops || c);
          if (tr && tr.length && (!sitePlayer.troop || !sitePlayer.troop.length)) sitePlayer.troop = tr;
          if (sitePlayer.troopLevels && Array.isArray(sitePlayer.troop) && sitePlayer.troop.length) break;
        }
      }
    }
    sitePlayer.raw = sitePlayer.raw || {};
    sitePlayer.raw.oracleLoadout = oracleLoadout || null;
    sitePlayer.raw._sources = {
      profile: 'whiteoutstats',
      heroes: sitePlayer.heroes ? 'whiteoutstats' : null,
      chiefGear: hasChiefGearPayload(sitePlayer.chiefGear) ? 'oracle' : null,
    };
    return sitePlayer;
  }

  if (oracleProfile || oracleLoadout) {
    const n = normalizeOraclePlayer(oracleProfile || oracleLoadout);
    if (n) {
      if (oracleChiefGear) n.chiefGear = oracleChiefGear;
      if (!n.heroes && oracleLoadout?.heroes) n.heroes = oracleLoadout.heroes;
      if (!n.troopLevels) {
        for (const c of [oracleProfile, oracleLoadout].filter(Boolean)) {
          const tl = normalizeTroopLevels(c);
          if (tl) { n.troopLevels = tl; const tr = normalizeTroops(c.expedition_troops || c.troops || c); if (tr && tr.length) n.troop = tr; break; }
        }
      }
      n.raw = n.raw || oracleProfile || {};
      n.raw.oracleLoadout = oracleLoadout || null;
      n.raw._sources = {
        profile: 'oracle',
        heroes: n.heroes ? 'oracle' : null,
        chiefGear: n.chiefGear ? 'oracle' : null,
      };
      return n;
    }
  }

  return null;
}

async function getState(kid) {
  const id = Number(kid);
  if (!Number.isFinite(id)) return null;
  // The site defaults to 20 alliances/players; ask for the full tracked set.
  const data = await siteFetch(`/kingdoms/${id}?alliances=100&players=100`);
  if (data && (data.ok || data.kid)) return data;
  const official = await officialFetch(`/states/${id}`);
  if (official) return official.state || official;
  return null;
}

// Cache full ingame-ranks payload per state (short TTL) so board switches are free
const _ingameRanksCache = new Map(); // kid -> { at, data }
const INGAME_TTL_MS = 60 * 1000;

// Full map payload cache (cities[] is large) and progression history cache
const _mapCache = new Map(); // kid -> { at, data }
const MAP_TTL_MS = 3 * 60 * 1000;
const _historyCache = new Map(); // uid -> { at, rows }
const HISTORY_TTL_MS = 5 * 60 * 1000;

async function fetchIngameRanks(kid) {
  const k = String(kid);
  const hit = _ingameRanksCache.get(k);
  if (hit && Date.now() - hit.at < INGAME_TTL_MS) return hit.data;
  const data = await siteFetch(`/kingdoms/${encodeURIComponent(k)}/ingame-ranks`);
  if (data && data.ok && data.boards) {
    _ingameRanksCache.set(k, { at: Date.now(), data });
    return data;
  }
  return data;
}



async function getStateAppointments(kid) {
  const id = Number(kid);
  if (!Number.isFinite(id)) return null;
  const d = await siteFetch(`/kingdoms/${id}/appointments`);
  if (d && (d.ok || d.ministers || d.offenders)) return d;
  return null;
}

async function getStateMap(kid, options = {}) {
  const id = Number(kid);
  if (!Number.isFinite(id)) return null;
  const hit = _mapCache.get(id);
  // The full cities payload is ~1.7 MB, so keep it warm for a few minutes.
  if (!options.fresh && hit && Date.now() - hit.at < MAP_TTL_MS) return hit.data;
  const d = await siteFetch(`/map?kid=${id}&layers=cities&limit=50000`, { timeout: 30000 });
  if (d && (d.kid != null || d.cities)) {
    _mapCache.set(id, { at: Date.now(), data: d });
    return d;
  }
  return hit ? hit.data : null;
}

// ── map refresh (public queue shared with the website) ──────────────────────
// The site accepts any client-generated `t_*` token; one stable token per bot
// process lets it correlate our status polls with our own update request.
const MAP_UPDATE_TOKEN = 't_bot_' + Math.random().toString(36).slice(2) + Date.now().toString(36);

/** Current queue/cooldown state for a state's map refresh. */
async function getMapUpdateStatus(kid, token = MAP_UPDATE_TOKEN) {
  const id = Number(kid);
  if (!Number.isFinite(id)) return { ok: false, status: 0, data: null };
  return siteJson(
    `/map/update/status?kid=${encodeURIComponent(id)}&token=${encodeURIComponent(token)}`
  );
}

/** Ask the site to refresh a state's map. Caller polls getMapUpdateStatus. */
async function requestMapUpdate(kid, token = MAP_UPDATE_TOKEN) {
  const id = Number(kid);
  if (!Number.isFinite(id)) return { ok: false, status: 0, data: null };
  const res = await siteJson(
    `/map/update?kid=${encodeURIComponent(id)}&token=${encodeURIComponent(token)}`,
    { method: 'POST' }
  );
  // The site may hand back a fresh request token; keep using ours otherwise.
  if (res.data && res.data.request_token) res.token = res.data.request_token;
  return res;
}

/**
 * Power/kills snapshots for the Progression tab (newest first).
 * The public profile payload has no history, so this is a separate feed.
 */
async function getPlayerHistory(uid, options = {}) {
  const id = String(uid == null ? '' : uid).trim();
  if (!id) return [];
  const hit = _historyCache.get(id);
  if (!options.fresh && hit && Date.now() - hit.at < HISTORY_TTL_MS) return hit.rows;
  const d = await siteFetch(`/players/${encodeURIComponent(id)}/history`);
  const rows = Array.isArray(d) ? d : (d && Array.isArray(d.history) ? d.history : []);
  if (rows.length) _historyCache.set(id, { at: Date.now(), rows });
  return rows;
}

async function getStateRanks(kid, board = '3', limit = 100) {
  // board may be numeric type id ("3") or legacy slug ("personal_power")
  const TYPE_BY_SLUG = {
    alliance_power: '1', alliance_kills: '2', personal_power: '3', power: '3',
    kill_count: '4', kills: '4', furnace_level: '5', furnace: '5',
    exploration_stage: '6', rebel_conquest: '6',
    hero_power: '7', hero_total_power: '8', hero_total: '8',
    troop_power: '9', building_power: '10', research_power: '11', research_center: '24',
    hero_no_equip: '12', hero_power_no_equip: '12',
    hero_equip: '13', hero_equip_power: '13',
    chief_gear: '14', chief_gear_power: '14', chief_charm: '15', chief_charm_power: '15',
    pet_power: '16', island_prosperity: '18', transfer_score: '19',
    labyrinth: '20', land_of_heroes: '21', cave_of_monsters: '22',
    charm_mine: '23', gear_forge: '25', gaia_heart: '26',
    master_total_power: '29', expert_power: '29',
  };
  const typeKey = String(TYPE_BY_SLUG[board] || board);

  // 1. Public site full snapshot
  try {
    const data = await fetchIngameRanks(kid);
    const boards = data?.boards || {};
    const block = boards[typeKey] || boards[String(Number(typeKey))];
    if (block && Array.isArray(block.rows)) {
      return block.rows.slice(0, limit);
    }
  } catch { /* continue */ }

  // 2. Official API if key connected (legacy slug path)
  const official = await officialFetch(
    `/states/${kid}/ranks?board=${encodeURIComponent(board)}&limit=${limit}`
  );
  if (official) return official.ranks || official.entries || official.list || official;

  return null;
}

async function getAlliance(kid, tag) {
  const lookup = await siteFetch(
    `/alliances/lookup?slug=${encodeURIComponent(tag)}&kid=${encodeURIComponent(kid)}`
  );
  if (lookup && (lookup.ok || lookup.alliance)) return lookup;
  const official = await officialFetch(
    `/alliances/${kid}/${encodeURIComponent(tag)}?include=info,roster`
  );
  if (official) return official;
  return null;
}

async function searchAlliances(kid, query = '') {
  const data = await siteFetch(
    `/search/alliances?q=${encodeURIComponent(query || 'a')}&kid=${encodeURIComponent(kid)}`
  );
  if (data?.results) return data.results;
  const ranks = await getStateRanks(kid, 'alliance_power', 100);
  return Array.isArray(ranks) ? ranks : [];
}

/**
 * Lightweight profile fetch for roster refresh (Whiteout Stats ONLY — no Oracle calls).
 * Returns the same field shape as oracleClient.normalizePlayer() so it can be fed
 * straight into players.updateOracleFields(), or null if Whiteout Stats has no
 * confident match for this FID.
 *   1) public site: /search?q=<fid> → /players/<uid>
 *   2) official API (only if an API key is stored)
 */
async function getRosterProfile(fid) {
  const idStr = String(fid).trim();
  let p = null;

  try {
    const search = await siteFetch(`/search?q=${encodeURIComponent(idStr)}`);
    const results = search?.results || [];
    // Strict FID match — never write another player's data into the roster.
    const match = results.find((r) => String(r.fid) === idStr);
    if (match) {
      let full = null;
      if (match.uid) full = await siteFetch(`/players/${match.uid}`);
      const src = full && (full.uid || full.fid) ? full : match;
      if (!src.fid || String(src.fid) === idStr) p = normalizeSitePlayer(src);
    }
  } catch { /* fall through to official */ }

  if (!p) {
    const official = await officialFetch(`/players/${encodeURIComponent(idStr)}?include=base`);
    if (official) p = normalizeOfficialPlayer(official);
  }
  if (!p || !p.chiefId) return null;

  const a = p.alliance || {};
  return {
    fid: idStr,
    game_name: p.name && p.name !== 'Unknown' ? p.name : null,
    state: p.stateId != null ? p.stateId : null,
    alliance_abbr: a.tag || null,
    alliance_id: a.id || null,
    alliance_name: a.name || null,
    alliance_rank: a.rank != null ? a.rank : null,
    power: p.power,
    kills: p.kills,
    town_hall_level: p.furnace,
    avatar_url: p.avatarUrl || null,
    // life_tree_level / oracle_uid intentionally omitted → existing DB values are kept
    raw: p.raw,
  };
}

module.exports = {
  getRosterProfile,
  getPlayer,
  getPlayerHistory,
  searchPlayers,
  getState,
  getStateRanks,
  getAlliance,
  searchAlliances,
  wosFetch,
  siteFetch,
  siteJson,
  officialFetch,
  isWosConfigured,
  isOfficialKeyConnected,
  testWosConnection,
  resolveWosKey,
  getStateMap,
  getMapUpdateStatus,
  requestMapUpdate,
  getStateAppointments,
  MAP_UPDATE_TOKEN,
};
