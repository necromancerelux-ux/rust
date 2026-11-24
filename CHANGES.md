# OmniCore (Rust) — security & reliability fixes

**Not compiled.** No Rust toolchain was available when these were written. Run
`cargo check && cargo clippy && cargo test` before deploying.

## Fixed
| Area | Change |
|---|---|
| Secrets | Real `.env` removed from the package; added `.gitignore` + `.env.example`. **You must still rotate the old tokens.** |
| Oracle | `oracle_component` and `oracle_paste_modal` are now bot-owner only (was: no check / any server admin). |
| Gift codes | Add and delete modals are now bot-owner only (codes are global across servers). |
| DB migration | Ministry table rebuild runs on one pinned connection inside a transaction; foreign keys restored afterwards. |
| Role assign | Session ID is 128-bit; every step checks the clicker is the user who opened the flow. |
| Giveaways | No entries after `ends_at`; departed members' entries are removed (GuildMemberRemoval); failed finish re-opens the giveaway so the sweep retries; errors are logged; weighted draw throttled (100 ms per member lookup). |
| /clear panel | Skips messages older than 14 days, handles a single message, reports skipped count. |
| Mutexes | All `std` `.lock().unwrap()` now recover from poisoning instead of panicking forever. |
| Live clock | **Removed entirely** (`liveclock.rs`, its 4 call sites, and the "Clock" field on the settings hub). No more per-second message edits. |
| UTC format | One canonical format `2026-11-19 08:31 UTC` (`ui/theme.rs: utc()`): footers, giveaway/ballot end times, DB `absolute()`. Absolute times no longer use Discord `<t:…:f>` (viewer-local). Relative `<t:…:R>` kept alongside. |
| Leveling | Cooldown map is pruned (no unbounded growth). |
| Gift search | LIKE wildcards escaped. |
| Lavalink | Refuses the default password `youshallnotpass` for non-local hosts. |

## Second pass
| Area | Change |
|---|---|
| Build | Font include path fixed to `../../assets/fonts/OpenSans.ttf`; `assets/fonts/OpenSans.ttf` (variable font from google/fonts) added. |
| Owner modals | `ownermodal:<kind>:<guild_id>` now routed by prefix (Player cap / Gift priority were always "unknown modal"). |
| Member leave | Two `GuildMemberRemoval` arms merged, so welcome cleanup and the leave log run. |
| WoS relay | `MESSAGE_CONTENT` intent added (also enable it in the developer portal). |
| Restart | Owner restart exits with 42 (starter.js restarts on it). |
| Messages | `messages_send_modal` sets `allowed_mentions`: only the stored mention type may ping everyone/here. |
| Ballots | `ballot_extend_modal` re-checks admin permission. |
| Music | Playback buttons require being in the bot's voice channel. |
| Startup | Command-registration failure is logged instead of killing the bot. |
| Tasks | Background jobs are wrapped so a panic is logged (not yet auto-restarted). |
| Sneakpeek | `is_article_url` no longer slices by bytes (no panic on non-ASCII). |
| Google | Default redirect URI is `http://localhost` (OOB flow is blocked by Google). |
| CI | `.github/workflows/release.yml` builds a musl binary and attaches it to a release. |

| Compile fix | `delete_message` call matched the wrong serenity signature (the crate did not compile). |
| Logging | Channel create/delete, role create/delete/update, server rename and message delete now emit log events (voice and member join/leave already did). |
| Errors | Users see a generic line for database/HTTP/Discord/IO errors; only our own messages are shown (`error::public`). Full errors are logged. |
| Google token | Connect/replace/disconnect are bot-owner only (the token is shared by every server). |
| Gift codes | Every sync announces all active, unannounced codes from the last 7 days, so a code whose first announcement reached no channel is retried. |
| Tasks | The 10 background jobs restart 5 s after a panic. |
| Secrets | `OMNICORE_SECRET_KEY` enables AES-256-GCM encryption at rest for Oracle credentials, the WoS API key and Google tokens. Old plain rows still work and are encrypted when next saved. |
| Commands | `/play`, `/volume`, `/rank`, `/leaderboard` added. |
| Build | `Cargo.lock` generated; `cargo check` (default and `--no-default-features`) passes and all tests pass. |

Not done: `/redeem` (no redemption backend exists in the Rust code), Gift/Player log categories, supervised restart of the Ministry sync loop.

## Behaviour changes to be aware of
- Server admins can no longer add/delete gift codes or touch Oracle; only the bot owner can.
- The settings hub and owner hub no longer tick; they show the time of the last render (footer is minute-precision UTC).

## Not done yet (see review)
Encrypt stored credentials; supervise/restart background tasks; response-size caps;
shared `reqwest::Client`; restore /rank /leaderboard /play /redeem; GitHub DB backup;
tests for permissions/giveaways/DB; `Cargo.lock`.

## UI + emoji (this pass)

| Area | Change |
|---|---|
| Card UI | Overview / table cards use the navy glass palette (dark navy fill, double edge ring, cyan accent numbers, purple-ready labels). Footer branding stays **OMNICORE** only — no third-party product name. |
| Cross-server emoji | New `src/emoji.rs` + DB tables `emoji_home_guild`, `material_emojis`, `emoji_file_hashes`. Uploads material PNGs from `assets/emoji-icons/` into one home guild (auto-picked or `EMOJI_GUILD_ID`). Tags are reused in every other server. |
| Calculator materials | Building calculator pulls wood/iron/fire crystal/refined FC icons via `emoji::material_tag` (custom when installed, Unicode fallback otherwise). |
| Env | `.env.example` documents optional `EMOJI_GUILD_ID`. |

### How to use the emoji system
1. Put the bot in a server where it has **Create Guild Expressions** / Manage Expressions.
2. Restart (or invite to a first server) — it auto-picks that server as home and uploads the PNGs.
3. Optional: set `EMOJI_GUILD_ID=<id>` to force a specific home server.
4. Other servers the bot joins get the same `<:mat_…:id>` tags automatically; no per-server upload.

## Material tags + card polish (follow-up)

| Area | Change |
|---|---|
| Plan calculators | Chief Gear / Chief Charm totals use `material_field_title` → custom emoji when installed |
| War Academy | Wood / Iron lines use `material_tag` |
| Player card | Purple labels + cyan numbers on stat boxes; **OMNICORE** footer |

## Full card layouts (INTEL / alliance / state / SVS)

| Card | Change |
|---|---|
| Player INTEL | Dense 4-column fact grid (power/ranks/labyrinth/shield/burn/location/troops), alliance strip, navy glass frame @ 1060px, OMNICORE footer |
| Alliance | Header + 4 stat blocks + top members with power share bars |
| State | Health grade chip + 4 stat boxes + health blurb |
| State VS | New `render_state_vs_card` — dual state cards + VS badge + compare rows |
| Data wiring | `overview_from_raw` pulls ranks, boards, troops, shield/burn; alliance roster uses role labels |

Map tiles / progression sparkline / crowns chart still follow-up (need map tile assets + time series).

## Remaining cards (progression / crowns / matchup / prep / appointments / maps)

New module `src/cards/extra.rs`:

| Renderer | Content |
|---|---|
| `render_progression_card` | Power + kill sparklines, milestones |
| `render_crowns_card` | Crowns / terms / conquests, thrones, reign history |
| `render_matchup_history_card` | Castles taken/lost tiles + SVS table |
| `render_prep_points_card` | Overall prep score box + daily home/away bars |
| `render_appointments_card` | Ministers + offenders seat grids |
| `render_map_card` | Isometric fertile/plains diamond, alliance-colored cities, focus ring |

Shared navy-glass frame + **OMNICORE** footer. Wire these from lookup handlers when the corresponding payload is available.
