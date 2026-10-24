# Map movers and ladders

Status: built on `codex/aa-doors-ladder` for the AA2.5 Pipeline doors and
ladders; generic, no game content. Not accepted by a human playtest.

## What a map can now say

**A mover** is a static model that moves along keys (a swinging door, a
hinged vent, a turning handle). Its scene node is an ordinary
`static_model` with one more property, `mover`, holding a
[`MoverDef`](../../crates/content/src/mover.rs) as JSON:

```json
{"id":"Mover27","keys":[{},{"rotation":[0,-1.5708,0]},{"rotation":[0,1.5708,0]}],
 "glide":"smooth","encroach":"stop","params":{"style":"UseControl","two_way":"1"}}
```

The node's transform is the mover at key 0. A key is an offset from it: a
translation in the map's axes and a rotation vector (axis times radians) in
the world frame about the node's origin, which holds more than one turn.
The model is built about its own origin (a door's hinge). `params` are the
package's facts about it; the engine keeps them and tells scripts.

**Sounds.** A mover may carry `"sounds":{"opening":"door.open","opened":..,
"closing":..,"closed":..,"moving":..,"volume":1.0}`, each a sound profile
name and all optional. A motion toward key 0 is closing, one away from it
is opening, so doors, gates and valve wheels read alike. Each client works
this out from the states it already receives: the one-shots play at the
mover when it starts or turns back, and when it comes to rest; `moving`
loops while it travels. A client that joins mid-swing hears nothing of the
start. `volume` (0 to 2) multiplies the sounds' own.

**A climb volume** is a `climb` scene node: a box (`half` in the node's axes),
a climbing `speed` and `strafe`, facing the node's local +Z. It becomes a
sensor collider that carries its facing and speed in its tag, so the host and
every client's collision copy hold the same ladders with no new message.

## Who owns what

| | Engine (mechanism) | Package (policy) |
|---|---|---|
| Door | keyed motion with easing, collision that moves with it, stops for a body in the way and says so, the same pose on every peer at a tick | who may start it, from which side, how fast, what it sounds like (`mover_goto`, `mover_stop`, `movers()`, `movers_near()`; capability `movers`) |
| Ladder | take hold, run along it with no gravity, let go on leaving the box, a jump or the floor | whether a body can climb at all and how (`climb` in its archetype's tuning) |

## Motion

`MoverState` is `from`, `to`, raw progress `alpha` at tick `since`, advancing
`rate` a tick while `moving`; every peer computes the pose from it and the
tick. `go_to` follows Unreal's `InterpolateTo`: turning back toward the key it
left carries the distance already covered. The default glide is
`3a^2 - 2a^3`.

The host steps movers before it moves players. A moving mover whose next
pose would overlap a player stops (`blocked`), and goes back only when told
to. Clients take the states in `Checkpoint::map_movers` / `Delta::map_movers`
(whole, on change); `CollisionMirror::set_map_movers` puts the predicted
world's doors where the host has them.

## Ladders in the motor

`PlayerTuning::climb` (`bri_motor::climb::Climb`) says a body may climb. A
body whose middle is in a volume takes hold when it faces it
(`facing`, default 0.9 of the dot), pushes toward it or not at all (`push`,
0.7), is not prone and is not falling faster than `grab_speed`. While held it
faces the volume, its move keys set its speed along the rungs (forward is up),
and gravity, drag and the run force are off. With `wall_pull` above zero the
body is also drawn toward the wall it faces until it touches or reaches the
far side of the volume, so a climber hangs where a map's ladder markers sit.
Leaving the volume, a jump
(`jump_speed`) or coming down onto a floor lets go. `Posture::climbing` is in
the player state, so the client predicts it.

A climb node may supply `facing_yaw` (one finite angle in radians) independently
of its sensor box orientation. Without it, the existing local +Z facing applies.
This allows diagonal rungs inside an axis-aligned authored trigger without
rotating and enlarging the trigger. The motor uses that direction for facing
and wall pull; containment still uses the exact sensor box.
