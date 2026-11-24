'use strict';

/**
 * Lookup feature entry point.
 * Wire `handleInteraction` into the bot's main interaction router.
 */

const playerPanel = require('./components/playerPanel');
const alliancePanel = require('./components/alliancePanel');
const statePanel = require('./components/statePanel');
const menu = require('./menu');

/**
 * Returns true if the interaction was handled by the Lookup system.
 */
async function handleInteraction(interaction) {
  if (!interaction.customId) return false;

  // Admin hub
  if (
    interaction.customId === 'lookup:show' ||
    interaction.customId === 'lookup:setchannel' ||
    interaction.customId === 'lookup:channelselect' ||
    interaction.customId === 'lookup:postpanels'
  ) {
    return menu.handleHubInteraction(interaction);
  }

  // Ack component interactions early (except modal-open buttons) to avoid "didn't respond in time"
  // when data fetches or image rendering take longer than 3 seconds.
  try {
    const id = interaction.customId;
    const opensModal = id === 'lookup:player:search' || id === 'lookup:alliance:search' || id === 'lookup:state:search';
    if (interaction.isMessageComponent && interaction.isMessageComponent() && !opensModal && !interaction.deferred && !interaction.replied) {
      await interaction.deferUpdate();
    }
  } catch (_) { /* handler will continue; it may reply/defer as needed */ }

  try {
    // Player panel + private flow
    if (interaction.customId.startsWith('lookup:player:')) {
      return await playerPanel.handlePanelInteraction(interaction);
    }

    // Alliance panel + private flow
    if (interaction.customId.startsWith('lookup:alliance:')) {
      return await alliancePanel.handlePanelInteraction(interaction);
    }

    // State panel + private flow
    if (interaction.customId.startsWith('lookup:state:')) {
      return await statePanel.handlePanelInteraction(interaction);
    }
  } catch (err) {
    console.error('[lookup] interaction handler error:', err && err.stack ? err.stack : err);
    try {
      const msg = 'Lookup interaction failed. Please try again.';
      if (interaction.deferred || interaction.replied) await interaction.followUp({ content: msg, ephemeral: true }).catch(() => {});
      else await interaction.reply({ content: msg, ephemeral: true }).catch(() => {});
    } catch (_) {}
    return true;
  }

  return false;
}

module.exports = {
  handleInteraction,
  menu,
  playerPanel,
  alliancePanel,
  statePanel,
};
