'use strict';

/**
 * Lookup admin hub — reached from Settings.
 * - Set panel channel
 * - Post/refresh persistent panels
 * - (API key entry moved to Owner panel / DM → Whiteout Stats)
 */

const {
  EmbedBuilder, ActionRowBuilder, ButtonBuilder, ButtonStyle,
  ChannelSelectMenuBuilder, ChannelType,
  ModalBuilder, TextInputBuilder, TextInputStyle,
  MessageFlags,
} = require('discord.js');

function __loadTheme() {
  const tries = ['./utils/theme','../utils/theme','../giftcode/utils/theme','../../utils/theme','./giftcode/utils/theme'];
  for (const p of tries) { try { return require(p); } catch (e) { if (e.code !== 'MODULE_NOT_FOUND') throw e; } }
  return {
    emColor1: 0x5865f2, emColor2: 0x57f287, emColor3: 0xfee75c,
    warnIcon: '⚠️', deniedIcon: '🚫', backIcon: '◀️',
    upperDivider: '────', lowerDivider: '────',
    withUtcFooter(embed) { try { return embed.setFooter({ text: 'UTC' }); } catch { return embed; } },
  };
}
const theme = __loadTheme();
function __loadPermissions() {
  const tries = ['../permissions/permissionManager','./permissions/permissionManager','../../permissions/permissionManager'];
  for (const p of tries) { try { return require(p); } catch (e) { if (e.code !== 'MODULE_NOT_FOUND') throw e; } }
  return { isAdmin: () => true, isOwner: () => true };
}
const permissions = __loadPermissions();

const store = require('./database/store');
const wosCredentials = require('./database/wosCredentials');
const dataClient = require('./services/dataClient');
const playerPanel = require('./components/playerPanel');
const alliancePanel = require('./components/alliancePanel');
const statePanel = require('./components/statePanel');

async function render(interaction, payload) {
  if (interaction.deferred || interaction.replied) await interaction.editReply(payload);
  else await interaction.update(payload);
}

function isOwner(interaction) {
  try {
    return Boolean(permissions.isOwner(String(interaction.user.id)));
  } catch {
    return false;
  }
}

function requireAdmin(interaction) {
  const uid = String(interaction.user.id);
  try {
    if (permissions.isOwner(uid) || permissions.isAdmin(uid)) return true;
  } catch { /* ignore */ }
  try {
    if (interaction.memberPermissions?.has?.('ManageGuild')) return true;
  } catch { /* ignore */ }
  // Discord built-in Administrator
  try {
    if (interaction.member?.permissions?.has?.('Administrator')) return true;
  } catch { /* ignore */ }
  return false;
}

async function deny(interaction) {
  const payload = {
    content: `${theme.deniedIcon || '🚫'} You don't have permission to use this.`,
    ephemeral: true,
  };
  if (interaction.replied || interaction.deferred) await interaction.followUp(payload);
  else await interaction.reply(payload);
}

function buildHubEmbed(settings, owner = false) {
  const channelLine = settings.channel_id
    ? `<#${settings.channel_id}>`
    : `${theme.warnIcon || '⚠️'} not set`;

  // Source names are Bot-Owner-only; everyone else just sees the panel settings.
  const apiLine = owner
    ? (wosCredentials.isConfigured()
        ? '📡 **Data** — Whiteout Stats (API key saved) · Oracle as backup'
        : '📡 **Data** — Whiteout Stats public site · Oracle as backup\n' +
          '_Add an optional API key in Owner panel / DM → **Whiteout Stats**._')
    : null;

  const embed = new EmbedBuilder()
    .setTitle('🔎 Lookup')
    .setDescription(
      [
        'Persistent private search panels for players, alliances and states.\n',
        `📌 **Panel Channel** — where the search panels live (currently ${channelLine})`,
        ...(apiLine ? [apiLine] : []),
        '',
        'Members click a button → enter an ID → see a private multi-tab result only they can view.',
      ].join('\n')
    )
    .setColor(theme.emColor1 || 0x5865f2);

  return theme.withUtcFooter ? theme.withUtcFooter(embed) : embed;
}

async function showHub(interaction) {
  const settings = store.getSettings(interaction.guildId);
  const row1 = new ActionRowBuilder().addComponents(
    new ButtonBuilder()
      .setCustomId('lookup:setchannel')
      .setLabel('Panel Channel')
      .setEmoji('📌')
      .setStyle(settings.channel_id ? ButtonStyle.Success : ButtonStyle.Danger),
    new ButtonBuilder()
      .setCustomId('lookup:postpanels')
      .setLabel('Post / Refresh Panels')
      .setEmoji('📤')
      .setStyle(ButtonStyle.Primary)
  );

  const backRow = new ActionRowBuilder().addComponents(
    new ButtonBuilder()
      .setCustomId('settings:back')
      .setLabel('Back to Settings')
      .setEmoji(theme.backIcon || '◀️')
      .setStyle(ButtonStyle.Secondary)
  );

  await render(interaction, {
    embeds: [buildHubEmbed(settings, isOwner(interaction))],
    components: [row1, backRow],
  });
}

async function handleHubInteraction(interaction) {
  const id = interaction.customId;

  if (id === 'lookup:show') {
    await showHub(interaction);
    return true;
  }

  if (id === 'lookup:setchannel') {
    if (!requireAdmin(interaction)) {
      await deny(interaction);
      return true;
    }
    const select = new ChannelSelectMenuBuilder()
      .setCustomId('lookup:channelselect')
      .setPlaceholder('Choose the channel for Lookup panels')
      .setChannelTypes(ChannelType.GuildText, ChannelType.GuildAnnouncement)
      .setMinValues(1)
      .setMaxValues(1);

    await render(interaction, {
      embeds: [
        new EmbedBuilder()
          .setTitle('📌 Choose Panel Channel')
          .setDescription('Select the channel where the Player / Alliance / State search panels will live.')
          .setColor(theme.emColor1 || 0x5865f2),
      ],
      components: [
        new ActionRowBuilder().addComponents(select),
        new ActionRowBuilder().addComponents(
          new ButtonBuilder()
            .setCustomId('lookup:show')
            .setLabel('Cancel')
            .setStyle(ButtonStyle.Secondary)
        ),
      ],
    });
    return true;
  }

  if (id === 'lookup:channelselect') {
    if (!requireAdmin(interaction)) {
      await deny(interaction);
      return true;
    }
    const channelId = interaction.values[0];
    store.setChannel(interaction.guildId, channelId);

    try {
      const channel = await interaction.guild.channels.fetch(channelId);
      if (!channel || !(channel.isTextBased?.() ?? channel.isText)) {
        await interaction.reply({
          content: 'That channel is not a text channel the bot can post in.',
          ephemeral: true,
        }).catch(() => {});
        return true;
      }

      await interaction.deferUpdate();
      const results = [];
      for (const [name, fn] of [
        ['Player', () => playerPanel.postPanel(channel, interaction.guildId)],
        ['Alliance', () => alliancePanel.postPanel(channel, interaction.guildId)],
        ['State', () => statePanel.postPanel(channel, interaction.guildId)],
      ]) {
        try {
          await fn();
          results.push(`✅ ${name}`);
        } catch (err) {
          console.error(`[lookup] post ${name} failed:`, err);
          results.push(`❌ ${name}: ${err.message || err}`);
        }
      }
      await showHub(interaction);
      await interaction.followUp({
        content: `Lookup panels posted in <#${channelId}>:\n${results.join('\n')}`,
        ephemeral: true,
      }).catch(() => {});
    } catch (e) {
      console.error('[lookup] channelselect failed:', e);
      const msg = `Couldn't post panels — bot needs **View Channel** + **Send Messages** there.\n\`${e.message || e}\``;
      if (interaction.deferred || interaction.replied) {
        await interaction.followUp({ content: msg, ephemeral: true }).catch(() => {});
      } else {
        await interaction.reply({ content: msg, ephemeral: true }).catch(() => {});
      }
    }
    return true;
  }

  if (id === 'lookup:postpanels') {
    if (!requireAdmin(interaction)) {
      await deny(interaction);
      return true;
    }
    const settings = store.getSettings(interaction.guildId);
    if (!settings.channel_id) {
      await interaction.reply({
        content: 'Set a Panel Channel first.',
        ephemeral: true,
      });
      return true;
    }

    await interaction.deferUpdate();
    const channel = await interaction.guild.channels.fetch(settings.channel_id).catch(() => null);
    if (!channel) {
      await interaction.followUp({ content: 'Channel not found.', ephemeral: true });
      return true;
    }

    const results = [];
    for (const [name, fn] of [
      ['Player', () => playerPanel.postPanel(channel, interaction.guildId)],
      ['Alliance', () => alliancePanel.postPanel(channel, interaction.guildId)],
      ['State', () => statePanel.postPanel(channel, interaction.guildId)],
    ]) {
      try {
        await fn();
        results.push(`✅ ${name}`);
      } catch (err) {
        console.error(`[lookup] post ${name} panel failed:`, err);
        results.push(`❌ ${name}: ${err.message || err}`);
      }
    }

    try {
      await showHub(interaction);
    } catch (err) {
      console.error('[lookup] showHub after post failed:', err);
    }
    await interaction.followUp({
      content: `Panels in <#${settings.channel_id}>:\n${results.join('\n')}`,
      ephemeral: true,
    });
    return true;
  }

  return false;
}

module.exports = {
  showHub,
  handleHubInteraction,
};
