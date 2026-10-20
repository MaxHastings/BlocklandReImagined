> **Historical note. Written 2026-10-14 (the file's date in the cloud project folder, `v0.3.0/overlap-review.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# v0.3.0 overlap review (HEAD 3a37b4d3)

I read the code for this review but didn't run it. No cargo build, no tests and no files changed.
Scope: 1a73131e..HEAD (portal split collision, pass groups, on_contact "vehicles",
parked-vehicle sleep, CTF bots, /tune live apply, boarding `struck`). Findings are ranked by user impact.

## Who decides "do A and B collide" today

| Path | Where | Honors pass groups? | Sees through portals? |
|---|---|---|---|
| Rapier pair test (collider `InteractionGroups`) | `motor/src/player.rs:59` `pass_groups`, `apply_groups` :1356 | yes | yes (fills/twins clone groups, `physics/src/portal.rs:281,203`) |
| Motor sweeps / soup | `player.rs:1248,1593` `moves_against(pass_group)` | yes | yes |
| Wheel rays | `vehicles/src/world/tires.rs:150-162` (body's own groups) | yes | yes |
| Portal hook filter | `physics/src/portal.rs:124` (runs after the group test) | n/a | n/a |
| Boarding (`struck`, from motor contacts) | `sim/src/session/movables.rs:437` | yes (motor) | yes |
| **Vehicle runs over / touches player** | `sim/src/session/vehicles.rs:1476` (parry `contact`) | **no** | no |
| **on_contact `"body"`** | `sim/src/session/packages/contacts.rs:82` (AABB overlap) | **no** | no |
| on_contact `"vehicles"` | `contacts.rs:206` (Rapier contact pairs) | yes | no (fills have no parent) |
| Spawn obstruction | `player.rs:991` `Player::clear` uses `MOVES_AGAINST` | no (group 0) | n/a |

There is no `should_collide` hook anywhere in code or docs. Add-Ons only get pass groups plus
`on_contact`. The client's `addon_physics.rs:73 Groups` is a separate cosmetic world from before this release, so it doesn't overlap with this work.
Portal copies and wheel rays honor pass groups the same way normal contacts do: the copies keep
their source's groups, and the rays filter by the body's own groups. Both are fine.

## 1. HIGH: run-over damage and "vehicle" on_contact ignore pass groups
`vehicle_contacts` (`sim/src/session/vehicles.rs:1424`, test at :1476-1489) decides a
vehicle touches a player with `parry::query::contact` on raw shapes. It never compares collision
groups. Rapier lets a same-group car pass through the player, but this path still files the player
under `touching`. That leads to `world.player_contact` → `Intent::RunOver` (damage = speed × runover_damage,
`vehicles/src/world.rs:1551`), `package_vehicle_contact` (on_contact `kind:"vehicle"`) and a kill
`credit` (:1567-1584).
**Scenario:** the documented team recipe `set_pass_group(p, team+1)` on players and their team
vehicles. A teammate drives through you at Warthog speed. You pass through visually and still die
from the run-over. The Stomper rules flatten anyone run over above 4 u/s.
Boarding uses a second path (`struck`, motor contacts, honors groups), so the two paths disagree on
the same touch. **Fix:** in the `.any(|(c, _)| ...)` at :1478 also require
`collider.collision_groups().test(physics.colliders[peer.player.collider()].collision_groups())`.
That is one line. Add a vehicle case to `crates/sim/tests/pass_groups.rs`, which has no vehicle or creature test at the moment.

## 2. MED-HIGH: on_contact "body" ignores pass groups
`package_body_contacts` (`contacts.rs:51-139`) pairs players by AABB overlap. Same-group players
overlap freely, so every walk-through fires `on_contact` (`"side"`), and dropping through a teammate
fires `"above"` with downward velocity, which is exactly the stomp test in
`packages/playground/playground-stomper-rules/stomper.rhai`. Mario/stomp Add-Ons combined with team
pass groups therefore flatten teammates. **Fix:** skip the pair when both `state().pass_group` are equal and
non-zero. Carry `pass_group` in the `boxes` tuple at :59.

## 3. MED: a vehicle's pass group lives only on its colliders
`movables/perform.rs:62-74` writes `pass_groups(g)` straight onto the vehicle's colliders. Nothing
stores it: it isn't in `VehicleSave` (`vehicles/src/world/checkpoint.rs`), the snapshot, or the
protocol. Players do store theirs (`PlayerState.pass_group`, `net/src/protocol.rs:530`).
(a) **Client prediction:** `prediction.rs:874-888` spawns the driven vehicle fresh with
`InteractionGroups::all()`, so a driver predicts hitting same-group players the host lets through,
then rubber-bands back. This breaks the doc's claim that prediction "moves the same way".
(b) A checkpoint restore drops the group. `restore_checkpoint` currently runs only in tests, so this
is latent.
(c) `set_pass_group(vehicle, 0)` writes `InteractionGroups::all()` over whatever groups the vehicle
had before. That is harmless today because vehicles have none.
**Fix:** add `pass_group: u8` to the vehicle instance, save and snapshot it, re-apply it on spawn and restore,
and have the predictor apply it from the snapshot.

## 4. MED: /tune live apply skips Add-On creatures
`retune_players` (`sim/src/session/tuning.rs:89-100`) retunes only `self.peers`. Package entities
built from an archetype (`session/packages.rs:2453-2472`, `Player::spawn_tagged` with
`archetypes.tuning(..)`) keep their spawn-time numbers until they respawn. `docs/modding/tuning.md` says
"Every number takes hold at once for everyone playing that body". For a creature body, that reads to a modder
as "sometimes it takes hold, sometimes not". This may be part of the owner's "glitchy /tune" report.
**Fix:** loop `packages.entities` whose body archetype matches and call `body.set_archetype(..)`, or
narrow the doc to "every player".

## 5. LOW: portal crossings miss Add-On and run-over events
- A car straddling a portal hits a *player* beyond the partner through a fixed fill copy
  (`portal.rs:249` copies any collider, the kinematic player's too). `post_step` shares impulses
  only with dynamic sources (`vehicles/src/world.rs:2156`), and `vehicle_contacts` (parry,
  same-side only) never sees the player. The result is that the car stops dead as if hitting a wall, with no
  run-over and no on_contact. The vehicle-vs-vehicle `"vehicles"` crash also doesn't fire through a portal:
  `package_vehicle_crashes` needs `collider.parent()`, and fills have none (`contacts.rs:222`).
- `Player::clear` (`player.rs:985`) treats a same-group teammate on a spawn point as
  an obstruction. The fallback `spawn_overlapping` exists, so this costs a farther spawn at worst.

## 6. Dead code / leftovers
- `PortalContacts::extend` (`physics/src/portal.rs:75`): no caller. `rules.extend` in
  `client/src/app/addons.rs:782` is unrelated. Delete it.
- `portal::past` (`portal.rs:50`) is `pub` but used only inside the module. Make it private, or leave it.
- `Session::player_tuning` (`tuning.rs:102`) and `Session::open_panel` (`packages/panels.rs:74`)
  are called only from tests (`body_switching.rs`, `panels.rs`, `roleplay.rs`, `wire_lite.rs`).
  They are fine as test accessors, but nothing in the engine uses them.
- No `dbg!`/`println!`/`todo!`/`TODO`/`allow(dead_code)`/new feature flags or env-var debug
  switches were added in `crates/*/src` across the range. No new pub type or const is unreferenced.

## 7. docs/modding vs code
- Every hook and function named in the new docs exists: `on_contact`, `on_object`, `on_panel`,
  `on_addon_event`, `api_<name>`, `show_panel`, `close_panel`, `call_addon`, `exports`, `emit`,
  `host_data`, `set_host_data`, `set_pass_group`, `camera_set`, `input_claim`, `camera_frame`.
  None is missing.
- `rules.md` **Pass groups** says a creature or vehicle can be grouped and that "the player's own prediction moves
  the same way". That holds for players on foot only (see 3). The doc also doesn't mention that run-over and
  on_contact still fire inside a group (1, 2). Fix the code rather than the doc.
- `tuning.md` "everyone playing that body" isn't true for creatures (see 4).
- `docs/progress/2026-10-13-modding-building-blocks.md:26` calls `set_pass_group` "Collision
  hooks" and says `moves_against` is "shared with the portal collision thread". The portal code
  actually reuses collider groups, not `moves_against`. The two agree, but the wording suggests a
  `should_collide`-style hook that doesn't exist.

## Smallest patch set before release
1. One-line group test in `vehicle_contacts` (#1).
2. Same-group skip in `package_body_contacts` (#2).
3. Either store and replicate the vehicle pass group (#3), or document "players and creatures only" and
   reject `ObjectRef::Vehicle` in `SetPassGroup` for 0.3.0.
4. Retune entities, or narrow the tuning doc (#4).
