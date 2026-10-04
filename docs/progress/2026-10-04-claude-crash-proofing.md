# 2026-10-04 Crash proofing: effects budget, motor bounds, session faults, device loss, clamp attribution

Branch `fix/crash-proofing` from origin/main 53f05cf (v0.2.3). Five findings
from code review, each checked against the code first, each fixed at its
mechanism with a guard test that fails on main and passes after.

## 1. Busy battles closed the game: "Effects frame exceeds GPU instance budget"

Cause: the client runs three effect worlds (map/brick `app/load.rs`, weapons
and actors `app/mod.rs`), each at `EffectsLimits::default()` (65536
particles, 256 lights, a flare sprite per light). `rebuild_effects_renderer`
sized the one `EffectsRenderer` for two (`2 x particles + 2 x lights` =
131584). `combine_effect_frames` merged all three without a cut, so three
full worlds (197376 sprites) made `EffectsRenderer::write` fail its
`ensure!`, the render error reached `platform.rs` `fail` and the game exited.
Mirror and probe views (`views.rs`) took the same merge.

Fix: `EffectsLimits::max_sprites` (particles plus one flare per light) and
`EffectsWorld::limits` in fx-runtime; `effects_instance_budget` sums the
three worlds' own limits (capped at the new `gpu::MAX_INSTANCES`) to size
the renderer. `combine_effect_frames` takes the renderer's
`max_instances()` and drops the farthest sprites past it (the merge is
far-first), returning `EffectFrameCuts { lights, sprites }`; the sprite cut
is `particles_cut` in `entity_counts`. Over budget now draws less, never
errors.

Guard: `app::tests::three_full_effect_worlds_fit_the_renderer_and_overflow_drops_the_farthest`.
With main's sizing it fails (`left: 131584, right: 197376`); after, it passes.

## 2. Host states outside the client's correction bounds disconnected the player

Verified: `Player::restore` refused velocity axes past 1000 and feet past
1e6, but the host never kept its own states inside them. `Player::place`
copied a vehicle's velocity to seated riders (`session/vehicles.rs`
`follow_seats`, `riding.rs`), and the motor let low-drag bodies (imported
player types may set any positive drag) or strong jets pass 1000 u/s. A
rejected correction failed `Motion::observe`, which `net_events.rs` turns
into `Failed` and a disconnect.

Fix: one definition in bri-motor: `MAX_FEET`, `MAX_SPEED`,
`PlayerState::check_bounds` (what `restore` accepts, returning a typed
`RejectedCorrection` naming the field) and `PlayerState::keep_in_bounds`
(applied by the host at the end of every motor tick and in `place`;
`set_motion` uses `MAX_SPEED` too). Client policy: an out-of-bounds
correction is brought into bounds and taken (a snap to the host pose), a
non-finite one is skipped, each field warned once; prediction's copies of
other players leave out an out-of-bounds body (`Predictor::set_others`).
A pose that is not this player's stays fatal (a protocol violation).

Guards: `sim/tests/prediction.rs`
`a_fast_rider_and_a_low_drag_fall_stay_valid_authoritative_corrections`
fails on main (`fast rider: ... velocity [0.0, 0.0, -1500.0]`; with the
rider half skipped, `low-drag fall: ... velocity [0.0, -1444.675, 0.0]`)
and passes after. `motion::tests::an_out_of_bounds_correction_resyncs_instead_of_disconnecting`
fails with main's client policy (`Invalid authoritative player correction:
velocity [5000.0, 0.0, 0.0]`) and passes after.

## 3. A movement fault exited to the desktop

Verified: `App::frame` propagated `advance_local_game` (motion advance, the
worker's movement send, mount posing) and `advance_world_presentation` with
`?` to `App::tick`, whose error makes the platform exit the event loop;
only `poll_network` faults ended just the session. The "vehicle frame
lookups" near frame.rs:338 are `Option` `?` inside closures and never
propagated.

Fix: `App::end_session_on_fault`, the one path for all three.

Guard: `app::tests::a_movement_fault_ends_the_session_not_the_game` (made-up
content; hosts single player, then sends a NaN look). On main `tick`
returns `Invalid look angles`; after, the frame succeeds and the session
shows `Failed` with that reason.

## 4. Device loss left horses on the old GPU device

Verified: `gpu_stopped` cleared players' `gpu`/`instance` but not
`Avatars::mount_meshes`, rebuilt only on an appearance change.

Fix: `AvatarMesh::gpu_stopped` owns what a mesh holds on the GPU;
`Avatars::gpu_stopped` applies it to players and horses and drops the
preview.

Guard: `crates/client/tests/horse_riding_render.rs` now loses the device
mid-ride and renders on a fresh `Headless` device. On main: wgpu panics on the
horse's old-device buffers (`Cannot get non-existent resource
BufferId(34,1)` in wgpu-core, or a `Queue::write_buffer` validation error
on the `identity scene instance` buffer); after, it passes. These GPU tests ran here on Mesa's lavapipe
(`apt-get install mesa-vulkan-drivers` in the container).

## 5. Crash attribution for f32::clamp

`f32::clamp` panics on NaN or reversed bounds inside
`core/src/num/f32.rs` and is not `#[track_caller]` (checked: a NaN bound
reports `library/core/src/num/f32.rs:1432:9`), which is why the Windows
firefight crash report cannot name its caller.

Fix: `bri_console::Clamp::clamped` (bri-console is the dependency-free
crate with the process log), `#[track_caller]`, implemented for f32 and
f64: identical to `clamp` for ordered bounds; otherwise one console warning
per call site naming the caller's file:line:column and the operands, a
defined value (each bound that is a number applies, lower first), and a
`debug_assert` so tests panic at the caller. The root `clippy.toml`
disallows `f32::clamp` and `f64::clamp` (verified clippy 1.93 accepts
primitive paths); clippy cannot tell constant bounds apart, so every float
clamp in client, render, ui, motor, vehicles, fx-runtime, audio, weapons
and physics (all targets) is migrated, found by the compiler (integer and
glam clamps keep `clamp`). Crate roots outside those nine that still call
`.clamp(` carry an explicit
`#![allow(clippy::disallowed_methods, reason = ...)]` until migrated:
addon-import, admin, audio-import, bls, chaos (+ its bot_navigation_spike
test), client-sandbox, content, convert, events, foliage, net,
package-runtime, progress, sim (+ tests/pong.rs), ui-import, weapons-import,
weather. Next: migrate content, sim, client-sandbox, weather and foliage,
which also run in the client process.

Guard: `bri-console` `clamp::tests::a_nan_bound_reports_the_callers_location_once`
(the panic names this test's own file and line; the site is recorded
once) and `ordered_bounds_clamp_as_the_standard_library_does`. Main has no
such helper.

## Commands

All with `CARGO_TARGET_DIR=/home/claude/bri-target CARGO_PROFILE_DEV_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`:

- `cargo test -p bri-client --lib -- three_full an_out_of_bounds a_movement_fault`
- `cargo test -p bri-client --test horse_riding_render`
- `cargo test -p bri-sim --test prediction`
- `cargo test -p bri-console --lib`
- `cargo clippy --all-targets -p <touched crates> -- -D warnings`

Every before/after result above was re-run with a per-package
`codegen-units = 23` override for every workspace package
(`cargo --config <file>`, not committed), so the shared target dir could
not hand this tree another worktree's bri-* artifacts: worktrees of one
repo share artifact hashes and dep-info paths are workspace-relative, and
twice an earlier build here linked another branch's bri-package and
bri-console.

Results with that config: `bri-client --lib` 445 passed (the two
icon-drawing tests need `<checkout>/target/` to exist for their PNG and
pass once it does; they fail in any checkout built with an outside
`CARGO_TARGET_DIR`, unrelated to this branch); `horse_riding_render`, the
sim `prediction` suite, and the lib tests of console, motor, physics,
fx-runtime, audio, weapons, vehicles, render and ui all pass. Clippy
`--all-targets -D warnings` is clean for the nine migrated crates, console,
sim and every crate that got the allow attribute.
