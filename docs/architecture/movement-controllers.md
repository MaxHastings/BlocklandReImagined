# Movement controllers

v0.3.0 workstream W3 (`/mnt/project-files/v0.3.0/plan.md`). An Add-On gives
a body its own movement (a wall kick, a dive, later a triple jump) without
new engine code per ability. The host runs it as the authority, every
client predicts with the same bytes, and the Blockhead keeps moving exactly
as it did. Written 2026-10-12 against `main` at `ebf02ca`; the prototype is
the branch `claude/v030-movement-controllers-w3puoe`.

## Assessment

The hard part the plan named, running Add-On WebAssembly on the server
deterministically, turned out tractable once the controller is scoped as
**policy over the existing motor**, not a replacement for it:

- The v20 motor (`crates/motor`) stays the only mover. Collision, sliding,
  stepping, gravity, water, ropes and openings are unchanged, so there is
  no second physics engine and no Torque fidelity is re-derived.
- A controller runs once per 32 ms Torque tick, before the motor moves the
  body. It reads the tick's input and the body's state, may sweep the body
  through the same polygons the motor collides with, and returns a
  velocity, whether it used the jump button, 16 bytes of memory and an
  animation intent. That is enough for wall kicks, dives, triple jumps,
  long jumps, double jumps, dashes, ground pounds and wall slides.
- Determinism comes from WebAssembly itself plus three host rules: a fresh
  instance every step, NaN canonicalisation, and fuel (never wall-clock)
  as the only budget. Same bytes and same inputs give the same bits on any
  machine, which is a stronger guarantee than the native motor has.
- The cost is small: 16 µs a step in release on this container (most of
  it instantiation): about 0.02 ms of every 32 ms tick per controller
  body on the host, and about 0.2 ms per correction for a client replaying
  300 ms of unacknowledged inputs.

What it cannot do, deliberately: change the collision body's shape per
tick, move through walls, push other bodies, or run between ticks. Those
stay engine mechanisms (archetype constants, `set_archetype`, the motor).
Anything a controller cannot express is the next seam to design, not a
reason to let it replace the motor.

One gap remains before players should meet controllers in a release,
recorded under "Next" below: other players' bodies do not yet show the
animation intent. Controllers need no join trust prompt (see "Distribution
and trust").

## Where it sits

```
input ─► Player::torque_tick
           energy, crouch, steering            (motor, unchanged)
           ┌───────────────────────────────┐
           │ controller (if the archetype  │  reads input + state + sweeps
           │ has one): velocity, take jump,│  writes velocity, memory,
           │ memory, intent                │  intent, "took the jump"
           └───────────────────────────────┘
           run surface, air control, jump, jets, resistance, drag,
           water, rope, player pushes, sweep/slide/step     (motor)
```

- `bri_motor::controller` defines the seam: `ControllerTick` (what the
  controller sees and decides), `Probe` (its sweeps), `MoveController`
  (the trait), `ControllerState` (its memory and intent, on
  `PlayerState::controller`) and `ControllerSource` (id and module bytes,
  on `Archetype::controller`). The motor crate never links a WebAssembly
  runtime.
- `bri-controller` (new crate) implements `MoveController` with Wasmtime.
- `PlayerTuning::controller` holds the compiled controller as a runtime
  handle (never serialised). Every path that already builds a body from the
  archetype table (join, respawn, `set_archetype`, prediction, correction,
  replay) therefore picks it up with no new plumbing.
- A body with no controller skips the hook entirely: not one float of the
  Blockhead's tick changes (`a_blockhead_beside_a_kicker_moves_exactly_as_without_the_add_on`
  compares every tick's feet and velocity bit for bit against a server
  without the Add-On).

## The interface (`api` 1)

A controller is a WebAssembly module exporting `memory`, `io() -> i32`
(where its 128-byte exchange record lives) and `step() -> i32` (0: use the
record; anything else: leave this tick to the motor). Its only possible
import is `bri.sweep(dx, dy, dz: f32, out: i32) -> i32`: sweep the body's
box by that vector, 1 when something is hit, writing fraction and outward
normal (four f32) at `out`.

| Offset | Type | In/out | What |
|---|---|---|---|
| 0 | u32 | in | interface version, 1 |
| 4 | u32 | in | flags: 1 grounded, 2 crouched, 4 jump, 8 crouch, 16 jet, 32 submerged |
| 8 | f32 | in | tick length, seconds |
| 12 | f32 | in | body scale |
| 16 | f32×3 | in | feet |
| 28 | f32×3 | in/out | velocity |
| 40 | f32×3 | in | facing on the ground plane, unit |
| 52 | f32×3 | in | where the movement keys push, on the ground plane |
| 64 | f32 | in | pitch |
| 68 | u32 | out | 1: the controller took the jump (the motor does not jump) |
| 72 | u32 | in/out | animation intent (low 8 bits kept) |
| 76 | 16 bytes | in/out | the controller's memory |

Choices behind it:

- **A record, not many host calls.** One write and one read per step keeps
  the boundary cheap and the ABI easy to version (the version word, and
  unused offsets up to 128 for additions).
- **Facing and push as vectors.** WebAssembly has no `sin`; giving the
  ground-plane vectors spares every controller a trigonometry library, and
  the engine computes them once, the same way on both sides.
- **Velocity, not position.** The controller decides how the body wants to
  move; the motor still decides where it can go. A controller cannot place
  a body inside a wall.
- **Jump is an input the controller may take.** A kick uses the button;
  the motor's own jump then does not fire on the same press.
- **Edge detection is the controller's.** It keeps "jump held last tick" in
  its memory, so "a new press" means the same thing on host and client and
  survives corrections.
- **Sweeps through the motor's own polygons.** `Probe` gathers Torque's
  working list (the same `Soup` the motor collides with, sorted by brick id)
  8 units around the feet and sweeps the body's box with the motor's own
  `ExtrudedPolyList` test (`torque::sweep_box`). A sweep starts 0.1 behind
  the body, so a wall it is already pressed against still reports its
  normal. At most 8 sweeps of at most 8 units (times the body's scale) per
  step.
- **Animation intent is a number the Add-On names.** The engine replicates
  it; what it looks like belongs to W2's animation roles (an archetype's
  `animations` map can name a clip per intent).

Real controllers will be written in Rust, C or Zig and compiled to
`wasm32-unknown-unknown`; the samples are hand-written WebAssembly text so
they read without a toolchain. A small Rust guest crate (record accessors,
`sweep` wrapper, `#[no_mangle] step`) is the obvious follow-up.

## Determinism and sandboxing

Running on the host is the riskiest part, because a host must never trust
what a controller returns any more than it trusts a client, and a client
must reach the same result the host did.

| Rule | How | Why |
|---|---|---|
| A step is a pure function | A fresh instance from `InstancePre` every step; memory and globals start from the module's initial image | Anything it remembers is in `PlayerState::controller`, which corrections restore and replays rerun. Enforced, not asked for (`a_step_cannot_remember_what_the_engine_does_not_see`) |
| Same float bits everywhere | WebAssembly arithmetic is IEEE-754, correctly rounded; `cranelift_nan_canonicalization` makes NaN bits canonical; relaxed SIMD off; no `threads` feature compiled | The one thing WebAssembly leaves to the machine is NaN's bits (`nan_bits_are_the_same_on_every_machine`) |
| No outside world | Only `bri.sweep` may be imported, checked at compile time; no WASI, clock, randomness or I/O | Every input is in the record or the world both sides share (`a_controller_imports_only_the_sweep`) |
| Bounded work | 200,000 units of fuel per step; no epoch deadline | Fuel runs out at the same instruction everywhere; a wall-clock deadline would not (`a_step_that_runs_out_of_work_changes_nothing`) |
| Bounded memory | 4 MiB, one memory, one table, 256 KiB stack | A compiled Rust module needs about 1.1 MiB |
| Bounded output | Velocity must be finite and is clamped to `MAX_SPEED`; intent is one byte; memory is 16 bytes | Same bounds the host keeps every player state in (`PlayerState::keep_in_bounds`) |
| A failed step changes nothing | Trap, out of fuel, bad pointer, too many sweeps, NaN: the tick runs as the motor alone would, on both sides; `MotionEvents::controller_fault` says why | Failures are as deterministic as results, so host and client still agree |
| Bounded size | 64 KiB a module, 1 MiB for a whole archetype table | The module travels in the checkpoint |

What is *not* bit-identical between host and client is the same as today:
the client's collision mirror and other players' poses can differ from the
host's world, and the native motor is compiled Rust floats on both sides.
That is ordinary prediction error, corrected as it is now.

## Distribution and trust

- An Add-On provides a `controller` (new content kind, server side like
  `archetype`, `.wasm`, at most 64 KiB) and names it from an archetype:
  `"controller": "<package>:controller/<name>"`. An archetype built on a
  base inherits the base's controller.
- The host compiles every controller when the Add-Ons load; one that does
  not compile refuses the Add-On with its reason, rather than leaving a
  body that moves unlike its Add-On.
- The module bytes travel in the checkpoint's archetype table, not through
  package sync. That guarantees the client runs exactly the bytes the host
  runs (no version skew between a cached package and the host's), costs a
  few hundred bytes per controller, and needs no client-side lookup. A
  content hash with a package-cache lookup is a later optimisation if
  modules grow.
- The client compiles the table when the checkpoint arrives. A controller
  that does not compile there is logged and that body is simply not
  predicted with it; the host's corrections still move it correctly
  (`a_client_without_the_controller_is_corrected_to_the_hosts_kick`).
- **No trust prompt.** Maxwell decided on 2026-10-12 that the join trust
  prompt is for things that can harm the player's computer, such as native
  code or DLLs, and controllers cannot. A controller is WebAssembly run by
  Wasmtime with no WASI and one import (`bri.sweep`, which reads the
  world's collision and writes only into the module's own memory). Its
  memory is capped, its work is capped by fuel, it gets a fresh instance
  every step, and all it can hand back is a velocity, a jump flag, 16 bytes
  of memory and an intent, which the motor still collides. It cannot read
  files, open sockets or touch anything outside the step. No part of the
  controller path loads native or otherwise unsandboxed code on a client;
  if one ever did, that part would need the prompt.

## Prediction and server correction

Nothing about the prediction loop changes. The controller is inside the
motor tick, so:

- `Predictor::step` runs it while predicting, `reconcile` restores the
  host's state (with the controller's memory) and replays unacknowledged
  inputs through it, and corrections within noise keep the local motion,
  as before.
- `PlayerState::controller` rides in every own pose (`Option`, so a
  Blockhead costs one byte). Remote poses do not carry it yet: other
  players see the movement (positions and velocities) but not the intent.
- Inputs remain intentions only (`MoveInput`); a client cannot send a
  kick, only a button press, so authority is unchanged.

Evidence (`crates/sim/tests/movement_controller.rs`, real `Session` and
`Predictor`, 150 ms each way, a fifth of input datagrams lost with six-input
redundancy, poses every third step with some skipped):

| Test | Result |
|---|---|
| `a_kick_is_predicted_under_150_ms_of_latency` | Largest correction 0 once inputs flow; client ends bit-identical to the host |
| `a_client_without_the_controller_is_corrected_to_the_hosts_kick` | A 2.9-unit correction when the client predicted a slide and the host kicked; then bit-identical |
| `two_clients_predict_two_different_controllers_at_once` | A Wall Kicker and a Diver on one host, each predicted by its own client with no visible correction |
| `the_wall_kicker_kicks_off_a_wall_and_the_blockhead_does_not` | Same inputs: the Blockhead slides down the wall; the kicker springs 2 units higher and away at 7.8 units/s |
| `a_blockhead_beside_a_kicker_moves_exactly_as_without_the_add_on` | Every tick's feet and velocity equal, bit for bit |

## Coexistence

| With | What happens |
|---|---|
| Blockhead and every v20 player type | No controller, hook skipped, unchanged. An Add-On *could* give a v20 type a controller through `adjusts`; that is the host's choice, as `adjusts` already is for constants |
| Switching bodies (`set_archetype`, W4) | The controller follows the archetype; a new controller starts with blank memory (`Player::set_archetype`), so it never reads another controller's bytes. Mid-kick switches keep the velocity, as W4 proposes |
| Vehicles | A seated rider's inputs go to the vehicle (`Predictor::record`); the walking motor and its controller do not run. Vehicle actors are unaffected. A rideable player-type mount (the horse) is a motor body, so a mount archetype may have a controller |
| Ropes, water, crouch, jets, openings | The motor still applies them after the controller, in the same order as before. Sweeps do not yet see through openings (portals); a controller next to one sees the wall behind it |
| Animation (W2) | Intent is a byte; W2's semantic roles can map intents to clips. Until then nothing draws it |
| Sockets, seats, held items (W2) | Untouched: a controller moves the body, the body carries its sockets |
| Bots | Bots drive bodies through `MoveInput` like players, so a bot in a controller body gets the controller. Bot planning does not know what a kick does; per AGENTS.md that stays explicitly unsupported until a controller can declare its affordances |

## Budgets

| Limit | Value |
|---|---|
| Fuel per step | 200,000 (the wall kick uses a few hundred) |
| Sweeps per step | 8, each at most 8 units × scale |
| Memory | 4 MiB per instance; 16 bytes kept between ticks |
| Module | 64 KiB; 1 MiB for all controllers in a table |
| Measured cost | 16.3 µs a step release, 34 µs debug (`cargo test -p bri-controller --release step_cost -- --nocapture`) |

A server-wide budget ("a tick's steps for all players must fit the tick",
falling back to the stock motor) is not built; at 16 µs a step it would
take about 2,000 controller bodies to fill one tick. Wasmtime's pooling
allocator would cut instantiation cost several times if it matters.

## The prototype

- `crates/motor/src/controller.rs`: the seam and `Probe`.
- `crates/motor/src/player.rs`: the hook in `torque_tick_among`;
  `PlayerState::controller`, `PlayerTuning::controller`,
  `MotionEvents::controller_fault`; fresh controller memory on a body change.
- `crates/motor/src/torque.rs`: `sweep_box`, the motor's own box sweep.
- `crates/motor/src/archetype.rs`: `Archetype::controller`,
  `Archetypes::bind_controllers`, table size bound.
- `crates/controller`: the Wasmtime host and its rule tests.
- `crates/package-runtime`, `crates/package`: the `controller` kind and
  `ArchetypeDef::controller`.
- `crates/sim/src/session/packages.rs`: the host binds when Add-Ons load;
  `crates/net/src/replica.rs`: the client binds when the checkpoint arrives.
- `packages/samples/sample-wall-kick` and `packages/samples/sample-dive`:
  two independent Add-Ons, each a body, a `/wallkick` or `/diver` command
  and a controller (one with sweeps, one without).
- `crates/net/protocol-changes/movement-controllers.md`: the wire change.

## Next

1. Remote intent: carry `intent` in `RemotePose` so other players' clients
   can draw kicks and dives; map intents to W2's animation roles.
2. Report `controller_fault` once per controller to the host's console and
   log (the motor already returns it every tick it happens).
3. Triple jump (needs a ground timer and jump count in memory; no new
   interface) and a long jump, as the plan's Demo 2 asks.
4. A Rust guest crate and a doc page in `docs/modding/` for authors.
5. Sweeps through openings (`Soup::open_passages`), if a controller next
   to a portal needs it.
6. A two-client test over the real network stack (`crates/net/tests`) to
   cover the checkpoint path end to end.
