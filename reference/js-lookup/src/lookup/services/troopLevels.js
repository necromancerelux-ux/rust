'use strict';

const TROOP_FIELDS = {
  infantry: [
    'infantry_level', 'infantryLevel', 'infantry_lv', 'infantryLv', 'inf_lv',
    'infantry_tier', 'infantryTier', 'inf_tier', 'infantry',
  ],
  lancer: [
    'lancer_level', 'lancerLevel', 'lancer_lv', 'lancerLv', 'lanc_lv',
    'lancer_tier', 'lancerTier', 'lanc_tier', 'lancer',
  ],
  marksman: [
    'marksman_level', 'marksmanLevel', 'marksman_lv', 'marksmanLv', 'marks_lv',
    'marksman_tier', 'marksmanTier', 'marks_tier', 'archer_level', 'archer_tier',
    'marksman', 'archer',
  ],
};

const TROOP_KIND_KEY = { infantry: 'inf', lancer: 'lan', marksman: 'mm' };

function troopTier(value) {
  if (value && typeof value === 'object') {
    value = value.level ?? value.tier ?? value.troop_level ?? value.troopLevel ?? value.lv;
  }
  if (value == null || value === '' || typeof value === 'boolean') return null;
  const tier = Number(value);
  return Number.isInteger(tier) && tier >= 1 && tier <= 12 ? tier : null;
}

/** Find the troop entry for one type (flat fields first, then `expedition_troops`). */
function troopEntry(source, type) {
  const keys = TROOP_FIELDS[type] || [];
  const nested = [
    source.troop_levels,
    source.troopLevels,
    source.troop_tiers,
    source.troopTiers,
    source.troops,
    source.expedition_troops,
  ].filter((v) => v && typeof v === 'object');
  const candidates = [source, ...nested.filter((v) => !Array.isArray(v))];

  for (const candidate of candidates) {
    for (const key of keys) {
      const tier = troopTier(candidate[key]);
      if (tier != null) {
        const fc = candidate[key] && typeof candidate[key] === 'object' ? Number(candidate[key]) || null : null;
        return { tier, fc };
      }
    }
    const arr = Array.isArray(candidate.expedition_troops) ? candidate.expedition_troops
      : Array.isArray(candidate.troops) ? candidate.troops
      : [];
    const short = TROOP_KIND_KEY[type];
    const entry = arr.find((t) => t && String(t.kind || '').toLowerCase() === short);
    const tier = troopTier(entry);
    if (tier != null) {
      const fc = entry ? (Number(entry.star) || null) : null;
      return { tier, fc };
    }
  }
  return null;
}

/** `{ infantry: {tier, fc} | null, lancer: {...}, marksman: {...} } | null` */
function normalizeTroopEntries(source) {
  if (!source || typeof source !== 'object' || Array.isArray(source)) return null;
  const out = {};
  let any = false;
  for (const type of Object.keys(TROOP_FIELDS)) {
    const e = troopEntry(source, type);
    out[type] = e;
    if (e) any = true;
  }
  return any ? out : null;
}

/** Backwards-compatible numeric levels: `{ infantry: tier, lancer: tier, marksman: tier } | null`. */
function normalizeTroopLevels(source) {
  const entries = normalizeTroopEntries(source);
  if (!entries) return null;
  const out = {};
  let any = false;
  for (const type of Object.keys(TROOP_FIELDS)) {
    out[type] = entries[type] ? entries[type].tier : null;
    if (out[type] != null) any = true;
  }
  return any ? out : null;
}

module.exports = { normalizeTroopLevels, normalizeTroopEntries };
