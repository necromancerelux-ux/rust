'use strict';

/**
 * Turns whatever the data client returns (public site / official / Oracle
 * fallback) into stable view-models for the panels and cards.
 * Every accessor is defensive: unknown shape → null/[] rather than throwing,
 * so a missing field degrades the card instead of breaking the message.
 */

const dataClient = require('./dataClient');
const { _util } = require('./oracleCards');
const { normalizeTroopLevels } = require('./troopLevels');

const pick = (o, ...keys) => {
  if (!o) return undefined;
  for (const k of keys) if (o[k] !== undefined && o[k] !== null && o[k] !== '') return o[k];
  return undefined;
};
const numOr = (v) => (v == null || v === '' || !Number.isFinite(Number(v)) ? null : Number(v));
const arr = (v) => (Array.isArray(v) ? v : []);
const tsOf = (v) => {
  if (v == null) return null;
  if (typeof v === 'number') return v > 1e12 ? v / 1000 : v;
  const t = Date.parse(v);
  return Number.isFinite(t) ? t / 1000 : null;
};
const ROLE = { 5: 'R5', 4: 'R4', 3: 'R3', 2: 'R2', 1: 'R1' };
const TROOP_NAME = { inf: 'Infantry', lan: 'Lancer', mm: 'Marksman' };
function troopEntriesOf(p) {
  const t = p && (p.troop || p.expedition_troops);
  let list = [];
  if (Array.isArray(t)) list = t.filter((x) => x && x.tier != null);
  else if (t && typeof t === 'object') list = Object.values(t).filter((x) => x && typeof x === 'object' && x.tier != null);
  const order = (k) => { const i = ['inf', 'lan', 'mm'].indexOf(String(k).toLowerCase()); return i < 0 ? 99 : i; };
  return [...list].sort((a, b) => (order(a.kind) - order(b.kind)) || (Number(b.tier) - Number(a.tier)));
}
function troopValue(t) {
  if (!t) return null;
  const tier = numOr(t.tier);
  const fc = numOr(t.fc != null ? t.fc : t.star);
  if (tier == null) return null;
  return 'T' + tier + (fc != null ? ' · FC' + fc : '');
}
function troopLineOf(p) {
  const list = troopEntriesOf(p);
  const parts = [];
  for (const t of list) {
    const name = TROOP_NAME[t.kind] || t.name || String(t.kind || '');
    const tier = numOr(t.tier);
    if (tier == null) continue;
    const fc = numOr(t.fc != null ? t.fc : t.star);
    parts.push(name + ' T' + tier + (fc != null ? ' · FC' + fc : ''));
  }
  return parts.length ? parts.join(' · ') : null;
}
const roleLabel = (v) => {
  if (v == null) return null;
  if (/^R\d$/i.test(String(v))) return String(v).toUpperCase();
  return ROLE[Number(v)] || String(v);
};

/* ───────────── boards (state leaderboards) ───────────── */
const BOARDS = [
  // key = in-game board type id (string) from whiteoutstats /ingame-ranks
  ['1', 'Alliance Power', 'Core'],
  ['2', 'Alliance Kills', 'Core'],
  ['3', 'Personal Power', 'Core'],
  ['4', 'Kill Count', 'Core'],
  ['5', 'Furnace Level', 'Core'],
  ['6', 'Exploration Stage', 'Core'],
  ['7', 'Hero Power', 'Core'],
  ['8', "Hero's Total Power", 'Core'],
  ['9', 'Troop Power', 'Core'],
  ['10', 'Building Power', 'Core'],
  ['11', 'Research Power', 'Core'],
  ['12', 'Hero Power Without Equip', 'Gear'],
  ['13', 'Hero Equip Power', 'Gear'],
  ['14', 'Chief Gear Power', 'Gear'],
  ['15', 'Chief Charm Power', 'Gear'],
  ['16', 'Pet Power', 'Core'],
  ['18', 'Island Prosperity', 'Other'],
  ['19', 'State Transfer Score', 'Other'],
  ['20', 'The Labyrinth', 'The Labyrinth'],
  ['21', 'Land of Heroes', 'The Labyrinth'],
  ['22', 'Cave of Monsters', 'The Labyrinth'],
  ['23', 'Charm Mine', 'The Labyrinth'],
  ['24', 'Research Center', 'The Labyrinth'],
  ['25', 'Gear Forge', 'The Labyrinth'],
  ['26', 'Gaia Heart', 'The Labyrinth'],
  ['29', 'Total Expert Power', 'Other'],
];
const boardName = (key) => (BOARDS.find((b) => b[0] === key) || [])[1] || key;
const boardGroup = (key) => (BOARDS.find((b) => b[0] === key) || [])[2] || 'Core';

/* ───────────── player ───────────── */
function playerView(p) {
  const r = p.raw?.player || p.raw || {};
  const ranksObj = p.ranks || {};
  const boards = [];

  // leaderboards array from public site: { name, value, kingdom_rank, rank_type, trail }
  const leaderboardList = Array.isArray(ranksObj)
    ? ranksObj
    : arr(ranksObj.boards || ranksObj.list || ranksObj.entries || ranksObj.leaderboards);

  for (const b of leaderboardList) {
    if (!b || typeof b !== 'object') continue;
    // Skip internal flags
    if (b === true || b === false) continue;
    const key = String(pick(b, 'rank_type', 'board', 'key', 'type', 'id') ?? '');
    if (!key || key === '_checked') continue;
    boards.push({
      key,
      group: boardGroup(key),
      name: pick(b, 'label', 'name', 'title') || boardName(key),
      score: _util.num(numOr(pick(b, 'score', 'value'))),
      rawScore: numOr(pick(b, 'score', 'value')),
      rank: numOr(pick(b, 'kingdom_rank', 'state_rank', 'rank', 'position')),
      pct: numOr(pick(b, 'percentile', 'pct')),
      global: pick(b, 'global_label') || (numOr(pick(b, 'global_rank')) ? '#' + _util.int(numOr(pick(b, 'global_rank'))) + (pick(b, 'global_total') ? ' / ' + _util.num(numOr(pick(b, 'global_total'))) : '') : null),
      age: pick(b, 'age_label', 'age') || null,
      trail: !!b.trail,
      valueLabel: pick(b, 'value_label') || null,
    });
  }

  // ranks_json object shape: { "3": { type, rank, score }, "20": {...}, "_checked": true }
  if (!boards.length && ranksObj && typeof ranksObj === 'object' && !Array.isArray(ranksObj)) {
    for (const [k, b] of Object.entries(ranksObj)) {
      if (k === '_checked' || !b || typeof b !== 'object') continue;
      const key = String(pick(b, 'type', 'rank_type') ?? k);
      boards.push({
        key,
        group: boardGroup(key),
        name: boardName(key),
        score: _util.num(numOr(pick(b, 'score', 'value'))),
        rawScore: numOr(pick(b, 'score', 'value')),
        rank: numOr(pick(b, 'rank', 'kingdom_rank', 'state_rank')),
        pct: null,
        global: null,
        age: null,
        trail: ['21', '22', '23', '24', '25', '26'].includes(String(key)),
        valueLabel: null,
      });
    }
  }

  // Flat ranks from profile fields when full board list is still absent
  if (!boards.length) {
    const flat = [
      ['3', p.powerRank ?? r.power_rank, p.power],
      ['4', p.killsRank ?? r.kills_rank, p.kills],
      ['5', p.stoveRank ?? r.stove_rank, p.furnace],
      ['19', p.migrantRank ?? r.migrant_rank, p.migrantScore ?? r.migrant_score],
      ['20', p.mysticRank ?? r.mystic_rank, p.mysticTrial ?? r.mystic_trial],
    ];
    for (const [key, rank, score] of flat) {
      if (rank != null || score != null) {
        boards.push({
          key,
          group: boardGroup(key),
          name: boardName(key),
          score: _util.num(numOr(score)),
          rawScore: numOr(score),
          rank: numOr(rank),
          pct: null,
          global: null,
          age: null,
        });
      }
    }
  }

  const crown = p.crowns || r.crowns || {};
  const asOf =
    tsOf(p.updatedAt?.detail) ||
    tsOf(p.updatedAt?.stats) ||
    tsOf(p.updatedAt?.lastSeen) ||
    tsOf(pick(r, 'updated_at', 'last_seen', 'fetched_at', 'detail_updated_at', 'stats_updated_at')) ||
    Date.now() / 1000;

  // Labyrinth progress lives on mystic_trial in the public API
  const labyrinth =
    numOr(pick(r, 'labyrinth', 'labyrinth_score')) ??
    numOr(p.mysticTrial) ??
    numOr(r.mystic_trial) ??
    null;
  const labRank =
    numOr(p.mysticRank) ??
    numOr(r.mystic_rank) ??
    (boards.find((b) => String(b.key) === '20' || /labyrinth/i.test(b.name || '')) || {}).rank ??
    null;

  // Exploration towers from trails_list / trails_json (NOT map movement)
  let trails = p.trails;
  if (trails && !Array.isArray(trails) && typeof trails === 'object') {
    trails = Object.values(trails).filter((t) => t && typeof t === 'object');
  }
  if ((!trails || !trails.length) && Array.isArray(r.trails_list)) trails = r.trails_list;
  if ((!trails || !trails.length) && r.trails_json) {
    const parsed = typeof r.trails_json === 'string' ? (() => { try { return JSON.parse(r.trails_json); } catch { return null; } })() : r.trails_json;
    if (parsed && typeof parsed === 'object') trails = Object.values(parsed).filter((t) => t && typeof t === 'object');
  }

  return {
    name: p.name,
    chiefId: p.chiefId,
    uid: p.uid,
    avatarUrl: p.avatarUrl || absAvatar(r),
    stateId: p.stateId,
    allianceTag: p.alliance?.tag,
    allianceName: p.alliance?.name,
    allianceFlagUrl: p.alliance?.flagUrl || null,
    role: roleLabel(p.alliance?.rankLabel || p.alliance?.rank || r.alliance_rank_label || r.alliance_rank),
    allianceMembers: p.alliance?.members ?? null,
    alliancePower: p.alliance?.power ?? null,
    lifeTree: numOr(pick(r, 'life_tree', 'life_tree_level', 'lifetree')),
    power: p.power,
    kills: p.kills,
    killsRank: numOr(p.killsRank ?? pick(r, 'kills_rank', 'kill_rank_state')),
    killsGlobal: numOr(pick(r, 'kills_rank_global', 'kill_rank_global')),
    powerRank: numOr(p.powerRank ?? r.power_rank),
    furnace: p.furnace,
    townCenter: p.townCenter ?? numOr(r.town_center_level) ?? p.furnace,
    labyrinth,
    labRank,
    vip: p.vip,
    vipActive: p.vipActive,
    // Activity fields (online / last_active_at / last_login) are stale on the
    // public feed, so only expose real timestamps for "last power change".
    lastPowerChangeAt: tsOf(p.updatedAt?.stats) || tsOf(r.stats_updated_at) || null,
    firstSeenAt: tsOf(p.updatedAt?.firstSeen) || tsOf(r.first_seen_at) || null,
    tgLevel: p.tgLevel,
    tgLabel: p.tgLabel || null,
    levelLabel: p.levelLabel || p.tgLabel || null,
    officeLabel: p.officeLabel || null,
    migrantScore: p.migrantScore ?? numOr(r.migrant_score),
    migrantRank: p.migrantRank ?? numOr(r.migrant_rank),
    shield: p.shield || null,
    burn: p.burn || null,
    mysticTrial: p.mysticTrial ?? numOr(r.mystic_trial),
    mysticRank: labRank,
    crowns: numOr(pick(crown, 'count', 'crowns')) ?? numOr(pick(r, 'crowns')) ?? 0,
    terms: numOr(pick(crown, 'terms')) ?? 0,
    conquests: numOr(pick(crown, 'conquests')) ?? 0,
    statesRuled: numOr(pick(crown, 'states_ruled', 'statesRuled')) ?? 0,
    thrones: arr(crown.thrones).map((t) => ({
      state: pick(t, 'kid', 'state'),
      badge: t.home ? 'HOME' : '',
      terms: (t.terms || 1) + ' term' + ((t.terms || 1) === 1 ? '' : 's'),
      when: pick(t, 'since', 'when') || '',
    })),
    reigns: arr(crown.reigns).map((t) => ({
      date: pick(t, 'date', 'day') || '',
      kind: pick(t, 'kind', 'type') || 'Reign',
      state: 'State ' + (pick(t, 'kid', 'state') ?? '—'),
      delta: pick(t, 'delta') || '+1',
    })),
    redeemNote: r.redeeming === false ? 'Not redeeming — tracked for stats only' : null,
    boards,
    asOf,
    hasGear: !!(
      p.chiefGear &&
      (Array.isArray(p.chiefGear)
        ? p.chiefGear.length
        : Array.isArray(p.chiefGear.pieces)
          ? p.chiefGear.pieces.length
          : Object.keys(p.chiefGear || {}).length)
    ),
    heroes: p.heroes,
    chiefGear: p.chiefGear,
    gearPrivacy: !!p.gearPrivacy,
    gearPrivacyMessage: p.gearPrivacyMessage || null,
    trails,
    glory: p.glory,
    troop: p.troop || null,
    troopLine: troopLineOf(p),
    x: p.x,
    y: p.y,
    allianceId: p.alliance?.id,
    history: r.history || r.snapshots || null,
    troopLevels: p.troopLevels ?? null,
    // Pass through for Oracle loadout / chief-gear render fallbacks
    raw: p.raw || r || null,
  };
}

function absAvatar(r) {
  if (!r) return null;
  if (r.avatar_url && /^https?:\/\//i.test(r.avatar_url)) return r.avatar_url;
  if (r.upload_image) {
    const path = String(r.upload_image);
    if (/^https?:\/\//i.test(path)) return path;
    return 'https://gof-formal-avatar.akamaized.net/avatar/' + path.replace(/^\//, '');
  }
  return null;
}

/** Normalise `/players/{uid}/history` rows into chart series. */
function historyToView(rows) {
  const list = arr(rows).map((r) => ({
    t: tsOf(pick(r, 'observed_at', 'observedAt', 'ts', 'at', 'date')),
    power: numOr(pick(r, 'power', 'new_power')),
    kills: numOr(pick(r, 'kills')),
    summary: pick(r, 'change_summary', 'summary', 'change'),
  })).filter((r) => r.t != null).sort((a, b) => a.t - b.t);
  const d = (t) => new Date(t * 1000).toISOString().slice(0, 10);
  return {
    power: list.filter((r) => r.power != null).map((r) => ({ t: r.t, v: r.power })),
    kills: list.filter((r) => r.kills != null).map((r) => ({ t: r.t, v: r.kills })),
    milestones: list.filter((r) => r.summary).slice(-6).reverse().map((r) => ({
      date: d(r.t), kind: 'Change', change: r.summary,
    })),
  };
}

function progressionView(v) {
  let h = v.history;
  if (Array.isArray(h)) h = historyToView(h);
  else if (h && Array.isArray(h.history)) h = historyToView(h.history);
  h = h && typeof h === 'object' ? h : {};
  const toSeries = (a) => arr(a).map((e, i) => ({ t: tsOf(pick(e, 't', 'ts', 'date', 'at')) ?? i, v: numOr(pick(e, 'v', 'value', 'power', 'kills')) })).filter((e) => e.v != null);
  const powerSeries = toSeries(h.power || h.power_history);
  const killSeries = toSeries(h.kills || h.kill_history);
  const first = Math.min(powerSeries[0]?.t ?? Infinity, killSeries[0]?.t ?? Infinity);
  const last = Math.max(powerSeries.slice(-1)[0]?.t ?? 0, killSeries.slice(-1)[0]?.t ?? 0);
  const d = (t) => new Date(t * 1000).toISOString().slice(0, 10);
  return {
    name: v.name, powerSeries, killSeries, powerSnaps: powerSeries.length, killSnaps: killSeries.length,
    rangeLabel: Number.isFinite(first) && last ? d(first) + ' → ' + d(last) : '',
    milestones: arr(h.milestones || h.changes).slice(0, 6).map((m) => ({ date: pick(m, 'date') || '', kind: pick(m, 'kind', 'type') || '', change: pick(m, 'change', 'text') || '' })),
    asOf: last || Date.now() / 1000,
  };
}

/* ───────────── alliance ───────────── */
async function loadAlliance(kid, tag) {
  const res = await dataClient.getAlliance(kid, tag);
  if (!res) return null;
  const a = res.alliance || res.info || res;
  const rosterRaw = arr(res.roster || res.members || a.roster || a.members_list || a.members);
  const roster = rosterRaw.map((m) => ({
    name: pick(m, 'nick_name', 'name', 'nick') || 'Unknown',
    fid: pick(m, 'fid', 'chief_id'), uid: pick(m, 'uid'),
    power: numOr(pick(m, 'power')), furnace: numOr(pick(m, 'stove_lv', 'furnace', 'furnace_level')),
    role: roleLabel(pick(m, 'rank_label', 'alliance_rank', 'role', 'rank')),
    roleNum: numOr(pick(m, 'alliance_rank', 'rank_num')) ?? 0,
    labyrinth: numOr(pick(m, 'labyrinth', 'labyrinth_score')) ?? 0,
    kills: numOr(pick(m, 'kills')),
    troopLevels: normalizeTroopLevels(m),
    troop: arr(m.expedition_troops || m.troop).map((t) => ({
      kind: t.kind, name: TROOP_NAME[t.kind] || t.name || String(t.kind || ''),
      tier: numOr(t.tier), star: numOr(t.star), fc: numOr(t.fc != null ? t.fc : t.star), count: numOr(t.count),
    })),
  }));
  const total = roster.reduce((s, m) => s + (m.power || 0), 0) || numOr(pick(a, 'power')) || 1;
  roster.forEach((m) => { m.share = m.power ? ((m.power / total) * 100).toFixed(2) + '%' : ''; });
  const lead = roster.find((m) => m.roleNum === 5 || m.role === 'R5');
  const ev = (list) => arr(list).map((e) => ({
    kind: pick(e, 'kind', 'type', 'field') || 'power', name: pick(e, 'nick_name', 'name') || 'Unknown',
    from: pick(e, 'from', 'old', 'before'), to: pick(e, 'to', 'new', 'after'),
    delta: numOr(pick(e, 'delta', 'diff')), value: numOr(pick(e, 'value', 'power', 'new')),
    ts: tsOf(pick(e, 'ts', 'at', 'time', 'date')),
    fid: pick(e, 'fid'), power: numOr(pick(e, 'power')), furnace: numOr(pick(e, 'furnace', 'stove_lv')),
    fromTag: pick(e, 'from_alliance', 'from_tag', 'from'),
  }));
  return {
    tag: pick(a, 'abbr', 'tag') || tag, name: pick(a, 'name') || '', stateId: kid,
    aid: pick(a, 'aid') || res.aid || null,
    leader: pick(a, 'leader_name', 'leader') || lead?.name || null, leaderRole: lead ? 'R5' : null,
    leaderAvatar: pick(a, 'leader_avatar', 'leader_avatar_url') || null,
    leaderUid: pick(a, 'leader_uid', 'leader') || null,
    power: numOr(pick(a, 'power')) ?? (roster.length ? total : null),
    kills: numOr(pick(a, 'kills', 'total_kills')),
    members: numOr(pick(a, 'members', 'member_count', 'count')) ?? (roster.length || null),
    memberMax: numOr(pick(a, 'member_max', 'max_members')),
    level: numOr(pick(a, 'lv', 'level')),
    labyrinth: numOr(pick(a, 'labyrinth', 'labyrinth_score')),
    powerRank: numOr(pick(a, 'power_rank', 'rank')), powerOf: numOr(pick(a, 'power_total', 'alliances_total')),
    killsRank: numOr(pick(a, 'kills_rank')), killsOf: numOr(pick(a, 'kills_total')),
    labRank: numOr(pick(a, 'labyrinth_rank')), labOf: numOr(pick(a, 'labyrinth_total')),
    notice: pick(a, 'notice') || null,
    manifesto: pick(a, 'manifesto') || null,
    flagUrl: pick(a, 'flag_url') || null,
    languageLabel: pick(a, 'language_label') || null,
    publicLabel: pick(a, 'public_label') || null,
    roster, top: [...roster].sort((x, y) => (y.power || 0) - (x.power || 0)).slice(0, 6),
    events: ev(res.events || res.activity || a.events), joins: ev(res.joins || res.recent_joins || a.joins),
    // No public join/departure feed exists → the panel must not promise one.
    joinsAvailable: arr(res.joins || res.recent_joins || a.joins).length > 0,
    snapshots: arr(res.snapshots || a.snapshots), furnaceList: roster.filter((m) => m.furnace).sort((x, y) => y.furnace - x.furnace),
    trackerPower: numOr(res.tracker_power),
    trackerPowerRank: numOr(res.tracker_power_rank),
    asOf: tsOf(pick(a, 'updated_at', 'detail_updated_at', 'fetched_at')) || Date.now() / 1000,
  };
}

/* ───────────── state ───────────── */
async function loadState(kid) {
  const s = await dataClient.getState(kid);
  if (!s) return null;
  const st = s.state || s.kingdom || s;
  const healthObj = (st.health && typeof st.health === 'object') ? st.health : {};
  // Health components are numeric scores: power, activity_7d, activity_30d, size, located
  const healthComps = (healthObj.components && typeof healthObj.components === 'object') ? healthObj.components : {};
  const part = (name, k) => {
    const v = healthComps[k] ?? st[k + '_score'] ?? st[k];
    const obj = v && typeof v === 'object' ? v : null;
    return {
      name,
      value: numOr(obj ? pick(obj, 'value', 'score') : v),
      grade: (obj && pick(obj, 'grade')) || (typeof v === 'string' ? v : null) || null,
      rank: (obj && pick(obj, 'rank_label')) || '',
    };
  };
  const king = st.king || st.president || null;
  const ruling = st.ruling_alliance || st.rulingAlliance || null;
  const totals = arr(st.ingame_totals || st.board_totals || st.top100_totals).map((t) => ({
    type: numOr(pick(t, 'type')),
    label: pick(t, 'label', 'name') || '—',
    total: numOr(pick(t, 'total', 'value', 'score')),
    n: numOr(pick(t, 'n', 'count')) ?? 100,
  }));
  // Prefer explicit king; fall back to court president
  const presidentName = pick(king, 'nick_name', 'name') || pick(st, 'president', 'president_name') || null;
  const presidentAvatar = absMaybe(pick(king, 'avatar_url') || pick(st, 'president_avatar', 'president_avatar_url'));
  const presidentRole = pick(king, 'office_name', 'role') || pick(st, 'president_label') || (presidentName ? 'President' : '');
  const allianceTag = pick(ruling, 'abbr', 'tag') || null;
  const allianceName = pick(ruling, 'name') || null;
  const allianceFlag = absMaybe(pick(ruling, 'flag_url'));

  return {
    id: kid,
    name: pick(st, 'name') || null,
    bannerUrl: absMaybe(pick(st, 'banner_url')),
    ageDays: numOr(pick(st, 'age_days')),
    created: pick(st, 'opened_on') || null,
    openTime: numOr(pick(st, 'open_time')),
    players: numOr(pick(st, 'player_count', 'tracked')) ?? (Array.isArray(st.players) ? st.players.length : numOr(pick(st, 'players'))),
    activePlayers: numOr(pick(st, 'active_players', 'located')),
    active7d: numOr(pick(st, 'active_7d')),
    active30d: numOr(pick(st, 'active_30d')),
    alliances: numOr(pick(st, 'alliance_count')) ?? (Array.isArray(st.alliances) ? st.alliances.length : numOr(pick(st, 'alliances'))),
    power: numOr(pick(st, 'power')),
    avgPower: numOr(pick(st, 'avg_power')),
    topPower: numOr(pick(st, 'top_power')),
    locatedPower: numOr(pick(st, 'located_power')),
    activityRatio: numOr(pick(st, 'activity_ratio')),
    powerRatio: numOr(pick(st, 'power_ratio')),
    activeRate7d: numOr(pick(st, 'active_rate_7d')),
    powerGain7d: numOr(pick(st, 'power_gain_7d')),
    alliancePower: numOr(pick(st, 'alliance_power')),
    allianceKills: numOr(pick(st, 'alliance_kills')),
    kills: numOr(pick(st, 'kills')),
    heroPower: numOr(pick(st, 'hero_power')),
    heroTotal: numOr(pick(st, 'hero_total')),
    troopPower: numOr(pick(st, 'troop_power')),
    buildingPower: numOr(pick(st, 'building_power')),
    sciencePower: numOr(pick(st, 'science_power')),
    boards: numOr(pick(st, 'boards', 'board_count')) ?? BOARDS.length,
    health: numOr(pick(healthObj, 'score') ?? pick(st, 'health_score', 'health')),
    healthRank: pick(healthObj, 'label') || pick(st, 'health_rank_label') || '',
    grade: pick(healthObj, 'grade') || pick(st, 'grade', 'health_grade') || '—',
    rank: numOr(pick(st, 'rank', 'power_rank')),
    activityRank: numOr(pick(st, 'activity_rank')),
    president: presidentName,
    presidentAvatar,
    presidentSub: presidentRole,
    presidentAlliance: pick(king, 'alliance_abbr') || allianceTag,
    rulingAllianceTag: allianceTag,
    rulingAllianceName: allianceName,
    rulingAllianceFlag: allianceFlag,
    rulingAlliancePower: numOr(pick(ruling, 'power', 'game_power', 'tracker_power')),
    boardTotals: totals,
    healthParts: [
      part('Power', 'power'),
      part('Activity · 7 days', 'activity_7d'),
      part('Activity · 30 days', 'activity_30d'),
      part('Size', 'size'),
      part('Located', 'located'),
    ],
    healthBlurb: pick(healthObj, 'blurb') || null,
    globalRanks: arr(st.global_ranks).map((g) => ({ name: pick(g, 'label', 'name'), value: _util.num(numOr(pick(g, 'value'))), rank: pick(g, 'rank_label') || '' })),
    topPlayers: arr(st.players || st.top_players).slice(0, 10).map((t) => ({
      name: (pick(t, 'alliance_abbr', 'abbr') ? '[' + pick(t, 'alliance_abbr', 'abbr') + '] ' : '') + (pick(t, 'nick_name', 'name') || ''),
      furnace: numOr(pick(t, 'stove_lv', 'furnace')),
      power: numOr(t.power),
      tgLabel: pick(t, 'tg_label') || null,
      avatarUrl: absMaybe(pick(t, 'avatar_url')),
    })),
    // vs
    movementSince: pick(st, 'movement_since') || '',
    overall: { value: numOr(pick(st, 'mmr_overall', 'rating')), rank: pick(st, 'mmr_overall_rank') || '' },
    battle: { value: numOr(pick(st, 'mmr_battle')), rank: pick(st, 'mmr_battle_rank') || '' },
    prep: { value: numOr(pick(st, 'mmr_prep')), rank: pick(st, 'mmr_prep_rank') || '' },
    prepWL: pick(st, 'prep_wl') || '', taken: numOr(pick(st, 'castles_taken')), lost: numOr(pick(st, 'castles_lost')),
    battles: numOr(pick(st, 'battles', 'battle_count')),
    nextDraw: pick(st, 'next_draw_label') || null, drawGroup: pick(st, 'draw_group_label') || null,
    prepRaces: numOr(pick(st, 'prep_races')) ?? 0, mirrorNote: pick(st, 'mirror_note') || null,
    activity: arr(st.activity || s.activity), allianceList: arr(st.alliance_list || st.alliances || st.alliances_list),
    pyramid: st.pyramid || null,
    matchup: (() => {
      const m = st.kvk_matchup || st.matchup || null;
      if (!m) return null;
      const us = m.us || {};
      const them = m.opponent || m.them || {};
      const compare = arr(m.compare).map((c) => ({
        field: pick(c, 'field'),
        label: pick(c, 'label', 'name') || pick(c, 'field') || '—',
        us: numOr(pick(c, 'us', 'left')),
        them: numOr(pick(c, 'them', 'right')),
        usPct: numOr(pick(c, 'us_pct', 'pct')),
        lead: pick(c, 'lead') || null,
        kind: pick(c, 'kind') || 'power',
      }));
      // Fallback compare from us/them if API omitted compare array
      if (!compare.length && (us.kid || them.kid)) {
        const rows = [
          ['located', 'Active Players', 'count'],
          ['gov_power', 'Chief Power', 'power'],
          ['kills', 'Chief Kills', 'power'],
          ['troop_power', 'Troop Power', 'power'],
          ['migrant_score', 'State Transfer Score', 'power'],
          ['mystic_trial', 'The Labyrinth', 'power'],
        ];
        for (const [f, label, kind] of rows) {
          const a = numOr(us[f]), b = numOr(them[f]);
          if (a == null && b == null) continue;
          const sum = (a || 0) + (b || 0) || 1;
          compare.push({
            field: f, label, us: a, them: b,
            usPct: Math.round(((a || 0) / sum) * 1000) / 10,
            lead: (a || 0) >= (b || 0) ? 'us' : 'them',
            kind,
          });
        }
      }
      return {
        season: pick(m, 'season'),
        stage: pick(m, 'stage_name', 'stage'),
        stateName: pick(m, 'state_name', 'state'),
        bye: !!m.bye,
        us: {
          kid: numOr(pick(us, 'kid')) ?? numOr(kid),
          bannerUrl: absMaybe(pick(us, 'banner_url')),
          located: numOr(pick(us, 'located')),
          govPower: numOr(pick(us, 'gov_power')),
          kills: numOr(pick(us, 'kills')),
          troopPower: numOr(pick(us, 'troop_power')),
          migrantScore: numOr(pick(us, 'migrant_score')),
          mysticTrial: numOr(pick(us, 'mystic_trial')),
        },
        them: {
          kid: numOr(pick(them, 'kid')),
          bannerUrl: absMaybe(pick(them, 'banner_url')),
          located: numOr(pick(them, 'located')),
          govPower: numOr(pick(them, 'gov_power')),
          kills: numOr(pick(them, 'kills')),
          troopPower: numOr(pick(them, 'troop_power')),
          migrantScore: numOr(pick(them, 'migrant_score')),
          mysticTrial: numOr(pick(them, 'mystic_trial')),
        },
        compare,
        fetchedAt: numOr(pick(m, 'fetched_at')),
      };
    })(),

    svsHistory: (() => {
      const kvk = (st.throne && st.throne.kvk) || st.kvk || {};
      const rows = arr(kvk.cycles || kvk.matchups || kvk.history).map((c) => ({
        dateTs: numOr(pick(c, 'appointed_at', 'ts', 'at')),
        opponentKid: numOr(pick(c, 'opponent_kid', 'opponent')),
        prep: pick(c, 'prep'),
        castle: pick(c, 'castle'),
        result: pick(c, 'result'),
        label: pick(c, 'label'),
        // Daily prep scores are public and make the Prep points view possible.
        prepScore: numOr(pick(c, 'prep_score')),
        prepOppScore: numOr(pick(c, 'prep_opp_score')),
        days: arr(c.days).map((d) => ({
          day: numOr(pick(d, 'day')),
          status: pick(d, 'status') || null,
          beginTs: numOr(pick(d, 'begin_ts')),
          home: numOr(pick(d, 'home_score')),
          away: numOr(pick(d, 'away_score')),
        })).filter((d) => d.home != null || d.away != null),
        kingName: pick(c, 'nick_name', 'name'),
        kingTag: pick(c, 'alliance_abbr', 'abbr'),
        kingRole: pick(c, 'role') || (c.high_king ? 'Supreme President' : 'King'),
        kingAvatar: absMaybe(pick(c, 'avatar_url')),
        highKing: !!c.high_king,
        season: pick(c, 'season'),
      })).filter((r) => r.dateTs || r.opponentKid != null);
      rows.sort((a, b) => (b.dateTs || 0) - (a.dateTs || 0));
      return {
        won: numOr(pick(kvk, 'won', 'wins')) ?? (Array.isArray(kvk.wins) ? kvk.wins.length : null),
        lost: numOr(pick(kvk, 'lost', 'losses')) ?? (Array.isArray(kvk.losses) ? kvk.losses.length : null),
        held: numOr(pick(kvk, 'held_n', 'held')) ?? (Array.isArray(kvk.held) ? kvk.held.length : null),
        latestPrep: numOr(pick(kvk, 'latest_prep')),
        rows,
      };
    })(),
    asOf: tsOf(pick(st, 'updated_at', 'fetched_at', 'rank_updated', 'ingame_ranks_at')) || Date.now() / 1000,
  };
}

function absMaybe(u) {
  if (!u) return null;
  if (/^https?:\/\//i.test(u)) return u;
  if (String(u).startsWith('/')) return 'https://www.whiteoutstats.com' + u;
  return u;
}

async function loadBoard(kid, key) {
  const rows = await dataClient.getStateRanks(kid, key, 100);
  return arr(rows).map((e, i) => {
    // Alliance boards use abbr/name; player boards use nick_name + alliance_abbr
    const isAlliance = e.aid != null && e.nick_name == null && (e.abbr != null || e.name != null);
    return {
      rank: numOr(pick(e, 'rank', 'position')) ?? i + 1,
      name: isAlliance
        ? (pick(e, 'name') || pick(e, 'abbr') || 'Unknown')
        : (pick(e, 'nick_name', 'name') || pick(e, 'label') || 'Unknown'),
      tag: isAlliance
        ? (pick(e, 'abbr', 'tag') || null)
        : (pick(e, 'alliance_abbr', 'abbr', 'tag') || null),
      score: numOr(pick(e, 'score', 'value', 'power', 'game_power')),
      fid: pick(e, 'fid', 'chief_id'),
      uid: pick(e, 'uid'),
      aid: pick(e, 'aid'),
      furnace: numOr(pick(e, 'stove_lv', 'furnace')),
      members: numOr(pick(e, 'member_count', 'members')),
      isAlliance: !!isAlliance,
      troopLevels: normalizeTroopLevels(e),
    };
  });
}

/**
 * State players with troop level data (highest tiers first). Uses the state's
 * tracked players and enriches each chief with its `expedition_troops`.
 * Tries every available path (uid/fid, direct profile, secondary sources) to
 * maximize coverage. Used by the state "Troop Level" view.
 */
async function loadStateTroopPlayers(kid, limit = 200) {
  const st = await dataClient.getState(kid);
  if (!st) return [];
  const rawPlayers = arr(st.players || st.top_players).slice(0, limit);
  const seen = new Set();
  const players = [];
  const pushUnique = (p) => {
    if (!p) return;
    const key = String(p.uid != null ? p.uid : (p.fid != null ? p.fid : (p.chief_id != null ? p.chief_id : '')));
    if (!key || seen.has(key)) return;
    seen.add(key);
    players.push(p);
  };
  rawPlayers.forEach(pushUnique);

  // Also harvest members from every tracked alliance roster (extra coverage)
  try {
    const stObj = st.state || st.kingdom || st;
    const alliances = arr(st.alliances || st.alliance_list || st.alliances_list || stObj.alliances || stObj.alliance_list || stObj.alliances_list);
    const tags = alliances.map((a) => pick(a, 'abbr', 'tag', 'slug')).filter(Boolean).slice(0, 80);
    const ACONC = 5;
    let ai = 0;
    const aworker = async () => {
      while (ai < tags.length) {
        const j = ai++;
        try {
          const al = await dataClient.getAlliance(kid, tags[j]);
          const aobj = al && (al.alliance || al.info || al);
          const roster = arr((al && (al.roster || al.members)) || (aobj && (aobj.roster || aobj.members)));
          for (const m of roster) pushUnique(m);
        } catch { /* ignore */ }
      }
    };
    await Promise.all(Array.from({ length: Math.min(ACONC, tags.length || 1) }, () => aworker()));
  } catch { /* ignore */ }

  // Cap to keep enrichment feasible while covering as many as possible
  if (players.length > 400) players.length = 400;

  const CONC = 8;
  const results = new Array(players.length);
  let idx = 0;
  const worker = async () => {
    while (idx < players.length) {
      const i = idx++;
      const p = players[i];
      let full = null;
      // Try every identifier/type combination; stop when troop data is found.
      // Bulk roster loading: never trigger slow site-side refreshes + polling.
      // Direct fetches still try for troop data; raw state expedition_troops is
      // the fallback via normalizeTroopLevels(p). No site-side refresh waits.
      const troopOpts = { wantTroops: false, skipOracle: true };
      const attempts = [];
      if (p.uid != null) attempts.push({ id: String(p.uid), idType: 'uid' });
      if (p.fid != null) attempts.push({ id: String(p.fid), idType: 'fid' });
      if (!attempts.length && p.chief_id != null) attempts.push({ id: String(p.chief_id), idType: 'fid' });
      for (const a of attempts) {
        try {
          full = await dataClient.getPlayer(a.id, { idType: a.idType, ...troopOpts });
        } catch { full = null; }
        if (full && full.troopLevels) break;
      }
      // Last resort: try raw id without idType hint
      if ((!full || !full.troopLevels)) {
        const rawId = p.uid != null ? String(p.uid) : (p.fid != null ? String(p.fid) : null);
        if (rawId) {
          try { const alt = await dataClient.getPlayer(rawId, troopOpts); if (alt && (alt.troopLevels || !full)) full = alt; } catch {}
        }
      }
      const troopLevels = (full && full.troopLevels) ? full.troopLevels : normalizeTroopLevels(p);
      const troopArr = (full && Array.isArray(full.troop) && full.troop.length) ? full.troop : normalizeTroopsFromRaw(full || p);
      results[i] = {
        name: (full && full.name) || (p.nick_name || p.name) || 'Unknown',
        tag: (p.alliance_abbr || p.abbr || (full && full.alliance && full.alliance.tag) || null) || null,
        score: (full && full.power) != null ? full.power : numOr(p.power),
        fid: (full && full.chiefId) || (p.fid != null ? String(p.fid) : null),
        uid: (full && full.uid) || (p.uid != null ? String(p.uid) : null),
        isAlliance: false,
        troopLevels,
        troop: troopArr,
        troopLine: troopLineOf(full || p),
      };
    }
  };
  await Promise.all(Array.from({ length: Math.min(CONC, players.length || 1) }, () => worker()));
  return results.filter(Boolean).map((r, i) => ({ rank: i + 1, ...r }));
}

function normalizeTroopsFromRaw(p) {
  if (!p) return [];
  const raw = Array.isArray(p.expedition_troops) ? p.expedition_troops : (Array.isArray(p.troop) ? p.troop : []);
  const label = { inf: 'Infantry', lan: 'Lancer', mm: 'Marksman' };
  const best = new Map();
  for (const t of raw) {
    if (!t || t.tier == null) continue;
    const key = String(t.kind || '').toLowerCase();
    const tier = Number(t.tier);
    const star = Number(t.star) || 0;
    const prev = best.get(key);
    if (prev && prev.tier >= tier) continue;
    best.set(key, { kind: key, name: label[key] || key, tier, star, fc: star, count: Number(t.count) || 0 });
  }
  return [...best.values()];
}

module.exports = { BOARDS, boardName, boardGroup, playerView, progressionView, historyToView, loadAlliance, loadState, loadBoard, loadStateTroopPlayers, pick, numOr, arr, tsOf, roleLabel };
