# 2026-10-01 Bundled originals ship their host rules; texture-less materials draw plain

Max, on test build v0.1.11-test-d6152ab58: the Hookshot "doesn't pull me to
the point where i shoot it", its model is "a white cube", and the Grapple
Rope makes its sound but "nothing comes out and no grapple behavior".

## Causes (from the test build's own files)

- `content/addons/` held no `*-rules` folder (testbuild-addons-list.txt).
  Import Add-On writes a port's host rules beside the import, as
  `<out>-rules`, and the import names them in `companions`.
  `tools/addon_bundle.py build` moved only the import into `addons/<id>`, so
  every ported original shipped without the rules that do what its scripts
  did: Grapple Rope, Hookshot, Fill Can, Trench Digging, Slayer and the rest.
  The Add-Ons screen skips a companion that is not installed, so nothing
  complained. Players' own Import was fine; only the release bundle lost them.
- The Hookshot's import report: `hookshot.dts material black50 has no
  texture`, then the model "presented as a placeholder cube". One material
  without a texture swapped the whole model for the cube.

## Fixes

- `addon_bundle.py`: fixed by the Bundled originals lane (771a5432, see
  `2026-10-01-bundle-ships-port-rules.md`); this branch builds on it.
- Importer: a material with no texture in the Add-On or the base game binds a
  plain white texture (`placeholder:white`) and the model is kept, tinted by
  the colour shift. Inferred, not measured: Torque drew a material whose
  bitmap it could not load untextured.

## Tests (each fails on the code before the fix: the bundled tests on
d2ae5267's `addon_bundle.py`, the import test on the old importer)

- `grapples.rs` `a_bundled_hookshot_pulls_its_shooter_to_the_wall` and
  `a_bundled_grapple_rope_ropes_its_holder`: build the bundle with
  `addon_bundle.py`, `install` it into a content root, turn the Add-On on
  through the Library (as the Add-Ons screen does), host it, fire it and
  check the player moves (pulled to the wall, roped to the ceiling). With
  `BRI_ADDON_SEARCH` set they run on Maxwell's real copies:
  `BRI_ADDON_SEARCH="S:/SteamLibrary/steamapps/common/Blockland/Add-Ons" cargo test -p bri-addon-import --test grapples bundled`.
  The other grapple tests now also turn their import on through the Library.
- The bundle tool's own test is the Bundled originals lane's
  `an_originals_host_rules_ship_beside_it_and_load`.
- `import.rs` `a_material_with_no_texture_draws_plain_and_keeps_its_model`
  (a tiny DTS written by the test).

Seen in the same test build, for other lanes: two Add-Ons name an item
"Sniper Rifle" so the loader leaves `weapon_sniper_rifle` out, and the HE
Grenade names `Weapon_Rocket Launcher/explosionSphere1.dts` (with a space).
