# Lookup JS → Rust parity gap matrix

Source of truth: `OmniCore-main (1).zip` / `reference/js-lookup/src/lookup/**` (JS bot).
Rust target: `src/lookup/**`, `src/ui/v2.rs`, `src/cards/**`, `src/commands/interactions.rs`, `src/commands/lookup.rs`, `src/ui/panels.rs`.

Status key: ✅ done · 🟡 partial · ❌ missing / mismatched

## 1. Public panels & modals

| Area | JS | Rust | Status | Notes |
|---|---|---|---|---|
| Public panels | 3 separate embeds (Player/Alliance/State), color `0x3BA3E0`, one Primary button each | `deploy:lookup` now posts 3 JS-style panels | ✅ | `ui/panels.rs` + deploy in `interactions.rs` |
| Panel button IDs | `lookup:player:search`, `lookup:alliance:search`, `lookup:state:search` | Accepted + emitted | ✅ | |
| Player modal | `lookup:player:modal`, field `player_id`, title "Search Player" | Accepted + emitted; also accepts old `query` | ✅ | |
| Alliance modal | `lookup:alliance:modal`, fields `state_id`, `query` (optional) | Accepted + emitted | ✅ | |
| State modal | `lookup:state:modal`, field `state_id`, title "Search State" | Accepted + emitted | ✅ | |
| Admin hub | `lookup:show/setchannel/channelselect/postpanels` via lookup/menu.js, `settings:lookup` opens hub | Rust has different settings flow (`settings:lookup`, deploy buttons) | ❌ | Need JS-style hub/menu parity or compatible routing |
| Panel refresh | JS reposts panels via menu "Post / Refresh Panels" | Rust deploy overwrites/posts; no identical menu flow | 🟡 | |

## 2. Component IDs / interaction routing

| JS ID | Purpose | Rust | Status |
|---|---|---|---|
| `lookup:player:tab` (select) | Player tab switch | Handled in sessions/action | 🟡 | Verify options/default/label parity |
| `lookup:player:go:state` | Open state from player | Handled | 🟡 | |
| `lookup:player:go:map` | Player map subview | Handled | 🟡 | |
| `lookup:player:go:alliance` | Open alliance from player | Intercepted to open alliance modal in Rust | ❌ | JS opens alliance private view directly |
| `lookup:player:go:trail` | Trails subview | Handled as tab | 🟡 | |
| `lookup:player:board:state` | Jump to standings/state board | Present in sessions.rs | 🟡 | Verify behavior |
| `lookup:player:pg:p:<n>` | Standings pager (numeric) | sessions uses prev/next; v2 pager uses tag:target format | ❌ | JS pager ids: `prefix:p:target:tag` |
| `lookup:player:tab:overview` | Back from map/trail | Present | 🟡 | |
| `lookup:player:map:refresh` | Refresh player map | Present | 🟡 | |
| `lookup:alliance:tab:<key>` | Alliance tab buttons | Handled | 🟡 | |
| `lookup:alliance:sort` | Member sort select | Handled (`board/sort`) | 🟡 | |
| `lookup:alliance:page:p:<n>` | Members/furnace pager | Rust uses `pg:p:n` in sessions? verify | ❌ | JS format differs |
| `lookup:alliance:member` / `player` | Open member player | `lookup:alliance:player` in JS handler; Rust? | ❌ | Verify |
| `lookup:alliance:map:refresh/back` | Alliance map | Present in sessions | 🟡 | |
| `lookup:alliance:troop:refresh` | Troop refresh | Rust has `tl:refresh` | ❌ | ID mismatch |
| `lookup:alliance:tl:pg:p:<n>` | Troop level pager | Partial | 🟡 | |
| `lookup:alliance:tl:back` | Back from troop level | Present | 🟡 | |
| `lookup:state:tab` (select) | State tab switch | Present | 🟡 | |
| `lookup:state:pg:p:<n>` | Rankings pager | Present | 🟡 | |
| `lookup:state:board` | Board select | Present | 🟡 | |
| `lookup:state:tl:*` | Troop level view | Partial | 🟡 | |
| `lookup:state:map:refresh/back` | State map | Present | 🟡 | |
| `lookup:state:go:*` | Subviews (map/appointments/history/battle/prep/president) | Partial | 🟡 | Verify all |
| `lookup:state:player` | Open player from state | Handled | 🟡 | |
| `lookup:state:alliance` | Open alliance from state | Handled | 🟡 | |
| Candidate pick | JS: `lookup:player:candidate`? (check) | Rust: `*:candidate` | 🟡 | Verify JS IDs |

## 3. Components V2 layout

| Area | JS | Rust | Status |
|---|---|---|---|
| V2 builder | `ui/v2.js` discord.js builders, `IS_COMPONENTS_V2` | `ui/v2.rs` raw JSON, flag `1<<15` | 🟡 | Works, but behavior must match |
| Container accent | `0x3BA3E0` | `0x3BA3E0` | ✅ | |
| Footer | `-# OmniCore · <label> <t:...:R>` | `v2.rs footer()` | 🟡 | Verify exact format |
| Tab dropdown (player/state) | `ui.select` with placeholder=current tab label, options default selected | `V2::select` exists; panels need to use it correctly | 🟡 | |
| Tab buttons (alliance) | `ui.tabButtons` active=Primary | `V2::tab_buttons` exists | 🟡 | |
| Pager IDs | `prefix:p:<target>:<tag>` (`first/prev/next/last/noop`) | Rust `v2::pager` uses `prefix:p:tag:target` and `prefix:noop` | ❌ | **ID format mismatch** |
| Image names | `player.png`, `standings.png`, `chief-gear.png`, `progression.png`, `crowns.png`, `player-map.png`, `alliance.png`, `state.png`, etc. | Mixed across sessions/panels | ❌ | Audit per tab |
| Edit payload | clears attachments, sets `IS_COMPONENTS_V2` | clears content/embeds, sets flags | ✅ | Recent fix |
| Ephemeral private results | JS uses `flags` with Ephemeral+V2 for replies; edits V2 | Rust V2Edit has ephemeral | 🟡 | Verify all flows |

## 4. Data/client parity

| Area | JS | Rust | Status |
|---|---|---|---|
| Player fetch | `getPlayer` hybrid: WS site search/direct + refresh/poll for troop data + official key + Oracle chief gear/charms fallback | `lookup/client.rs` simpler; `lookup/data.rs` parallel WS+Oracle merge | ❌ | Missing troop refresh polling, field merge rules, chief gear preference |
| Search players | `siteFetch(/search?q=)` | `search_players` exists | 🟡 | |
| Alliance lookup | `/alliances/lookup?slug=&kid=` + search `/search/alliances` | present | 🟡 | |
| State fetch | `/kingdoms/{kid}?alliances=100&players=100` | present | 🟡 | |
| Ingame ranks/boards | `/kingdoms/{kid}/ingame-ranks`, cached 60s | `state_boards` exists | 🟡 | |
| State map | `/map?kid=&layers=cities&limit=50000`, 3min cache | present, 60s cache | 🟡 | Cache TTL differs |
| Map refresh | status/request/poll with token, cooldown/IP cooldown handling, in-flight dedupe | partial in interactions | 🟡 | JS `mapRefresh.js` behavior more complete |
| Player history | `/players/{uid}/history`, 5min cache | `player_history` exists | 🟡 | |
| Appointments | `/kingdoms/{kid}/appointments` | exists | 🟡 | |
| Field normalization | `lookupData.js` rich view models (boards, troop lines, roles, crowns, history series, etc.) | `types.rs` + sessions helpers; less complete | ❌ | Major parity gap |
| Troop levels | `normalizeTroopLevels/Entries` with expedition_troops + nested fields | `troop_levels.rs` exists | 🟡 | Verify same inputs/outputs |

## 5. Cards/images

| JS renderer | Rust | Status |
|---|---|---|
| `playerOverview` | `render_player_overview_card` | 🟡 | Verify visual/field parity |
| `playerStandings` | `render_player_standings_card`? check | ❌ | Verify exists/wired |
| `playerProgression` | progression renderer exists? | 🟡 | |
| `playerCrowns` | crowns renderer exists? | 🟡 | |
| Alliance overview | `render_alliance_overview_card` | 🟡 | |
| State overview | `render_state_overview_card` | 🟡 | |
| State vs State | `render_state_vs_state_card`? | 🟡 | |
| State matchup history | `render_state_matchup_history_card`? | 🟡 | |
| Maps (player/alliance/state) | `render_map_card` | 🟡 | |
| Prep points / appointments | `render_state_prep_points_card` / appointments? | 🟡 | |
| Arena lineup | `render_arena_lineup` | 🟡 | |
| Chief gear / loadout | loadout/chief gear cards exist | 🟡 | |
| Attachment names | JS-specific names per tab | mixed | ❌ | Must match JS for parity where visible |

## 6. High-priority next fixes

1. **Unify component ID format**: make Rust emit/accept JS IDs everywhere (pager `p:target:tag`, alliance member select, troop refresh id, candidate IDs, tab select IDs).
2. **Player/Alliance/State V2 panel parity**: align `src/lookup/panels/*.rs` structure to JS `render/draw` (titles, body, image placement, buttons, selects, footers, empty states).
3. **Session routing parity**: route all lookup component/modal interactions through one canonical path; remove legacy embed render for V2 flows.
4. **Data normalization parity**: port `lookupData.js` view-model logic (boards, roles, troop bits, history/progression, alliance roster sorting/paging, state appointments/history/prep) into Rust.
5. **Cards parity audit**: verify every JS card has Rust equivalent with same fields/labels; wire missing ones.
6. **End-to-end tests**: add tests for ID parsing, session tab routing, and view-model mapping.

## Commands

```powershell
cargo check --locked --no-default-features
.\scripts\dev-check.ps1
```
