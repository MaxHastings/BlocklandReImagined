# 2026-10-06 Bug hunt by pattern

Max asked to go after the bugs the recent patterns predict, before players
find them. The patterns are in `/mnt/project-files/audits/bug-patterns-2026-10-06.md`
(project files): a rule stricter than v20's, two copies of one fact, two
answers to "where is it", combinations, machines we don't have, a quiet
"no". Two read-only sweeps of the code (bots excluded) gave 14 candidates;
each below was checked against the code before changing it.

## Fixed (one commit each, each test fails without its fix)

- **Lost GPU (machines we don't have):** `clippy.toml` bans
  `wgpu::util::DeviceExt::create_buffer_init`, which panics once the device
  is lost; `bri_render::BufferInit` replaced every call in 7e0dc07. Only the
  helper's own regression test and the weather fixture example still call
  it, with an allow. Checked that the ban fires by removing one allow.
- **Click reach (stricter than v20):** `Command::Activate` cast 5 units
  flat. `Player::ActivateStuff` casts 10 and activates a brick within
  `$Game::BrickActivateRange` 5 times the player's scale
  (`docs/research/ui-ux/02-controls-and-interaction-rules.md`). A bigger
  player could not click buttons v20 reaches. Test
  `tools::a_click_reaches_bricks_five_units_times_the_players_scale`.
- **Wrench Send refused (a quiet "no" / stricter rule):** wrench, events and
  printer Send compared the whole brick with the one inspected, colour
  included, so a brick its own events recolour (a flashing relay loop)
  could never be wrenched. The guard now compares only what the dialog
  read and sends. Test
  `tools::a_brick_recoloured_while_its_dialog_is_open_still_takes_the_edit`;
  the older test that another wrench edit is refused still passes.

`cargo test -p bri-sim --no-fail-fast`: all pass but
`a_bot_jets_over_to_someone_above_it` (already in gate-known-failures).
`cargo clippy -p bri-sim --tests -D warnings`,
`cargo clippy -p bri-render -p bri-weather -p bri-client-sandbox --all-targets -D warnings`:
clean.

## Found, not fixed here

- **The server has no posed eye (two answers).** `Player::eye()` is the
  standing or crouch eye; a rider's feet are the seat, so a seated rider's
  eye is a full standing height above the seat. Every server ray from a
  seat or `/sit` starts above the drawn head: shots
  (`session/weapons.rs`), clicks (`session.rs` Activate), hammer
  (`tools.rs` `native_hammer_target`), carried objects (`movables.rs`),
  dropped items, event `SpawnProjectile`, Add-On aim. Client side, the
  brick ghost ray (`building.rs` `archetypes.eye`) and name tags
  (`app/mod.rs`) use the same standing eye. The crouch eye is already
  baked from `m.dts`; a sit and seat eye baked the same way would fix all
  of them, but needs the v20 Eye node read on the PC content.
- **Hammer ignores stack ownership.** v20 lets you hammer bricks others
  built on your stack (`hammerImage::onHitObject`, the research doc above);
  `Simulation::stack_owner` exists and the duplicator uses it, but the
  hammer's kill goes through the brick owner's trust. Needs the kill path
  to accept the stack rule too.
- **Taking bricks in hand refused by an Add-On drops the reason**
  (`session.rs` `Command::BrickHand`, `package_policy(...).is_err()`); the
  tool equip path shows it.
- **Riding refused silently, and an offline owner's vehicle can't be ridden**
  (`vehicles.rs` `can_ride`); the hammer already treats an offline owner as
  open. v20 behaviour here is from memory, so it needs checking first.
- **One bad row refuses a whole events edit** (`validate_event_rows`), where
  the Add-On review path drops rows one by one.

Checked and not a bug: the random brick colour kept after the pref is
turned off matches how v20's temp brick keeps its colour until a paint is
picked (unverified against the script).
