'use strict';

/**
 * Oracle-style image cards (dark navy glass panel, purple section label,
 * light-blue headline numbers, footer "WOS ORACLE … as of <date>").
 * All renderers are pure: they take a plain view-model and return a PNG Buffer.
 */

const { createCanvas, loadImage } = require('@napi-rs/canvas');

const C = {
  bg1: '#0a1626', bg2: '#10294a', edge: '#27507f', edge2: '#14304f',
  text: '#eef4fb', dim: '#8fa5bf', faint: '#5d7390',
  label: '#b78cff', num: '#6cc4ff', green: '#4fd37c', red: '#ff5d5d',
  amber: '#f0a24a', gold: '#f5c542', row: 'rgba(120,170,230,0.07)', line: 'rgba(130,170,220,0.18)',
  bar: '#4aa8f0', barBg: 'rgba(255,255,255,0.10)', orange: '#f59f3a',
};
const FONT = '"Segoe UI", "Noto Sans", "DejaVu Sans", Arial, sans-serif';
const W = 1060;

const f = (size, weight = '400') => `${weight} ${size}px ${FONT}`;

function rr(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function num(n) {
  if (n == null || n === '' || !Number.isFinite(Number(n))) return '—';
  n = Number(n);
  const a = Math.abs(n), s = n < 0 ? '−' : '';
  const t = (v, d) => String(Number(v.toFixed(d)));
  if (a >= 1e9) return s + t(a / 1e9, 2) + 'B';
  if (a >= 1e6) return s + t(a / 1e6, 2) + 'M';
  if (a >= 1e3) return s + t(a / 1e3, 1) + 'K';
  return s + String(Math.round(a));
}
const int = (n) => (n == null || !Number.isFinite(Number(n)) ? '—' : Number(n).toLocaleString('en-US'));

function utc(ts) {
  const d = ts ? new Date(ts > 1e12 ? ts : ts * 1000) : new Date();
  const M = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
  const p = (x) => String(x).padStart(2, '0');
  return `${d.getUTCDate()} ${M[d.getUTCMonth()]} ${d.getUTCFullYear()}, ${p(d.getUTCHours())}:${p(d.getUTCMinutes())} UTC`;
}
function ago(ts) {
  if (!ts) return 'just now';
  const s = Math.max(0, Date.now() / 1000 - (ts > 1e12 ? ts / 1000 : ts));
  if (s < 90) return 'just now';
  if (s < 3600) return Math.round(s / 60) + 'm ago';
  if (s < 86400) return Math.round(s / 3600) + 'h ago';
  if (s < 86400 * 30) return Math.round(s / 86400) + 'd ago';
  return Math.round(s / (86400 * 30)) + 'mo ago';
}

function fit(ctx, str, maxW) {
  str = String(str ?? '');
  if (ctx.measureText(str).width <= maxW) return str;
  while (str.length > 1 && ctx.measureText(str + '…').width > maxW) str = str.slice(0, -1);
  return str + '…';
}

/** Base frame; returns { canvas, ctx } sized W×H */
function frame(H) {
  const canvas = createCanvas(W, H);
  const ctx = canvas.getContext('2d');
  const g = ctx.createLinearGradient(0, 0, W, H);
  g.addColorStop(0, C.bg1); g.addColorStop(1, C.bg2);
  rr(ctx, 0, 0, W, H, 26); ctx.fillStyle = g; ctx.fill();
  rr(ctx, 14, 14, W - 28, H - 28, 18);
  ctx.strokeStyle = C.edge; ctx.lineWidth = 2; ctx.stroke();
  rr(ctx, 20, 20, W - 40, H - 40, 14);
  ctx.strokeStyle = C.edge2; ctx.lineWidth = 1; ctx.stroke();
  // corner studs
  ctx.fillStyle = '#3a6fa8';
  for (const [x, y] of [[26, 26], [W - 26, 26], [26, H - 26], [W - 26, H - 26]]) {
    ctx.beginPath(); ctx.arc(x, y, 3, 0, Math.PI * 2); ctx.fill();
  }
  return { canvas, ctx };
}

function footerBar(ctx, H, right) {
  ctx.textBaseline = 'alphabetic';
  ctx.font = f(13, '700'); ctx.fillStyle = C.faint; ctx.textAlign = 'left';
  ctx.fillText('OMNICORE', 42, H - 34);
  ctx.font = f(13); ctx.textAlign = 'right';
  ctx.fillText(right || '', W - 42, H - 34);
  ctx.textAlign = 'left';
}

function label(ctx, str, x, y) {
  ctx.font = f(13, '800'); ctx.fillStyle = C.label; ctx.textAlign = 'left';
  ctx.fillText(String(str).toUpperCase(), x, y);
}
function small(ctx, str, x, y, align = 'left') {
  ctx.font = f(13, '600'); ctx.fillStyle = C.dim; ctx.textAlign = align;
  ctx.fillText(String(str).toUpperCase(), x, y); ctx.textAlign = 'left';
}
function hline(ctx, y, x1 = 42, x2 = W - 42) {
  ctx.strokeStyle = C.line; ctx.lineWidth = 1;
  ctx.beginPath(); ctx.moveTo(x1, y); ctx.lineTo(x2, y); ctx.stroke();
}

async function img(url) {
  if (!url) return null;
  try {
    const full = /^https?:\/\//i.test(url) ? url : 'https://www.whiteoutstats.com' + (url.startsWith('/') ? url : '/' + url);
    const ctl = new AbortController();
    const t = setTimeout(() => ctl.abort(), 7000);
    const res = await fetch(full, { signal: ctl.signal, headers: { 'User-Agent': 'Mozilla/5.0', Accept: 'image/*' } });
    clearTimeout(t);
    if (!res.ok) return null;
    return await loadImage(Buffer.from(await res.arrayBuffer()));
  } catch { return null; }
}

function circleImage(ctx, image, x, y, d, ring = C.edge) {
  ctx.save();
  ctx.beginPath(); ctx.arc(x + d / 2, y + d / 2, d / 2, 0, Math.PI * 2); ctx.clip();
  if (image) ctx.drawImage(image, x, y, d, d);
  else { ctx.fillStyle = '#1c3556'; ctx.fillRect(x, y, d, d); }
  ctx.restore();
  ctx.beginPath(); ctx.arc(x + d / 2, y + d / 2, d / 2, 0, Math.PI * 2);
  ctx.strokeStyle = ring; ctx.lineWidth = 2; ctx.stroke();
}

function statBlock(ctx, x, y, lab, value, sub, subColor, valColor) {
  small(ctx, lab, x, y);
  ctx.font = f(34, '800'); ctx.fillStyle = valColor || C.num; ctx.textAlign = 'left';
  ctx.fillText(String(value), x, y + 40);
  if (sub) { ctx.font = f(13, '600'); ctx.fillStyle = subColor || C.green; ctx.fillText(sub, x, y + 62); }
}

/* ───────────────────────── PLAYER · OVERVIEW ───────────────────────── */
async function playerOverview(p) {
  // Match Whiteout Stats Profile INTEL card (no Record / Alliance body sections)
  const stateTag = p.stateId != null ? 'State ' + p.stateId : null;
  const rankInState = (rank) => {
    if (rank == null) return '—';
    return '#' + int(rank) + (p.stateId != null ? ' in State ' + p.stateId : '');
  };
  // Shared format: F1–F30, then 30-1…30-4, FC 1, FC 1-1…FC 10 (matches Building Calculator)
  const { formatFurnace } = require('../../giftcode/utils/furnaceFormat');
  const furnaceLabel = (() => {
    if (p.levelLabel && /^(FC\s*\d|30-)/i.test(String(p.levelLabel))) return p.levelLabel;
    const n = Number(p.furnace);
    if (!Number.isFinite(n) || n <= 0) return p.levelLabel || '—';
    return formatFurnace(n) || p.levelLabel || '—';
  })();
  const shieldLabel = p.shield?.active
    ? (p.shield.label || 'Shielded')
    : (p.shield?.label || 'No shield');
  const burnLabel = p.burn?.active
    ? (p.burn.label || 'Burning')
    : (p.burn?.label || 'Not burning');
  const locationLabel = (p.x != null && p.y != null) ? ('X ' + p.x + ' · Y ' + p.y) : '—';

  // Prefer board ranks when flat ranks missing
  const boardRank = (keys) => {
    const list = p.boards || [];
    for (const k of keys) {
      const b = list.find((x) => String(x.key || '').toLowerCase() === k || String(x.name || '').toLowerCase().includes(k.replace(/_/g, ' ')));
      if (b && b.rank != null) return b.rank;
    }
    return null;
  };
  const labRank = p.labRank ?? p.mysticRank ?? boardRank(['labyrinth', '20', 'the labyrinth', 'mystic_trial']) ?? null;
  const powerRank = p.powerRank ?? boardRank(['personal_power', 'power', '3']);
  const killsRank = p.killsRank ?? boardRank(['kill_count', 'kills', '4']);
  const transferRank = p.migrantRank ?? boardRank(['transfer_score', 'migrant', '19']);
  const labProgress = p.labyrinth != null ? p.labyrinth : p.mysticTrial;

  // 4-column INTEL grid (label above value), matching site order
  const facts = [
    ['Power', num(p.power), C.num],
    ['Furnace', furnaceLabel, C.text],
    ['Kills', num(p.kills), C.text],
    ['Power Rank', rankInState(powerRank), C.num],
    ['Kills Rank', rankInState(killsRank), C.text],
    ['State Transfer Score', p.migrantScore != null ? int(p.migrantScore) : '—', C.text],
    ['State Transfer Rank', rankInState(transferRank), C.text],
    ['Labyrinth Progress', labProgress != null ? int(labProgress) : '—', C.text],
    ['Labyrinth Rank', rankInState(labRank), C.text],
    ['Shield', shieldLabel, p.shield?.active ? C.num : C.dim],
    ['Burn', burnLabel, p.burn?.active ? C.red : C.dim],
    ['Location', locationLabel, C.text],
    ['Chief ID', p.chiefId != null ? String(p.chiefId) : '—', C.text],
    ['State', stateTag || '—', C.num],
    ['Alliance rank', p.role || '—', C.text],
    ['Last power change', p.lastPowerChangeAt ? ago(p.lastPowerChangeAt) : '—', C.dim],
    ['Last updated', ago(p.asOf), C.dim],
  ];
  // Troop levels (T1–T12 · FC) — matches website Troop Levels display
  const troopList = Array.isArray(p.troop)
    ? p.troop.filter((t) => t && t.tier != null)
    : ((p.troopLevels || {}).infantry != null || (p.troopLevels || {}).lancer != null || (p.troopLevels || {}).marksman != null)
      ? ['infantry', 'lancer', 'marksman'].map((k) => ({
        kind: k,
        name: k === 'infantry' ? 'Infantry' : k === 'lancer' ? 'Lancer' : 'Marksman',
        tier: p.troopLevels ? p.troopLevels[k] : null,
        fc: null,
      }))
      : [];
  for (const t of troopList) {
    facts.push(['Troop · ' + (t.name || 'Troop'), (t.tier != null ? 'T' + t.tier + ' · FC' + (t.fc != null ? t.fc : '—') : '—'), C.num]);
  }

  const cols = 4;
  const rows = Math.ceil(facts.length / cols);
  const cellH = 58;
  const gridTop = 250;
  const H = gridTop + rows * cellH + 70;
  const { canvas, ctx } = frame(H);

  // Avatar + identity
  const av = await img(p.avatarUrl);
  circleImage(ctx, av, 48, 42, 100);
  ctx.textAlign = 'left';
  ctx.font = f(34, '800'); ctx.fillStyle = C.text;
  ctx.fillText(fit(ctx, p.name, 700), 172, 82);

  ctx.font = f(15, '600'); ctx.fillStyle = C.dim;
  ctx.fillText('ID ' + (p.chiefId ?? '—'), 172, 108);
  if (p.stateId != null) {
    ctx.font = f(15, '700'); ctx.fillStyle = C.num;
    ctx.fillText('State #' + p.stateId, 172, 130);
  }

  if (p.crowns > 0) {
    ctx.font = f(48); ctx.fillStyle = C.gold; ctx.textAlign = 'right';
    ctx.fillText('♛', W - 52, 90); ctx.textAlign = 'left';
  }

  // Alliance strip (header only — no Alliance tab body)
  const stripY = 156;
  rr(ctx, 42, stripY, W - 84, 52, 12);
  ctx.fillStyle = 'rgba(120,170,230,0.08)';
  ctx.fill();
  ctx.strokeStyle = C.edge; ctx.lineWidth = 1.2;
  rr(ctx, 42, stripY, W - 84, 52, 12); ctx.stroke();

  if (p.allianceTag || p.allianceName) {
    const flag = await img(p.allianceFlagUrl);
    if (flag) {
      ctx.save();
      rr(ctx, 56, stripY + 10, 32, 32, 6); ctx.clip();
      ctx.drawImage(flag, 56, stripY + 10, 32, 32);
      ctx.restore();
    } else {
      rr(ctx, 56, stripY + 10, 32, 32, 6);
      ctx.fillStyle = 'rgba(255,255,255,0.06)'; ctx.fill();
      ctx.font = f(14, '700'); ctx.fillStyle = C.dim; ctx.textAlign = 'center';
      ctx.fillText('⚔', 72, stripY + 32); ctx.textAlign = 'left';
    }
    ctx.font = f(18, '800'); ctx.fillStyle = C.text;
    const tag = p.allianceTag ? '[' + p.allianceTag + ']' : '';
    ctx.fillText(fit(ctx, tag + (p.allianceName ? ' ' + p.allianceName : ''), 520), 100, stripY + 33);
    const metaBits = [];
    if (p.role) metaBits.push(p.role);
    if (p.allianceMembers != null) metaBits.push(int(p.allianceMembers) + ' members');
    if (p.stateId != null) metaBits.push('State ' + p.stateId);
    if (metaBits.length) {
      ctx.font = f(13, '600'); ctx.fillStyle = C.dim;
      ctx.fillText(metaBits.join(' · '), 100 + ctx.measureText(fit(ctx, tag + (p.allianceName ? ' ' + p.allianceName : ''), 520)).width + 16, stripY + 33);
    }
  } else {
    ctx.font = f(15, '600'); ctx.fillStyle = C.dim;
    ctx.fillText('No alliance', 60, stripY + 32);
  }

  // INTEL section title
  label(ctx, 'Intel', 48, 236);
  hline(ctx, 246);

  // Fact grid
  const gridX = 48;
  const gridW = W - 96;
  const cellW = gridW / cols;
  facts.forEach((fact, i) => {
    const col = i % cols;
    const row = Math.floor(i / cols);
    const x = gridX + col * cellW;
    const y = gridTop + row * cellH;
    if (row % 2 === 0) {
      // subtle row banding across full width handled per cell
      ctx.fillStyle = C.row;
      ctx.fillRect(x, y, cellW - 8, cellH - 6);
    }
    ctx.font = f(12, '700'); ctx.fillStyle = C.dim; ctx.textAlign = 'left';
    ctx.fillText(String(fact[0]).toUpperCase(), x + 10, y + 20);
    ctx.font = f(18, '700'); ctx.fillStyle = fact[2] || C.text;
    ctx.fillText(fit(ctx, String(fact[1]), cellW - 24), x + 10, y + 44);
  });

  if (p.redeemNote) {
    ctx.font = f(13, '600'); ctx.fillStyle = C.amber; ctx.textAlign = 'left';
    ctx.fillText(p.redeemNote, 48, H - 64);
  }
  footerBar(ctx, H, 'Profile as of ' + utc(p.asOf));
  return canvas.toBuffer('image/png');
}

/* ───────────────────────── PLAYER · STANDINGS ───────────────────────── */
async function playerStandings(p) {
  const rows = p.boards || [];
  const groups = [];
  for (const b of rows) {
    let g = groups.find((x) => x.name === b.group);
    if (!g) groups.push(g = { name: b.group || 'Core', items: [] });
    g.items.push(b);
  }
  const H = 170 + groups.length * 40 + rows.length * 44 + 40;
  const { canvas, ctx } = frame(H);
  const av = await img(p.avatarUrl);
  circleImage(ctx, av, 46, 40, 64);
  label(ctx, 'Leaderboard standings', 128, 56);
  ctx.font = f(28, '800'); ctx.fillStyle = C.text;
  ctx.fillText(fit(ctx, p.name + '  ·  State ' + p.stateId, 560), 128, 94);
  small(ctx, 'Boards', W - 220, 62, 'right'); small(ctx, 'In top 10', W - 60, 62, 'right');
  ctx.font = f(30, '800'); ctx.fillStyle = C.text; ctx.textAlign = 'right';
  ctx.fillText(String(rows.length), W - 220, 98); ctx.fillText(String(rows.filter((b) => b.rank && b.rank <= 10).length), W - 60, 98); ctx.textAlign = 'left';
  hline(ctx, 124);
  const X = { score: 360, state: 560, global: 700, age: W - 60 };
  small(ctx, 'Board', 62, 150); small(ctx, 'Score', X.score, 150); small(ctx, 'State', X.state, 150);
  small(ctx, 'Global', X.global, 150); small(ctx, 'Age', X.age, 150, 'right');
  let y = 180;
  for (const g of groups) {
    ctx.font = f(12, '800'); ctx.fillStyle = C.faint; ctx.fillText(g.name.toUpperCase(), 62, y); y += 14;
    g.items.forEach((b, i) => {
      if (i % 2 === 0) { ctx.fillStyle = C.row; ctx.fillRect(48, y, W - 96, 40); }
      ctx.font = f(18, '600'); ctx.fillStyle = C.text; ctx.fillText(b.name, 62, y + 27);
      ctx.fillStyle = C.text; ctx.fillText(b.score, X.score, y + 27);
      ctx.fillStyle = C.num; ctx.font = f(18, '800'); ctx.fillText(b.rank ? '#' + b.rank : '—', X.state, y + 27);
      if (b.rank) {
        ctx.fillStyle = C.barBg; ctx.fillRect(X.state, y + 33, 80, 3);
        ctx.fillStyle = C.bar; ctx.fillRect(X.state, y + 33, Math.max(6, 80 * (b.pct ?? 0.4)), 3);
      }
      ctx.font = f(16); ctx.fillStyle = C.dim; ctx.fillText(b.global || '—', X.global, y + 27);
      ctx.textAlign = 'right'; ctx.fillText(b.age || '', X.age, y + 27); ctx.textAlign = 'left';
      y += 44;
    });
    y += 26;
  }
  footerBar(ctx, H, 'Boards as of ' + utc(p.asOf));
  return canvas.toBuffer('image/png');
}

/* ───────────────────────── PLAYER · PROGRESSION ───────────────────────── */
function spark(ctx, pts, x, y, w, h, color) {
  if (!pts || pts.length < 2) { ctx.font = f(14); ctx.fillStyle = C.faint; ctx.fillText('Not enough snapshots yet', x, y + h / 2); return; }
  const xs = pts.map((p) => p.t), ys = pts.map((p) => p.v);
  const x0 = Math.min(...xs), x1 = Math.max(...xs), lo = Math.min(...ys), hi = Math.max(...ys);
  const px = (t) => x + ((t - x0) / Math.max(1, x1 - x0)) * w;
  const py = (v) => y + h - ((v - lo) / Math.max(1e-9, hi - lo || 1)) * (h - 8) - 4;
  ctx.beginPath(); ctx.moveTo(px(pts[0].t), y + h);
  pts.forEach((p) => ctx.lineTo(px(p.t), py(p.v)));
  ctx.lineTo(px(pts[pts.length - 1].t), y + h); ctx.closePath();
  ctx.fillStyle = color + '2a'; ctx.fill();
  ctx.beginPath(); pts.forEach((p, i) => (i ? ctx.lineTo(px(p.t), py(p.v)) : ctx.moveTo(px(p.t), py(p.v))));
  ctx.strokeStyle = color; ctx.lineWidth = 2.5; ctx.stroke();
  const lp = pts[pts.length - 1];
  ctx.beginPath(); ctx.arc(px(lp.t), py(lp.v), 5, 0, Math.PI * 2); ctx.fillStyle = color; ctx.fill();
}
async function playerProgression(p) {
  const ms = (p.milestones || []).slice(0, 6);
  const H = 560 + ms.length * 30;
  const { canvas, ctx } = frame(H);
  label(ctx, 'Progression', 46, 58);
  ctx.font = f(32, '800'); ctx.fillStyle = C.text; ctx.fillText(fit(ctx, p.name, 760), 46, 96);
  ctx.font = f(16); ctx.fillStyle = C.dim;
  ctx.fillText(`${p.rangeLabel || ''}  ·  ${p.powerSnaps ?? 0} power  ·  ${p.killSnaps ?? 0} kill count snapshots`, 46, 124);
  small(ctx, 'Power', 46, 156);
  spark(ctx, p.powerSeries, 46, 164, W - 92, 110, C.num);
  ctx.font = f(14); ctx.fillStyle = C.dim; ctx.fillText(num(p.powerSeries?.[0]?.v), 46, 296);
  ctx.textAlign = 'right'; ctx.fillStyle = C.num; ctx.font = f(16, '800'); ctx.fillText(num(p.powerSeries?.slice(-1)[0]?.v), W - 46, 296); ctx.textAlign = 'left';
  small(ctx, 'Kill count', 46, 332);
  spark(ctx, p.killSeries, 46, 340, W - 92, 110, C.orange);
  ctx.font = f(14); ctx.fillStyle = C.dim; ctx.fillText(num(p.killSeries?.[0]?.v), 46, 472);
  ctx.textAlign = 'right'; ctx.fillStyle = C.orange; ctx.font = f(16, '800'); ctx.fillText(num(p.killSeries?.slice(-1)[0]?.v), W - 46, 472); ctx.textAlign = 'left';
  small(ctx, 'Milestones', 46, 512);
  ms.forEach((m, i) => {
    const y = 540 + i * 30;
    ctx.font = f(15); ctx.fillStyle = C.dim; ctx.fillText(m.date, 46, y);
    ctx.fillText(m.kind, 200, y);
    ctx.fillStyle = C.text; ctx.fillText(fit(ctx, m.change || '', W - 406), 360, y);
  });
  footerBar(ctx, H, 'Last snapshot ' + utc(p.asOf));
  return canvas.toBuffer('image/png');
}

/* ───────────────────────── PLAYER · CROWNS ───────────────────────── */
async function playerCrowns(p) {
  const reigns = (p.reigns || []).slice(0, 6);
  const H = 360 + reigns.length * 34;
  const { canvas, ctx } = frame(H);
  label(ctx, 'Crowns', 46, 58);
  ctx.font = f(32, '800'); ctx.fillStyle = C.text; ctx.fillText(fit(ctx, p.name, 760), 46, 98);
  if (p.crowns > 0) { ctx.font = f(54); ctx.fillStyle = C.gold; ctx.textAlign = 'right'; ctx.fillText('♛', W - 52, 96); ctx.textAlign = 'left'; }
  hline(ctx, 124);
  const cols = [['Crowns', p.crowns], ['Terms', p.terms], ['Conquests', p.conquests], ['States ruled', p.statesRuled]];
  const cw = (W - 84) / cols.length;
  cols.forEach((c, i) => statBlock(ctx, 46 + i * cw, 150, c[0], c[1] ?? 0, null, null, i === 0 ? C.num : C.text));
  hline(ctx, 240);
  small(ctx, 'Thrones', 46, 266);
  (p.thrones || []).slice(0, 3).forEach((t, i) => {
    const y = 296 + i * 32;
    ctx.font = f(18, '800'); ctx.fillStyle = C.text; ctx.fillText('State ' + t.state, 46, y);
    ctx.font = f(14, '700'); ctx.fillStyle = C.green; ctx.fillText(t.badge || '', 380, y);
    ctx.font = f(15); ctx.fillStyle = C.dim; ctx.fillText(t.terms || '', 640, y);
    ctx.textAlign = 'right'; ctx.fillText(t.when || '', W - 46, y); ctx.textAlign = 'left';
  });
  const ry = 296 + Math.max(1, (p.thrones || []).slice(0, 3).length) * 32 + 14;
  small(ctx, 'Reign history', 46, ry);
  reigns.forEach((r, i) => {
    const y = ry + 30 + i * 32;
    ctx.font = f(15); ctx.fillStyle = C.dim;
    ctx.fillText(r.date, 46, y); ctx.fillText(r.kind, 220, y); ctx.fillText(r.state, 380, y);
    ctx.textAlign = 'right'; ctx.fillStyle = C.text; ctx.fillText(r.delta || '', W - 46, y); ctx.textAlign = 'left';
  });
  footerBar(ctx, H, 'Crown record as of ' + utc(p.asOf));
  return canvas.toBuffer('image/png');
}

/* ───────────────────────── ALLIANCE · OVERVIEW ───────────────────────── */
async function allianceOverview(a) {
  const top = (a.top || []).slice(0, 6);
  const H = 400 + top.length * 50;
  const { canvas, ctx } = frame(H);
  label(ctx, 'Alliance', 56, 62);
  ctx.font = f(40, '800'); ctx.fillStyle = C.text;
  ctx.fillText(fit(ctx, `[${a.tag}] ${a.name}`, 700), 56, 108);
  ctx.font = f(18); ctx.fillStyle = C.dim;
  const subParts = [
    'State ' + a.stateId,
    a.level != null ? 'Lv ' + a.level : null,
    a.leaderRole ? a.leaderRole + ' ' + (a.leader || '') : (a.leader || null),
    a.languageLabel || null,
  ].filter(Boolean);
  ctx.fillText(fit(ctx, subParts.join(' · '), 700), 56, 140);
  const av = await img(a.leaderAvatar || a.flagUrl);
  circleImage(ctx, av, W - 140, 44, 84);
  const memberLabel = a.memberMax != null
    ? (a.members ?? '—') + ' / ' + a.memberMax
    : String(a.members ?? '—');
  const cols = [
    ['Power', num(a.power), a.powerRank ? '#' + a.powerRank + (a.powerOf ? ' / ' + a.powerOf : '') : (a.trackerPowerRank ? '#' + a.trackerPowerRank + ' tracked' : null), C.num],
    ['Kills', num(a.kills), a.killsRank ? '#' + a.killsRank + (a.killsOf ? ' / ' + a.killsOf : '') : null, C.text],
    ['Members', memberLabel, a.memberMax != null ? 'capacity' : 'tracked', C.text],
    ['Labyrinth', a.labyrinth != null ? int(a.labyrinth) : '—', a.labRank ? '#' + a.labRank + (a.labOf ? ' / ' + a.labOf : '') + ' ranked' : null, C.text],
  ];
  const cw = (W - 112) / cols.length;
  cols.forEach((c, i) => statBlock(ctx, 56 + i * cw, 182, c[0], c[1], c[2], C.green, c[3]));
  // Optional manifesto / notice snippet
  const blurb = (a.notice || a.manifesto || '').replace(/\s+/g, ' ').trim();
  if (blurb) {
    ctx.font = f(13); ctx.fillStyle = C.dim;
    ctx.fillText(fit(ctx, blurb, W - 120), 56, 268);
  }
  hline(ctx, blurb ? 288 : 272, 56, W - 56);
  const topY = blurb ? 312 : 296;
  small(ctx, 'Top members', 56, topY); small(ctx, 'Share of power', W - 56, topY, 'right');
  const max = Math.max(1, ...top.map((m) => m.power || 0));
  top.forEach((m, i) => {
    const y = topY + 16 + i * 50;
    if (i % 2 === 0) { ctx.fillStyle = C.row; ctx.fillRect(48, y - 2, W - 96, 46); }
    rr(ctx, 60, y + 8, 40, 26, 7); ctx.strokeStyle = C.edge; ctx.lineWidth = 1.5; ctx.stroke();
    ctx.font = f(13, '800'); ctx.fillStyle = C.dim; ctx.textAlign = 'center'; ctx.fillText(m.role || 'R?', 80, y + 26); ctx.textAlign = 'left';
    ctx.font = f(20, '700'); ctx.fillStyle = C.text; ctx.fillText(fit(ctx, m.name, 340), 116, y + 29);
    ctx.textAlign = 'right'; ctx.fillText(num(m.power), 600, y + 29); ctx.textAlign = 'left';
    ctx.fillStyle = C.barBg; ctx.fillRect(624, y + 17, 210, 8);
    ctx.fillStyle = C.bar; ctx.fillRect(624, y + 17, 210 * ((m.power || 0) / max), 8);
    ctx.font = f(15, '700'); ctx.fillStyle = C.dim; ctx.fillText(m.share || '', 852, y + 29);
    ctx.textAlign = 'right'; ctx.fillText(m.furnace ? 'F' + m.furnace : '', W - 70, y + 29); ctx.textAlign = 'left';
  });
  footerBar(ctx, H, `Top ${top.length} of ${a.members ?? '?'} tracked · Roster as of ${utc(a.asOf)}`);
  return canvas.toBuffer('image/png');
}

/* ───────────────────────── STATE · OVERVIEW ───────────────────────── */
const gradeColor = (g) => (/^A/.test(g) ? C.green : /^B/.test(g) ? '#7bd58a' : /^C/.test(g) ? C.amber : C.red);

function fmtAge(days) {
  if (days == null || !Number.isFinite(Number(days))) return '—';
  const d = Math.round(Number(days));
  return d + ' day' + (d === 1 ? '' : 's');
}
function fmtCreated(s) {
  if (!s) return '—';
  // expect YYYY-MM-DD
  const m = String(s).match(/^(\d{4})-(\d{2})-(\d{2})/);
  if (!m) return String(s);
  const months = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
  return Number(m[3]) + ' ' + months[Number(m[2]) - 1] + ' ' + m[1];
}

async function stateOverview(s) {
  const totals = Array.isArray(s.boardTotals) ? s.boardTotals : [];
  // Layout: hero header + 6 stat tiles + board totals grid
  const totalRows = Math.ceil(Math.max(totals.length, 1) / 4);
  const H = 420 + totalRows * 78 + 36;
  const { canvas, ctx } = frame(H);

  // —— Banner / capital icon ——
  const banner = await img(s.bannerUrl);
  if (banner) {
    // rounded square icon centered
    const bx = W / 2 - 40, by = 28, bs = 80;
    rr(ctx, bx, by, bs, bs, 16);
    ctx.save();
    ctx.clip();
    ctx.drawImage(banner, bx, by, bs, bs);
    ctx.restore();
    ctx.strokeStyle = C.edge;
    ctx.lineWidth = 2;
    rr(ctx, bx, by, bs, bs, 16);
    ctx.stroke();
  }

  // STATE label
  ctx.font = f(12, '700');
  ctx.fillStyle = C.label;
  ctx.textAlign = 'center';
  ctx.fillText('STATE', W / 2, banner ? 128 : 48);

  // Title
  ctx.font = f(40, '800');
  ctx.fillStyle = C.text;
  ctx.fillText('State ' + s.id, W / 2, banner ? 168 : 88);
  ctx.textAlign = 'left';

  // President + Alliance chips under title
  const chipY = banner ? 188 : 108;
  const chips = [];
  if (s.president) {
    chips.push({ kind: 'pres', label: 'PRESIDENT', name: s.president, avatar: s.presidentAvatar });
  }
  if (s.rulingAllianceTag) {
    chips.push({ kind: 'ally', label: 'ALLIANCE', name: '[' + s.rulingAllianceTag + ']', avatar: s.rulingAllianceFlag });
  }
  if (chips.length) {
    // measure total width to center
    const gap = 14;
    const chipWs = chips.map((ch) => {
      ctx.font = f(14, '700');
      return 28 + 10 + ctx.measureText(ch.name).width + 18;
    });
    const totalW = chipWs.reduce((a, b) => a + b, 0) + gap * (chips.length - 1);
    let x = (W - totalW) / 2;
    for (let i = 0; i < chips.length; i++) {
      const ch = chips[i];
      const w = chipWs[i];
      rr(ctx, x, chipY, w, 34, 17);
      ctx.fillStyle = 'rgba(120,170,230,0.10)';
      ctx.fill();
      ctx.strokeStyle = C.edge;
      ctx.lineWidth = 1.5;
      rr(ctx, x, chipY, w, 34, 17);
      ctx.stroke();
      // mini avatar
      const av = await img(ch.avatar);
      if (av) {
        circleImage(ctx, av, x + 6, chipY + 3, 28, C.edge);
      } else {
        ctx.beginPath();
        ctx.arc(x + 20, chipY + 17, 12, 0, Math.PI * 2);
        ctx.fillStyle = C.edge2;
        ctx.fill();
      }
      ctx.font = f(14, '700');
      ctx.fillStyle = C.text;
      ctx.textAlign = 'left';
      ctx.fillText(ch.name, x + 40, chipY + 22);
      x += w + gap;
    }
  }

  // —— Primary stat tiles (Age, Created, Power, Chiefs, Active, Alliances) ——
  const primaryY = chipY + (chips.length ? 54 : 16);
  const primary = [
    ['AGE OF STATE', fmtAge(s.ageDays)],
    ['CREATED', fmtCreated(s.created)],
    ['POWER', num(s.power)],
    ['CHIEFS', int(s.players)],
    ['ACTIVE PLAYERS', int(s.activePlayers)],
    ['ALLIANCES', int(s.alliances)],
  ];
  const pw = (W - 84 - 5 * 10) / 6;
  primary.forEach((c, i) => {
    const x = 42 + i * (pw + 10);
    rr(ctx, x, primaryY, pw, 72, 12);
    ctx.fillStyle = 'rgba(120,170,230,0.06)';
    ctx.fill();
    ctx.strokeStyle = C.edge;
    ctx.lineWidth = 1.2;
    rr(ctx, x, primaryY, pw, 72, 12);
    ctx.stroke();
    ctx.font = f(11, '700');
    ctx.fillStyle = C.dim;
    ctx.textAlign = 'center';
    ctx.fillText(c[0], x + pw / 2, primaryY + 22);
    ctx.font = f(20, '800');
    ctx.fillStyle = C.text;
    ctx.fillText(fit(ctx, String(c[1]), pw - 12), x + pw / 2, primaryY + 52);
    ctx.textAlign = 'left';
  });

  // Active players note
  const noteY = primaryY + 88;
  ctx.font = f(12);
  ctx.fillStyle = C.faint;
  ctx.fillText('Active Players: Chiefs currently on the map. The game removes inactive cities after about 3 weeks.', 42, noteY);

  // —— TOP 100 BOARD TOTALS ——
  const secY = noteY + 28;
  hline(ctx, secY, 42, W - 42);
  ctx.font = f(13, '700');
  ctx.fillStyle = C.label;
  ctx.fillText('TOP 100 BOARD TOTALS', 42, secY + 28);
  ctx.font = f(12);
  ctx.fillStyle = C.faint;
  ctx.fillText('Sum of each in-game ranking', 42, secY + 48);

  const gridY = secY + 64;
  const cols = 4;
  const gap = 12;
  const tileW = (W - 84 - gap * (cols - 1)) / cols;
  const tileH = 66;
  const list = totals.length
    ? totals
    : [
        // fallback from flat fields if totals array empty
        { label: 'Alliance power', total: s.alliancePower },
        { label: 'Alliance kills', total: s.allianceKills },
        { label: 'Chief power', total: s.power },
        { label: 'Kills', total: s.kills },
        { label: 'Hero power', total: s.heroPower },
        { label: 'Hero total', total: s.heroTotal },
        { label: 'Troop power', total: s.troopPower },
        { label: 'Building power', total: s.buildingPower },
        { label: 'Research power', total: s.sciencePower },
      ].filter((t) => t.total != null);

  list.forEach((t, i) => {
    const col = i % cols;
    const row = Math.floor(i / cols);
    const x = 42 + col * (tileW + gap);
    const y = gridY + row * (tileH + gap);
    rr(ctx, x, y, tileW, tileH, 12);
    ctx.fillStyle = 'rgba(120,170,230,0.06)';
    ctx.fill();
    ctx.strokeStyle = C.edge;
    ctx.lineWidth = 1.2;
    rr(ctx, x, y, tileW, tileH, 12);
    ctx.stroke();
    ctx.font = f(11, '700');
    ctx.fillStyle = C.dim;
    ctx.textAlign = 'center';
    ctx.fillText(fit(ctx, String(t.label || '').toUpperCase(), tileW - 16), x + tileW / 2, y + 22);
    ctx.font = f(22, '800');
    ctx.fillStyle = C.num;
    ctx.fillText(num(t.total), x + tileW / 2, y + 48);
    ctx.textAlign = 'left';
  });

  footerBar(ctx, H, 'State overview · ' + utc(s.asOf));
  return canvas.toBuffer('image/png');
}

/* ───────────────────────── STATE · STATE VS STATE ───────────────────────── */
async function stateVsState(s) {
  const m = s.matchup || null;
  const compare = (m && Array.isArray(m.compare) && m.compare.length)
    ? m.compare
    : [
        { label: 'Active Players', us: s.activePlayers, them: null, kind: 'count' },
      ];

  const rowH = 42;
  const H = 210 + compare.length * rowH + 50;
  const { canvas, ctx } = frame(H);

  // Header
  label(ctx, 'STATE VERSUS STATE (SVS)', 50, 52);
  if (m?.season != null) {
    ctx.font = f(13, '700');
    ctx.fillStyle = C.dim;
    ctx.textAlign = 'right';
    ctx.fillText('Season ' + m.season, W - 50, 52);
    ctx.textAlign = 'left';
  }

  // Two state cards + VS
  const cardW = 280, cardH = 110;
  const leftX = 80, rightX = W - 80 - cardW, cardY = 72;
  const midX = W / 2;

  async function drawStateCard(x, side) {
    const kid = side?.kid ?? (side === m?.us ? s.id : null);
    const banner = side?.bannerUrl || null;
    rr(ctx, x, cardY, cardW, cardH, 16);
    ctx.fillStyle = 'rgba(120,170,230,0.06)';
    ctx.fill();
    ctx.strokeStyle = C.edge;
    ctx.lineWidth = 1.5;
    rr(ctx, x, cardY, cardW, cardH, 16);
    ctx.stroke();

    const icon = await img(banner);
    const ix = x + cardW / 2 - 28, iy = cardY + 12;
    if (icon) {
      rr(ctx, ix, iy, 56, 56, 12);
      ctx.save();
      ctx.clip();
      ctx.drawImage(icon, ix, iy, 56, 56);
      ctx.restore();
      ctx.strokeStyle = C.edge;
      ctx.lineWidth = 1.5;
      rr(ctx, ix, iy, 56, 56, 12);
      ctx.stroke();
    }
    ctx.font = f(12, '700');
    ctx.fillStyle = C.dim;
    ctx.textAlign = 'center';
    ctx.fillText(kid != null ? '#' + kid : '—', x + cardW / 2, cardY + 82);
    ctx.font = f(18, '800');
    ctx.fillStyle = C.text;
    ctx.fillText(kid != null ? 'State ' + kid : '—', x + cardW / 2, cardY + 100);
    ctx.textAlign = 'left';
  }

  const us = m?.us || { kid: s.id, bannerUrl: s.bannerUrl, located: s.activePlayers };
  const them = m?.them || null;
  await drawStateCard(leftX, us);
  await drawStateCard(rightX, them);

  // VS badge
  ctx.beginPath();
  ctx.arc(midX, cardY + cardH / 2, 22, 0, Math.PI * 2);
  ctx.fillStyle = C.edge2;
  ctx.fill();
  ctx.strokeStyle = C.edge;
  ctx.lineWidth = 2;
  ctx.stroke();
  ctx.font = f(14, '800');
  ctx.fillStyle = C.text;
  ctx.textAlign = 'center';
  ctx.fillText('VS', midX, cardY + cardH / 2 + 5);
  ctx.textAlign = 'left';

  // STATE SNAPSHOT
  const snapY = cardY + cardH + 28;
  ctx.font = f(12, '700');
  ctx.fillStyle = C.label;
  ctx.fillText('STATE SNAPSHOT', 50, snapY);

  const barLeft = 200;
  const barRight = W - 200;
  const barW = barRight - barLeft;
  const startY = snapY + 28;

  compare.forEach((row, i) => {
    const y = startY + i * rowH;
    const a = row.us != null ? Number(row.us) : 0;
    const b = row.them != null ? Number(row.them) : 0;
    const sum = (a + b) || 1;
    const pctA = row.usPct != null ? Number(row.usPct) / 100 : a / sum;
    const pctB = 1 - pctA;

    // values
    ctx.font = f(16, '800');
    ctx.fillStyle = C.amber;
    ctx.textAlign = 'right';
    ctx.fillText(row.kind === 'count' ? int(row.us) : num(row.us), barLeft - 14, y + 6);
    ctx.fillStyle = C.num;
    ctx.textAlign = 'left';
    ctx.fillText(row.kind === 'count' ? int(row.them) : num(row.them), barRight + 14, y + 6);

    // label centered above bar
    ctx.font = f(11, '700');
    ctx.fillStyle = C.dim;
    ctx.textAlign = 'center';
    ctx.fillText(String(row.label || '').toUpperCase(), midX, y - 10);

    // dual bar background
    const bh = 10;
    const by = y - 2;
    ctx.fillStyle = 'rgba(255,255,255,0.08)';
    rr(ctx, barLeft, by, barW, bh, 5);
    ctx.fill();

    // left (us) amber
    const wA = Math.max(2, barW * Math.max(0, Math.min(1, pctA)));
    ctx.fillStyle = C.amber;
    rr(ctx, barLeft, by, wA, bh, 5);
    ctx.fill();

    // right (them) blue - draw from right
    const wB = Math.max(2, barW * Math.max(0, Math.min(1, pctB)));
    ctx.fillStyle = C.num;
    rr(ctx, barRight - wB, by, wB, bh, 5);
    ctx.fill();

    ctx.textAlign = 'left';
  });

  const footer = m
    ? ('SvS' + (m.season != null ? ' · Season ' + m.season : '') + (m.stage ? ' · ' + m.stage : '') + ' · ' + utc(m.fetchedAt || s.asOf))
    : ('No active matchup · ' + utc(s.asOf));
  footerBar(ctx, H, footer);
  return canvas.toBuffer('image/png');
}


async function stateMatchupHistory(s) {
  const h = s.svsHistory || { won: null, lost: null, rows: [] };
  const rows = (h.rows || []).slice(0, 14);
  const rowH = 36;
  const H = 170 + rows.length * rowH + 40;
  const { canvas, ctx } = frame(H);

  // Date range title
  const times = rows.map((r) => r.dateTs).filter(Boolean);
  let rangeLabel = 'SVS';
  if (times.length) {
    const fmt = (ts) => {
      const d = new Date((ts > 1e12 ? ts : ts * 1000));
      const M = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
      return d.getUTCDate() + ' ' + M[d.getUTCMonth()].toUpperCase() + ' ' + d.getUTCFullYear();
    };
    const newest = fmt(Math.max(...times));
    const oldest = fmt(Math.min(...times));
    rangeLabel = newest === oldest ? ('SVS · ' + newest) : ('SVS · ' + oldest + ' – ' + newest);
  }
  // Prefer newest-first label like screenshot "SVS · 20 OCT 2026"
  if (times.length) {
    const d = new Date((Math.max(...times) > 1e12 ? Math.max(...times) : Math.max(...times) * 1000));
    const M = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
    rangeLabel = 'SVS · ' + d.getUTCDate() + ' ' + M[d.getUTCMonth()].toUpperCase() + ' ' + d.getUTCFullYear();
  }
  label(ctx, rangeLabel, 50, 52);

  // Castles taken / lost tiles
  const tileY = 70;
  const tw = 200, th = 64;
  function tile(x, title, value, color) {
    rr(ctx, x, tileY, tw, th, 14);
    ctx.fillStyle = 'rgba(120,170,230,0.06)';
    ctx.fill();
    ctx.strokeStyle = C.edge;
    ctx.lineWidth = 1.5;
    rr(ctx, x, tileY, tw, th, 14);
    ctx.stroke();
    ctx.font = f(11, '700');
    ctx.fillStyle = C.dim;
    ctx.textAlign = 'center';
    ctx.fillText(title, x + tw / 2, tileY + 22);
    ctx.font = f(28, '800');
    ctx.fillStyle = color;
    ctx.fillText(value != null ? String(value) : '—', x + tw / 2, tileY + 50);
    ctx.textAlign = 'left';
  }
  tile(50, 'CASTLES TAKEN', h.won, C.green);
  tile(270, 'CASTLES LOST', h.lost, C.red);

  // Table header
  const headerY = tileY + th + 28;
  const cols = [
    { k: 'date', x: 50, w: 120 },
    { k: 'opp', x: 180, w: 100 },
    { k: 'prep', x: 300, w: 90 },
    { k: 'castle', x: 400, w: 90 },
    { k: 'king', x: 510, w: W - 560 },
  ];
  ctx.font = f(11, '700');
  ctx.fillStyle = C.dim;
  ctx.fillText('DATE', cols[0].x, headerY);
  ctx.fillText('OPPONENT', cols[1].x, headerY);
  ctx.fillText('PREP', cols[2].x, headerY);
  ctx.fillText('CASTLE', cols[3].x, headerY);
  ctx.fillText('KING', cols[4].x, headerY);
  hline(ctx, headerY + 10, 50, W - 50);

  const outcomeColor = (v) => {
    const s = String(v || '').toLowerCase();
    if (s === 'won' || s === 'held') return C.green;
    if (s === 'lost') return C.red;
    return C.dim;
  };
  const outcomeLabel = (v) => {
    if (v == null || v === '') return '—';
    const s = String(v);
    return s.charAt(0).toUpperCase() + s.slice(1).toLowerCase();
  };
  const dateLabel = (ts) => {
    if (!ts) return '—';
    const d = new Date(ts > 1e12 ? ts : ts * 1000);
    const M = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
    return d.getUTCDate() + ' ' + M[d.getUTCMonth()] + ' ' + d.getUTCFullYear();
  };

  for (let i = 0; i < rows.length; i++) {
    const r = rows[i];
    const y = headerY + 36 + i * rowH;
    if (i % 2 === 1) {
      ctx.fillStyle = C.row;
      ctx.fillRect(42, y - 22, W - 84, rowH);
    }
    ctx.font = f(14, '600');
    ctx.fillStyle = C.text;
    ctx.fillText(dateLabel(r.dateTs), cols[0].x, y);
    ctx.fillStyle = C.dim;
    ctx.fillText(r.opponentKid != null ? '#' + r.opponentKid : '—', cols[1].x, y);

    ctx.font = f(14, '700');
    ctx.fillStyle = outcomeColor(r.prep);
    ctx.fillText(outcomeLabel(r.prep), cols[2].x, y);
    ctx.fillStyle = outcomeColor(r.castle);
    ctx.fillText(outcomeLabel(r.castle), cols[3].x, y);

    // King avatar + name
    const av = await img(r.kingAvatar);
    if (av) circleImage(ctx, av, cols[4].x, y - 18, 26, C.edge);
    const kingLine = (r.kingTag ? '[' + r.kingTag + '] ' : '') + (r.kingName || '—');
    ctx.font = f(14, '700');
    ctx.fillStyle = C.text;
    ctx.fillText(fit(ctx, kingLine, 320), cols[4].x + 34, y - 4);
    if (r.kingRole) {
      ctx.font = f(11, '600');
      ctx.fillStyle = r.highKing ? C.amber : C.dim;
      ctx.fillText(r.kingRole, cols[4].x + 34, y + 12);
    }
  }

  footerBar(ctx, H, 'Matchup history · ' + utc(s.asOf));
  return canvas.toBuffer('image/png');
}


async function stateMap(s, mapData) {
  const cities = (mapData && mapData.cities) || [];
  const alliances = (mapData && mapData.alliances) || [];
  const stats = (mapData && mapData.stats) || {};

  // Match Whiteout Stats isometric map — focused on the green fertile/plains diamond
  const MAP_CENTER = 600;
  const FERTILE_R = 150;
  const PLAINS_R = 300;
  // Slightly beyond fertile so clusters on the ring stay visible
  const VIEW_R = 220;

  const H = 640;
  const { canvas, ctx } = frame(H);

  label(ctx, 'STATE MAP', 50, 52);
  ctx.font = f(28, '800');
  ctx.fillStyle = C.text;
  ctx.fillText('State ' + s.id, 50, 92);
  const cityN = cities.length || stats.cities || 0;
  const allyN = alliances.length || 0;
  ctx.font = f(14, '600');
  ctx.fillStyle = C.dim;
  ctx.fillText(int(cityN) + ' cities · ' + int(allyN) + ' alliances · fertile zone', 50, 118);

  // Plot rect
  const mx = 40, my = 135, mw = W - 80, mh = 420;
  // Clip to rounded rect
  ctx.save();
  rr(ctx, mx, my, mw, mh, 16);
  ctx.clip();

  // Background (ice / outside)
  ctx.fillStyle = '#2a323c';
  ctx.fillRect(mx, my, mw, mh);

  // Iso projection centered on capital, scaled so VIEW_R fits the box
  // worldToScreen: x' = (dx - dy)*s + cx, y' = -(dx + dy)*s + cy
  // Diamond width for radius R is ~ 2*R*s*√2 in each axis of the iso axes
  const half = Math.min(mw, mh) * 0.48;
  // For cheb R, the iso diamond spans 2*R*s on both screen axes after (dx-dy) etc.
  const scale = half / (VIEW_R * Math.SQRT2);
  const cx = mx + mw / 2;
  const cy = my + mh / 2 + 6;

  function worldToScreen(wx, wy) {
    const dx = wx - MAP_CENTER;
    const dy = wy - MAP_CENTER;
    return {
      x: (dx - dy) * scale + cx,
      y: -(dx + dy) * scale + cy,
    };
  }

  function fillZone(r, fill, stroke) {
    // World square [C-r, C+r]² → iso diamond
    const c0 = worldToScreen(MAP_CENTER - r, MAP_CENTER - r);
    const c1 = worldToScreen(MAP_CENTER + r, MAP_CENTER - r);
    const c2 = worldToScreen(MAP_CENTER + r, MAP_CENTER + r);
    const c3 = worldToScreen(MAP_CENTER - r, MAP_CENTER + r);
    ctx.beginPath();
    ctx.moveTo(c0.x, c0.y);
    ctx.lineTo(c1.x, c1.y);
    ctx.lineTo(c2.x, c2.y);
    ctx.lineTo(c3.x, c3.y);
    ctx.closePath();
    ctx.fillStyle = fill;
    ctx.fill();
    if (stroke) {
      ctx.strokeStyle = stroke;
      ctx.lineWidth = 1.2;
      ctx.stroke();
    }
  }

  // Grid lines inside plains (subtle)
  function drawGrid(r, step, color) {
    ctx.strokeStyle = color;
    ctx.lineWidth = 1;
    for (let v = MAP_CENTER - r; v <= MAP_CENTER + r; v += step) {
      const a = worldToScreen(v, MAP_CENTER - r);
      const b = worldToScreen(v, MAP_CENTER + r);
      ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
      const c = worldToScreen(MAP_CENTER - r, v);
      const d = worldToScreen(MAP_CENTER + r, v);
      ctx.beginPath(); ctx.moveTo(c.x, c.y); ctx.lineTo(d.x, d.y); ctx.stroke();
    }
  }

  // Outer plains (green) then fertile (darker green) then capital core
  fillZone(PLAINS_R, '#2f6a45', 'rgba(200,220,160,0.18)');
  drawGrid(PLAINS_R, 50, 'rgba(0,0,0,0.12)');
  fillZone(FERTILE_R, '#1a4a32', 'rgba(120,190,140,0.28)');
  drawGrid(FERTILE_R, 50, 'rgba(0,0,0,0.10)');
  fillZone(48, '#0f3324', 'rgba(80,160,100,0.25)');

  // Capital marker
  const cap = worldToScreen(MAP_CENTER, MAP_CENTER);
  ctx.beginPath();
  ctx.fillStyle = '#e8c547';
  ctx.arc(cap.x, cap.y, 4, 0, Math.PI * 2);
  ctx.fill();

  // Alliance colors
  const palette = [
    '#2dd4bf', '#f59e0b', '#34d399', '#c084fc', '#f87171', '#a78bfa',
    '#4ade80', '#f472b6', '#38bdf8', '#fb923c', '#e0b84a', '#67e8f9',
    '#818cf8', '#fb7185', '#86efac', '#e07a4a',
  ];
  const colorByAid = new Map();
  alliances.slice(0, 48).forEach((a, i) => {
    colorByAid.set(String(a.aid), palette[i % palette.length]);
  });

  // Cities inside / near the green view
  for (const c of cities) {
    if (c.x == null || c.y == null) continue;
    const dx = Math.abs(Number(c.x) - MAP_CENTER);
    const dy = Math.abs(Number(c.y) - MAP_CENTER);
    // Chebyshev distance — keep a little past fertile so edge clusters show
    if (Math.max(dx, dy) > VIEW_R + 30) continue;
    const p = worldToScreen(Number(c.x), Number(c.y));
    const col = colorByAid.get(String(c.aid)) || 'rgba(200,210,220,0.7)';
    ctx.beginPath();
    ctx.fillStyle = col;
    ctx.arc(p.x, p.y, 2.4, 0, Math.PI * 2);
    ctx.fill();
  }

  ctx.restore();

  // Border over clip
  rr(ctx, mx, my, mw, mh, 16);
  ctx.strokeStyle = C.edge;
  ctx.lineWidth = 1.5;
  ctx.stroke();

  // Terrain key + top alliances
  ctx.font = f(11, '700');
  ctx.fillStyle = C.dim;
  ctx.fillText('FERTILE', 50, 575);
  ctx.fillStyle = '#1a4a32';
  ctx.fillRect(105, 566, 14, 10);
  ctx.fillStyle = C.dim;
  ctx.fillText('PLAINS', 130, 575);
  ctx.fillStyle = '#2f6a45';
  ctx.fillRect(180, 566, 14, 10);
  ctx.fillStyle = C.dim;
  ctx.fillText('CASTLE', 205, 575);
  ctx.fillStyle = '#e8c547';
  ctx.beginPath(); ctx.arc(260, 571, 4, 0, Math.PI * 2); ctx.fill();

  const top = alliances.slice(0, 5);
  let lx = 290;
  top.forEach((a, i) => {
    const col = colorByAid.get(String(a.aid)) || palette[i];
    ctx.fillStyle = col;
    ctx.fillRect(lx, 566, 10, 10);
    ctx.fillStyle = C.text;
    ctx.font = f(12, '600');
    const tag = a.abbr || a.tag || '?';
    ctx.fillText('[' + tag + ']', lx + 14, 575);
    lx += 70;
  });

  const ago = stats.location_updated_ago || null;
  footerBar(ctx, H, ago ? ('Map · locations ' + ago + ' · ' + utc(s.asOf)) : ('Map · ' + utc(s.asOf)));
  return canvas.toBuffer('image/png');
}

/** Alliance hive map — same isometric view as state map, with this alliance's cities highlighted. */
async function allianceMap(a, mapData) {
  const cities = (mapData && mapData.cities) || [];
  const alliances = (mapData && mapData.alliances) || [];
  const stats = (mapData && mapData.stats) || {};
  const targetAid = a.aid != null ? String(a.aid) : null;
  const targetTag = String(a.tag || a.abbr || '').toUpperCase();

  const MAP_CENTER = 600;
  const FERTILE_R = 150;
  const PLAINS_R = 300;
  const VIEW_R = 220;

  const H = 640;
  const { canvas, ctx } = frame(H);

  label(ctx, 'HIVE MAP', 50, 52);
  ctx.font = f(26, '800');
  ctx.fillStyle = C.text;
  ctx.fillText(fit(ctx, '[' + (a.tag || '?') + '] ' + (a.name || ''), 700), 50, 92);

  // Match cities belonging to this alliance (by aid preferred, tag fallback)
  const mine = cities.filter((c) => {
    if (targetAid && String(c.aid) === targetAid) return true;
    if (targetTag && String(c.abbr || c.tag || c.alliance_abbr || '').toUpperCase() === targetTag) return true;
    return false;
  });
  const others = cities.filter((c) => !mine.includes(c));

  ctx.font = f(14, '600');
  ctx.fillStyle = C.dim;
  ctx.fillText(
    int(mine.length) + ' cities · State ' + (a.stateId ?? '?') + ' · fertile zone',
    50, 118
  );

  const mx = 40, my = 135, mw = W - 80, mh = 420;
  ctx.save();
  rr(ctx, mx, my, mw, mh, 16);
  ctx.clip();

  ctx.fillStyle = '#2a323c';
  ctx.fillRect(mx, my, mw, mh);

  const half = Math.min(mw, mh) * 0.48;
  const scale = half / (VIEW_R * Math.SQRT2);
  const cx = mx + mw / 2;
  const cy = my + mh / 2 + 6;

  function worldToScreen(wx, wy) {
    const dx = wx - MAP_CENTER;
    const dy = wy - MAP_CENTER;
    return {
      x: (dx - dy) * scale + cx,
      y: -(dx + dy) * scale + cy,
    };
  }

  function fillZone(r, fill, stroke) {
    const c0 = worldToScreen(MAP_CENTER - r, MAP_CENTER - r);
    const c1 = worldToScreen(MAP_CENTER + r, MAP_CENTER - r);
    const c2 = worldToScreen(MAP_CENTER + r, MAP_CENTER + r);
    const c3 = worldToScreen(MAP_CENTER - r, MAP_CENTER + r);
    ctx.beginPath();
    ctx.moveTo(c0.x, c0.y);
    ctx.lineTo(c1.x, c1.y);
    ctx.lineTo(c2.x, c2.y);
    ctx.lineTo(c3.x, c3.y);
    ctx.closePath();
    ctx.fillStyle = fill;
    ctx.fill();
    if (stroke) {
      ctx.strokeStyle = stroke;
      ctx.lineWidth = 1.2;
      ctx.stroke();
    }
  }

  function drawGrid(r, step, color) {
    ctx.strokeStyle = color;
    ctx.lineWidth = 1;
    for (let v = MAP_CENTER - r; v <= MAP_CENTER + r; v += step) {
      const a0 = worldToScreen(v, MAP_CENTER - r);
      const b0 = worldToScreen(v, MAP_CENTER + r);
      ctx.beginPath(); ctx.moveTo(a0.x, a0.y); ctx.lineTo(b0.x, b0.y); ctx.stroke();
      const c0 = worldToScreen(MAP_CENTER - r, v);
      const d0 = worldToScreen(MAP_CENTER + r, v);
      ctx.beginPath(); ctx.moveTo(c0.x, c0.y); ctx.lineTo(d0.x, d0.y); ctx.stroke();
    }
  }

  fillZone(PLAINS_R, '#2f6a45', 'rgba(200,220,160,0.18)');
  drawGrid(PLAINS_R, 50, 'rgba(0,0,0,0.12)');
  fillZone(FERTILE_R, '#1a4a32', 'rgba(120,190,140,0.28)');
  drawGrid(FERTILE_R, 50, 'rgba(0,0,0,0.10)');
  fillZone(48, '#0f3324', 'rgba(80,160,100,0.25)');

  const cap = worldToScreen(MAP_CENTER, MAP_CENTER);
  ctx.beginPath();
  ctx.fillStyle = '#e8c547';
  ctx.arc(cap.x, cap.y, 4, 0, Math.PI * 2);
  ctx.fill();

  // Dim other cities
  for (const c of others) {
    if (c.x == null || c.y == null) continue;
    const dx = Math.abs(Number(c.x) - MAP_CENTER);
    const dy = Math.abs(Number(c.y) - MAP_CENTER);
    if (Math.max(dx, dy) > VIEW_R + 30) continue;
    const p = worldToScreen(Number(c.x), Number(c.y));
    ctx.beginPath();
    ctx.fillStyle = 'rgba(180,190,200,0.35)';
    ctx.arc(p.x, p.y, 2.0, 0, Math.PI * 2);
    ctx.fill();
  }

  // Highlight this alliance's cities
  const highlight = '#2dd4bf';
  for (const c of mine) {
    if (c.x == null || c.y == null) continue;
    const dx = Math.abs(Number(c.x) - MAP_CENTER);
    const dy = Math.abs(Number(c.y) - MAP_CENTER);
    if (Math.max(dx, dy) > VIEW_R + 30) continue;
    const p = worldToScreen(Number(c.x), Number(c.y));
    ctx.beginPath();
    ctx.fillStyle = highlight;
    ctx.arc(p.x, p.y, 3.6, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.strokeStyle = 'rgba(45,212,191,0.55)';
    ctx.lineWidth = 1.5;
    ctx.arc(p.x, p.y, 6.5, 0, Math.PI * 2);
    ctx.stroke();
  }

  ctx.restore();

  rr(ctx, mx, my, mw, mh, 16);
  ctx.strokeStyle = C.edge;
  ctx.lineWidth = 1.5;
  ctx.stroke();

  // Legend
  ctx.font = f(11, '700');
  ctx.fillStyle = C.dim;
  ctx.fillText('FERTILE', 50, 575);
  ctx.fillStyle = '#1a4a32';
  ctx.fillRect(105, 566, 14, 10);
  ctx.fillStyle = C.dim;
  ctx.fillText('PLAINS', 130, 575);
  ctx.fillStyle = '#2f6a45';
  ctx.fillRect(180, 566, 14, 10);
  ctx.fillStyle = C.dim;
  ctx.fillText('CASTLE', 205, 575);
  ctx.fillStyle = '#e8c547';
  ctx.beginPath(); ctx.arc(260, 571, 4, 0, Math.PI * 2); ctx.fill();

  ctx.fillStyle = highlight;
  ctx.fillRect(290, 566, 10, 10);
  ctx.fillStyle = C.text;
  ctx.font = f(12, '600');
  ctx.fillText('[' + (a.tag || '?') + '] cities', 304, 575);

  const ago = stats.location_updated_ago || null;
  footerBar(ctx, H, ago
    ? ('Hive map · ' + int(mine.length) + ' cities · locations ' + ago + ' · ' + utc(a.asOf))
    : ('Hive map · ' + int(mine.length) + ' cities · ' + utc(a.asOf)));
  return canvas.toBuffer('image/png');
}

/** Player location on the state map — highlights this chief's city. */
async function playerMap(p, mapData) {
  const cities = (mapData && mapData.cities) || [];
  const alliances = (mapData && mapData.alliances) || [];
  const stats = (mapData && mapData.stats) || {};
  const px = p.x != null ? Number(p.x) : null;
  const py = p.y != null ? Number(p.y) : null;
  const hasPos = Number.isFinite(px) && Number.isFinite(py);

  const MAP_CENTER = 600;
  const FERTILE_R = 150;
  const PLAINS_R = 300;
  const VIEW_R = 220;

  const H = 640;
  const { canvas, ctx } = frame(H);

  label(ctx, 'LOCATION', 50, 52);
  ctx.font = f(26, '800');
  ctx.fillStyle = C.text;
  ctx.fillText(fit(ctx, p.name || 'Chief', 700), 50, 92);

  ctx.font = f(14, '600');
  ctx.fillStyle = C.dim;
  const coordLabel = hasPos
    ? ('State ' + (p.stateId ?? '?') + ' · X ' + px + ' · Y ' + py)
    : ('State ' + (p.stateId ?? '?') + ' · position unknown');
  ctx.fillText(coordLabel, 50, 118);

  const mx = 40, my = 135, mw = W - 80, mh = 420;
  ctx.save();
  rr(ctx, mx, my, mw, mh, 16);
  ctx.clip();

  ctx.fillStyle = '#2a323c';
  ctx.fillRect(mx, my, mw, mh);

  const half = Math.min(mw, mh) * 0.48;
  const scale = half / (VIEW_R * Math.SQRT2);
  const cx = mx + mw / 2;
  const cy = my + mh / 2 + 6;

  function worldToScreen(wx, wy) {
    const dx = wx - MAP_CENTER;
    const dy = wy - MAP_CENTER;
    return {
      x: (dx - dy) * scale + cx,
      y: -(dx + dy) * scale + cy,
    };
  }

  function fillZone(r, fill, stroke) {
    const c0 = worldToScreen(MAP_CENTER - r, MAP_CENTER - r);
    const c1 = worldToScreen(MAP_CENTER + r, MAP_CENTER - r);
    const c2 = worldToScreen(MAP_CENTER + r, MAP_CENTER + r);
    const c3 = worldToScreen(MAP_CENTER - r, MAP_CENTER + r);
    ctx.beginPath();
    ctx.moveTo(c0.x, c0.y);
    ctx.lineTo(c1.x, c1.y);
    ctx.lineTo(c2.x, c2.y);
    ctx.lineTo(c3.x, c3.y);
    ctx.closePath();
    ctx.fillStyle = fill;
    ctx.fill();
    if (stroke) {
      ctx.strokeStyle = stroke;
      ctx.lineWidth = 1.2;
      ctx.stroke();
    }
  }

  function drawGrid(r, step, color) {
    ctx.strokeStyle = color;
    ctx.lineWidth = 1;
    for (let v = MAP_CENTER - r; v <= MAP_CENTER + r; v += step) {
      const a0 = worldToScreen(v, MAP_CENTER - r);
      const b0 = worldToScreen(v, MAP_CENTER + r);
      ctx.beginPath(); ctx.moveTo(a0.x, a0.y); ctx.lineTo(b0.x, b0.y); ctx.stroke();
      const c0 = worldToScreen(MAP_CENTER - r, v);
      const d0 = worldToScreen(MAP_CENTER + r, v);
      ctx.beginPath(); ctx.moveTo(c0.x, c0.y); ctx.lineTo(d0.x, d0.y); ctx.stroke();
    }
  }

  fillZone(PLAINS_R, '#2f6a45', 'rgba(200,220,160,0.18)');
  drawGrid(PLAINS_R, 50, 'rgba(0,0,0,0.12)');
  fillZone(FERTILE_R, '#1a4a32', 'rgba(120,190,140,0.28)');
  drawGrid(FERTILE_R, 50, 'rgba(0,0,0,0.10)');
  fillZone(48, '#0f3324', 'rgba(80,160,100,0.25)');

  const cap = worldToScreen(MAP_CENTER, MAP_CENTER);
  ctx.beginPath();
  ctx.fillStyle = '#e8c547';
  ctx.arc(cap.x, cap.y, 4, 0, Math.PI * 2);
  ctx.fill();

  // Dim other cities
  for (const c of cities) {
    if (c.x == null || c.y == null) continue;
    // Skip the player's own city if we can match by coords or fid
    if (hasPos && Number(c.x) === px && Number(c.y) === py) continue;
    const dx = Math.abs(Number(c.x) - MAP_CENTER);
    const dy = Math.abs(Number(c.y) - MAP_CENTER);
    if (Math.max(dx, dy) > VIEW_R + 30) continue;
    const pt = worldToScreen(Number(c.x), Number(c.y));
    ctx.beginPath();
    ctx.fillStyle = 'rgba(180,190,200,0.4)';
    ctx.arc(pt.x, pt.y, 2.0, 0, Math.PI * 2);
    ctx.fill();
  }

  // Highlight player position
  if (hasPos) {
    const pt = worldToScreen(px, py);
    // Outer glow ring
    ctx.beginPath();
    ctx.strokeStyle = 'rgba(251,191,36,0.45)';
    ctx.lineWidth = 8;
    ctx.arc(pt.x, pt.y, 14, 0, Math.PI * 2);
    ctx.stroke();
    // Inner ring
    ctx.beginPath();
    ctx.strokeStyle = '#fbbf24';
    ctx.lineWidth = 2.5;
    ctx.arc(pt.x, pt.y, 9, 0, Math.PI * 2);
    ctx.stroke();
    // Pin dot
    ctx.beginPath();
    ctx.fillStyle = '#fbbf24';
    ctx.arc(pt.x, pt.y, 5, 0, Math.PI * 2);
    ctx.fill();
  }

  ctx.restore();

  rr(ctx, mx, my, mw, mh, 16);
  ctx.strokeStyle = C.edge;
  ctx.lineWidth = 1.5;
  ctx.stroke();

  // Legend
  ctx.font = f(11, '700');
  ctx.fillStyle = C.dim;
  ctx.fillText('FERTILE', 50, 575);
  ctx.fillStyle = '#1a4a32';
  ctx.fillRect(105, 566, 14, 10);
  ctx.fillStyle = C.dim;
  ctx.fillText('PLAINS', 130, 575);
  ctx.fillStyle = '#2f6a45';
  ctx.fillRect(180, 566, 14, 10);
  ctx.fillStyle = C.dim;
  ctx.fillText('CASTLE', 205, 575);
  ctx.fillStyle = '#e8c547';
  ctx.beginPath(); ctx.arc(260, 571, 4, 0, Math.PI * 2); ctx.fill();

  if (hasPos) {
    ctx.fillStyle = '#fbbf24';
    ctx.beginPath(); ctx.arc(290, 571, 5, 0, Math.PI * 2); ctx.fill();
    ctx.fillStyle = C.text;
    ctx.font = f(12, '600');
    ctx.fillText('Your city', 300, 575);
  }

  const ago = stats.location_updated_ago || null;
  footerBar(ctx, H, ago
    ? ('Location · ' + (hasPos ? px + ', ' + py : 'unknown') + ' · ' + ago + ' · ' + utc(p.asOf))
    : ('Location · ' + (hasPos ? px + ', ' + py : 'unknown') + ' · ' + utc(p.asOf)));
  return canvas.toBuffer('image/png');
}



async function statePrepPoints(st, matchupRow) {
  const r = matchupRow || {};
  const usKid = st.id;
  const themKid = r.opponentKid != null ? r.opponentKid : '?';
  const days = (r.days || []).slice().sort((a, b) => (Number(a.day) || 0) - (Number(b.day) || 0));
  const rows = days.length ? days : [];
  const H = 170 + Math.max(rows.length, 1) * 62 + 110;
  const { canvas, ctx } = frame(Math.max(H, 520));

  label(ctx, 'PREPARATION RANKINGS', 50, 52);
  ctx.font = f(30, '800');
  ctx.fillStyle = C.text;
  ctx.fillText(fit(ctx, 'State ' + usKid + ' vs ' + themKid, W - 100), 50, 92);

  const totalUs = r.prepScore != null ? Number(r.prepScore) : rows.reduce((s, d) => s + (Number(d.home) || 0), 0);
  const totalThem = r.prepOppScore != null ? Number(r.prepOppScore) : rows.reduce((s, d) => s + (Number(d.away) || 0), 0);
  const usWin = totalUs >= totalThem;

  // Overall box
  const boxX = W - 420, boxY = 120, boxW = 370, boxH = 170;
  rr(ctx, boxX, boxY, boxW, boxH, 18);
  ctx.fillStyle = 'rgba(120,170,230,0.08)';
  ctx.fill();
  ctx.strokeStyle = C.edge; ctx.lineWidth = 1.4; ctx.stroke();
  ctx.font = f(13, '700'); ctx.fillStyle = C.dim;
  ctx.fillText('Overall Preparation Score', boxX + 24, boxY + 30);
  ctx.font = f(34, '800'); ctx.fillStyle = usWin ? C.num : C.text;
  ctx.fillText(num(totalUs), boxX + 24, boxY + 78);
  ctx.font = f(18, '800'); ctx.fillStyle = usWin ? C.green : C.red;
  ctx.fillText('(' + (usWin ? 'W' : 'L') + ')', boxX + 24, boxY + 104);
  ctx.font = f(18, '700'); ctx.fillStyle = C.text;
  ctx.textAlign = 'center';
  ctx.fillText(String(usKid), boxX + boxW / 2 - 40, boxY + 138);
  ctx.fillStyle = C.dim; ctx.fillText('vs', boxX + boxW / 2, boxY + 138);
  ctx.fillStyle = C.text; ctx.fillText(String(themKid), boxX + boxW / 2 + 40, boxY + 138);
  ctx.textAlign = 'right';
  ctx.font = f(34, '800'); ctx.fillStyle = !usWin ? '#ff6b7a' : C.text;
  ctx.fillText(num(totalThem), boxX + boxW - 24, boxY + 78);
  ctx.font = f(18, '800'); ctx.fillStyle = !usWin ? C.green : C.red;
  ctx.fillText('(' + (!usWin ? 'W' : 'L') + ')', boxX + boxW - 24, boxY + 104);
  ctx.textAlign = 'left';

  // Bars/list
  const leftX = 50, chartW = boxX - 90;
  let y = 140;
  if (!rows.length) {
    ctx.font = f(16, '600'); ctx.fillStyle = C.dim;
    ctx.fillText('No daily prep breakdown available.', leftX, y + 10);
  } else {
    const maxV = Math.max(1, ...rows.map(d => Math.max(Number(d.home) || 0, Number(d.away) || 0)));
    for (const d of rows) {
      const home = Number(d.home) || 0;
      const away = Number(d.away) || 0;
      const dayLabel = (d.status && String(d.status).trim()) ? String(d.status).trim() : ('Day ' + (d.day != null ? d.day : '?'));
      ctx.font = f(13, '700'); ctx.fillStyle = C.label;
      ctx.fillText(fit(ctx, 'Day ' + (d.day != null ? d.day : '?') + ' � ' + dayLabel, chartW), leftX, y);
      const by = y + 10;
      const barH = 14;
      // home bar (blue)
      const hw = Math.max(4, Math.round((home / maxV) * chartW));
      rr(ctx, leftX, by, chartW, barH, 7); ctx.fillStyle = C.barBg; ctx.fill();
      rr(ctx, leftX, by, hw, barH, 7); ctx.fillStyle = C.bar; ctx.fill();
      ctx.font = f(12, '700'); ctx.fillStyle = C.text; ctx.textAlign = 'right';
      ctx.fillText(num(home), leftX + chartW - 8, by + 11);
      ctx.textAlign = 'left';
      // away bar (red)
      const ay = by + 18;
      const aw = Math.max(4, Math.round((away / maxV) * chartW));
      rr(ctx, leftX, ay, chartW, barH, 7); ctx.fillStyle = C.barBg; ctx.fill();
      rr(ctx, leftX, ay, aw, barH, 7); ctx.fillStyle = '#ff5d6c'; ctx.fill();
      ctx.font = f(12, '700'); ctx.fillStyle = C.text; ctx.textAlign = 'right';
      ctx.fillText(num(away), leftX + chartW - 8, ay + 11);
      ctx.textAlign = 'left';
      y += 62;
    }
  }

  const verdict = r.prep || r.result || '';
  const season = r.season != null ? ('Season ' + r.season + ' � ') : '';
  footerBar(ctx, Math.max(H, 520), season + 'Prep points � ' + (verdict ? (String(verdict) + ' � ') : '') + utc(st.asOf));
  return canvas.toBuffer('image/png');
}
async function stateAppointments(s, data) {
  const ministers = (data && data.ministers) || [];
  const offenders = (data && data.offenders) || [];
  const filledM = ministers.filter((m) => !m.vacant && (m.nick_name || m.uid)).length;
  const filledO = offenders.filter((m) => !m.vacant && (m.nick_name || m.uid)).length;

  const absUrl = (u) => {
    if (!u) return null;
    if (String(u).startsWith('http')) return String(u);
    return 'https://www.whiteoutstats.com' + (String(u).startsWith('/') ? u : '/' + u);
  };

  const cols = 6;
  const cellW = 140;
  const cellH = 150;
  const gap = 12;

  const minRows = Math.max(1, Math.ceil(Math.max(ministers.length, 6) / cols));
  const offRows = Math.max(1, Math.ceil(Math.max(offenders.length, 6) / cols));
  const H = 130 + minRows * (cellH + gap) + 60 + offRows * (cellH + gap) + 50;
  const { canvas, ctx } = frame(Math.max(H, 520));

  label(ctx, 'APPOINTMENTS', 50, 52);
  ctx.font = f(26, '800');
  ctx.fillStyle = C.text;
  ctx.fillText('State ' + s.id, 50, 88);

  async function drawGrid(title, countLabel, seats, startY) {
    ctx.font = f(14, '700');
    ctx.fillStyle = C.label;
    ctx.fillText(title, 50, startY);
    const titleW = ctx.measureText(title).width;
    ctx.font = f(12, '700');
    const chip = countLabel;
    const tw = ctx.measureText(chip).width + 20;
    rr(ctx, 50 + titleW + 14, startY - 14, tw, 20, 10);
    ctx.fillStyle = 'rgba(120,170,230,0.12)';
    ctx.fill();
    ctx.fillStyle = C.num;
    ctx.fillText(chip, 50 + titleW + 24, startY);

    const y0 = startY + 18;
    const list = seats.slice();
    while (list.length < 6) list.push({ vacant: true, role: '—' });

    for (let i = 0; i < list.length; i++) {
      const seat = list[i];
      const col = i % cols;
      const row = Math.floor(i / cols);
      const x = 50 + col * (cellW + gap);
      const y = y0 + row * (cellH + gap);

      rr(ctx, x, y, cellW, cellH, 14);
      ctx.fillStyle = 'rgba(120,170,230,0.06)';
      ctx.fill();
      ctx.strokeStyle = C.edge;
      ctx.lineWidth = 1.2;
      rr(ctx, x, y, cellW, cellH, 14);
      ctx.stroke();

      ctx.font = f(10, '700');
      ctx.fillStyle = C.dim;
      ctx.textAlign = 'center';
      const role = String(seat.role || seat.office_name || 'Seat').toUpperCase();
      ctx.fillText(fit(ctx, role, cellW - 12), x + cellW / 2, y + 18);

      const vacant = !!seat.vacant || !(seat.nick_name || seat.uid);
      if (vacant) {
        const icon = await img(absUrl(seat.icon_url));
        if (icon) ctx.drawImage(icon, x + cellW / 2 - 22, y + 40, 44, 44);
        else {
          ctx.beginPath();
          ctx.fillStyle = 'rgba(255,255,255,0.08)';
          ctx.arc(x + cellW / 2, y + 62, 22, 0, Math.PI * 2);
          ctx.fill();
        }
        ctx.font = f(13, '600');
        ctx.fillStyle = C.dim;
        ctx.fillText('Vacant', x + cellW / 2, y + 120);
      } else {
        const av = await img(seat.avatar_url);
        circleImage(ctx, av, x + cellW / 2 - 28, y + 36, 56, C.edge);
        const tag = seat.alliance_abbr ? '[' + seat.alliance_abbr + '] ' : '';
        const name = tag + (seat.nick_name || 'Unknown');
        ctx.font = f(12, '700');
        ctx.fillStyle = C.text;
        ctx.fillText(fit(ctx, name, cellW - 12), x + cellW / 2, y + 118);
      }
      ctx.textAlign = 'left';
    }
    return y0 + Math.ceil(list.length / cols) * (cellH + gap);
  }

  let y = 115;
  y = await drawGrid('MINISTERS', filledM + '/' + Math.max(ministers.length, 6), ministers, y);
  y += 28;
  await drawGrid('OFFENDERS', String(filledO), offenders, y);

  footerBar(ctx, Math.max(H, 520), 'Appointments · ' + utc(s.asOf));
  return canvas.toBuffer('image/png');
}

/** Shared format helpers used by lookupData view-models */
const _util = { num, int, utc, ago, fit };

module.exports = {
  playerOverview, playerStandings, playerProgression, playerCrowns, playerMap,
  allianceOverview, stateOverview, stateVsState, stateMatchupHistory, statePrepPoints, stateMap, allianceMap, stateAppointments,
  _util,
};
