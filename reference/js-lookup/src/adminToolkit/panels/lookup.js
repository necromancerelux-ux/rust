'use strict';

/**
 * Admin Toolkit panel: Lookup — new persistent private panels (not forum trackers).
 */
const { refreshBtn, open } = require('./_helpers');

function lookupMenu() {
  return require('../../lookup/menu');
}
function playerPanel() {
  return require('../../lookup/components/playerPanel');
}
function alliancePanel() {
  return require('../../lookup/components/alliancePanel');
}
function statePanel() {
  return require('../../lookup/components/statePanel');
}

module.exports = {
  key: 'lookup',
  emoji: '🔎',
  title: 'LOOKUP',
  postName: '🔎 Lookup',
  blurb: 'Private player / alliance / state search. Results only visible to the searcher.',
  status() {
    return [
      'Open Lookup Settings to set the panel channel and post search buttons.',
      'Or start a private search below.',
    ];
  },
  buttons: [[
    open('settings', 'Open Lookup Settings', '⚙️', (i) => lookupMenu().showHub(i), 'primary', { defer: false }),
    open('player', 'Search Player', '👤', async (i) => {
      i.customId = 'lookup:player:search';
      await playerPanel().handlePanelInteraction(i);
    }, 'secondary', { defer: false }),
    open('alliance', 'Search Alliance', '🤝', async (i) => {
      i.customId = 'lookup:alliance:search';
      await alliancePanel().handlePanelInteraction(i);
    }, 'secondary', { defer: false }),
    open('state', 'Search State', '🏰', async (i) => {
      i.customId = 'lookup:state:search';
      await statePanel().handlePanelInteraction(i);
    }, 'secondary', { defer: false }),
    refreshBtn,
  ]],
};
