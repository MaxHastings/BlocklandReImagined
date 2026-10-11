# 2026-10-11 Soccer crash: stale Add-On physics handles

v0.2.9 release blocker 1. Max's v0.2.8 multiplayer soccer session with bots
ended when the hosting client panicked at
`crates/client/src/addon_physics.rs:281:44` ("No element at index"), taking
the hosted server with it (crash-20261010-214554).

## Cause

`AddOnPhysics` kept pushes (`Kick`) and holds in lists of their own, each
holding the body's rapier `RigidBodyHandle`. A body removed before the next
step left the handle naming nothing, and `forces()` indexed the rapier set
with it. Pushes wait for a step: at 165 fps a 6.1 ms frame often holds no
8.3 ms (120 Hz) step, so a push made on one frame waits into the next.

The shipped Ragdoll Add-On does exactly this in a match: a corpse is blasted
(every limb pushed), and the player respawns on the next frame (every limb
removed). With one corpse it was harmless only by accident: an Add-On left
with no bodies cleared its waiting pushes. With a second corpse on the
pitch (bots dying round a soccer match) the step looked up the removed
limbs and panicked at line 281, the line in Max's report.

The same flaw had three siblings in that file: a hold and a remove in one
frame (panicked in `carried_mass`, line 214), a projectile's hit waiting
past a frame with no step, and a body made again under a handle it had
(the old body went, what was asked of it stayed).

## Fix

Pushes, hits and holds now live on the `Body` they act on, and every removal
(the Add-On's `Remove`, a body made again under its handle, a body the
solver threw to infinity) goes through one `remove`. A request cannot
outlive its body or reach the body made in its place. Rapier handles are
already generation-checked, so the remaining indexing is by bodies the map
owns, the invariant the rest of the file relies on.

## The wider class

An audit of every stored handle, index and id dereferenced with a panicking
lookup across sim, motor, physics, vehicles, weapons, events, net,
`local_physics` and `brick_debris` found one shared root: package hooks run
synchronously inside the session's own loops and their operations apply at
once, so a hook can remove what the loop holds the id of.

- Fixed here: `step_items`. An `on_pickup` that removes its own spawner
  (`remove_brick(info.spawner)`, then `"take"`) made the loop look the brick
  up again and panic (`items.rs:197`, "OrdMap::index: invalid key"). The
  loop now finds each player, spawner and brick again after the hook, and
  reconciles the spawners so one the rule removed offers nothing after.
- Not fixed here (bots are another thread's): `step_bots` copies the bot
  list once per tick, and a hook run inside one bot's step (`on_damage`
  from a bite, `on_drop`, `on_chat`) can `remove_bot` another, which then
  panics at `self.bots.brains[&bot]` in `step_bot`; a hook removing the
  attacker itself panics in `fire.rs` and at `bots.rs` after the bite.
  Fix: skip a bot no longer in `brains` at the top of the loop, and use
  `get` after each hook.
- Guarded, with reasons recorded in the PR: vehicles (dismount before
  removal), events runtime, brick colliders (`take()` before retire),
  motor players (own their handles), `brick_debris` and `local_physics`
  (map entry and body removed together), weapons, net peers.

## Evidence

- `cargo test -p bri-client --lib addon_physics`: 21 pass. On the v0.2.8
  code the new tests fail: push-then-remove and push / empty frame / remove
  at `:281:44`, hold-then-remove at `:214:39`, a shot's hit at `:281:44`,
  a push meant for a removed body at `:281:44`, and the real Ragdoll blasted
  then respawned at 165 fps beside another corpse at `:281:44`.
- `cargo test -p bri-sim --test hook_lifecycle`: fails on main at
  `items.rs:197:65`, passes with the fix; `items` and `item_hooks` pass.
- `cargo test -p bri-client --lib`: 494 pass, 6 fail only for want of a GPU
  adapter in the cloud container (render and atlas tests).
- `cargo clippy -p bri-client -p bri-sim --all-targets -- -D warnings`.

## Not reproduced

The full two-player soccer session (two clients, bots, the soccer ball) was
not run headlessly. The crash itself is reproduced with the shipped Ragdoll
Add-On's own code driving `AddOnPhysics` through the client sandbox; the
network, rendering and the soccer Add-On are not in that test.

## Next

- Route the bot-loop fix to the bots thread.
- A panic in client-only cosmetic code on the host still ends the hosted
  server; isolating the hosted server from the host's client is worth a
  separate look.
