'use strict';

const { Events } = require('discord.js');

/**
 * Discord interaction routing (slash components, modals, buttons, selects).
 * Loaded once from index.js via registerInteractionHandler(client).
 */

// ── Interaction routing ─────────────────────────────────────────────────
const { handlePermissionInteraction, renderAdminList } = require('./permissions/adminMenu');
const { showMainMenu } = require('./commands/settingsMenu');
const { handleQuarantineInteraction } = require('./notifications/quarantineView');
const wizard = require('./notifications/wizard');
const editor = require('./notifications/editor');
const templates = require('./notifications/templates');
const notifHub = require('./notifications/menu');
const notifSettings = require('./notifications/settings');
const customNotif = require('./notifications/customNotif');
const liveClock = require('./utils/liveClock');
const messagesHub = require('./messages/menu');
const messagesCreate = require('./messages/create');
const messagesManage = require('./messages/manage');
const messagesGroups = require('./messages/groups');
const messagesLiveEdit = require('./messages/liveEdit');
const birthdayHub = require('./birthday/menu');
const musicHandlers = require('./music/handlers');
const musicHub = require('./music/menu');
const birthdayTranslatePanel = require('./birthday/translatePanel');
const birthdayPanel = require('./birthday/panel');
const birthdayEditor = require('./birthday/editor');
const birthdayBrowse = require('./birthday/browse');
const birthdayCleanup = require('./birthday/cleanup');
let lookupFeature = null;
try { lookupFeature = require('./lookup'); } catch (e) { console.error('[lookup] load failed:', e); }
const rolesHub = require('./roles/menu');
const rolesPanel = require('./roles/panel');
const rolesTranslatePanel = require('./roles/translatePanel');
const welcomeHub = require('./welcome/menu');
const welcomeEditor = require('./welcome/editor');
const welcomeCleanup = require('./welcome/cleanup');
const translate = require('./utils/translate');
const calculatorHub = require('./calculator/menu');
const calcTranslateMenu = require('./calculator/translateMenu');
const buildingCalculator = require('./calculator/buildingCalculator');
const chiefGearCalculator = require('./calculator/chiefGearCalculator');
const charmCalculator = require('./calculator/charmCalculator');
const warAcademyCalculator = require('./calculator/warAcademyCalculator');
const petCalculator = require('./calculator/petCalculator');
const svsCalculator = require('./calculator/svsCalculator');
const ministryHub = require('./ministry/menu');
const ministryPanelV2 = require('./ministry/panelV2');
const indexHub = require('./serverIndex/menu');
const { startReminderScheduler } = require('./joinDm/scheduler');
const { DM_REGISTER_PREFIX } = require('./joinDm/dmSender');
const wosMenu = require('./wos/menu');
const wosCleanup = require('./wos/cleanup');
const brandingMenu = require('./branding/menu');
const ticketMenu = require('./tickets/menu');
const giveawayMenu = require('./giveaways/menu');
const pollMenu = require('./polls/menu');
const ballotMenu = require('./ballots/menu');
const autoRoleMenu = require('./autorole/menu');
const levelMenu = require('./leveling/menu');
const logMenu = require('./logging/menu');
const tempVoiceMenu = require('./tempvoice/menu');
const tempVoiceControls = require('./tempvoice/controls');

// ── Player ID / Gift Code (Gift Code / Player ID feature) ───────────
const giftPanel = require('./giftcode/components/devPanel/router');
const playerMenu = require('./giftcode/components/devPanel/playerMenu');
const giftCodesMenu = require('./giftcode/components/devPanel/giftCodesMenu');
const { ensureGuild: ensureGiftcodeGuild, getGuildSettings: getGiftcodeGuildSettings } = require('./giftcode/database/guilds');
const giftcodePlayers = require('./giftcode/database/players');
const { postAdminLog, autoRemoveEmbed } = require('./giftcode/services/adminLog');
const giftPublicButtons = require('./giftcode/components/panelInteractions');
const idTranslatePanel = require('./giftcode/components/idTranslatePanel');
const { handleRemoveIdSelect, SELECT_ID: GIFTCODE_REMOVE_ID_SELECT } = require('./giftcode/components/removeIdSelect');
const { handleRegisterModalSubmit, MODAL_ID: GIFTCODE_REGISTER_MODAL_ID } = require('./giftcode/components/registerModal');
const {
  handleAddPlayerModalSubmit, handleViewFailedPlayersButton, handleRetryFailedPlayersButton,
  MODAL_ID: GIFTCODE_ADDPLAYER_MODAL_ID, VIEW_FAILED_PREFIX: GIFTCODE_VIEW_FAILED_PREFIX,
  RETRY_PREFIX: GIFTCODE_ADDPLAYER_RETRY_PREFIX,
} = require('./giftcode/components/addPlayerModal');
const { handleUseCodeModalSubmit, MODAL_ID: GIFTCODE_USE_CODE_MODAL_ID } = require('./giftcode/components/useCodeModal');
const { handleNicknameModalSubmit, MODAL_ID: GIFTCODE_NICKNAME_MODAL_ID } = require('./giftcode/components/nicknameModal');
const { handleClaimButton, APPROVE_PREFIX: GIFTCODE_APPROVE_PREFIX, DENY_PREFIX: GIFTCODE_DENY_PREFIX } = require('./giftcode/components/claimApproval');
const { handleRetryFailedButton, RETRY_PREFIX: GIFTCODE_RETRY_PREFIX } = require('./giftcode/components/giftCodeRetry');
const { refreshPanelStats } = require('./giftcode/components/idPanel');

/**
 * @param {import('discord.js').Client} client
 */
function registerInteractionHandler(client) {
const adminToolkit = require('./adminToolkit');
adminToolkit.install(); // R4/R5 scoped permission wrapper (see adminToolkit/permissions.js)

// The whole handler runs through the Toolkit wrapper: interactions that come from a Toolkit post (or a hub the
// Toolkit opened for an R4/R5 member) pass the existing admin checks for that guild only.
client.on(Events.InteractionCreate, (interaction) => adminToolkit.wrapInteraction(interaction, async () => {
  try {
    // ── R4/R5 Admin Toolkit buttons / modals ──────────────────────────
    if (typeof interaction.customId === 'string'
      && (interaction.customId.startsWith('atk:') || interaction.customId.startsWith('atkmodal:'))) {
      if (await adminToolkit.handleInteraction(interaction)) return;
    }

    // Live ticking UTC clock (see utils/liveClock.js): any interaction on a
    // message stops that message's tick loop first. Whichever handler ends
    // up rendering a screen restarts its own clock if that screen wants one
    // (currently: the main Settings Menu and the Notification hub) — this
    // just guarantees a stale interval never keeps overwriting a *different*
    // screen shown afterwards on the same ephemeral message.
    if ((interaction.isMessageComponent() || interaction.isModalSubmit()) && interaction.message) {
      liveClock.stopClock(interaction.message.id);
    }


    // ── Lookup (persistent panels + private flows) ─────────────────────
    if (lookupFeature && typeof interaction.customId === 'string' && interaction.customId.startsWith('lookup:')) {
      try {
        if (await lookupFeature.handleInteraction(interaction)) return;
      } catch (e) {
        console.error('[lookup] handleInteraction error:', e);
        try {
          const payload = { content: `Lookup error: ${e.message || e}`, ephemeral: true };
          if (interaction.replied || interaction.deferred) await interaction.followUp(payload);
          else await interaction.reply(payload);
        } catch { /* ignore */ }
        return;
      }
    }

    if (interaction.isChatInputCommand()) {
      const command = client.commands.get(interaction.commandName);
      if (!command) return;
      await command.execute(interaction);
      return;
    }

    if (interaction.isModalSubmit()) {
      if (interaction.customId.startsWith('wizmodal:')) {
        await wizard.handleWizardModalSubmit(interaction);
      } else if (interaction.customId.startsWith('neditmodal:')) {
        await editor.handleEditorModalSubmit(interaction);
      } else if (interaction.customId.startsWith('tmplmodal:')) {
        await templates.handleTemplateModalSubmit(interaction);
      } else if (interaction.customId.startsWith('notifsetmodal:')) {
        await notifSettings.handleSettingsModalSubmit(interaction);
      } else if (interaction.customId.startsWith('cnotifmodal:')) {
        await customNotif.handleCustomNotifModalSubmit(interaction);
      } else if (interaction.customId.startsWith('msgcmodal:')) {
        await messagesCreate.handleCreateModalSubmit(interaction);
      } else if (interaction.customId.startsWith('msgmmodal:')) {
        await messagesManage.handleManageModalSubmit(interaction);
      } else if (interaction.customId.startsWith('msggmodal:')) {
        await messagesGroups.handleGroupingModalSubmit(interaction);
      } else if (interaction.customId.startsWith('msglmodal:')) {
        await messagesLiveEdit.handleLiveEditModalSubmit(interaction);
      } else if (interaction.customId.startsWith('bdaymodal:')) {
        // Four sub-modules share this prefix (public set-birthday modal,
        // admin field editor, admin per-user edit, clean-up delay) — try
        // each in turn.
        (await birthdayPanel.handlePanelModalSubmit(interaction))
          || (await birthdayEditor.handleEditorModalSubmit(interaction))
          || (await birthdayBrowse.handleBrowseModalSubmit(interaction))
          || (await birthdayCleanup.handleModalSubmit(interaction));
      } else if (interaction.customId.startsWith('bdaytrmodal:')) {
        await birthdayTranslatePanel.handleMenuModalSubmit(interaction);
      } else if (interaction.customId.startsWith('welcmodal:')) {
        // Edit-embed and clean-up delay share this prefix — try each in turn.
        (await welcomeEditor.handleEditorModalSubmit(interaction))
          || (await welcomeCleanup.handleModalSubmit(interaction));
      } else if (interaction.customId.startsWith('wossneakmodal:')) {
        await wosMenu.handleSneakModalSubmit(interaction);
      } else if (interaction.customId.startsWith('wosmodal:')) {
        await wosCleanup.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('buildingcalcmodal:')) {
        await buildingCalculator.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('waracademycalcmodal:')) {
        await warAcademyCalculator.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('petcalcmodal:')) {
        await petCalculator.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('minv2modal:')) {
        const handled = await ministryPanelV2.handleInteraction(interaction);
        if (!handled) { /* ignore */ }
      } else if (interaction.customId.startsWith('svscalcmodal:')) {
        await svsCalculator.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('brandingmodal:')) {
        await brandingMenu.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('musicmodal:')) {
        await musicHandlers.handleModal(interaction);
      } else if (interaction.customId.startsWith('ticketmodal:')) {
        await ticketMenu.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('ownermodal:')) {
        const ownerPanel = require('./ownerPanel/menu');
        await ownerPanel.handleInteraction(interaction);
      } else if (interaction.customId.startsWith('gwmodal:')) {
        await giveawayMenu.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('ballotmodal:')) {
        await ballotMenu.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('levelsmodal:')) {
        await levelMenu.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('tempvoicemodal:')) {
        await tempVoiceMenu.handleModalSubmit(interaction);
      } else if (interaction.customId.startsWith('tvcmodal:')) {
        await tempVoiceControls.handleModal(interaction);
      } else if (giftPanel.isDevPanelCustomId(interaction.customId)) {
        await giftPanel.handleModalSubmit(interaction);
      } else if (interaction.customId === GIFTCODE_REGISTER_MODAL_ID || interaction.customId.startsWith(GIFTCODE_REGISTER_MODAL_ID + ':')) {
        await handleRegisterModalSubmit(interaction);
      } else if (interaction.customId === GIFTCODE_ADDPLAYER_MODAL_ID) {
        await handleAddPlayerModalSubmit(interaction);
      } else if (interaction.customId === GIFTCODE_USE_CODE_MODAL_ID) {
        await handleUseCodeModalSubmit(interaction);
      } else if (interaction.customId === GIFTCODE_NICKNAME_MODAL_ID) {
        await handleNicknameModalSubmit(interaction);
      }
      return;
    }

    if (interaction.isButton() || interaction.isStringSelectMenu() || interaction.isUserSelectMenu() || interaction.isRoleSelectMenu() || interaction.isChannelSelectMenu()) {
      const id = interaction.customId;

      if (id.startsWith('atkset:')) {
        await require('./adminToolkit/settingsMenu').handleInteraction(interaction);
        return;
      }

      if (id.startsWith('perm:')) {
        await handlePermissionInteraction(interaction);
        return;
      }

      // ── Owner Panel ─────────────────────────────────────────────────
      if (id.startsWith('owner:')) {
        const ownerPanel = require('./ownerPanel/menu');
        if (await ownerPanel.handleInteraction(interaction)) return;
      }

      // ── Player ID / Gift Code (ported subsystem) ──────────────────────
      if (id.startsWith(GIFTCODE_APPROVE_PREFIX) || id.startsWith(GIFTCODE_DENY_PREFIX)) {
        await handleClaimButton(interaction);
        return;
      }
      if (id.startsWith(GIFTCODE_VIEW_FAILED_PREFIX)) {
        await handleViewFailedPlayersButton(interaction);
        return;
      }
      if (id.startsWith(GIFTCODE_ADDPLAYER_RETRY_PREFIX)) {
        await handleRetryFailedPlayersButton(interaction);
        return;
      }
      if (id.startsWith(GIFTCODE_RETRY_PREFIX)) {
        await handleRetryFailedButton(interaction);
        return;
      }
      if (giftPanel.isDevPanelCustomId(id)) {
        // isButton() distinguishes it from the select-menu variants also
        // covered by this outer if — both dispatch tables live in router.js.
        const handled = interaction.isButton()
          ? await giftPanel.handleButton(interaction)
          : await giftPanel.handleSelectMenu(interaction);
        if (handled) return;
      }
      if (interaction.isStringSelectMenu() && id === GIFTCODE_REMOVE_ID_SELECT) {
        await handleRemoveIdSelect(interaction);
        return;
      }
      if (id === idTranslatePanel.TRANSLATE_BUTTON_ID || id.startsWith('wscotr:')) {
        const handled = await idTranslatePanel.handleMenuInteraction(interaction);
        if (handled) return;
      }
      if (id.startsWith(DM_REGISTER_PREFIX)) {
        // Same handler as panel Register — reads guild id from customId
        await giftPublicButtons['wsco_register'](interaction);
        return;
      }
      if (giftPublicButtons[id]) {
        await giftPublicButtons[id](interaction);
        return;
      }

      if (id.startsWith('notif_q|')) {
        await handleQuarantineInteraction(interaction);
        return;
      }

      if (id.startsWith('wiz:')) {
        await wizard.handleWizardInteraction(interaction);
        return;
      }

      if (id.startsWith('cnotif:')) {
        await customNotif.handleCustomNotifInteraction(interaction);
        return;
      }

      if (id.startsWith('nedit:')) {
        await editor.handleEditorInteraction(interaction);
        return;
      }

      if (id.startsWith('tmpl:')) {
        await templates.handleTemplateInteraction(interaction);
        return;
      }

      if (id.startsWith('notifhub:')) {
        await notifHub.handleHubInteraction(interaction);
        return;
      }

      if (id.startsWith('notifset:')) {
        await notifSettings.handleSettingsInteraction(interaction);
        return;
      }

      if (id.startsWith('msghub:')) {
        await messagesHub.handleHubInteraction(interaction);
        return;
      }

      if (id.startsWith('msgc:')) {
        await messagesCreate.handleCreateInteraction(interaction);
        return;
      }

      if (id.startsWith('msgm:')) {
        await messagesManage.handleManageInteraction(interaction);
        return;
      }

      if (id.startsWith('msgg:')) {
        await messagesGroups.handleGroupingInteraction(interaction);
        return;
      }

      if (id.startsWith('msgl:')) {
        await messagesLiveEdit.handleLiveEditInteraction(interaction);
        return;
      }

      if (id === 'music:show' || id === 'music:setchannel' || id === 'music:channelselect') {
        await musicHub.handleHubInteraction(interaction);
        return;
      }

      if (id.startsWith('music:')) {
        await musicHandlers.handleButton(interaction);
        return;
      }

      if (id.startsWith('translate:')) {
        await translate.handleTranslateButton(interaction);
        return;
      }

      if (id.startsWith('translate_lang:')) {
        await translate.handleLanguageSelect(interaction);
        return;
      }

      if (id === 'bday:panelset') {
        // Public "Add birthday" button — anyone can click it, not just admins.
        await birthdayPanel.handlePanelInteraction(interaction);
        return;
      }

      if (id === 'bday:translate' || id.startsWith('bdaytr:')) {
        // Public "Translate" button and everything on the private menu it
        // opens — anyone can use these, not just admins (it's their own data).
        await birthdayTranslatePanel.handleMenuInteraction(interaction);
        return;
      }

      if (id.startsWith('bday:')) {
        await birthdayHub.handleHubInteraction(interaction);
        return;
      }

      if (id === 'role:assign') {
        // Public "Assign Role" button — anyone can click it, but the
        // handler itself gates what happens next (bot admin, or staff
        // explicitly granted that specific role).
        await rolesPanel.handlePanelInteraction(interaction);
        return;
      }

      if (id === 'role:translate' || id.startsWith('roletr:')) {
        await rolesTranslatePanel.handleMenuInteraction(interaction);
        return;
      }

      if (id.startsWith('roleassign:')) {
        await rolesPanel.handleAssignInteraction(interaction);
        return;
      }

      if (id.startsWith('role:')) {
        await rolesHub.handleHubInteraction(interaction);
        return;
      }

      if (id.startsWith('welc:')) {
        await welcomeHub.handleHubInteraction(interaction);
        return;
      }

      if (id.startsWith('calctr:')) {
        // Shared interactive Translate menu for all three calculators —
        // public panel button plus everything on the private menu it opens.
        await calcTranslateMenu.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('buildingcalc:')) {
        // Public panel's "Calculate" button and its picker/result screens —
        // anyone can use these, not just admins.
        await buildingCalculator.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('chiefgearcalc:')) {
        await chiefGearCalculator.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('charmcalc:')) {
        await charmCalculator.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('waracademycalc:')) {
        await warAcademyCalculator.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('petcalc:')) {
        await petCalculator.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('svscalc:')) {
        await svsCalculator.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('minv2:')) {
        await ministryPanelV2.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('ministryhub:')) {
        await ministryHub.handleHubInteraction(interaction);
        return;
      }
      if (id.startsWith('indexhub:')) {
        await indexHub.handleHubInteraction(interaction);
        return;
      }

      if (id.startsWith('calc:')) {
        // Admin-only hub for configuring each calculator's panel channel.
        await calculatorHub.handleHubInteraction(interaction);
        return;
      }

      if (id.startsWith('wos:')) {
        await wosMenu.handleHubInteraction(interaction);
        return;
      }

      if (id.startsWith('branding:')) {
        await brandingMenu.handleHubInteraction(interaction);
        return;
      }

      // ── Tickets / Giveaways / Polls ───────────────────────────────────
      // Each router handles its own public buttons (open ticket, enter,
      // vote…) before applying its admin check, so these are open to all.
      if (id.startsWith('ticket:')) {
        await ticketMenu.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('gw:')) {
        await giveawayMenu.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('poll:')) {
        await pollMenu.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('ballot:')) {
        await ballotMenu.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('levels:') || id.startsWith('lvl:')) {
        await levelMenu.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('logs:')) {
        await logMenu.handleInteraction(interaction);
        return;
      }

      if (id.startsWith('autorole:')) {
        await autoRoleMenu.handleInteraction(interaction);
        return;
      }

      // Owner controls inside a temp voice channel (owner check is inside).
      if (id.startsWith('tvc:')) {
        await tempVoiceControls.handleButton(interaction);
        return;
      }

      if (id.startsWith('tempvoice:')) {
        await tempVoiceMenu.handleInteraction(interaction);
        return;
      }

      if (id === 'settings:back') {
        if (await adminToolkit.handleBack(interaction)) return; // pressed inside a Toolkit post
        await showMainMenu(interaction);
        return;
      }

      if (id.startsWith('settings:permissions:')) {
        // Only Bot Owner or Server Bot Admin may open the Permissions menu.
        // Sub-Admins are intentionally blocked (prevents privilege escalation).
        const { requirePermissionsAccess } = require('./commands/settingsMenu');
        if (!requirePermissionsAccess(interaction)) {
          await interaction.reply({
            content: '🔒 Only the **Bot Owner** or **Server Bot Admin** can manage permissions.',
            ephemeral: true,
          }).catch(() => {});
          return;
        }
        const page = Number(id.split(':')[2] || 0);
        await renderAdminList(interaction, page);
        return;
      }

      // ── Phase 2: subscription feature gates ───────────────────────────
      {
        const { requireSettingsFeature } = require('./subscriptions/guards');
        const gated = [
          'settings:notifications',
          'settings:messages',
          'settings:music',
          'settings:birthday',
          'settings:welcome',
          'settings:calculator',
          'settings:ministry',
          'settings:index',
          'settings:roles',
          'settings:wos',
          'settings:branding',
          'settings:tickets',
          'settings:giveaways',
          'settings:polls',
          'settings:logs',
          'settings:levels',
          'settings:tempvoice',
          'settings:playerid',
          'settings:giftcode',
          'settings:lookup',
        ];
        if (gated.includes(id)) {
          if (!(await requireSettingsFeature(interaction, id))) return;
        }
      }

      if (id === 'settings:discordtools') {
        const { showDiscordTools } = require('./commands/settingsMenu');
        await showDiscordTools(interaction);
        return;
      }

      if (id === 'settings:notifications') {
        await notifHub.showHub(interaction);
        return;
      }

      if (id === 'settings:messages') {
        await messagesHub.showHub(interaction);
        return;
      }

      if (id === 'settings:music') {
        await musicHub.showHub(interaction);
        return;
      }

      if (id === 'settings:birthday') {
        await birthdayHub.showHub(interaction);
        return;
      }

      if (id === 'settings:welcome') {
        await welcomeHub.showHub(interaction);
        return;
      }

      if (id === 'settings:calculator') {
        await calculatorHub.showHub(interaction);
        return;
      }

      if (id === 'settings:ministry') {
        await ministryHub.showHub(interaction);
        return;
      }
      if (id === 'settings:index') {
        await indexHub.showHub(interaction);
        return;
      }

      if (id === 'settings:roles') {
        await rolesHub.showHub(interaction);
        return;
      }

      if (id === 'settings:wos') {
        await wosMenu.showHub(interaction);
        return;
      }

      if (id === 'settings:branding') {
        await brandingMenu.showHub(interaction);
        return;
      }

      if (id === 'settings:tickets') {
        await ticketMenu.showHub(interaction);
        return;
      }

      if (id === 'settings:giveaways') {
        await giveawayMenu.showHub(interaction);
        return;
      }

      if (id === 'settings:polls') {
        await pollMenu.showHub(interaction);
        return;
      }

      if (id === 'settings:logs') {
        await logMenu.showHub(interaction);
        return;
      }

      if (id === 'settings:levels') {
        await levelMenu.showHub(interaction);
        return;
      }

      if (id === 'settings:tempvoice') {
        await tempVoiceMenu.showHub(interaction);
        return;
      }

      if (id === 'settings:playerid') {
        await playerMenu.showPlayerMenu(interaction);
        return;
      }

      if (id === 'settings:lookup') {
        if (!lookupFeature) {
          await interaction.reply({ content: 'Lookup module failed to load. Check bot logs.', ephemeral: true }).catch(() => {});
          return;
        }
        try {
          await lookupFeature.menu.showHub(interaction);
        } catch (e) {
          console.error('[lookup] showHub error:', e);
          await interaction.reply({ content: `Lookup error: ${e.message || e}`, ephemeral: true }).catch(() => {});
        }
        return;
      }

      if (id === 'settings:giftcode') {
        await giftCodesMenu.showGiftCodesMenu(interaction);
        return;
      }

      // Subscription product removed — ignore legacy buttons if any remain
      if (id === 'settings:subscribe' || id === 'settings:plans' || id === 'settings:submanage') {
        const payload = {
          content: 'Subscriptions have been removed. All features are available on every server.',
          ephemeral: true,
        };
        if (interaction.replied || interaction.deferred) await interaction.followUp(payload).catch(() => {});
        else if (interaction.isMessageComponent?.()) await interaction.update({ content: payload.content, embeds: [], components: [] }).catch(async () => {
          await interaction.reply(payload).catch(() => {});
        });
        else await interaction.reply(payload).catch(() => {});
        return;
      }

      // Phase 4 — Owner Panel (Bot Owner only, official bot server only)
      if (id === 'settings:owner') {
        const ownerPanel = require('./ownerPanel/menu');
        const permissions = require('./permissions/permissionManager');
        const official = require('./subscriptions/officialServer');
        if (!permissions.isBotOwner(interaction.user.id)) {
          await interaction.reply({ content: '🔒 **Bot Owner only.**', ephemeral: true }).catch(() => {});
          return;
        }
        if (!official.isOfficialGuild(interaction.guildId)) {
          await interaction.reply({
            content: '👑 Owner Panel is only available on the **official bot server**, or via `/settings` in a **DM** with the bot.',
            ephemeral: true,
          }).catch(() => {});
          return;
        }
        await ownerPanel.showHub(interaction);
        return;
      }
    }
  } catch (err) {
    console.error(`Error handling interaction [${interaction?.customId || interaction?.commandName || interaction?.type || '?'}]:`, err && err.stack ? err.stack : err);
    const payload = { content: 'Something went wrong handling that.', ephemeral: true };
    try {
      if (interaction.deferred || interaction.replied) await interaction.followUp(payload);
      else await interaction.reply(payload);
    } catch {
      /* interaction may already be unusable — ignore */
    }
  }
}));

}

module.exports = { registerInteractionHandler };
