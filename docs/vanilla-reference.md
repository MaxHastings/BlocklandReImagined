# Vanilla content reference

Maxwell designated `E:\Downloads\B4v21Launcher\versions\Blockland v20` on
2026-09-26. This installation stays read-only. The previous C: installation is
secondary evidence, not the definition of which community packages to carry over.

## Checked comparison

`tools/compare_v20_references.py` compared SHA-256 hashes of loose files and
decompressed ZIP members in `base/`, `Add-Ons/` and `saves/`. It did not execute
scripts or inspect settings, cache databases, logs, screenshots or launcher modules.

- 2,843 reference assets: all byte-identical to corresponding earlier assets.
- No changed, newly introduced, duplicate or unreadable reference assets.
- 79 add-on archives, including 14 map archives containing 14 playable mission
  declarations. A fifteenth `.mis` is the editor's new-mission template.
- All 54 packages named by the recovered stock default-enabled list are present.
  The previous recovered core/default-script evidence remains applicable because
  all corresponding base files match. Literal registration inventory remains
  9 core inputs, 65 core outputs and 4 add-on registration candidates; these are
  not counts of implemented event behaviors.
- The older inventory has 452 additional indexed paths and two problematic
  community archives. Those archive errors mean the old-only count is not a
  complete inventory of every old archive's contents.
- Six map archives differ only by extra `.ml` mission-lighting caches in the old
  installation: Bedroom, Bedroom Dark, Kitchen, Kitchen Dark, Slopes and Tutorial.
  Their shared mission/geometry/image members match exactly.

The reproducible detailed comparison is local at
`artifacts/reference-audit/source-comparison.json`. Compact source hashes, package
counts, map identities and conversion failures are in
`vanilla-reference-inventory.json`. The independent census and default/event scan
are alongside the detailed comparison.

Presence establishes the user-designated content baseline, not historical proof
that every package shipped in every official distribution. In particular,
Return to Blockland and obsolete launcher/service features do not become alpha
gameplay requirements merely because their files are present.

## Required map coverage

All these maps belong in the alpha coverage checklist. None is accepted solely
because its mission declaration or geometry converts.

| Map | Archive | Native mission conversion in maps-pass-006 |
| --- | --- | --- |
| Bedroom | Map_Bedroom | Converted |
| Bedroom - Dark | Map_BedroomDark | Converted |
| Construct | Map_Construct | Converted |
| Destruct | Map_Destruct | Converted |
| Halloween Slate | Map_Halloween_Slate | Converted |
| Kitchen | Map_Kitchen | Converted |
| Kitchen - Dark | Map_KitchenDark | Converted |
| Skylands | Map_Skylands | Converted |
| Slate | Map_Slate | Converted |
| Slate Desert | Map_Slate_Desert | Converted |
| Slate Sea Revised | Map_Slate_Sea_Revised | Converted |
| Slate Storm Revised | Map_Slate_Storm_Revised | Converted; script behavior remains required |
| The Slopes | Map_Slopes | Converted |
| Tutorial | Map_Tutorial | Converted; script behavior remains required |

Tutorial is present in both references. Earlier UI-audit prose saying it was
missing is incorrect; its own asset manifest already locates the mission. Keep
the original tutorial flow in scope rather than hiding or repointing its button.

## Conversion and lighting implications

A fresh conversion directly from the new reference produced `content/maps-pass-006`:
385 successful asset conversions, two failures, zero scan/archive errors. All map
object exports convert; Tutorial/Slate Storm setup scripts remain explicit pending
native behavior, with line ranges/hashes and retained original provenance.
`1x1x5spike.blb` is not accepted as a standalone brick; the editor marker
`octahedron.dts` uses DTS v18. Resolve required dependencies deliberately.

Existing generated packs remain useful because shared source bytes match.
`map-bundle-009` uses primary assets plus secondary lighting caches for six maps.
`--lighting-cache-root` must be supplied explicitly. The converter verifies primary
source hashes against conversion provenance, secondary mission/geometry byte
identity, mission CRC and unique complete interior slot/dimension association.
Those caches are derived data, not evidence of extra stock content. Their resource
CRC fields are sentinels, so independent verification of the cached lighting result
and a bake directly from cache-free originals remain work. No white terrain fallback.

Slate has an authored collision interior with no visible surfaces. Its source
sky renders below the horizon; inventing a visible floor would change the source.
Its remaining dynamic environment behavior is still part of fidelity acceptance.

Source selection and runtime acceptance remain separate: current integrated map
selection/loading and offscreen architecture rendering cover the entire table.
Tutorial instruction/trigger logic, weather/water/decorations and remaining map
behaviors still need implementation and acceptance.
