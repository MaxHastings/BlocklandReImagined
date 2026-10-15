# 2026-10-14 Engine gaps found by porting a shooter as an Add-On

A private Add-On port of a vehicle shooter was used to stress-test what an
Add-On game can do. Every gap it found was closed in the engine as a general
mechanism with no game-specific names; the Add-On's own polish waits.

## What changed

- **Footsteps and landings.** An archetype's `sounds.footsteps`, `stride`
  and `land` play per client from the poses it already draws (no wire cost).
  `crates/client/src/footsteps.rs`.
- **Board with use.** A vehicle with `board_on_use` is entered with the use
  key, into its first free seat.
- **Quick use and image states.** Rules `quick_use` (use an inventory item
  without holding it) and `set_image_state` (jump a held image to a named
  state), for grenade and melee keys.
- **HUD.** A layout's `name_tags` style (who shows: always, aimed, never;
  colour, outline, height) and a `tools` region that hides the stock tool
  bar when claimed.
- **Aim assist.** An image's `aim_assist { angle, range, slowdown }` bends a
  shot onto the nearest non-ally body in its cone and slows the look over one.
- **Map decals.** Projectile decals mark the map as well as bricks
  (`CueKind::Decal.brick` is optional).
- **Screen and body flash.** Rules `screen_flash(player, colour, seconds)`
  and `body_flash(player, colour, seconds)` (a see-through coloured shell
  over the body), for shield hits and similar.
- **Lag compensation.** Moves carry the server tick the client last saw;
  an aimed hitscan shot tests other players where that shooter saw them
  (`crates/sim/src/rewind.rs`, at most 60 ticks back). Projectiles and
  world-only casts are unchanged.
- **Tyre skids and squeal.** A vehicle's `skid { emitter, sound, min_slip,
  full_slip }` runs an emitter at each grounded wheel sliding across its
  steered axle and a squeal loop that follows the slide; client-side from
  the presented motion. `bri-addon-check` checks the squeal like an engine
  sound.

- **Flyers steer by look.** A `Flying` vehicle with `steering.toward_look`
  pitches its nose to the driver's look as well as turning to it (the
  client holds the look still in the world as the nose moves), so a hover
  craft or a flyer goes where the driver looks.
- **Sticky projectiles.** A projectile's `stick_to_bodies` sticks it to any
  player or vehicle it hits and it rides along (`Projectile::stuck_to`);
  clients draw it on the body as they draw it, with no position traffic.

- **Stance accuracy.** A shot's `crouched_spread` and `airborne_spread`.
- **Prone and lean.** An archetype's `movement.prone` (box, eye, crawl
  speeds) and `movement.lean` (eye slide within half the box's width, view
  and body tilt, ease time); rules `set_prone` and `set_lean`, bound to
  keys by an Add-On's `binds.json`. `PlayerState::posture` carries them, so
  prediction replays them. The lying body is drawn face down along its
  facing; its box stays the short upright one, so its head and feet are
  not hit. Hit regions (`head`, `torso`, `legs`) already reached
  `on_damage`.

Steering toward the look already existed for wheeled vehicles (the
Playground buggy has the Warthog's tags), so the handling gap named in the
assessment was content, not engine.

Protocol notes: `footsteps`, `map-decals`, `screen-flash`, `body-flash`,
`move-seen-tick`, `projectile-stuck-to`, `player-posture` in `crates/net/protocol-changes/`.

## Decisions

- First-person arms belong to the glTF importer (owned by the map thread),
  which agreed to add a general `--first-person` option.
- A two-weapon carry is rules plus the `tools` region; the chase camera
  already exists (`driver_view`); impact puffs are the projectile's
  `explosion`/`blood_effect`. None needed engine work.

## Evidence

- `cargo test -p bri-sim`, `-p bri-net`, `-p bri-client --lib` (552 passed),
  `cargo clippy --workspace --all-targets -D warnings` before the skids.
- Skids: `cargo test -p bri-client --lib skid`,
  `cargo test -p bri-vehicles`, `cargo test -p bri-package-runtime --test
  check engine_sound`, clippy on the touched crates clean.
- New tests: `aim_assist.rs`, `screen_flash.rs`, `weapon_query.rs`
  (`an_aimed_shot_meets_a_body_where_its_shooter_saw_it`), footsteps,
  HUD name tags, playground ground decal, skid and squeal.

## Next

The Add-On itself (out of the public repo) adopts these fields; the
first-person arms import option comes from the importer thread. No
interactive check was run: Maxwell playtests.
