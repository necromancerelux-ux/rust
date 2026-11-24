'use strict';

/**
 * Discord-style image cards matching WoS Oracle bot layout:
 * dark blue rounded cards with avatar, stats grid, ranks.
 */

const { createCanvas, loadImage } = require('@napi-rs/canvas');

function fmt(n) {
  if (n == null || !Number.isFinite(Number(n))) return '—';
  n = Number(n);
  const a = Math.abs(n);
  if (a >= 1e9) return (n / 1e9).toFixed(2) + 'B';
  if (a >= 1e6) return (n / 1e6).toFixed(2) + 'M';
  if (a >= 1e3) return (n / 1e3).toFixed(1) + 'K';
  return String(Math.round(n));
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

async function loadAvatar(url) {
  if (!url) return null;
  try {
    const full = /^https?:\/\//i.test(url) ? url : 'https://www.whiteoutstats.com' + (url.startsWith('/') ? url : '/' + url);
    const res = await fetch(full, { headers: { 'User-Agent': 'Mozilla/5.0', Accept: 'image/*' }, timeout: 8000 });
    if (!res.ok) return null;
    return await loadImage(Buffer.from(await res.arrayBuffer()));
  } catch {
    return null;
  }
}

/**
 * Player overview card — matches Oracle player overview screenshot.
 */
async function renderPlayerOverviewCard(player) {
  const r = player.raw || player;
  const W = 720;
  const H = 260;
  const canvas = createCanvas(W, H);
  const ctx = canvas.getContext('2d');

  // background
  ctx.fillStyle = '#0f1419';
  roundRect(ctx, 0, 0, W, H, 16);
  ctx.fill();

  // inner panel
  ctx.fillStyle = '#15202b';
  roundRect(ctx, 12, 12, W - 24, H - 24, 14);
  ctx.fill();

  // accent bar
  ctx.fillStyle = '#3b82f6';
  ctx.fillRect(12, 12, 4, H - 24);

  const name = String(player.name || r.nick_name || 'Chief');
  const tag = player.alliance?.tag || r.alliance_abbr || '';
  const fid = player.chiefId || r.fid || r.governor_id || '';
  const kid = player.stateId ?? r.kid ?? '';
  const rankLabel = r.alliance_rank_label || (player.alliance?.rankLabel) || '';
  const aname = player.alliance?.name || r.alliance_name || '';
  const furnace = player.furnace ?? r.stove_lv ?? '—';
  const fc = r.tg_label || r.level_label || (r.tg_level != null ? 'FC' + r.tg_level : '');

  // avatar
  const av = await loadAvatar(player.avatarUrl || r.avatar_url);
  const ax = 36, ay = 36, as = 72;
  ctx.save();
  ctx.beginPath();
  ctx.arc(ax + as / 2, ay + as / 2, as / 2, 0, Math.PI * 2);
  ctx.closePath();
  ctx.clip();
  if (av) ctx.drawImage(av, ax, ay, as, as);
  else {
    ctx.fillStyle = '#1e293b';
    ctx.fillRect(ax, ay, as, as);
  }
  ctx.restore();
  // avatar ring
  ctx.strokeStyle = '#334155';
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.arc(ax + as / 2, ay + as / 2, as / 2 + 1, 0, Math.PI * 2);
  ctx.stroke();

  // name
  ctx.fillStyle = '#f1f5f9';
  ctx.font = 'bold 26px sans-serif';
  ctx.fillText(name, 128, 58);
  if (tag) {
    const nw = ctx.measureText(name).width;
    ctx.fillStyle = '#94a3b8';
    ctx.font = 'bold 18px sans-serif';
    ctx.fillText(tag, 128 + nw + 8, 58);
  }

  // subtitle
  ctx.fillStyle = '#64748b';
  ctx.font = '14px sans-serif';
  const sub = [
    fid ? 'Chief ID ' + fid : null,
    rankLabel || (aname ? aname : null),
    fc || null,
  ].filter(Boolean).join(' · ');
  ctx.fillText(sub.slice(0, 70), 128, 82);

  // stat boxes
  const stats = [
    { label: 'POWER', value: fmt(player.power ?? r.power) },
    { label: 'KILLS', value: fmt(player.kills ?? r.kills) },
    { label: 'FURNACE', value: furnace !== '—' ? 'F' + furnace : '—' },
    { label: 'VIP', value: String(player.vip ?? r.vip_level ?? r.vip ?? '—') },
  ];
  const boxW = 140;
  const gap = 12;
  const startX = 36;
  const boxY = 130;
  stats.forEach((s, i) => {
    const x = startX + i * (boxW + gap);
    ctx.fillStyle = '#0b1220';
    roundRect(ctx, x, boxY, boxW, 70, 10);
    ctx.fill();
    ctx.fillStyle = '#64748b';
    ctx.font = '11px sans-serif';
    ctx.fillText(s.label, x + 14, boxY + 22);
    ctx.fillStyle = '#e2e8f0';
    ctx.font = 'bold 22px sans-serif';
    ctx.fillText(String(s.value), x + 14, boxY + 52);
  });

  // state chip bottom
  ctx.fillStyle = '#334155';
  ctx.font = '12px sans-serif';
  ctx.fillText(kid !== '' && kid != null ? 'State ' + kid : '', 36, H - 28);

  return canvas.toBuffer('image/png');
}

/**
 * Standings table card
 */
async function renderStandingsCard(player) {
  const r = player.raw || player;
  const boards = r.leaderboards || [];
  const rows = boards.length ? boards.slice(0, 12) : [];
  const W = 720;
  const rowH = 28;
  const H = 90 + Math.max(rows.length, 1) * rowH + 24;
  const canvas = createCanvas(W, H);
  const ctx = canvas.getContext('2d');

  ctx.fillStyle = '#0f1419';
  roundRect(ctx, 0, 0, W, H, 16);
  ctx.fill();
  ctx.fillStyle = '#15202b';
  roundRect(ctx, 12, 12, W - 24, H - 24, 14);
  ctx.fill();

  const name = String(player.name || r.nick_name || 'Chief');
  ctx.fillStyle = '#f1f5f9';
  ctx.font = 'bold 18px sans-serif';
  ctx.fillText(name + ' · State ' + (player.stateId ?? r.kid ?? '—'), 28, 42);

  ctx.fillStyle = '#64748b';
  ctx.font = '12px sans-serif';
  ctx.fillText('BOARD', 28, 72);
  ctx.fillText('SCORE', 320, 72);
  ctx.fillText('RANK', 480, 72);

  if (!rows.length) {
    ctx.fillStyle = '#94a3b8';
    ctx.font = '14px sans-serif';
    ctx.fillText('No standings available.', 28, 110);
  } else {
    rows.forEach((b, i) => {
      const y = 96 + i * rowH;
      ctx.fillStyle = i % 2 ? '#0b1220' : 'transparent';
      if (i % 2) {
        roundRect(ctx, 20, y - 18, W - 40, rowH, 4);
        ctx.fill();
      }
      ctx.fillStyle = '#e2e8f0';
      ctx.font = '13px sans-serif';
      ctx.fillText(String(b.name || '—').slice(0, 28), 28, y);
      ctx.fillText(b.value_label || fmt(b.value), 320, y);
      const rank = b.kingdom_rank_label || (b.kingdom_rank != null ? '#' + b.kingdom_rank : '—');
      ctx.fillStyle = '#38bdf8';
      ctx.fillText(String(rank), 480, y);
    });
  }

  return canvas.toBuffer('image/png');
}

/**
 * Alliance overview card
 */
async function renderAllianceOverviewCard(alliance) {
  const W = 720;
  const H = 280;
  const canvas = createCanvas(W, H);
  const ctx = canvas.getContext('2d');

  ctx.fillStyle = '#0f1419';
  roundRect(ctx, 0, 0, W, H, 16);
  ctx.fill();
  ctx.fillStyle = '#15202b';
  roundRect(ctx, 12, 12, W - 24, H - 24, 14);
  ctx.fill();
  ctx.fillStyle = '#3b82f6';
  ctx.fillRect(12, 12, 4, H - 24);

  const tag = alliance.tag || alliance.abbr || '?';
  const name = alliance.name || 'Alliance';
  ctx.fillStyle = '#f1f5f9';
  ctx.font = 'bold 24px sans-serif';
  ctx.fillText('[' + tag + '] ' + name, 32, 52);

  ctx.fillStyle = '#94a3b8';
  ctx.font = '14px sans-serif';
  ctx.fillText(
    'State ' + (alliance.stateId || alliance.kid || '—') +
    (alliance.leader ? ' · ' + alliance.leader : ''),
    32, 78
  );

  const stats = [
    { label: 'POWER', value: fmt(alliance.power) },
    { label: 'MEMBERS', value: String(alliance.members ?? alliance.count ?? '—') },
    { label: 'RANK', value: alliance.rank != null ? '#' + alliance.rank : '—' },
    { label: 'LEVEL', value: alliance.lv != null ? String(alliance.lv) : '—' },
  ];
  stats.forEach((s, i) => {
    const x = 32 + i * 165;
    ctx.fillStyle = '#0b1220';
    roundRect(ctx, x, 100, 150, 64, 10);
    ctx.fill();
    ctx.fillStyle = '#64748b';
    ctx.font = '11px sans-serif';
    ctx.fillText(s.label, x + 12, 122);
    ctx.fillStyle = '#e2e8f0';
    ctx.font = 'bold 20px sans-serif';
    ctx.fillText(s.value, x + 12, 150);
  });

  // top members
  const roster = (alliance.roster || []).slice(0, 5);
  if (roster.length) {
    ctx.fillStyle = '#64748b';
    ctx.font = '11px sans-serif';
    ctx.fillText('TOP MEMBERS', 32, 190);
    roster.forEach((m, i) => {
      const y = 210 + i * 12;
      if (y > H - 20) return;
      ctx.fillStyle = '#cbd5e1';
      ctx.font = '12px sans-serif';
      const line = (m.nick_name || m.name || '?') + '  ' + fmt(m.power) + (m.stove_lv != null ? '  F' + m.stove_lv : '');
      ctx.fillText(line.slice(0, 60), 32, y);
    });
  }

  return canvas.toBuffer('image/png');
}

/**
 * State overview card
 */
async function renderStateOverviewCard(state) {
  const W = 720;
  const H = 300;
  const canvas = createCanvas(W, H);
  const ctx = canvas.getContext('2d');
  const kid = state.kid || state.id;
  const health = state.health || {};

  ctx.fillStyle = '#0f1419';
  roundRect(ctx, 0, 0, W, H, 16);
  ctx.fill();
  ctx.fillStyle = '#15202b';
  roundRect(ctx, 12, 12, W - 24, H - 24, 14);
  ctx.fill();
  ctx.fillStyle = '#3b82f6';
  ctx.fillRect(12, 12, 4, H - 24);

  ctx.fillStyle = '#f1f5f9';
  ctx.font = 'bold 26px sans-serif';
  ctx.fillText('State ' + kid, 32, 52);

  ctx.fillStyle = '#94a3b8';
  ctx.font = '14px sans-serif';
  ctx.fillText(
    (state.player_count ?? state.located ?? '—') + ' tracked players · ' +
    (state.alliance_count ?? '—') + ' alliances',
    32, 78
  );

  // health badge
  const grade = health.grade || health.label || '';
  if (grade) {
    ctx.fillStyle = '#1d4ed8';
    roundRect(ctx, W - 100, 28, 56, 36, 8);
    ctx.fill();
    ctx.fillStyle = '#fff';
    ctx.font = 'bold 18px sans-serif';
    ctx.fillText(String(grade), W - 88, 52);
  }

  const stats = [
    { label: 'POWER', value: fmt(state.power) },
    { label: 'AVG POWER', value: fmt(state.avg_power) },
    { label: 'TOP POWER', value: fmt(state.top_power) },
    { label: 'ACTIVE 7D', value: String(state.active_7d ?? '—') },
  ];
  stats.forEach((s, i) => {
    const x = 32 + i * 165;
    ctx.fillStyle = '#0b1220';
    roundRect(ctx, x, 100, 150, 64, 10);
    ctx.fill();
    ctx.fillStyle = '#64748b';
    ctx.font = '11px sans-serif';
    ctx.fillText(s.label, x + 12, 122);
    ctx.fillStyle = '#e2e8f0';
    ctx.font = 'bold 18px sans-serif';
    ctx.fillText(String(s.value), x + 12, 150);
  });

  if (health.blurb) {
    ctx.fillStyle = '#94a3b8';
    ctx.font = '13px sans-serif';
    ctx.fillText(String(health.blurb).slice(0, 80), 32, 200);
  }

  if (health.components) {
    const c = health.components;
    const bits = [
      'Whales ' + (c.power ?? '—'),
      'Activity ' + (c.activity_7d ?? '—'),
      'Size ' + (c.size ?? '—'),
    ];
    ctx.fillStyle = '#64748b';
    ctx.font = '12px sans-serif';
    ctx.fillText(bits.join('   ·   '), 32, 230);
  }

  return canvas.toBuffer('image/png');
}

module.exports = {
  renderPlayerOverviewCard,
  renderStandingsCard,
  renderAllianceOverviewCard,
  renderStateOverviewCard,
  fmt,
};
