'use strict';
const ownerOnly = require('../../utils/ownerOnly');

/**
 * Alliance lookup — private Oracle-style container.
 * Tab BUTTONS, active tab highlighted, then a titled body.
 * Tabs: Overview · Members · Furnace · Notice (+ Map sub-view)
 */

const { ModalBuilder, TextInputBuilder, TextInputStyle, ActionRowBuilder } = require('discord.js');
const ui = require('../ui/v2');
const makePanel = require('../ui/panelPost');
const dataClient = require('../services/dataClient');
const cards = require('../services/oracleCards');
const data = require('../services/lookupData');
const mapRefresh = require('../services/mapRefresh');

const TABS = [
  { key: 'overview', label: 'Overview' }, { key: 'members', label: 'Members' },
  { key: 'furnace', label: 'Furnace' }, { key: 'troopLevel', label: 'Troop Level' }, { key: 'notice', label: 'Notice' },
];
const SORTS = [
  { value: 'power', label: 'Power' }, { value: 'labyrinth', label: 'The Labyrinth score' },
  { value: 'furnace', label: 'Furnace' }, { value: 'role', label: 'Role' }, { value: 'name', label: 'Name' },
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
  type: 'alliance', title: '🤝 Alliance Lookup', buttonId: 'lookup:alliance:search', buttonLabel: 'Search Alliance', emoji: '🔍',
  lines: ['Search alliances by **State** then pick a tag.', '', '• Private results — only you see them', '• Overview · Members · Furnace · Notice'],
});

function searchModal() {
  return new ModalBuilder().setCustomId('lookup:alliance:modal').setTitle('Search Alliance').addComponents(
    new ActionRowBuilder().addComponents(
      new TextInputBuilder().setCustomId('state_id').setLabel('State number').setPlaceholder('e.g. 3105').setRequired(true).setStyle(TextInputStyle.Short).setMaxLength(6)),
    new ActionRowBuilder().addComponents(
      new TextInputBuilder().setCustomId('query').setLabel('Alliance tag or name').setPlaceholder('e.g. MYT').setRequired(false).setStyle(TextInputStyle.Short).setMaxLength(20))
  );
}

const nameOf = (a) => '[' + a.tag + '] ' + a.name;

function tabHeader(c, active) {
  // Map is a sub-view (like state map), not a main tab — still highlight nothing extra
  const key = TABS.some((t) => t.key === active) ? active : 'overview';
  ui.tabButtons(c, 'lookup:alliance:tab', TABS, key);
  ui.sep(c);
}

function playerSelect(c, list) {
  ui.select(c, 'lookup:alliance:player', 'Open a player…', list.filter((m) => m.fid).slice(0, 25).map((m) => ({
    label: m.name, value: String(m.fid), description: [ui.fmt(m.power), m.furnace ? 'F' + m.furnace : null, m.role].filter(Boolean).join(' · '),
  })));
}

function sorted(roster, mode) {
  const r = [...roster];
  if (mode === 'labyrinth') r.sort((a, b) => b.labyrinth - a.labyrinth);
  else if (mode === 'furnace') r.sort((a, b) => (b.furnace || 0) - (a.furnace || 0));
  else if (mode === 'role') r.sort((a, b) => b.roleNum - a.roleNum || (b.power || 0) - (a.power || 0));
  else if (mode === 'name') r.sort((a, b) => a.name.localeCompare(b.name));
  else r.sort((a, b) => (b.power || 0) - (a.power || 0));
  return r;
}

function sortTroopLevelRoster(roster) {
  return [...(roster || [])].sort((m1, m2) => {
    const aMax = Math.max(m1.troopLevels?.infantry || 0, m1.troopLevels?.lancer || 0, m1.troopLevels?.marksman || 0);
    const bMax = Math.max(m2.troopLevels?.infantry || 0, m2.troopLevels?.lancer || 0, m2.troopLevels?.marksman || 0);
    if (bMax !== aMax) return bMax - aMax;
    if (!!m2.troopLevels !== !!m1.troopLevels) return (m2.troopLevels ? 1 : 0) - (m1.troopLevels ? 1 : 0);
    return (m2.power || 0) - (m1.power || 0);
  });
}

async function ensureAllianceTroopLevels(s, { force = false } = {}) {
  const a = s.alliance;
  if (!a) return;
  const roster = a.roster || [];
  if (force) {
    for (const m of roster) { delete m.troopLevels; delete m.troop; }
  }
  const CONC = 8;
  let idx = 0;
  const enrich = async () => {
    while (idx < roster.length) {
      const i = idx++;
      const m = roster[i];
      if (m.troopLevels) continue;
      const attempts = [];
      if (m.uid != null) attempts.push({ id: String(m.uid), idType: 'uid' });
      if (m.fid != null) attempts.push({ id: String(m.fid), idType: 'fid' });
      let found = null;
      for (const atry of attempts) {
        try { found = await dataClient.getPlayer(atry.id, { idType: atry.idType, wantTroops: false, skipOracle: true }); } catch { found = null; }
        if (found && found.troopLevels) break;
      }
      if (found && found.troopLevels) { m.troopLevels = found.troopLevels; if (!m.troop || !m.troop.length) m.troop = found.troop; }
    }
  };
  await Promise.all(Array.from({ length: Math.min(CONC, roster.length || 1) }, () => enrich()));
  s.tlSorted = sortTroopLevelRoster(roster);
  s.tlWithData = roster.filter((m) => m.troopLevels).length;
  s.tlTotal = roster.length;
}

async function render(s) {
  const a = s.alliance, tab = s.tab;
  const c = ui.container(); const files = [];
  tabHeader(c, tab);

  if (tab === 'overview') {
    ui.title(c, nameOf(a));
    files.push(ui.attachment(await cards.allianceOverview(a), 'alliance.png')); ui.image(c, 'alliance.png');
    playerSelect(c, a.top);
    ui.buttons(c, [
      { id: 'lookup:alliance:go:map', label: 'Open map', emoji: '🗺️' },
      { id: 'lookup:alliance:go:state', label: 'S' + a.stateId },
    ]);
    ui.footer(c, 'roster as of', a.asOf);
  } else if (tab === 'map') {
    ui.title(c, nameOf(a) + ' — Hive map');
    const mapData = s.map || await dataClient.getStateMap(a.stateId);
    try {
      if (mapData && (mapData.cities || []).length) {
        files.push(ui.attachment(await cards.allianceMap(a, mapData), 'alliance-map.png'));
        ui.image(c, 'alliance-map.png');
        const mineCount = (mapData.cities || []).filter((city) => {
          if (a.aid != null && String(city.aid) === String(a.aid)) return true;
          const t = String(a.tag || a.abbr || '').toUpperCase();
          if (t && String(city.abbr || city.tag || city.alliance_abbr || '').toUpperCase() === t) return true;
          return false;
        }).length;
        const ago = mapData.stats && mapData.stats.location_updated_ago;
        ui.text(c, '**' + mineCount + '** cities on the map for this alliance · State ' + a.stateId + (ago ? ' · locations ' + ago : ''));
      } else {
        ui.text(c, 'Map data is not available for this state right now.');
      }
    } catch (_) {
      ui.text(c, 'Could not load map data.');
    }
    if (s.mapNotice) ui.text(c, s.mapNotice);
    ui.buttons(c, [
      { id: 'lookup:alliance:map:back', label: 'Back to overview', emoji: '↩️' },
      { id: 'lookup:alliance:map:refresh', label: 'Refresh map', emoji: '🔄' },
      ...(ownerOnly.isOwner(s.uid) ? [{ label: 'Open full map', url: 'https://whiteoutstats.com/map?kid=' + a.stateId, emoji: '🗺️' }] : []),
    ]);
    ui.footer(c, 'map as of', (mapData && mapData.stats && mapData.stats.location_updated_at) || a.asOf);
  } else if (tab === 'members') {
    let list = a.roster;
    list = sorted(list, s.sort);
    const pg = ui.paginate(list, 15, s.page);
    ui.title(c, nameOf(a) + ' — Members');
    ui.text(c, pg.items.map((m, i) => {
      const tlBits = troopBits(m);
      const tlStr = tlBits.length ? ` [${tlBits.join(' / ')}]` : '';
      return `${ui.rankChip(pg.page * 15 + i + 1)} **${m.name}** · ${ui.fmt(m.power)}${m.furnace ? ' · F' + m.furnace : ''}${m.role ? ' · ' + m.role : ''}${tlStr}`;
    }).join('\n') || 'No members listed.');
    ui.select(c, 'lookup:alliance:sort', 'Sort members…', SORTS.map((o) => ({ ...o, default: o.value === s.sort })));
    playerSelect(c, pg.items);
    ui.pager(c, 'lookup:alliance:pg', pg.page, pg.total);
    ui.footerRaw(c, `Page ${pg.page + 1} / ${pg.total} · ${a.roster.length} members`);
  } else if (tab === 'troopLevel') {
    // Troop Level ranking — use cached enriched+sorted list (pagination should not re-fetch)
    if (!s.tlSorted) await ensureAllianceTroopLevels(s);
    const sortedList = s.tlSorted || [];
    const withData = s.tlWithData != null ? s.tlWithData : sortedList.filter((m) => m.troopLevels).length;
    const totalMembers = s.tlTotal != null ? s.tlTotal : sortedList.length;
    const pg = ui.paginate(sortedList, 15, s.page);
    ui.title(c, nameOf(a) + ' — Troop Level');
    ui.text(c, `Members ranked by highest troop tier (T12 → T1). ${withData}/${totalMembers} members with troop data.`);
    ui.text(c, pg.items.map((m, i) => {
      const bits = troopBits(m);
      const tlBits = bits.length ? bits.join('  ·  ') : 'No troop data';
      const line = `${ui.rankChip(pg.page * 15 + i + 1)} **${m.name}** · ${ui.fmt(m.power)}\n   ${tlBits}`;
      return line;
    }).join('\n') || 'No members found.');
    ui.pager(c, 'lookup:alliance:tl:pg', pg.page, pg.total);
    ui.buttons(c, [
      { id: 'lookup:alliance:tl:refresh', label: 'Refresh troop levels', emoji: '🔄' },
      { id: 'lookup:alliance:tl:back', label: '← Back to overview', emoji: '↩️' },
    ]);
    ui.footerRaw(c, `Page ${pg.page + 1} / ${pg.total} · ${withData}/${totalMembers} with troop data`);
  } else if (tab === 'furnace') {
    const pg = ui.paginate(a.furnaceList, 15, s.page);
    ui.title(c, nameOf(a) + ' — Furnace levels');
    ui.text(c, pg.items.map((m, i) => `${ui.rankChip(pg.page * 15 + i + 1)} **${m.name}** · F${m.furnace} · ${ui.fmt(m.power)}`).join('\n') || 'No furnace data.');
    ui.pager(c, 'lookup:alliance:pg', pg.page, pg.total);
    ui.footerRaw(c, `Page ${pg.page + 1} / ${pg.total}`);
  } else if (tab === 'notice') {
    ui.title(c, nameOf(a) + ' — Notice');
    const notice = a.notice ? String(a.notice).trim() : '';
    const manifesto = a.manifesto ? String(a.manifesto).trim() : '';
    if (!notice && !manifesto) ui.text(c, 'This alliance has no notice or manifesto posted.');
    else {
      if (notice) ui.text(c, notice.slice(0, 1800));
      if (manifesto) ui.text(c, (notice ? '\n\n**Manifesto**\n' : '') + manifesto.slice(0, 1200));
    }
    ui.footer(c, 'notice as of', a.asOf);
  }
  return { container: c, files };
}

async function draw(interaction, s) {
  const { container, files } = await render(s);
  await interaction.editReply(ui.editPayload(container, files));
}

async function openAlliance(interaction, kid, tag) {
  const a = await data.loadAlliance(kid, tag);
  if (!a) { await interaction.editReply({ components: [], files: [], content: 'Alliance not found.' }); return null; }
  const s = { alliance: a, tab: 'overview', page: 0, sort: 'power' };
  setSession(interaction.user.id, s);
  await draw(interaction, s);
  return s;
}
const showAlliance = (interaction, kid, tag) => openAlliance(interaction, kid, tag);

/** Open alliance in a separate ephemeral message (keeps parent panel intact). */
async function openAlliancePrivate(interaction, kid, tag) {
  try {
    if (!interaction.deferred && !interaction.replied) if (!interaction.deferred && !interaction.replied) { try { await interaction.deferUpdate(); } catch (_) {} }
  } catch (_) {}
  const a = await data.loadAlliance(kid, tag);
  if (!a) {
    await interaction.followUp({ content: 'Alliance not found.', ephemeral: true });
    return null;
  }
  const s = { alliance: a, tab: 'overview', page: 0, sort: 'power' };
  setSession(interaction.user.id, s);
  const { container, files } = await render(s);
  await interaction.followUp({ ...ui.editPayload(container, files), ephemeral: true });
  return s;
}
const showAlliancePrivate = (interaction, kid, tag) => openAlliancePrivate(interaction, kid, tag);

async function handlePanelInteraction(interaction) {
  const id = interaction.customId;
  if (id === 'lookup:alliance:search') { await interaction.showModal(searchModal()); return true; }

  if (id === 'lookup:alliance:modal') {
    const stateRaw = interaction.fields.getTextInputValue('state_id').trim();
    const query = (interaction.fields.getTextInputValue('query') || '').trim();
    if (!/^\d+$/.test(stateRaw)) { await interaction.reply({ content: 'Enter a numeric State ID.', ephemeral: true }); return true; }
    const kid = Number(stateRaw);
    await interaction.deferReply({ ephemeral: true });
    let list = [];
    try { list = (await dataClient.searchAlliances(kid, query || 'a')) || []; } catch { list = []; }
    if (query) {
      const q = query.toLowerCase();
      list = list.filter((x) => [x.abbr, x.tag, x.name, x.slug].some((v) => String(v || '').toLowerCase().includes(q)));
    }
    if (!list.length) { await interaction.editReply({ content: 'No alliances found for State ' + kid + (query ? ' matching "' + query + '"' : '') + '.' }); return true; }
    const exact = list.find((x) => String(x.abbr || x.tag || x.slug || '').toUpperCase() === query.toUpperCase());
    const chosen = exact || (list.length === 1 ? list[0] : null);
    if (chosen) { await openAlliance(interaction, kid, chosen.abbr || chosen.tag || chosen.slug); return true; }
    const c = ui.container();
    ui.title(c, 'State ' + kid + ' — pick an alliance');
    setSession(interaction.user.id, { kid, pick: true });
    ui.select(c, 'lookup:alliance:pick', 'Select an alliance…', list.slice(0, 25).map((x) => ({
      label: '[' + (x.abbr || x.tag) + '] ' + (x.name || ''), value: String(x.abbr || x.tag || x.slug), description: x.power ? ui.fmt(x.power) + ' power' : undefined,
    })));
    await interaction.editReply(ui.editPayload(c));
    return true;
  }

  const s = getSession(interaction.user.id);
  if (!s) { await interaction.reply({ content: 'This lookup expired — press **Search Alliance** to start again.', ephemeral: true }); return true; }
  if (!interaction.deferred && !interaction.replied) { try { await interaction.deferUpdate(); } catch (_) {} }

  if (id === 'lookup:alliance:pick') { await openAlliance(interaction, s.kid, interaction.values[0]); return true; }
  if (!s.alliance) return true;

  // Older messages still carry a "Joined" tab; there is no public join feed,
  // so send those clicks to Members instead.
  if (id === 'lookup:alliance:tab:joined') { s.tab = 'members'; s.page = 0; await draw(interaction, s); return true; }

  if (id === 'lookup:alliance:map:refresh') {
    if (s.alliance.stateId == null) { s.tab = 'map'; await draw(interaction, s); return true; }
    const c0 = ui.container();
    ui.text(c0, '🔄 Refreshing the map from the website… this can take up to a minute.');
    await interaction.editReply(ui.editPayload(c0));

    const res = await mapRefresh.refreshStateMap(s.alliance.stateId);
    if (res.ok) { s.map = res.map; s.mapNotice = null; }
    else { s.mapNotice = '⚠️ ' + (res.message || 'Map refresh failed.'); }
    s.tab = 'map';
    await draw(interaction, s);
    return true;
  }

  if (id === 'lookup:alliance:map:back') { s.tab = 'overview'; await draw(interaction, s); return true; }
  if (id === 'lookup:alliance:tl:back') { s.tab = 'overview'; await draw(interaction, s); return true; }
  if (id === 'lookup:alliance:tl:refresh') {
    s.page = 0;
    const w = ui.container();
    ui.text(w, '⏳ Please wait a while…\nGetting troop level data for every alliance member from the website. This may take some time.');
    await interaction.editReply(ui.editPayload(w));
    await ensureAllianceTroopLevels(s, { force: true });
    await draw(interaction, s);
    return true;
  }
  if (id.startsWith('lookup:alliance:tab:')) {
    s.tab = id.split(':')[3]; s.page = 0;
    if (s.tab === 'troopLevel') {
      if (!s.tlSorted) {
        const w = ui.container();
        ui.text(w, '⏳ Please wait a while…\nGetting troop level data for every alliance member from the website. This may take some time.');
        await interaction.editReply(ui.editPayload(w));
        await ensureAllianceTroopLevels(s);
      }
    }
    await draw(interaction, s);
    return true;
  }
  if (id.startsWith('lookup:alliance:pg:p:')) { s.page = Number(id.split(':')[4]) || 0; await draw(interaction, s); return true; }
  if (id === 'lookup:alliance:sort') { s.sort = interaction.values[0]; s.page = 0; await draw(interaction, s); return true; }
  if (id.startsWith('lookup:alliance:tl:pg:p:')) {
    const parts = id.split(':');
    s.page = Number(parts[5]) || 0;
    await draw(interaction, s);
    return true;
  }
  if (id === 'lookup:alliance:player') { await require('./playerPanel').openById(interaction, interaction.values[0], 'overview', { idType: 'fid' }); return true; }
  if (id === 'lookup:alliance:go:state') { await require('./statePanel').showState(interaction, s.alliance.stateId); return true; }
  if (id === 'lookup:alliance:go:map') { s.tab = 'map'; await draw(interaction, s); return true; }
  return true;
}

module.exports = { ...panel, handlePanelInteraction, openAlliance, showAlliance, openAlliancePrivate, showAlliancePrivate };
