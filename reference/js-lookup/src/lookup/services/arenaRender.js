'use strict';

/**
 * Combined Arena Lineup image (5 heroes + gear) for the private Player panel.
 * Adapts Whiteout Stats / Oracle hero shapes and reuses icon URLs.
 */

const { createCanvas, loadImage } = require('@napi-rs/canvas');

const ASSET_BASE = (process.env.ORACLE_BASE_URL || 'https://wosoracle.com').replace(/\/$/, '');
const SITE_ASSET_BASE = (process.env.WHITEOUTSTATS_SITE_URL || 'https://www.whiteoutstats.com').replace(/\/$/, '');

async function fetchImage(url) {
  if (!url) return null;
  try {
    const res = await fetch(url, {
      headers: {
        'User-Agent': 'Mozilla/5.0',
        Accept: 'image/png,image/*',
        Referer: `${ASSET_BASE}/`,
      },
    });
    if (!res.ok) return null;
    const ct = res.headers.get('content-type') || '';
    if (!ct.includes('image')) return null;
    const buf = Buffer.from(await res.arrayBuffer());
    if (buf.length < 80) return null;
    return loadImage(buf);
  } catch {
    return null;
  }
}

/** Try Oracle CDN first, then whiteoutstats icon path. */
async function fetchImageCandidates(urls) {
  for (const u of urls) {
    if (!u) continue;
    const img = await fetchImage(u);
    if (img) return img;
  }
  return null;
}

function heroBustUrls(id) {
  if (id == null) return [];
  return [
    `${ASSET_BASE}/assets/wos/heroes_bust/${id}.png`,
    `${SITE_ASSET_BASE}/assets/icons/hero_headpic_${id}.png`,
  ];
}

function heroGearUrls(eid) {
  if (eid == null) return [];
  return [
    `${ASSET_BASE}/assets/wos/hero_gear/${eid}.png`,
    `${SITE_ASSET_BASE}/assets/icons/equipment_icon_${eid}.png`,
  ];
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function drawStar(ctx, cx, cy, r, color = '#f5c542') {
  ctx.beginPath();
  for (let i = 0; i < 10; i++) {
    const ang = -Math.PI / 2 + (i * Math.PI) / 5;
    const rad = i % 2 === 0 ? r : r * 0.45;
    const px = cx + Math.cos(ang) * rad;
    const py = cy + Math.sin(ang) * rad;
    if (i === 0) ctx.moveTo(px, py);
    else ctx.lineTo(px, py);
  }
  ctx.closePath();
  ctx.fillStyle = color;
  ctx.fill();
}

/**
 * Normalize a single hero from either Whiteout Stats or Oracle shape.
 *
 * Whiteout Stats shape (public site /players/:uid):
 *   id, lv, star (often 1–30 ascension), name, pos
 *   exclusive_equipemnt: { id, elv }   // API typo: equipemnt
 *   exclusive_gear_level / widget_level
 *   gear_tiles: [{ eid, sid, slot, slv, rlv, gear_level, exclusive, icon, ... }]
 *   hero_equipment.equip_detail: { [uid]: { eid, slv, rlv, sid, ... } }
 *
 * Oracle shape:
 *   exclusive_gear / exclusive_equip, equipment: [{ eid, slv, rlv }]
 */
function normalizeHero(h) {
  if (!h || typeof h !== 'object') return null;
  const id = h.id ?? h.hero_id ?? h.hid;
  if (id == null) return null;

  const level = h.level ?? h.lv ?? h.hero_level ?? null;
  // Prefer display star count (1–5) when present; raw star may be ascension 1–30
  const star =
    h.star_level ??
    h.stars ??
    (h.star != null && Number(h.star) <= 5 ? h.star : null) ??
    h.star ??
    null;

  // Exclusive / widget — Whiteout Stats uses misspelled exclusive_equipemnt
  let exclusiveId = null;
  let exclusiveLv = null;
  const exObj =
    h.exclusive_equipemnt || // site API typo
    h.exclusive_equipment ||
    h.exclusive_gear ||
    null;
  if (exObj && typeof exObj === 'object') {
    exclusiveId = exObj.id ?? exObj.eid ?? null;
    exclusiveLv =
      exObj.elv ??
      exObj.level ??
      exObj.slv ??
      h.exclusive_gear_level ??
      h.widget_level ??
      null;
  } else if (h.exclusive_equip != null) {
    exclusiveId = h.exclusive_equip;
    exclusiveLv = h.exclusive_equip_lv ?? h.exclusive_gear_level ?? h.widget_level ?? null;
  } else if (h.exclusive_gear_level != null || h.widget_level != null) {
    exclusiveLv = h.exclusive_gear_level ?? h.widget_level;
  }

  // Gear slots (non-exclusive)
  // Priority: gear_tiles (site enriched) → hero_equipment.equip_detail → gear/equipment arrays
  const gearSlots = [];
  const seenEid = new Set();

  const pushSlot = (g) => {
    if (!g || typeof g !== 'object') return;
    const eid = g.eid ?? g.id ?? g.equip_id;
    if (eid == null) return;
    // Skip exclusive if already captured (site shows exclusive as separate weapon tile)
    if (g.exclusive || g.slot === 'Exclusive' || g.sid === 0) {
      if (exclusiveId == null) exclusiveId = eid;
      if (exclusiveLv == null) {
        exclusiveLv =
          g.elv ??
          g.gear_level ??
          g.level ??
          (Number(g.slv) <= 30 ? g.slv : null) ??
          null;
      }
      return;
    }
    if (seenEid.has(String(eid))) return;
    seenEid.add(String(eid));
    // Whiteout Stats: slv = imbue (+200), rlv = forge level (Lv. 20)
    // gear_level often mirrors forge; enhancement_level is rare
    const imbue = g.slv != null ? Number(g.slv) : (g.enhancement_level != null ? Number(g.enhancement_level) : null);
    const forge =
      g.rlv != null
        ? Number(g.rlv)
        : g.refine_level != null
          ? Number(g.refine_level)
          : g.gear_level != null
            ? Number(g.gear_level)
            : g.level != null && Number(g.level) <= 30
              ? Number(g.level)
              : null;
    gearSlots.push({
      eid,
      imbue: Number.isFinite(imbue) ? imbue : null,
      refine: Number.isFinite(forge) ? forge : null,
      // legacy level field: prefer forge for label when present
      level: Number.isFinite(forge) ? forge : (Number.isFinite(imbue) && imbue <= 30 ? imbue : null),
      slot: g.slot ?? g.sid ?? null,
    });
  };

  if (Array.isArray(h.gear_tiles)) {
    for (const g of h.gear_tiles) pushSlot(g);
  }

  const detail =
    h.hero_equipment &&
    typeof h.hero_equipment === 'object' &&
    h.hero_equipment.equip_detail &&
    typeof h.hero_equipment.equip_detail === 'object'
      ? h.hero_equipment.equip_detail
      : null;
  if (detail) {
    // Sort by sid so order is stable (1 weapon, 2 gloves, …)
    const rows = Object.values(detail).sort(
      (a, b) => (Number(a?.sid) || 0) - (Number(b?.sid) || 0)
    );
    for (const g of rows) pushSlot(g);
  }

  const rawGear = Array.isArray(h.gear)
    ? h.gear
    : Array.isArray(h.equipment)
      ? h.equipment
      : [];
  for (const g of rawGear) pushSlot(g);

  return {
    id,
    name: h.name || `Hero ${id}`,
    level,
    star,
    exclusiveId,
    exclusiveLv,
    gear: gearSlots,
    position: h.position ?? h.pos ?? h.slot ?? 0,
  };
}

/**
 * Render one combined image: up to 5 heroes in a vertical stack,
 * each with bust + exclusive + 4–5 gear pieces (matches the screenshot style).
 * @returns {Promise<Buffer|null>}
 */
async function renderArenaLineup(heroesRaw, playerName = '') {
  const list = (Array.isArray(heroesRaw) ? heroesRaw : [])
    .map(normalizeHero)
    .filter(Boolean)
    .sort((a, b) => (a.position || 0) - (b.position || 0))
    .slice(0, 5);

  if (!list.length) return null;

  const rowH = 118;
  const pad = 14;
  const gap = 10;
  const width = 520;
  const height = pad * 2 + list.length * rowH + (list.length - 1) * gap;

  const canvas = createCanvas(width, height);
  const ctx = canvas.getContext('2d');

  // Background
  ctx.fillStyle = '#0a0e14';
  ctx.fillRect(0, 0, width, height);

  // Soft header bar
  ctx.fillStyle = '#121820';
  roundRect(ctx, 6, 6, width - 12, height - 12, 12);
  ctx.fill();

  for (let i = 0; i < list.length; i++) {
    const h = list[i];
    const y = pad + i * (rowH + gap);

    // Row background
    ctx.fillStyle = i % 2 === 0 ? '#111820' : '#0e141c';
    roundRect(ctx, pad, y, width - pad * 2, rowH, 10);
    ctx.fill();

    // Bust
    const bust = await fetchImageCandidates(heroBustUrls(h.id));
    const bustSize = 90;
    const bx = pad + 10;
    const by = y + (rowH - bustSize) / 2;
    ctx.fillStyle = '#1a2330';
    roundRect(ctx, bx, by, bustSize, bustSize, 8);
    ctx.fill();
    if (bust) {
      ctx.drawImage(bust, bx + 4, by + 4, bustSize - 8, bustSize - 8);
    }

    // Level badge
    if (h.level != null) {
      ctx.fillStyle = 'rgba(0,0,0,0.7)';
      roundRect(ctx, bx + 4, by + 4, 42, 18, 5);
      ctx.fill();
      ctx.fillStyle = '#f1c40f';
      ctx.font = 'bold 11px sans-serif';
      ctx.fillText(`Lv.${h.level}`, bx + 8, by + 17);
    }

    // Stars (star_level 1–5, or raw ascension 1–30 → /6)
    if (h.star != null) {
      const n = Number(h.star);
      const starCount = Math.min(
        5,
        Math.max(1, n <= 5 ? Math.round(n) : Math.floor(n / 6) || 1)
      );
      for (let s = 0; s < starCount; s++) {
        drawStar(ctx, bx + 12 + s * 14, by + bustSize - 10, 5);
      }
    }

    // Gear icons row (exclusive first, then up to 4 regular pieces) — mirrors whiteoutstats
    const icons = [];
    if (h.exclusiveId != null) {
      icons.push({ eid: h.exclusiveId, level: h.exclusiveLv, imbue: null, exclusive: true });
    }
    for (const g of h.gear.slice(0, 4)) {
      if (g.eid != null) {
        icons.push({
          eid: g.eid,
          level: g.refine ?? g.level,
          imbue: g.imbue,
          exclusive: false,
        });
      }
    }

    const slotSize = 52;
    let ix = bx + bustSize + 18;
    const iy = y + (rowH - slotSize) / 2 - 4;

    for (const item of icons.slice(0, 5)) {
      // Slot background — gold frame for exclusive weapon
      if (item.exclusive) {
        ctx.fillStyle = '#2a2208';
        ctx.strokeStyle = '#f1c40f';
      } else {
        ctx.fillStyle = '#1a2330';
        ctx.strokeStyle = '#3d4f63';
      }
      roundRect(ctx, ix, iy, slotSize, slotSize, 7);
      ctx.fill();
      ctx.lineWidth = 1.5;
      roundRect(ctx, ix, iy, slotSize, slotSize, 7);
      ctx.stroke();

      const gImg = await fetchImageCandidates(heroGearUrls(item.eid));
      if (gImg) {
        ctx.drawImage(gImg, ix + 6, iy + 6, 40, 40);
      }

      // Imbue badge (e.g. +200) — top-right, matches site gear-tile-imbue
      if (item.imbue != null && Number(item.imbue) > 0) {
        const badge = `+${item.imbue}`;
        ctx.font = 'bold 10px sans-serif';
        const tw = ctx.measureText(badge).width + 6;
        const bx2 = ix + slotSize - tw - 2;
        const by2 = iy + 2;
        ctx.fillStyle = 'rgba(0,0,0,0.72)';
        roundRect(ctx, bx2, by2, tw, 14, 4);
        ctx.fill();
        ctx.fillStyle = Number(item.imbue) >= 200 ? '#fbbf24' : '#e8eaed';
        ctx.textAlign = 'center';
        ctx.fillText(badge, bx2 + tw / 2, by2 + 11);
        ctx.textAlign = 'left';
      }

      // Forge / exclusive level under icon (Lv. 20 or Lv. 10)
      if (item.level != null) {
        ctx.fillStyle = '#e8eaed';
        ctx.font = 'bold 11px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(`Lv.${item.level}`, ix + slotSize / 2, iy + slotSize + 14);
        ctx.textAlign = 'left';
      }

      ix += slotSize + 10;
    }
  }

  return canvas.toBuffer('image/png');
}

module.exports = {
  renderArenaLineup,
  normalizeHero,
};
