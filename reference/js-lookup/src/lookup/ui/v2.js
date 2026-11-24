'use strict';

/**
 * Components V2 building blocks that reproduce the Oracle-style layout:
 *
 *   ┃ [ tab buttons / tab dropdown ]
 *   ┃ ───────────────
 *   ┃ ## Title
 *   ┃ body text / rendered image card
 *   ┃ [ dropdown ]  [ pager « ‹ 1/4 › » ]  [ link buttons ]
 *   ┃ ───────────────
 *   ┃ -# WoS Oracle · data as of <relative time>
 *
 * Everything lives inside ONE container so the left accent bar spans the
 * whole message, exactly like the reference screenshots.
 */

const {
  ContainerBuilder, TextDisplayBuilder, SeparatorBuilder, SeparatorSpacingSize,
  MediaGalleryBuilder, MediaGalleryItemBuilder, ActionRowBuilder, ButtonBuilder,
  ButtonStyle, StringSelectMenuBuilder, StringSelectMenuOptionBuilder,
  AttachmentBuilder, MessageFlags,
} = require('discord.js');

const ACCENT = 0x3ba3e0; // light-blue bar from the reference messages
const BRAND = 'OmniCore';

function container() {
  return new ContainerBuilder().setAccentColor(ACCENT);
}

function text(c, content) {
  const s = String(content == null ? '' : content).slice(0, 3900);
  if (s.trim()) c.addTextDisplayComponents(new TextDisplayBuilder().setContent(s));
  return c;
}

function title(c, t) { return text(c, '## ' + t); }

function sep(c, divider = true) {
  c.addSeparatorComponents(
    new SeparatorBuilder().setDivider(divider).setSpacing(SeparatorSpacingSize.Small)
  );
  return c;
}

/** Image card (attachment://name) */
function image(c, name) {
  c.addMediaGalleryComponents(
    new MediaGalleryBuilder().addItems(new MediaGalleryItemBuilder().setURL('attachment://' + name))
  );
  return c;
}

function attachment(buffer, name) {
  return new AttachmentBuilder(buffer, { name });
}

/** Discord relative timestamp, e.g. "8 minutes ago" */
function rel(ms) {
  const t = ms instanceof Date ? ms.getTime() : Number(ms);
  if (!Number.isFinite(t)) return 'just now';
  return '<t:' + Math.floor((t > 1e12 ? t : t * 1000) / 1000) + ':R>';
}

/** "WoS Oracle · roster as of 8 minutes ago" */
function footer(c, label, when) {
  sep(c, true);
  text(c, '-# ' + BRAND + ' · ' + label + (when != null ? ' ' + rel(when) : ''));
  return c;
}

/** Free-form footer (e.g. "Page 1 / 4 · 60 of 297 events · retrieved just now") */
function footerRaw(c, str) {
  sep(c, true);
  text(c, '-# ' + BRAND + ' · ' + str);
  return c;
}

/** Row of up to 5 buttons. defs: {id,label,style?,disabled?,emoji?,url?} */
function buttons(c, defs) {
  if (!defs || !defs.length) return c;
  const row = new ActionRowBuilder();
  for (const d of defs.slice(0, 5)) {
    const b = new ButtonBuilder().setLabel(String(d.label).slice(0, 80));
    if (d.url) b.setStyle(ButtonStyle.Link).setURL(d.url);
    else b.setCustomId(d.id).setStyle(d.style || ButtonStyle.Secondary);
    if (d.emoji) b.setEmoji(d.emoji);
    if (d.disabled) b.setDisabled(true);
    row.addComponents(b);
  }
  c.addActionRowComponents(row);
  return c;
}

/**
 * Tab strip made of buttons. Active tab is blurple (Primary).
 * Splits into rows of 4 so a 7-tab strip becomes 4 + 3 like the reference.
 */
function tabButtons(c, prefix, tabs, active, perRow = 4) {
  for (let i = 0; i < tabs.length; i += perRow) {
    buttons(c, tabs.slice(i, i + perRow).map((t) => ({
      id: prefix + ':' + t.key,
      label: t.label,
      style: t.key === active ? ButtonStyle.Primary : ButtonStyle.Secondary,
    })));
  }
  return c;
}

/**
 * Dropdown. opts: [{label, value, description?, default?}]
 * `placeholder` is what Discord shows when nothing is selected — in the
 * reference, tab dropdowns show the CURRENT tab name there.
 */
function select(c, id, placeholder, opts) {
  const seen = new Set();
  const list = (opts || []).filter((o) => {
    const v = String(o.value ?? '').slice(0, 100);
    if (!v || !String(o.label ?? '').trim() || seen.has(v)) return false;
    seen.add(v); return true;
  }).slice(0, 25);
  if (!list.length) return c;
  const m = new StringSelectMenuBuilder().setCustomId(id).setPlaceholder(String(placeholder).slice(0, 150));
  m.addOptions(list.map((o) => {
    const b = new StringSelectMenuOptionBuilder()
      .setLabel(String(o.label).slice(0, 100))
      .setValue(String(o.value).slice(0, 100));
    if (o.description) b.setDescription(String(o.description).slice(0, 100));
    if (o.default) b.setDefault(true);
    return b;
  }));
  c.addActionRowComponents(new ActionRowBuilder().addComponents(m));
  return c;
}

/** « ‹ 1 / 4 › »  — centre button is a disabled page indicator */
function pager(c, prefix, page, total) {
  const last = Math.max(0, total - 1);
  // ids must be unique inside one message, even when two buttons target the same page
  const mk = (tag, target, label, disabled) =>
    ({ id: prefix + ':p:' + target + ':' + tag, label, style: ButtonStyle.Secondary, disabled });
  buttons(c, [
    mk('first', 0, '«', page <= 0),
    mk('prev', Math.max(0, page - 1), '‹', page <= 0),
    { id: prefix + ':noop', label: (page + 1) + ' / ' + Math.max(1, total), style: ButtonStyle.Secondary, disabled: true },
    mk('next', Math.min(last, page + 1), '›', page >= last),
    mk('last', last, '»', page >= last),
  ]);
  return c;
}

/** Build the reply payload for any interaction method. */
function payload(c, files = [], ephemeral = true) {
  let flags = MessageFlags.IsComponentsV2;
  if (ephemeral) flags |= MessageFlags.Ephemeral;
  return { components: [c], files, flags };
}

/** For editReply/update — flags can't change Ephemeral after the fact. */
function editPayload(c, files = []) {
  return { components: [c], files, attachments: [], flags: MessageFlags.IsComponentsV2 };
}

/* ───────── formatting helpers shared by all panels ───────── */

function fmt(n) {
  if (n == null || n === '' || !Number.isFinite(Number(n))) return '—';
  n = Number(n);
  const a = Math.abs(n);
  const s = n < 0 ? '−' : '';
  if (a >= 1e9) return s + trim(a / 1e9, 2) + 'B';
  if (a >= 1e6) return s + trim(a / 1e6, 2) + 'M';
  if (a >= 1e3) return s + trim(a / 1e3, 1) + 'K';
  return s + String(Math.round(a));
}
function trim(v, d) { return String(Number(v.toFixed(d))); }
function signed(n) { n = Number(n); return (n >= 0 ? '+' : '−') + fmt(Math.abs(n)); }
function int(n) { return n == null || !Number.isFinite(Number(n)) ? '—' : Number(n).toLocaleString('en-US'); }

/** Compact age chip like `4h` `1d` `6d` */
function ageChip(ts) {
  const t = Number(ts);
  if (!Number.isFinite(t)) return '';
  const sec = Math.max(0, Date.now() / 1000 - (t > 1e12 ? t / 1000 : t));
  let v;
  if (sec < 3600) v = Math.max(1, Math.round(sec / 60)) + 'm';
  else if (sec < 86400) v = Math.round(sec / 3600) + 'h';
  else if (sec < 86400 * 30) v = Math.round(sec / 86400) + 'd';
  else v = Math.round(sec / (86400 * 30)) + 'mo';
  return '`' + v + '`';
}

/** `#001` style rank chip */
function rankChip(n) { return '`#' + String(n).padStart(3, '0') + '`'; }

function paginate(list, per, page) {
  const total = Math.max(1, Math.ceil(list.length / per));
  const p = Math.min(Math.max(0, page | 0), total - 1);
  return { items: list.slice(p * per, p * per + per), page: p, total };
}

module.exports = {
  ACCENT, BRAND, container, text, title, sep, image, attachment, rel, footer, footerRaw,
  buttons, tabButtons, select, pager, payload, editPayload,
  fmt, signed, int, ageChip, rankChip, paginate, MessageFlags,
};
