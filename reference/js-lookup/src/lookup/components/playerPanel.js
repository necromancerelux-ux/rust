'use strict';
const ownerOnly = require('../../utils/ownerOnly');

/**
 * Player lookup — private, Oracle-style container message.
 * Tabs live in a dropdown whose placeholder shows the CURRENT tab, exactly
 * like the reference. Each tab = one rendered image card + footer.
 */

const {
  ModalBuilder, TextInputBuilder, TextInputStyle, ActionRowBuilder,
} = require('discord.js');
const ui = require('../ui/v2');
const makePanel = require('../ui/panelPost');
const dataClient = require('../services/dataClient');
const cards = require('../services/oracleCards');
const data = require('../services/lookupData');
const mapRefresh = require('../services/mapRefresh');

let renderArenaLineup = null;
try { renderArenaLineup = require('../services/arenaRender').renderArenaLineup; } catch { /* optional */ }

let renderChiefGearCard = null;
let findLordGearPieces = null;
try {
  const lcr = require('../../giftcode/services/lookupCardRender');
  renderChiefGearCard = lcr.renderChiefGearCard;
  findLordGearPieces = lcr.findLordGearPieces;
} catch { /* optional */ }

const TABS = [
  { key: 'overview', label: 'Overview' },
  { key: 'standings', label: 'Standings' },
  { key: 'heroes', label: 'Heroes & Gear' },
  { key: 'progression', label: 'Progression' },
  { key: 'crowns', label: 'Crowns' },
];
const tabLabel = (k) => (TABS.find((t) => t.key === k) || {}).label || 'Overview';

const sessions = new Map();
const TTL = 15 * 60 * 1000;
function setSession(uid, s) { s.uid = String(uid); sessions.set(uid, { ...s, exp: Date.now() + TTL }); }
function getSession(uid) {
  const s = sessions.get(uid);
  if (!s) return null;
  if (Date.now() > s.exp) { sessions.delete(uid); return null; }
  return s;
}

const panel = makePanel({
  type: 'player', title: '🔎 Player Lookup', buttonId: 'lookup:player:search', buttonLabel: 'Search Player', emoji: '🔍',
  lines: ['Search any chief by **name or Player ID**.', '', '• Private results — only you see them', '• Overview · Standings · Heroes & Gear · Progression · Crowns'],
});

function searchModal() {
  return new ModalBuilder().setCustomId('lookup:player:modal').setTitle('Search Player').addComponents(
    new ActionRowBuilder().addComponents(
      new TextInputBuilder().setCustomId('player_id').setLabel('Player name or ID').setPlaceholder('e.g. Hafnar or 500423433')
        .setRequired(true).setStyle(TextInputStyle.Short).setMaxLength(40)
    )
  );
}

function tabSelect(c, active) {
  const key = TABS.some((t) => t.key === active) ? active : 'overview';
  ui.select(c, 'lookup:player:tab', tabLabel(key), TABS.map((t) => ({ label: t.label, value: t.key })));
}

/** Exploration towers from trails_list / trails_json (Land of Heroes, etc.). */
function normalizeTowers(raw) {
  if (!raw) return [];
  let list = [];
  if (Array.isArray(raw)) list = raw;
  else if (typeof raw === 'object') list = Object.values(raw).filter((v) => v && typeof v === 'object');
  return list.map((e) => ({
    name: e.name || e.label || ('Tower ' + (e.tower_id ?? e.id ?? '?')),
    stage: e.stage != null ? Number(e.stage) : null,
    score: e.score != null ? Number(e.score) : (e.level_id != null ? Number(e.level_id) : null),
    rank: e.rank != null ? Number(e.rank) : null,
    towerId: e.tower_id != null ? Number(e.tower_id) : null,
    valueLabel: e.value_label || null,
  })).filter((t) => t.name).sort((a, b) => (a.towerId || 0) - (b.towerId || 0));
}

/** Build the message for one tab. Returns { container, files }. */
async function render(session, tab) {
  const v = session.view;
  const c = ui.container();
  const files = [];
  tabSelect(c, tab);
  ui.sep(c);

  if (tab === 'overview') {
    ui.title(c, v.name);
    files.push(ui.attachment(await cards.playerOverview(v), 'player.png')); ui.image(c, 'player.png');
    const row = [];
    if (v.stateId) row.push({ id: 'lookup:player:go:state', label: 'S' + v.stateId });
    if (v.allianceTag) row.push({ id: 'lookup:player:go:alliance', label: '[' + v.allianceTag + '] Alliance' });
    if (v.x != null && v.y != null) row.push({ id: 'lookup:player:go:map', label: 'Locate on map', emoji: '🗺️' });
    row.push({ id: 'lookup:player:go:trail', label: 'Trails' });
    ui.buttons(c, row);
    ui.footer(c, 'profile as of', v.asOf);
  } else if (tab === 'standings') {
    ui.title(c, v.name + ' — Leaderboard standings');
    files.push(ui.attachment(await cards.playerStandings(v), 'standings.png')); ui.image(c, 'standings.png');
    const st = v.boards.filter((b) => b.rank);
    ui.select(c, 'lookup:player:board:state', 'Open a state board…', st.slice(0, 25).map((b) => ({
      label: b.name, value: b.key || b.name, description: '#' + b.rank + ' in S' + v.stateId + ' · ' + b.score,
    })));
    const gl = v.boards.filter((b) => b.global);
    ui.select(c, 'lookup:player:board:global', 'Open a global board…', gl.slice(0, 25).map((b) => ({
      label: b.name, value: b.key || b.name, description: b.global,
    })));
    ui.footer(c, 'boards as of', v.asOf);
  } else if (tab === 'heroes') {
    ui.title(c, v.name + ' — Heroes & Gear');
    if (!v.hasGear && !v.heroes) ui.text(c, 'No chief gear or arena heroes stored for this chief yet.');
    else {
      const bits = [];
      if (v.tgLabel || v.tgLevel != null) bits.push(v.tgLabel || ('TG' + v.tgLevel));
      if (v.vip != null) bits.push('VIP ' + v.vip + (v.vipActive ? '' : ' (inactive)'));
      if (v.levelLabel) bits.push(v.levelLabel);
      if (bits.length) ui.text(c, bits.join(' · '));
    }

    // Chief gear + charms — prefer Oracle loadout pieces, fall back to site gear
    let chiefPieces = null;
    let oracleLoadout = v.raw?.oracleLoadout || null;

    const extractPieces = (src) => {
      if (!src) return null;
      if (Array.isArray(src) && src.length) return src;
      if (Array.isArray(src.pieces) && src.pieces.length) return src.pieces;
      if (findLordGearPieces) {
        const found = findLordGearPieces(src);
        if (Array.isArray(found) && found.length) return found;
      }
      return null;
    };

    chiefPieces = extractPieces(oracleLoadout) || extractPieces(v.chiefGear) || extractPieces(v.raw);

    // Lazy-load Oracle loadout on demand if we still don't have gear (and not privacy-hidden)
    if ((!chiefPieces || !chiefPieces.length) && !v.gearPrivacy) {
      try {
        const oracle = require('../../giftcode/services/oracleClient');
        if (oracle && typeof oracle.fetchPlayerLoadout === 'function' && oracle.isConfigured && oracle.isConfigured()) {
          const fid = v.chiefId || v.uid || (session.player && (session.player.chiefId || session.player.uid));
          if (fid) {
            const res = await oracle.fetchPlayerLoadout(String(fid), { fresh: true });
            if (res && res.ok !== false) {
              oracleLoadout = res.loadout || res.data || res;
              if (!v.raw) v.raw = {};
              v.raw.oracleLoadout = oracleLoadout;
              chiefPieces = extractPieces(oracleLoadout) || chiefPieces;
            }
          }
        }
      } catch { /* optional */ }
    }

    let renderedChief = false;
    if (renderChiefGearCard && Array.isArray(chiefPieces) && chiefPieces.length) {
      try {
        const buf = await renderChiefGearCard(chiefPieces);
        if (buf) {
          files.push(ui.attachment(buf, 'chief-gear.png'));
          ui.image(c, 'chief-gear.png');
          renderedChief = true;
        }
      } catch { /* skip */ }
    }

    if (!renderedChief) {
      if (v.gearPrivacy) {
        ui.text(c, '🔒 ' + (v.gearPrivacyMessage || 'Chief Gear privacy is active — equipment is hidden on the public site.'));
      } else {
        ui.text(c, 'Chief gear & charms are not available for this chief right now. If the website recently added gear data, try again in a moment.');
      }
    }

    // Arena heroes + hero gear (Whiteout Stats)
    if (renderArenaLineup && v.heroes) {
      try {
        files.push(ui.attachment(await renderArenaLineup(v.heroes, v.name), 'arena.png'));
        ui.image(c, 'arena.png');
      } catch { /* skip image */ }
    }
    ui.footer(c, 'loadout as of', v.asOf || Date.now());
  } else if (tab === 'progression') {
    ui.title(c, v.name + ' — Progression');
    // Snapshot history is a separate public feed; fetch it lazily so normal
    // lookups stay fast.
    if (v.uid && !(v.history && (Array.isArray(v.history) || v.history.power || v.history.kills))) {
      try {
        const rows = await dataClient.getPlayerHistory(v.uid);
        if (rows.length) v.history = rows;
      } catch { /* card degrades to "not enough snapshots" */ }
    }
    files.push(ui.attachment(await cards.playerProgression(data.progressionView(v)), 'progression.png')); ui.image(c, 'progression.png');
    ui.footer(c, 'last snapshot', v.asOf);
  } else if (tab === 'crowns') {
    ui.title(c, v.name + ' — Crowns');
    files.push(ui.attachment(await cards.playerCrowns(v), 'crowns.png')); ui.image(c, 'crowns.png');
    ui.footer(c, 'crown record as of', v.asOf);
  } else if (tab === 'map') {
    ui.title(c, v.name + ' — Location');
    const fullMapBtn = ownerOnly.isOwner(session.uid)
      ? [{ label: 'Open full map', url: 'https://whiteoutstats.com/map?kid=' + v.stateId, emoji: '🗺️' }]
      : [];
    // Reuse the payload fetched by a refresh so coordinates and dots stay in sync.
    const mapData = session.map || (v.stateId != null ? await dataClient.getStateMap(v.stateId) : null);
    if (v.stateId == null) {
      ui.text(c, 'No state is linked to this chief, so a map cannot be loaded.');
    } else {
      const city = mapData ? mapRefresh.findCity(mapData, v) : null;
      if (city && city.x != null && city.y != null) { v.x = city.x; v.y = city.y; }
      const refreshBtn = { id: 'lookup:player:map:refresh', label: 'Refresh map', emoji: '🔄' };
      if (v.x == null || v.y == null) {
        ui.text(c, 'Coordinates are not available for this chief right now.');
        ui.buttons(c, [{ id: 'lookup:player:tab:overview', label: '← Back to overview' }, refreshBtn, ...fullMapBtn]);
      } else {
        try {
          if (mapData && (mapData.cities || []).length) {
            files.push(ui.attachment(await cards.playerMap(v, mapData), 'player-map.png'));
            ui.image(c, 'player-map.png');
            ui.text(c, 'State **' + v.stateId + '** · X **' + v.x + '** · Y **' + v.y + '**');
          } else {
            ui.text(c, 'State **' + v.stateId + '** · X **' + v.x + '** · Y **' + v.y + '**\n\nMap overlay data is not available right now.');
          }
        } catch (_) {
          ui.text(c, 'State **' + v.stateId + '** · X **' + v.x + '** · Y **' + v.y + '**\n\nCould not load map data.');
        }
        ui.buttons(c, [{ id: 'lookup:player:tab:overview', label: '← Back to overview' }, refreshBtn, ...fullMapBtn]);
      }
    }
    if (session.mapNotice) ui.text(c, session.mapNotice);
    ui.footer(c, 'position as of', (mapData && mapData.stats && mapData.stats.location_updated_at) || v.asOf);
  } else if (tab === 'trail') {
    ui.title(c, v.name + ' — Exploration trails');
    // Prefer dedicated trails payload; fall back to trail-flagged leaderboards
    let towers = normalizeTowers(v.trails);
    if (!towers.length && Array.isArray(v.boards)) {
      towers = v.boards.filter((b) => b.trail).map((b) => ({
        name: b.name,
        stage: null,
        score: b.rawScore,
        rank: b.rank,
        towerId: Number(b.key) || null,
        valueLabel: b.valueLabel,
      }));
    }
    if (!towers.length) {
      ui.text(c, 'No exploration trail scores recorded for this chief yet.');
    } else {
      const lines = towers.map((t, i) => {
        const rank = t.rank != null ? ui.rankChip(t.rank) : ui.rankChip(i + 1);
        const stage = t.valueLabel || (t.stage != null ? ('stage ' + t.stage) : null);
        const score = t.score != null ? ui.int(t.score) : '—';
        const bits = [rank + ' **' + t.name + '**', score];
        if (stage) bits.push(stage);
        if (t.rank != null) bits.push('#' + t.rank + ' in state');
        return bits.join(' · ');
      });
      ui.text(c, lines.join('\n'));
    }
    ui.buttons(c, [{ id: 'lookup:player:tab:overview', label: '← Back to overview' }]);
    ui.footer(c, 'trails as of', v.asOf);
  }
  return { container: c, files };
}

async function show(interaction, session, tab) {
  session.tab = tab;
  const { container, files } = await render(session, tab);
  await interaction.editReply(ui.editPayload(container, files));
}

/** Entry for other panels: open a player by chief ID inside the current ephemeral message. */
async function openById(interaction, id, tab = 'overview', opts = {}) {
  const fetchOpts = { wantTroops: true, troopRefreshAttempts: 1, troopRefreshDelayMs: 1000, ...opts };
  const p = await dataClient.getPlayer(id, fetchOpts);
  if (!p) {
    if (interaction.deferred || interaction.replied) await interaction.followUp({ content: 'Player not found.', ephemeral: true });
    else await interaction.reply({ content: 'Player not found.', ephemeral: true });
    return;
  }
  const session = { player: p, view: { ...data.playerView(p), raw: p.raw, trails: p.trails }, tab };
  setSession(interaction.user.id, session);
  await show(interaction, session, tab);
}

/** Open player in a separate ephemeral message (keeps parent panel intact). */
async function openByIdPrivate(interaction, id, tab = 'overview', opts = {}) {
  try {
    if (!interaction.deferred && !interaction.replied) if (!interaction.deferred && !interaction.replied) { try { await interaction.deferUpdate(); } catch (_) {} }
  } catch (_) {}
  const fetchOpts = { wantTroops: true, troopRefreshAttempts: 1, troopRefreshDelayMs: 1000, ...opts };
  const p = await dataClient.getPlayer(id, fetchOpts);
  if (!p) {
    await interaction.followUp({ content: 'Player not found.', ephemeral: true });
    return;
  }
  const session = { player: p, view: { ...data.playerView(p), raw: p.raw, trails: p.trails }, tab };
  setSession(interaction.user.id, session);
  const { container, files } = await render(session, tab);
  await interaction.followUp({ ...ui.editPayload(container, files), ephemeral: true });
}

async function handlePanelInteraction(interaction) {
  const id = interaction.customId;

  if (id === 'lookup:player:search') { await interaction.showModal(searchModal()); return true; }

  if (id === 'lookup:player:modal') {
    const raw = interaction.fields.getTextInputValue('player_id').trim();
    if (!raw) { await interaction.reply({ content: 'Enter a player name or Player ID.', ephemeral: true }); return true; }
    await interaction.deferReply({ ephemeral: true });
    const results = await dataClient.searchPlayers(raw);
    const looksLikeId = /^\d{4,}$/.test(raw.trim());
    if (!results.length) {
      if (looksLikeId) {
        await openById(interaction, raw.trim(), 'overview');
        return true;
      }
      await interaction.editReply({ content: 'No player found for `' + raw + '`.' });
      return true;
    }

    const rawId = raw.trim();
    const exactFid = results.find((r) => r && r.fid != null && String(r.fid) === rawId);
    const exactUid = results.find((r) => r && r.uid != null && String(r.uid) === rawId);
    const exactIdMatch = exactUid || exactFid || null;

    // Player ID is a unique identity: open directly, never force disambiguation.
    if (exactIdMatch) {
      await openById(
        interaction,
        exactIdMatch.uid || exactIdMatch.fid || rawId,
        'overview',
        exactIdMatch.uid ? { idType: 'uid' } : { idType: 'fid' }
      );
      return true;
    }
    if (looksLikeId) {
      await openById(interaction, rawId, 'overview');
      return true;
    }
    if (results.length === 1) {
      const chosen = results[0];
      await openById(
        interaction,
        chosen.uid || chosen.fid,
        'overview',
        chosen.uid ? { idType: 'uid' } : { idType: 'fid' }
      );
      return true;
    }

    // Name search matched several chiefs — let the user disambiguate.
    const c = ui.container();
    ui.title(c, 'Found ' + results.length + ' chiefs');
    ui.text(c, 'Pick the chief you meant for **' + raw + '**.');
    ui.select(c, 'lookup:player:pick', 'Select a chief…', results.slice(0, 25).map((r) => ({
      label: String(r.nick_name || ('Chief ' + (r.uid || r.fid))).slice(0, 100),
      value: String(r.uid || r.fid),
      description: [
        'ID ' + (r.fid || r.uid),
        r.kid != null ? 'State ' + r.kid : null,
        r.power != null ? ui.fmt(r.power) : null,
        r.alliance_abbr ? '[' + r.alliance_abbr + ']' : null,
      ].filter(Boolean).join(' · '),
    })));
    await interaction.editReply(ui.editPayload(c));
    return true;
  }

  if (id === 'lookup:player:pick') {
    if (!interaction.deferred && !interaction.replied) { try { await interaction.deferUpdate(); } catch (_) {} }
    await openById(interaction, interaction.values[0], 'overview', { idType: 'uid' });
    return true;
  }

  const session = getSession(interaction.user.id);
  if (!session) {
    await interaction.reply({ content: 'This lookup expired — press **Search Player** to start again.', ephemeral: true });
    return true;
  }
  if (!interaction.deferred && !interaction.replied) { try { await interaction.deferUpdate(); } catch (_) {} }

  if (id === 'lookup:player:map:refresh') {
    if (session.view.stateId == null) { await show(interaction, session, 'map'); return true; }
    const c0 = ui.container();
    ui.text(c0, '🔄 Refreshing the map from the website… this can take up to a minute.');
    await interaction.editReply(ui.editPayload(c0));

    const res = await mapRefresh.refreshStateMap(session.view.stateId);
    if (res.ok) {
      session.map = res.map;
      session.mapNotice = null;
      const city = mapRefresh.findCity(res.map, session.view);
      if (city) {
        if (city.x != null && city.y != null) { session.view.x = city.x; session.view.y = city.y; }
        if (city.kid != null) session.view.stateId = city.kid;
      } else {
        session.mapNotice = '⚠️ The map was refreshed, but this chief was not found on it. The coordinates above are from the previous scan.';
      }
    } else {
      session.mapNotice = '⚠️ ' + (res.message || 'Map refresh failed.');
    }
    await show(interaction, session, 'map');
    return true;
  }

  if (id === 'lookup:player:tab' && interaction.isStringSelectMenu()) { await show(interaction, session, interaction.values[0]); return true; }
  if (id.startsWith('lookup:player:tab:')) { await show(interaction, session, id.split(':')[3]); return true; }

  if (id === 'lookup:player:go:map') { await show(interaction, session, 'map'); return true; }
  if (id === 'lookup:player:go:trail') { await show(interaction, session, 'trail'); return true; }

  if (id === 'lookup:player:go:state') {
    await require('./statePanel').showState(interaction, session.view.stateId); return true;
  }
  if (id === 'lookup:player:go:alliance') {
    await require('./alliancePanel').showAlliance(interaction, session.view.stateId, session.view.allianceTag); return true;
  }
  if (id === 'lookup:player:board:state' || id === 'lookup:player:board:global') {
    await require('./statePanel').showBoard(interaction, session.view.stateId, interaction.values[0], 0); return true;
  }
  return true;
}

module.exports = { ...panel, handlePanelInteraction, openById, openByIdPrivate, render };
