# Lookup parity workspace

Goal: port the JavaScript OmniCore `lookup` feature (Player / Alliance / State) to Rust with
100% feature, layout, and behavior parity — same Components V2 UI, labels, tabs, data flow,
cards, and component IDs.

## Branch and layout

- Working branch: `lookup/js-parity` (base: `main` at commit `bee8fb9` / later fixes).
- JS reference copy: `reference/js-lookup/` (extracted from `OmniCore-main (1).zip`).
  - `reference/js-lookup/src/lookup/**` — the feature itself
  - `reference/js-lookup/src/interactionRouter.js` — routing of `lookup:*`
  - `reference/js-lookup/src/adminToolkit/panels/lookup.js` — settings entry
  - `reference/js-lookup/LOOKUP_*.md` — original notes
- Rust source: `src/lookup/**`, `src/ui/v2.rs`, `src/cards/**`, `src/commands/lookup.rs`,
  lookup sections of `src/commands/interactions.rs`.

## Tooling (verified available)

- Rust / Cargo `1.99.1`, `rustfmt 1.10.0`, `clippy 0.1.99`.
- Installed targets: `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-musl`,
  `aarch64-unknown-linux-musl`, `aarch64-linux-android`, `wasm32-unknown-unknown`.
- GitHub CLI `2.105.0` (logged in as `necromancerelux-ux`, token has `repo` + `workflow`).
- Release workflow: `.github/workflows/release.yml` — builds ARM64 + x86_64 on every push to `main`.

## Day-to-day commands

```powershell
# Fast compile check
cargo check --locked --no-default-features

# Full local guard (format + clippy + check + tests)
.\scripts\dev-check.ps1

# Cross-target compile checks (no linker needed)
cargo check --locked --no-default-features --target x86_64-unknown-linux-musl
cargo check --locked --no-default-features --target aarch64-unknown-linux-musl

# Format
cargo fmt

# Ship: merge lookup/js-parity into main; push main to trigger auto-release
```

## Notes

- `main` is the release branch; keep WIP on `lookup/js-parity`.
- ZIP files are ignored via `.gitignore` (`*.zip`).
- Components V2 is implemented as raw JSON in `src/ui/v2.rs` because crates.io serenity 0.12.5
  has no typed Components V2 builders. Message edits must explicitly clear `content`/`embeds`.
