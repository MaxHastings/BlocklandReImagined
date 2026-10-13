# 2026-10-13 v0.3.0 release gate: engine observations (a)-(d), /botreload, fight step

The v0.3.0 release gate (`crates/net/tests/release_gate.rs`, Max's session
against his private Mario and Halo Add-Ons) listed four engine observations
and two FAILs in the fight step. Each engine item below was traced to its
cause, fixed in the engine where the engine was at fault, and given a
regression test that fails before the fix.

## (b) Moves queued at a dismount were never acknowledged

Cause: the host's `Dismounted` intent (and teleports, deaths, teledoors,
water returns) cleared `peer.inputs` without moving `processed_move`
forward. The next pose did not acknowledge the dropped moves, so the client
kept replaying moves the host never ran until a later move passed them.

Fix: `Peer::discard_inputs` counts the dropped moves as consumed (the
newest queued sequence becomes `processed_move`), at every site that
cleared the queue. Test: `moves_queued_at_a_dismount_are_acknowledged`
(`crates/sim/tests/vehicles.rs`).

## (c) A controller body crept up off the floor and lost jumps

Cause: the motor, not the Halo controller (its source never touches
vertical speed). `torque::update_pos` leaves every hit moving
`NORMAL_ELASTICITY` (0.01) away from the surface; v20's run force removes
that on the next tick. An archetype whose Add-On controller owns its
running sets `acceleration` 0 (Spartan, Mario), so nothing removed it: the
body rose 0.01 units/s, left the 0.013 contact slab, was no longer
grounded, and the controller dropped a jump pressed then. A motor test
traced feet rising 0.0096 per 96 steps and `grounded` going false every
~0.4 s.

Fix: on a run surface, for a body with a controller, the motor removes what
the run force leaves of that elasticity. Bodies without a controller are
unchanged: doing it for every body shifted a bot's boarding in
`a_bot_drives_the_look_steered_buggy_at_a_runner`, because v20's saturated
run force keeps part of it while accelerating. Test:
`crates/motor/tests/still_controller.rs`. The gate's Chief step now waits
5 s before each of three jump presses and requires all three to take.

That fix let such a body rest perfectly still, which exposed a second
fault: a push into the floor slower than 0.01 (one of Mario's rifle rounds,
fired from a little above, left Chief moving 0.0026 units/s down) moved the
body too little each tick for the sweep to see the floor. Nothing took it
off, so Chief sank through the fixture room's floor and fell out of the
world (two-client gate, about one run in five). The motor now also takes
off such a body's small speed into a run surface, as v20's run force would;
larger speeds hit the floor as before. Test:
`a_body_whose_controller_owns_its_running_is_not_pushed_through_the_floor`
(fails before: the body sinks after 456 steps of a 0.0026 push).

## (a) Mario jetted out of the Warthog was seated again on its roof

Measured with a throwaway gate probe: the jet puts Mario 2.2 up the seat
(`Armor::doDismount`'s first point), over the Warthog's single box hull,
so he lands on its roof (grounded true, before and after the (c) fix).
Mario cannot jet and gets going slowly, and once MinMountTime (120 ticks)
passed the contact check seated him again, then his held keys drove the
Warthog off (which is how (d) was seen).

Cause in the engine: `vehicle_contacts` seated any player whose box touched
a vehicle from above. v20 seats from `WheeledVehicleData::onCollision`,
which only a hit of the player's own move (`Player::updatePos`) raises:
landing on the vehicle, not standing or walking on it. Fix: the sweep
contacts of each move mark the vehicles it struck; boarding needs a strike
that tick. Touching, run-overs and refusals are unchanged. Test:
`standing_on_a_vehicle_does_not_seat_a_rider_but_landing_on_it_does`. The
dismount placement itself matches v20 and is unchanged.

## (d) A vehicle off the map edge falls forever: v20 does the same

No v20 out-of-bounds recovery is documented in the repository. The v20
function index (`docs/research/ui-ux/data/v20-function-index.tsv`) has only
`Armor::onLeaveMissionArea`/`onEnterMissionArea` (7 lines each) and
`GameConnection::onLeaveMissionArea`/`onEnterMissionArea` (3 lines), and no
vehicle or kill-height handler; vehicles are recovered only by the spawn
brick's wrench Respawn (`fxDTSBrick::recoverVehicle`) or by destruction.
Nothing was implemented. In the gate, the fall came from (a): the
reseated Mario's held keys drove the Warthog off the fixture room's
floor, which has no walls.

## /botreload re-read every bot kind unfiltered

Cause: hosting filtered bot kinds through `playable_bot_kinds`; the
`/botreload` closure returned every enabled Add-On's kinds, bringing back
ones whose body the game leaves out. Fix: `HostSetup::session_with` wraps
the reload closure with the same filter, against the bodies the game had
at start. Test: `bot_reload_keeps_only_the_bots_the_game_can_run`
(`crates/net/tests/host_setup.rs`; before the fix it reloaded 2 kinds
instead of 1).

## Release gate harness

- Leaving the Warthog: jet, then walk off the roof on the driver's side
  (the gun is on the other), holding the key four seconds in one move: in
  the air Mario's controller can carry him toward the middle first, and on
  the roof he turns and gets going slowly.
  Both leave steps are now required steps (FAIL, not KNOWN); the respawn
  fallback stays only so later steps run after a failure.
- The fight: Max clears the bots (`/clearBots`) first, both are revived and
  walk to fixed spots 20 apart (Chief steps up to 10 for his rocket), and
  each aims at the middle of the other's
  body (half its archetype's stand height) instead of feet + 1.2. The
  rocket's launch, flight, Chief's eye and where Chief stood through the
  fight are reported.
- The earlier FAILs: headless, the Stompers (respawned by their brick)
  killed Max after the respawn fallback; with two clients the rocket was
  fired at a Mario who had just respawned, aimed at feet + 1.2 from the
  friend's replica.
- With those fixed, about one two-client rocket in five still went by
  Mario at 20 apart. A host-side trace showed the eye ray hitting Mario and
  `correctMuzzleVector` aiming at that point; the miss is the Halo rocket
  launcher's own `spread` (0.001667, v20's random turn of up to 1.5
  degrees), against Mario's 0.37 either side of his middle. The headless
  runs happened to hit every time. Not an engine fault;
  from 10 apart every rocket lands. Chief's earlier fall out of the world
  in the same step was the engine fault above. (Mario's rifle stays at 20:
  from 10 its burst kills Chief before he can fire.)

## Commands

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
cargo test -p bri-motor; cargo test -p bri-sim --no-fail-fast
cargo test -p bri-net --test host_setup
BRI_PRIVATE_ADDONS=<root> cargo test -p bri-net --test release_gate -- --nocapture --test-threads=1
```

Gate runs (headless and two clients, private Add-Ons identical to the
shared zip), in order:
- Before the floor fix and the rocket spot: headless passed every run;
  two clients failed the leave step until the driver's side and the
  four-second hold, then failed the fight in about one run in three (Chief
  fell through the floor, or a rocket spread past Mario).
- After the floor fix (786b2005), seven two-client runs passed in a row;
  then one full run in three still failed two-client on rocket spread.
- Moving both to 10 apart made Mario's rifle kill Chief before his rocket
  (headless FAIL, 3 of 3), so only the rocket moved to 10 apart.
- Final script: see the handover for the last three runs. Every run's only
  KNOWN line is Mario's sounds (content: the Add-On has none).

Still open: Mario has no sounds (content). Real v20 maps and a GPU are
still needed for what the gate cannot see.
