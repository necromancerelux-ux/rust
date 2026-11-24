'use strict';
const ownerOnly = require('../../utils/ownerOnly');

/**
 * State lookup — private Oracle-style container.
 * Overview · State vs State · Rankings · Alliances
 * (+ map, appointments, matchup history as sub-views)
 */

const { ModalBuilder, TextInputBuilder, TextInputStyle, ActionRowBuilder } = require('discord.js');
const ui = require('../ui/v2');
const makePanel = require('../ui/panelPost');
const cards = require('../services/oracleCards');
const data = require('../services/lookupData');
const dataClient = require('../services/dataClient');
const mapRefresh = require('../services/mapRefresh');

const TABS = [
  { key: 'overview', label: 'Overview' },
  { key: 'svs', label: 'State vs State' },
  { key: 'boards', label: 'Rankings' },
  { key: 'alliances', label: 'Alliances' },
  { key: 'troopLevel', label: 'Troop Level' },
  { key: 'appointments', label: 'Appointments' },
];

/** Per-member troop line bits, e.g. ["Infantry · T12 · FC10"]. */
function troopBits(m) {
  const arr = (m && Array.isArray(m.troop)) ? m.troop : [];
  const bits = [];
  for (const t of arr) {
    const name = t.name || ({ inf: 'Infantry', lan: 'Lancer', mm: 'Marksman' }[t.kind] || t.kind);
    const tier = t.tier;
    const fc = t.fc != null ? t.fc : (t.star != null ? t.star : null);
    if (tier == null) continue;
    bits.push(fc != null ? `${name} · T${tier} · FC${fc}` : `${name} · T${tier}`);
  }
  if (bits.length) return bits;
  const tl = (m && m.troopLevels) || {};
  const names = { infantry: 'Infantry', lancer: 'Lancer', marksman: 'Marksman' };
  const bits2 = [];
  for (const k of ['infantry', 'lancer', 'marksman']) {
    const tier = tl[k];
    if (tier == null) continue;
    bits2.push(`${names[k]} · T${tier}`);
  }
  return bits2;
}

const tabLabel = (k) => (TABS.find((t) => t.key === k) || {}).label
  || (k === 'battle' ? 'State vs State'
    : k === 'map' ? 'Map'
    : k === 'appointments' ? 'Appointments'
    : k === 'history' ? 'Matchup history'
    : k === 'prep' ? 'Prep points'
    : 'Overview');

const sessions = new Map();
const TTL = 15 * 60 * 1000;
const setSession = (u, s) => { s.uid = String(u); sessions.set(u, { ...s, exp: Date.now() + TTL }); };
function getSession(u) {
  const s = sessions.get(u);
  if (!s) return null;
  if (Date.now() > s.exp) { sessions.delete(u); return null; }
  return s;
}

const panel = makePanel({
  type: 'state', title: '🏰 State Lookup', buttonId: 'lookup:state:search', buttonLabel: 'Search State', emoji: '🔍',
  lines: [
    'Look up any **State** by number.',
    '',
    '• Private results — only you see them',
    '• Overview · State vs State · Rankings · Alliances',
  ],
});

function searchModal() {
  return new ModalBuilder().setCustomId('lookup:state:modal').setTitle('Search State').addComponents(
    new ActionRowBuilder().addComponents(
      new TextInputBuilder().setCustomId('state_id').setLabel('State number').setPlaceholder('e.g. 3105').setRequired(true).setStyle(TextInputStyle.Short).setMaxLength(6))
  );
}

function tabSelect(c, active) {
  const key = TABS.some((t) => t.key === active) ? active : 'overview';
  ui.select(c, 'lookup:state:tab', tabLabel(key), TABS.map((t) => ({ label: t.label, value: t.key })));
}

function boardSelect(c, s) {
  ui.select(c, 'lookup:state:board', data.boardName(s.board), data.BOARDS.map(([key, label, group]) => ({
    label, value: key, description: (group || 'Rankings') + ' · top 100', default: key === s.board,
  })));
}

function profileSelect(c, entries, label = 'Open a player') {
  const opts = entries
    .map((e) => {
      const id = e.uid || e.fid || e.chief_id;
      if (!id || e.isAlliance) return null;
      return {
        label: ((e.tag ? '[' + e.tag + '] ' : '') + (e.name || 'Unknown')).slice(0, 100),
        value: String(id),
        description: e.score != null ? ui.fmt(e.score) : undefined,
      };
    })
    .filter(Boolean)
    .slice(0, 25);
  if (opts.length) ui.select(c, 'lookup:state:player', label, opts);
}

function allianceSelect(c, entries, label = 'Open an alliance') {
  const opts = entries
    .map((e) => {
      const tag = e.tag || e.abbr || e.alliance_abbr;
      if (!tag && !e.aid) return null;
      const value = tag || String(e.aid);
      const name = e.name && e.name !== tag ? e.name : '';
      return {
        label: ('[' + (tag || '???') + ']' + (name ? ' ' + name : '')).slice(0, 100),
        value: String(value),
        description: e.score != null ? ui.fmt(e.score) : (e.members != null ? e.members + ' members' : undefined),
      };
    })
    .filter(Boolean)
    .slice(0, 25);
  if (opts.length) ui.select(c, 'lookup:state:alliance', label, opts);
}

async function render(s) {
  const st = s.state, tab = s.tab;
  const c = ui.container(); const files = [];
  tabSelect(c, tab);
  ui.sep(c);

  if (tab === 'overview') {
    ui.title(c, 'State ' + st.id);
    files.push(ui.attachment(await cards.stateOverview(st), 'state.png')); ui.image(c, 'state.png');
    const bits = [];
    if (st.president) bits.push('**President** · ' + st.president);
    if (st.rulingAllianceTag) bits.push('**Alliance** · [' + st.rulingAllianceTag + ']' + (st.rulingAllianceName ? ' ' + st.rulingAllianceName : ''));
    if (st.rank != null) bits.push('**Rank** · #' + st.rank);
    if (st.grade && st.grade !== '—') bits.push('**Health** · ' + st.grade + (st.health != null ? ' (' + st.health + ')' : ''));
    if (bits.length) ui.text(c, bits.join(' · '));
    ui.buttons(c, [
      { id: 'lookup:state:go:map', label: 'Open map', emoji: '🗺️' },
    ]);
    ui.footer(c, 'data as of', st.asOf);
    } else if (tab === 'svs') {
    const m = st.matchup;
    ui.title(c, 'State ' + st.id + ' - State vs State' + (m?.season != null ? ' · Season ' + m.season : ''));
    try {
      files.push(ui.attachment(await cards.stateVsState(st), 'svs.png'));
      ui.image(c, 'svs.png');
    } catch (_) {
      ui.text(c, 'Could not render the State vs State card right now. Try again in a moment.');
    }
    if (m?.them?.kid != null) {
      ui.text(c, '**Opponent** · State ' + m.them.kid + (m.stage ? ' · ' + m.stage : '') + (m.stateName ? ' · ' + m.stateName : ''));
    } else {
      ui.text(c, 'No active SvS matchup for this state right now.');
    }
    if (st.mirrorNote) ui.text(c, st.mirrorNote);
    if (st.nextDraw) ui.text(c, 'Next draw ' + st.nextDraw + (st.drawGroup ? ' · ' + st.drawGroup : ''));
    // Only offer sub-views that actually have public data behind them.
    const svsButtons = [];
    const prepRows = ((st.svsHistory && st.svsHistory.rows) || []).filter((r) => r.prepScore != null || (r.days || []).length);
    if (st.battles != null || (m && m.them && m.them.kid != null)) svsButtons.push({ id: 'lookup:state:go:battle', label: 'Sunfire Castle battle' });
    if (((st.svsHistory && st.svsHistory.rows) || []).length) svsButtons.push({ id: 'lookup:state:go:history', label: 'Matchup history' });
    if (prepRows.length) svsButtons.push({ id: 'lookup:state:go:prep', label: 'Prep points' });
    if (st.president) svsButtons.push({ id: 'lookup:state:go:president', label: 'President - ' + st.president });
    if (svsButtons.length) ui.buttons(c, svsButtons);
    else ui.text(c, 'No State-vs-State details are published for this state right now.');
    ui.footer(c, 'data as of', st.asOf);
  } else if (tab === 'boards') {
    let list = s.entries;
    const pg = ui.paginate(list, 15, s.page);
    ui.title(c, 'State ' + st.id + ' — ' + data.boardName(s.board) + ' · ' + list.length + ' ranked');
    ui.text(c, pg.items.map((e) => {
      const tl = e.troopLevels;
      const tlBits = [];
      if (tl?.infantry != null) tlBits.push(`I${tl.infantry}`);
      if (tl?.lancer != null) tlBits.push(`L${tl.lancer}`);
      if (tl?.marksman != null) tlBits.push(`M${tl.marksman}`);
      const tlStr = tlBits.length ? ` [${tlBits.join('/')}]` : '';
      const name = e.isAlliance
        ? `**[${e.tag || '???'}] ${e.name}**` + (e.members != null ? ` · ${e.members} members` : '')
        : (e.tag ? `**[${e.tag}] ${e.name}**` : `**${e.name}**`) + (e.furnace != null ? ` · F${e.furnace}` : '');
      return `${ui.rankChip(e.rank)} ${name} · ${ui.int(e.score)}${tlStr}`;
    }).join('\n') || 'No rankings for this board yet.');
    boardSelect(c, s);
    ui.pager(c, 'lookup:state:pg', pg.page, pg.total);
    const boardKey = String(s.board || '');
    const isAllianceBoard =
      boardKey === '1' || boardKey === '2' ||
      /alliance/i.test(data.boardName(s.board) || '') ||
      (pg.items.length > 0 && pg.items.every((e) => e.isAlliance));
    if (isAllianceBoard) allianceSelect(c, pg.items, 'Open an alliance');
    else profileSelect(c, pg.items, 'Open a player');
    ui.footer(c, 'data as of', st.asOf);
  } else if (tab === 'troopLevel') {
    // Troop Level ranking — players sorted by highest troop tier (T12 → T1)
    if (!s.tlPlayers) {
      s.tlPlayers = await data.loadStateTroopPlayers(st.id);
      s.tlPage = 0;
    }
    const allPlayers = s.tlPlayers || [];
    const withData = allPlayers.filter((e) => e.troopLevels).length;
    const sorted = [...allPlayers].sort((a, b) => {
      const aMax = Math.max(a.troopLevels?.infantry || 0, a.troopLevels?.lancer || 0, a.troopLevels?.marksman || 0);
      const bMax = Math.max(b.troopLevels?.infantry || 0, b.troopLevels?.lancer || 0, b.troopLevels?.marksman || 0);
      if (bMax !== aMax) return bMax - aMax;
      if (!!b.troopLevels !== !!a.troopLevels) return (b.troopLevels ? 1 : 0) - (a.troopLevels ? 1 : 0);
      return (b.score || 0) - (a.score || 0);
    });
    const pg = ui.paginate(sorted, 15, s.tlPage);
    ui.title(c, 'State ' + st.id + ' — Troop Level Rankings');
    ui.text(c, `Players ranked by highest troop tier (T12 → T1). ${withData}/${allPlayers.length} players with troop data.`);
    ui.text(c, pg.items.map((e, i) => {
      const bits = troopBits(e);
      const tlBits = bits.length ? bits.join('  ·  ') : 'No troop data';
      const name = (e.tag ? `**[${e.tag}] ${e.name}**` : `**${e.name}**`);
      return `${ui.rankChip(pg.page * 15 + i + 1)} ${name} · ${ui.fmt(e.score)}\n   ${tlBits}`;
    }).join('\n') || 'No players found.');
    ui.pager(c, 'lookup:state:tl:pg', pg.page, pg.total);
    ui.buttons(c, [
      { id: 'lookup:state:tl:refresh', label: 'Refresh troop levels', emoji: '🔄' },
      { id: 'lookup:state:tl:back', label: '← Back to overview' },
    ]);
    ui.footer(c, 'troop levels as of', st.asOf);
  } else if (tab === 'alliances') {
    const pg = ui.paginate(st.allianceList, 15, s.page);
    ui.title(c, 'State ' + st.id + ' — Alliances');
    ui.text(c, pg.items.map((a, i) => `${ui.rankChip(pg.page * 15 + i + 1)} **[${a.abbr || a.tag}] ${a.name}** · ${ui.fmt(a.power)} · ${a.members ?? a.member_count ?? '—'} members · ${ui.fmt(a.kills)} kills`).join('\n') || 'No alliances tracked.');
    ui.pager(c, 'lookup:state:pg', pg.page, pg.total);
    allianceSelect(c, pg.items.map((a) => ({
      tag: a.abbr || a.tag,
      abbr: a.abbr || a.tag,
      name: a.name,
      aid: a.aid,
      members: a.members ?? a.member_count,
      score: a.power,
    })), 'Open an alliance');
    ui.footer(c, 'data as of', st.asOf);
  } else if (tab === 'map') {
    ui.title(c, 'State ' + st.id + ' — Map');
    const mapData = s.map || await dataClient.getStateMap(st.id);
    try {
      if (mapData && (mapData.cities || []).length) {
        files.push(ui.attachment(await cards.stateMap(st, mapData), 'state-map.png'));
        ui.image(c, 'state-map.png');
        const stats = mapData.stats || {};
        const bits = [];
        if (mapData.cities) bits.push('**' + mapData.cities.length + '** cities');
        if (mapData.alliances) bits.push('**' + mapData.alliances.length + '** alliances');
        if (stats.location_updated_ago) bits.push('locations ' + stats.location_updated_ago);
        if (bits.length) ui.text(c, bits.join(' · '));
      } else {
        ui.text(c, 'Map data is not available for this state right now.');
      }
    } catch (e) {
      ui.text(c, 'Could not load map data.');
    }
    if (s.mapNotice) ui.text(c, s.mapNotice);
    ui.buttons(c, [
      { id: 'lookup:state:map:refresh', label: 'Refresh map', emoji: '🔄' },
      ...(ownerOnly.isOwner(s.uid) ? [{ label: 'Open full map', url: 'https://whiteoutstats.com/map?kid=' + st.id, emoji: '🗺️' }] : []),
      { id: 'lookup:state:map:back', label: '← Back to overview' },
    ]);
    ui.footer(c, 'map as of', (mapData && mapData.stats && mapData.stats.location_updated_at) || st.asOf);
  } else if (tab === 'prep') {
    ui.title(c, 'State ' + st.id + ' - Prep points');
    const rows = (((st.svsHistory && st.svsHistory.rows) || [])
      .filter((r) => r.prepScore != null || (r.days || []).length))
      .slice(0, 6);
    if (!rows.length) {
      ui.text(c, 'Prep scores are not published for this state yet.');
    } else {
      const latest = rows[0];
      const opponent = latest.opponentKid != null ? 'State ' + latest.opponentKid : 'Unknown opponent';
      const verdict = latest.prep === 'won' ? 'won' : latest.prep === 'lost' ? 'lost' : (latest.result || null);
      try {
        files.push(ui.attachment(await cards.statePrepPoints(st, latest), 'prep-points.png'));
        ui.image(c, 'prep-points.png');
      } catch (_) {
        // fallback to text if rendering fails
      }
      ui.text(c, '**Latest:** ' + opponent + ' · S' + (latest.season ?? '?') + (verdict ? ' · prep ' + verdict : '') + ' · ' + ui.fmt(latest.prepScore) + ' vs ' + ui.fmt(latest.prepOppScore));
      if (rows.length > 1) {
        const extra = rows.slice(1).map((r) => {
          const opp = r.opponentKid != null ? 'State ' + r.opponentKid : 'Unknown opponent';
          const v = r.prep === 'won' ? 'won' : r.prep === 'lost' ? 'lost' : (r.result || null);
          return '- **' + opp + '** · S' + (r.season ?? '?') + (v ? ' · ' + v : '') + ' · ' + ui.fmt(r.prepScore) + ' vs ' + ui.fmt(r.prepOppScore);
        }).join('\n');
        ui.text(c, extra);
      }
    }
    ui.buttons(c, [{ id: 'lookup:state:svs:back', label: '← Back to State vs State' }]);
    ui.footer(c, 'prep scores as of', st.asOf);
  } else if (tab === 'appointments') {
    ui.title(c, 'State ' + st.id + ' — Appointments');
    try {
      const ap = await dataClient.getStateAppointments(st.id);
      if (ap && ((ap.ministers && ap.ministers.length) || (ap.offenders && ap.offenders.length))) {
        files.push(ui.attachment(await cards.stateAppointments(st, ap), 'appointments.png'));
        ui.image(c, 'appointments.png');
        const fm = (ap.ministers || []).filter((m) => !m.vacant && (m.nick_name || m.uid)).length;
        const fo = (ap.offenders || []).filter((m) => !m.vacant && (m.nick_name || m.uid)).length;
        ui.text(c, '**Ministers** · ' + fm + '/' + Math.max((ap.ministers || []).length, 6) + '  ·  **Offenders** · ' + fo);
      } else {
        ui.text(c, 'No appointment data available for this state yet.');
      }
    } catch (e) {
      ui.text(c, 'Could not load appointments.');
    }
    ui.footer(c, 'data as of', st.asOf);
  } else if (tab === 'history') {
    ui.title(c, 'State ' + st.id + ' - Matchup history');
    if (st.svsHistory && (st.svsHistory.rows || []).length) {
      try {
        files.push(ui.attachment(await cards.stateMatchupHistory(st), 'svs-history.png'));
        ui.image(c, 'svs-history.png');
      } catch (_) {
        ui.text(c, 'Could not render matchup history right now. Try again in a moment.');
      }
    } else {
      ui.text(c, 'No matchup history recorded yet.');
    }
    ui.buttons(c, [{ id: 'lookup:state:svs:back', label: '← Back to State vs State' }]);
    ui.footer(c, 'data as of', st.asOf);
  } else if (tab === 'battle') {
    ui.title(c, 'State ' + st.id + ' · Sunfire Castle battle');
    ui.text(c, s.battleText || 'No Sunfire Castle battle on record for this state yet.');
    ui.buttons(c, [{ id: 'lookup:state:svs:back', label: '← Back to State vs State' }]);
    ui.footerRaw(c, 'retrieved just now');
  } else if (tab === 'note') {
    ui.title(c, 'State ' + st.id);
    ui.text(c, s.note || 'Nothing recorded yet.');
    const fromPrep = (s.note || '').toLowerCase().includes('prep');
    ui.buttons(c, [{
      id: fromPrep ? 'lookup:state:svs:back' : 'lookup:state:overview:back',
      label: fromPrep ? '← Back to State vs State' : '← Back to overview',
    }]);
    ui.footer(c, 'data as of', st.asOf);
  }
  return { container: c, files };
}

async function draw(interaction, s) {
  const { container, files } = await render(s);
  await interaction.editReply(ui.editPayload(container, files));
}

async function ensure(interaction, kid) {
  const st = await data.loadState(kid);
  if (!st) { await interaction.editReply({ components: [], files: [], content: 'State ' + kid + ' not found.' }); return null; }
  const s = { state: st, tab: 'overview', page: 0, board: '3', entries: [] };
  setSession(interaction.user.id, s);
  await draw(interaction, s);
  return s;
}

const showState = (interaction, kid) => ensure(interaction, kid);

async function showBoard(interaction, kid, key) {
  let s = getSession(interaction.user.id);
  if (!s || !s.state || Number(s.state.id) !== Number(kid)) {
    const st = await data.loadState(kid);
    if (!st) { await interaction.editReply({ components: [], files: [], content: 'State ' + kid + ' not found.' }); return; }
    s = { state: st, tab: 'boards', page: 0, board: key || '3', entries: [] };
    setSession(interaction.user.id, s);
  }
  s.tab = 'boards';
  s.board = key || s.board || '3';
  s.page = 0;
  s.entries = await data.loadBoard(kid, s.board);
  await draw(interaction, s);
}

async function handlePanelInteraction(interaction) {
  const id = interaction.customId;
  if (id === 'lookup:state:search') { await interaction.showModal(searchModal()); return true; }

  if (id === 'lookup:state:modal') {
    const raw = interaction.fields.getTextInputValue('state_id').trim();
    if (!/^\d+$/.test(raw)) { await interaction.reply({ content: 'Enter a numeric State ID.', ephemeral: true }); return true; }
    await interaction.deferReply({ ephemeral: true });
    await showState(interaction, Number(raw));
    return true;
  }

  const s = getSession(interaction.user.id);
  if (!s) { await interaction.reply({ content: 'This lookup expired — press **Search State** to start again.', ephemeral: true }); return true; }
  if (!interaction.deferred && !interaction.replied) { try { await interaction.deferUpdate(); } catch (_) {} }

  if (id === 'lookup:state:tab') {
    s.tab = interaction.values[0]; s.page = 0;
    if (s.tab === 'boards' && !s.entries.length) s.entries = await data.loadBoard(s.state.id, s.board);
    if (s.tab === 'troopLevel' && !s.tlPlayers) {
      const w = ui.container();
      ui.text(w, '⏳ Please wait a while…\nGetting troop level data for every state player from the website. This may take some time.');
      await interaction.editReply(ui.editPayload(w));
      s.tlPlayers = await data.loadStateTroopPlayers(s.state.id);
      s.tlPage = 0;
    }
    await draw(interaction, s); return true;
  }
  if (id.startsWith('lookup:state:tab:')) {
    s.tab = id.split(':')[3]; s.page = 0;
    if (s.tab === 'troopLevel' && !s.tlPlayers) {
      const w = ui.container();
      ui.text(w, '⏳ Please wait a while…\nGetting troop level data for every state player from the website. This may take some time.');
      await interaction.editReply(ui.editPayload(w));
      s.tlPlayers = await data.loadStateTroopPlayers(s.state.id);
      s.tlPage = 0;
    }
    await draw(interaction, s);
    return true;
  }
  if (id.startsWith('lookup:state:pg:p:')) { s.page = Number(id.split(':')[4]) || 0; await draw(interaction, s); return true; }
  if (id === 'lookup:state:board') { s.board = interaction.values[0]; s.page = 0; s.entries = await data.loadBoard(s.state.id, s.board); await draw(interaction, s); return true; }
  if (id.startsWith('lookup:state:tl:pg:p:')) {
    const parts = id.split(':');
    s.tlPage = Number(parts[5]) || 0;
    await draw(interaction, s);
    return true;
  }
  if (id === 'lookup:state:tl:refresh') {
    s.tlPage = 0;
    const w = ui.container();
    ui.text(w, '⏳ Please wait a while…\nGetting troop level data for every state player from the website. This may take some time.');
    await interaction.editReply(ui.editPayload(w));
    s.tlPlayers = await data.loadStateTroopPlayers(s.state.id);
    await draw(interaction, s);
    return true;
  }
  if (id === 'lookup:state:tl:back') { s.tab = 'overview'; s.page = 0; await draw(interaction, s); return true; }
  if (id === 'lookup:state:map:back') { s.tab = 'overview'; await draw(interaction, s); return true; }
  if (id === 'lookup:state:svs:back') { s.tab = 'svs'; await draw(interaction, s); return true; }
  if (id === 'lookup:state:prep:back') { s.tab = 'svs'; await draw(interaction, s); return true; }
  if (id === 'lookup:state:overview:back') { s.tab = 'overview'; await draw(interaction, s); return true; }

  if (id === 'lookup:state:player') {
    await require('./playerPanel').openByIdPrivate(interaction, interaction.values[0]);
    return true;
  }
  if (id === 'lookup:state:alliance') {
    await require('./alliancePanel').openAlliancePrivate(interaction, s.state.id, interaction.values[0]);
    return true;
  }

  if (id === 'lookup:state:go:map') { s.tab = 'map'; await draw(interaction, s); return true; }
  if (id === 'lookup:state:map:refresh') {
    const c0 = ui.container();
    ui.text(c0, '🔄 Refreshing the map from the website… this can take up to a minute.');
    await interaction.editReply(ui.editPayload(c0));

    const res = await mapRefresh.refreshStateMap(s.state.id);
    if (res.ok) { s.map = res.map; s.mapNotice = null; }
    else { s.mapNotice = '⚠️ ' + (res.message || 'Map refresh failed.'); }
    s.tab = 'map';
    await draw(interaction, s);
    return true;
  }
  if (id === 'lookup:state:go:appointments') { s.tab = 'appointments'; await draw(interaction, s); return true; }
  if (id === 'lookup:state:go:history') { s.tab = 'history'; await draw(interaction, s); return true; }
  if (id === 'lookup:state:go:battle') { s.tab = 'battle'; await draw(interaction, s); return true; }
  if (id === 'lookup:state:go:prep') { s.tab = 'prep'; await draw(interaction, s); return true; }
  if (id === 'lookup:state:go:president' && s.state.president) {
    s.tab = 'note';
    s.note = '**' + s.state.president + '** — ' + (s.state.presidentSub || 'President');
    await draw(interaction, s);
    return true;
  }
  return true;
}

module.exports = { ...panel, handlePanelInteraction, showState, showBoard, render };
