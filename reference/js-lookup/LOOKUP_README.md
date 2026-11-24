# Lookup (new system)

## What changed
- Forum-style trackers replaced with **persistent private panels** (birthday-style).
- Data: public whiteoutstats.com API (no key) → optional official key → WoS Oracle fallback.
- Source names are never shown to users.

## Files
- `src/lookup/**` — new feature
- `src/interactionRouter.js` — routes `lookup:*` and `settings:lookup`
- `src/adminToolkit/panels/lookup.js` — Admin Toolkit entry
- `src/giftcode/components/devPanel/lookupMenu.js` — old menu redirects to new hub

## Use
1. Settings → Lookup (or Admin Toolkit → Lookup → Open Lookup Settings)
2. Panel Channel → bot posts Player / Alliance / State search panels
3. Members click Search → private results only they see

## Optional API key
Settings → Lookup → Connect API Key (not required for public site data).
