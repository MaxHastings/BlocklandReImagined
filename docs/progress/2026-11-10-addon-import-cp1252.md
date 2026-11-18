# 2026-11-10 Addon importer: CP1252 fallback for legacy text (branch `bionic/legacy-addon-import-fixes`)

Follow-up to the SomewhereVille saves import (see
`content/render-pack/2026-11-10-somewhereville-saves-import.md`, moved there by
the user). The importer decoded every legacy text file with
`String::from_utf8_lossy`, so pre-UTF-8 Add-Ons' CP1252 bytes (0xB0 = ° in brick
names, smart quotes in descriptions) became U+FFFD and old saves could no
longer match the imported display names.

## The fix

New `legacy_text()` (`crates/addon-import/src/lib.rs`): strict UTF-8 first; on
failure, byte-wise CP1252 via a hand-rolled 32-entry table for 0x80–0x9F (no
new dependency). Its five undefined slots (0x81, 0x8D, 0x8F, 0x90, 0x9D) map to
U+FFFD. `script_text()` now routes through it; CRLF normalisation is unchanged.

Every site that decodes legacy Add-On source bytes into text now uses
`legacy_text()`:

- `lib.rs` — `script_text()` (the display-name path), SPDX licence sniffing,
  `required_addons` over `.cs`, and the four `tscript::without_comments` script
  scans.
- `map_audio.rs` — audio declaration scan of `.cs`.
- `reference.rs` — installed-reference script ingestion.
- `v20_map.rs` — mission (`.mis`/`.ter`) reads, foliage replicator line lookup,
  and the map-script folder/zip collection.

Left alone deliberately: `ports.rs` (new-format `.rhai` rules, UTF-8 by
contract), `gltf/read.rs` (our own generated output), and `lib.rs`'s
`required_if_present` header scan (operates on already-decoded string bytes).

Note: the BLS loader (`crates/bls/src/bls.rs`, `windows_1252()`) already had a
correct strict-UTF-8 → CP1252 fallback for saves; it differs only in mapping
the five undefined slots to C1 controls instead of U+FFFD. That difference is
harmless (save data vs display text) and was left as-is.

## Tests

Six new unit tests in `lib.rs` (`cargo test -p bri-addon-import`: 85 lib tests
pass, plus the crate's integration suites): valid UTF-8 untouched (no byte-wise
re-read), CP1252 degree sign, the punctuation block (0x93/0x94/0x97/0x95/0x99/
0x80), undefined slots → U+FFFD, Latin-1 agreement above 0x7F, and CRLF
normalisation through the fallback path.

## Re-imports (scope: user-limited)

Re-imported with the fixed `target/release/bri-import-addon` from the source
ZIPs in `.research/to port/SomewhereVille Forest 10th Anniversary/Required Add-ons/`:

- `content/render-pack/brick_extraramps` ← `Brick_ExtraRamps.zip` (the only
  corrupted render-pack import).
- `content/addons/brick_1randompack` ← `Brick_1RandomPack.zip` (sha256
  `4b6d8c1f…`, byte-identical to the copy it replaced — like-for-like).

Before → after U+FFFD occurrences: brick_1randompack 250 → 0, brick_extraramps
51 → 0. A diff against pre-import backups shows brick_1randompack is unchanged
except encoding: same 123 bricks, 55 display names fixed, zero other diffs; the
extraramps "Ambiguous" note on `brick2x2x2rampupcornerdata` predates this
re-import. Backups (session scratchpad):
`~/.lmstudio/scratchpads/yp/brick_{1randompack,extraramps}-backup-20261110.tar.gz`.

## Verification: per-save resolution

Mirrors the loader (`crates/bls/src/bls.rs`): strict UTF-8 → CP1252 on save
bytes, case-insensitive exact `display_name` match against stock catalog + all
installed `assets/bricks.json`. Instances resolved before → after (before =
both packages still corrupted):

| Save | Before | After | Newly resolved distinct names |
|---|---|---|---|
| BSL Station | 37442/38046 | 37871/38046 | 21 (all ° ramps) |
| Slender The Eight Pages | 18143/18280 | 18263/18280 | 8 — matches prior analysis |
| SomewhereVille Forest V2 | 19293/19866 | 19860/19866 | 13 — matches prior analysis |
| The Dark | 9987/9989 | 9987/9989 | 0 (expected) |

Remaining unresolved names are the documented stock-catalog gaps, no add-on
provides them: BSL Station (doors — Airlock/Blast/Elevator/Hangar/Pressure/
Sliding/Unisex Bathroom — plates `2x12`/`3x4`/…, `1x2F Overhang`, `2x2F Round
Print`, `45° Inv Ramp Corner`, …), Slender (`2x Cube Print`, `Security Door`),
SV V2 (`1x1x5 Wedge`, `1x2F Overhang`), The Dark (`Giant Fan Flat`,
`Music (5x Range)`).

`target/release/bri-client --check content` passes (exit 0) with no warnings
from the re-imported packages.

## Still corrupted, out of scope this round

Per the user's scope decision, only the render-pack imports and the two named
packages were re-imported. These installed packages still carry U+FFFD from the
old importer (82 files total at time of writing); most have no source ZIP in
`.research/`, so they need their original ZIPs before a future re-import pass:

- `content/addons/brick_invertedcorners` (named earlier; no local source found).
- ~20 weapon packages (`weapon_skins_*`, `weapon_package_*`,
  `weapon_melee_extended*`) under `content/addons/`.
- Maps `map_paradise`, `map_emeraldisles4`, `map_ambience` under
  `content/addons/`.
- `content/surda-pack/vc`, `content/surda-pack/bot_zombie_left4block` (source
  folders exist in `.research/surda-pack/`; whether they re-import cleanly is
  untested).

## State of the branch

Uncommitted at time of writing: the four source files above, plus the two
re-imported content packages (both untracked by git). No engine changes beyond
the importer crate. Next step is the user's call on committing; in-game
playtest of the four saves follows per the usual boundary.

## Final check: nothing else from the SomewhereVille folder needs re-importing

Follow-up verification answered whether any other add-on from
`.research/to port/SomewhereVille Forest 10th Anniversary/` still needed a
re-import. Two independent scans, both conclusive:

- **Source-ZIP scan** (strict UTF-8 decode of every `.cs/.txt/.mis/.ter/…`
  entry in all 25 ZIPs — the 24 `Required Add-ons/` plus `Vehicle_NewJeep.zip`):
  only `Brick_1RandomPack.zip` and `Brick_ExtraRamps.zip` contain non-UTF-8
  bytes (0xB0 = ° in `uiName`). The other 23 are strictly UTF-8, so the old
  lossy importer could not have produced U+FFFD from them. Both affected ZIPs
  were already re-imported above.
- **Installed-content scan** (byte search for EF BF BD across all of
  `content/`): zero hits in any package corresponding to those 25 ZIPs —
  all 13 render-pack packages (`brick_*`, `event_daynightcycle`,
  `light_ambient`, `vehicle_newjeep`) and the 12 pre-existing
  `content/addons/` ones are clean.

This corrects the earlier note's "the 12 new imports carry it too" — of those
12, only `brick_extraramps` actually carried legacy bytes. Remaining U+FFFD
under `content/` is confined to the out-of-scope packages listed above (weapon
package.json/reports, `brick_invertedcorners`) plus incidental EF BF BD byte
runs inside compressed media files (`.ogg/.png/.jpg`, one nested zip in
surda-pack), which are not importer corruption.

NewJeep follow-up (DTS material-name and hub-node importer fixes, re-import,
and the vehicle enhancement round): see `docs/progress/2026-11-10-newjeep-import-fix.md`.
