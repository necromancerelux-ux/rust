'use strict';

/** Persistent public "Search" panel factory shared by Player / Alliance / State. */
const { EmbedBuilder, ActionRowBuilder, ButtonBuilder, ButtonStyle } = require('discord.js');
const store = require('../database/store');

module.exports = function makePanel({ type, title, lines, buttonId, buttonLabel, emoji }) {
  function buildPanelEmbed() {
    return new EmbedBuilder().setTitle(title).setDescription(lines.join('\n')).setColor(0x3ba3e0);
  }
  function buildPanelRow() {
    return new ActionRowBuilder().addComponents(
      new ButtonBuilder().setCustomId(buttonId).setLabel(buttonLabel).setEmoji(emoji).setStyle(ButtonStyle.Primary)
    );
  }
  async function postPanel(channel, guildId) {
    const message = await channel.send({ embeds: [buildPanelEmbed()], components: [buildPanelRow()] });
    store.setPanelMessage(guildId, type, message.id);
    return message;
  }
  return { buildPanelEmbed, buildPanelRow, postPanel };
};
