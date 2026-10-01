# 2026-10-01 Shared item names and missing explosion shapes

Max's test build of main d6152ab58 showed two errors with the bundled
originals enabled.

## "Ambiguous weapon item display name: Sniper Rifle"

Kaje's Sniper Rifle (`weapon_sniper_rifle:weapon/sniperrifleitem`) and the
Adventure Pack's (`weapon_adventurepack:weapon/sniperrifleitem`, from
`Weapon_sniperRifle.cs`) both have `uiName = "Sniper Rifle"`. The weapons
loader (`WeaponContent::load_with`) and the wrench item list
(`ToolUi::install_items`) each refused a display name used twice, so startup
left the Sniper Rifle Add-On out. Modern Warbattles' hunting rifle and Tier 2's
Military Sniper say "Sniper Rifle" too, so the same thing would happen again.

v20 lists every item whatever its name. Now items are keyed only by their
namespaced id and names may repeat:

- `bri_world::item_aliases` is the one rule that turns display names into the
  aliases old saves bind `+-ITEM <name>` lines by. A shared name binds to the
  first item in load order: the base game, then Add-Ons in `packages.json`
  order.
- `WeaponContent::item_choices` lists items sharing a name in load order (not
  id order), and both the weapons loader and the wrench list build their
  aliases from that order, so they always agree.

## "Unconverted explosion shape Add-Ons/Weapon_Rocket Launcher/explosionSphere1.dts"

The HE Grenade original sets `hegrenadeExplosion.explosionShape` to
`Add-Ons/Weapon_Rocket Launcher/explosionSphere1.dts`, with a space. The stock
folder is `Weapon_Rocket_Launcher`, so no package provides that file. Frogs
Weaponry (Tier) uses the same path in several explosions. `ExplosionShapes::load`
returned an error for a missing shape whose explosion has no Add-On prefix
(explosions are keyed by bare Torque name), which failed the whole content
rebuild each time Add-Ons changed ("Add-On change not applied", the "Request
Rejected" box).

What v20 does with that path: Torque's resource manager finds no file at a
folder that does not exist, so no shape could be drawn. These Add-Ons are
widely played with working grenades, so the explosion itself clearly still
went off. This is inferred from the path and the Add-Ons' history; the real
game was not run (testing boundary).

Now a shape that no package converted (path not provided, or its DTS
conversion failed) means no shape: the explosion keeps its particles, lights,
sounds and damage, and the console notes it once at load. A shape that is
provided but fails to read is still handled as before (a cosmetic stand-in for
an Add-On). The Add-On import report's existing "file ... is not in this
Add-On" finding for an `explosionShape` now says what happens at runtime.

No guess maps the space to an underscore: v20 does not, and the fix would
then differ from the original.

Correction (2026-10-01, later): the v20 exe shows the original drew the
rocket's sphere for a shape it could not load, so such an explosion now does
too; see [2026-10-01-he-grenade-look-and-sound.md](2026-10-01-he-grenade-look-and-sound.md).

## Guard tests (fail on d6152ab58)

- `bri_net::content_identity::tests::items_sharing_a_display_name_all_load_and_saves_bind_the_first_loaded`
- `bri_client::tool_ui::tests::items_sharing_a_display_name_are_all_choices_and_bind_the_first_given`
- `bri_client::explosion_shapes::tests::an_explosion_whose_shape_no_package_converted_loads_without_one`

## Next

The release lane re-checks on Max's PC that the test build loads with every
bundled original enabled and no "Add-Ons Left Out" or "Request Rejected" box.
