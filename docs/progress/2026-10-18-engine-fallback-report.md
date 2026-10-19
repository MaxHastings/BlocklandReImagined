# 2026-10-18 Counted fallback report

Branch `codex/engine-fallback-report`. Two read-only audits listed about 120
places where loaders carry on silently after bad or missing input; the first
pass routes the ones a player would notice through one generic path,
`bri_console::fallback` (see [../architecture/fallbacks.md](../architecture/fallbacks.md)).

- `bri-client --check` now loads every listed map (visual and collision) and
  writes `logs/fallbacks.json` (`kind`, `addon`, `item`, `fallback`, `count`).
- Every Add-On health problem is also a counted fallback.
- Unknown JSON keys: `bri_content::strict` (serde_ignored) reports them for the map scene, interiors, shapes, terrain and the weapons pack, under `fallback::with_owner`.
- Sounds: every audio error that cannot play is a `sound` fallback.
- Not done: unknown keys in effects, bricks, avatar and manifests, per-frame HUD binding failures, importer drops,
  grips/muzzle warnings (listed in the architecture page).
- Checks: console, content and render unit tests; `cargo check -p bri-client
  --all-targets`; clippy/tests/fmt come with `land.py --engine-branch`.
