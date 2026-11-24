# Lookup V2 rewrite notes

## Done in this step

### Oracle client (`src/oracle/client.rs`)
- Auth header is now **`x-api-token`** (matches the live site).
- Optional `cf_clearance` cookie support.
- **Test Data** reports exact HTTP status + 401 / Cloudflare / poisoned / JSON ok.

### Owner panel (`src/owner.rs`)
- Label stays **API Key**.
- Help text explains the free browser-token flow (DevTools → Network → copy `x-api-token`).

### Components V2 UI kit (`src/ui/v2.rs`)
- Full mirror of the JS `lookup/ui/v2.js` helpers.
- `V2` builder → single accented `CreateContainer`.
- Helpers: text, title, sep, image, footer, buttons, tab_buttons, select, pager, payload, edit_payload, fmt, rank_chip, age_chip, paginate.

### Dual-source data layer (`src/lookup/data.rs`)
- `fetch_player(lookup, oracle, query)` races Whiteout Stats + Oracle in parallel.
- Basics from whichever answers first; Oracle fills empty fields and supplies loadout.
- Returns `PlayerBundle { profile, whiteout, oracle, loadout, primary_source, note }`.

### Player V2 panel (`src/lookup/panels/player.rs`)
- Tabs: Overview · Standings · Heroes & Gear · Progression · Crowns (same as JS).
- Tab dropdown + separator + title/body/image + action buttons + footer.
- Private (ephemeral) by default.
- Overview supports an optional PNG card attachment; text fallback when no card is rendered yet.

## How free Oracle data works
1. Open https://wosoracle.com in a browser.
2. F12 → Network → any `/api/…` request.
3. Copy the `x-api-token` header value.
4. Owner Panel → 🔮 WoS Oracle → Connect API Key → paste.

No Discord login is required on the bot side.

## Next
1. Wire `fetch_player` into the Lookup command / interaction handlers.
2. Switch `sessions::render` player path to `panels::render_player_v2`.
3. Port Alliance + State panels to V2.
4. Port image card renderers (overview / gear / arena) from the JS bot.
5. Live test in a Discord test server.

## Wired in this step
- `commands/lookup.rs::player_by_query` → dual-source `fetch_player`
- `commands/lookup.rs::player_bundle_by_query` → full bundle for sessions
- `lookup_view_edit` → Player tabs use `edit_player_v2` (Components V2)
- Player modal / candidate pick → V2 overview when `Kind::Player`
- Alliance / State still on the embed path (next port)

## Build note
Serenity 0.12.5 provides CreateContainer / CreateTextDisplay / MessageFlags::IS_COMPONENTS_V2.
If a compile error appears on `.components(vec![CreateComponent])` for EditInteractionResponse,
pin serenity to the same revision that ships full V2 builders (docs: serenity next).

## Alliance + State V2 (this step)
- `lookup/panels/alliance.rs` — tab **buttons**, same keys as JS
- `lookup/panels/state.rs` — tab **dropdown**, same keys as JS
- `lookup_view_edit` + modal/candidate open paths all use V2 for Player / Alliance / State
- Embed path in `sessions::render` remains as fallback for non-V2 edge cases

## 2026-11-24 session (player + alliance + state V2)

### Player
- Full V2 layout matching JS: tab dropdown, image cards, action buttons (S{state}, [tag] Alliance, Locate on map, Trails).
- Extra views: map, trail.
- Card pre-render for overview / standings / heroes / progression / crowns.

### Alliance
- Overview image card support via `render_alliance_overview_card`.
- Signature: `edit_alliance_v2(session, tab, note, card_png)`.

### State
- Full V2 rewrite with tab dropdown, map sub-view, rank chips, troop-level bits.
- Overview image card via `render_state_overview_card`.
- Signature: `edit_state_v2(session, tab, note, card_png)`.

### Still open
- Richer alliance members/sort/pager + troop-level enrichment.
- Map refresh live data path + board open handlers end-to-end smoke.
- Clean `cargo check` (crate mirror flakiness in this environment).

## Handler audit (2026-11-24)

- `lookup_session_action` routes `player:|alliance:|state:` component IDs.
- Tab switches (`tab` / `tab:<key>`), `go:map`, `go:trail`, `map:refresh`, pager, board, sort all update session then call `lookup_view_edit` → V2 panels.
- Cross-panel: `player:go:alliance` opens alliance search modal; state→alliance select builds Alliance session + V2 overview.
- Map refresh polls job status, sets notice, re-renders map tab via V2.
- Modals: `lookup:player_modal`, `lookup:alliance_search_modal`, `lookup:state_modal` wired.

## Alliance members / troopLevel (2026-11-24)

- Members tab: sort select (Power / Furnace / Role / Name) via `session.board`, rank chips, furnace + role + troop bits, page footer.
- Troop Level tab: lists members with troop tier/FC bits, prefers rows that have troop data.
- Handler: `lookup:alliance:sort` writes sort key into `session.board` and resets page.
