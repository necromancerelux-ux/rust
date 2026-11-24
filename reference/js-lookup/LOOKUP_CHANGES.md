# Lookup feature — changes & locations

## Summary
Replaces the old forum-based WOS lookup with **persistent birthday-style panels** and private (ephemeral) multi-tab results.

**Data sources (never shown to users):**
1. **Primary:** public `https://www.whiteoutstats.com/api` — **no API key required**
2. **Optional:** official `https://api.whiteoutstats.com/v1` if owner pastes a key
3. **Fallback:** existing WoS Oracle client (silent)

---

## New files

| Path | Purpose |
|------|---------|
| `src/lookup/index.js` | Feature entry — `handleInteraction` |
| `src/lookup/menu.js` | Admin hub (channel, post panels, optional API key) |
| `src/lookup/database/store.js` | Panel channel + message IDs per guild |
| `src/lookup/database/wosCredentials.js` | Optional official API key storage (owner paste) |
| `src/lookup/services/dataClient.js` | Unified client: public site → official key → Oracle |
| `src/lookup/services/arenaRender.js` | Combined arena lineup image (canvas) |
| `src/lookup/components/playerPanel.js` | Player persistent panel + private tabs |
| `src/lookup/components/alliancePanel.js` | Alliance persistent panel + private tabs |
| `src/lookup/components/statePanel.js` | State persistent panel + private tabs |

## Modified files

| Path | Change |
|------|--------|
| `src/adminToolkit/interactionRouter.js` | Require `../lookup`; early `lookup:` routing; `settings:lookup` → new hub |

---

## How to use

1. **Settings → Lookup** (`settings:lookup`)
2. Set **Panel Channel**
3. **Post / Refresh Panels** (Player + Alliance + State)
4. Optional: **Connect API Key** for higher official API limits (not required)
5. Members click Search → private modal → ephemeral multi-tab results

## Fallback behaviour
- Player / state / alliance first hit public whiteoutstats site APIs
- If that fails and an official key is stored, uses official API
- If that fails, uses Oracle (same credentials as Profile Import)
- Users never see source names or “data as of …” from those sites

## Notes
- Old forum tracking under giftcode `devPanel/lookupMenu` is no longer opened from Settings → Lookup
- You can remove or disable forum auto-refresh jobs separately if desired
- Arena combined image + chief gear card reuse existing canvas assets from Oracle CDN paths for icons
