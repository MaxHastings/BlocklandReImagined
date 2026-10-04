# 2026-10-04 Three gate failures under load, fixed at the cause

Each of these failed once in a full gate run on Maxwell's loaded PC and passed
when run alone.

## Gravity Gun stopped as "too heavy" (`showcase::the_gravity_gun_renders_offscreen`)

"its shaders would take about 1737 ms a frame on this graphics card". A
product bug, not just a test one. `gpu::calibrate` timed each calibration
pass by the wall clock (submit, then wait) and stopped at the first loop cap
whose pass took 10 ms. Load only ever adds time. A busy GPU, or this thread
waiting to be scheduled, could stretch a pass at cap 3 (well under a
millisecond of real work) past 10 ms. Calibration then stopped there and took
the GPU to be dozens of times slower than it is. `LayerRenderer::prepare`
refused any Add-On whose estimate went over 500 ms. A player whose game
calibrated during startup shader compiles, or while another program used the
GPU, could have had the Gravity Gun effects stopped the same way.

Fix (`crates/client-sandbox/src/gpu.rs`):
- Where the device has `TIMESTAMP_QUERY`, calibration passes are timed with
  GPU timestamps. Waiting behind other work and thread scheduling are then
  not counted. Without timestamps, or when a driver writes none, the wall
  clock is still used.
- `speed_from_passes`: a pass long enough to stop calibration is timed again,
  up to three times, and the fastest time counts. A single slow sample can
  no longer end calibration early. Passes below the threshold are timed only
  once, so calibration costs almost nothing extra.
- `too_heavy`: before refusing, `prepare` measures the GPU again (up to twice)
  and keeps the fastest speed seen. Only shaders still over the limit at that
  speed are refused. Genuinely heavy shaders are still refused, as before.
  The renderer keeps the corrected speed, so later frames' loop caps use it
  too.

Unit tests of the decision logic (no GPU):
- `one_pass_slowed_by_load_does_not_make_the_gpu_look_slow` adds 40 ms to the
  first pass with a loop. It fails with one sample per pass, the old
  behaviour: 1.6e5 measured against 1.0e7 real.
- `a_refusal_from_a_busy_measurement_is_measured_again` fails without the
  re-measure: `Some(2000.0)`, not `None`.
- `shaders_too_heavy_at_the_fastest_speed_seen_are_still_refused` and
  `a_failed_pass_fails_calibration` check the protection that remains.

On llvmpipe here, the timestamps were confirmed to be written (temporary
debug print, then removed). Calibration took 112 ms.

## Host nose under load (`vehicle_first_person::invert_mouse_in_vehicles_..::content`)

"invert None: host nose +0.005, predicted view -0.071". The hosted server
ticks on the wall clock and runs one of a seated driver's moves each tick.
When the next move is late, it repeats the last one (`SeatedPace`), and a
repeat turns nothing. The test produces the push in client time: 60 moves,
two per look. It holds the client only until the host has taken all but 4
of them, so the host has about 33 ms of moves queued. On a loaded machine
the test thread could not always send moves as fast as the host ticked. The
host then ran the push over more ticks than moves. Its accumulated mouse
steering at the last move was the same: the push's turns added up. With the
shipped prefs auto-return steering is off, and a wheeled jeep has no flight
damping. Its nose was different. A push stretched over more ticks dips the
nose sooner while the jeep skims the ground at take-off speed. The nose can
then strike the ground and bounce back up by the last move. The client
predicts one tick per move, so its view moved the same as in a quiet run.

Fix (test only, `crates/client/tests/vehicle_first_person.rs`): the host
check reads `VehiclePose::mouse_steering[1]` across the push's moves.
Positive steering dips the nose (`VehiclesWorld::step`). This is what the
host does with the inverted or plain mouse, and it does not depend on host
pacing. The predicted-view check is unchanged. The host's nose change is
still printed, but no longer checked. Making the host's own nose physics
deterministic would need a stepped clock for the hosted server (a `net`
seam). That is not built here.

The content variant was not run: there is no generated content in this
container. The synthetic variant was compiled.

## Bare "The system cannot find the path specified" (`default_add_ons`, `minigame_screens`)

The cause was environmental: the main checkout's `content/addons` was being
restored during the run. Behaviour is unchanged. The bare errors came from:
- `default_add_ons::bundled_shared_geometry_hosts_and_real_doors_change_footprint`:
  `copy_dir`'s `read_dir(from)` on `<content>/addons/<id>`, and the other
  file calls in `copy_dir`, `stand_in_as`, `Checkout::new` and the test body.
  These now name their path.
- `minigame_screens::source_minigame_tasks_render_offscreen`:
  `std::fs::read(content/addons/gamemode_slayer-rules/behaviour.json)`.
  That read, the artifacts folder and the PNG writes now name their path.

## Commands

With `CARGO_TARGET_DIR=/home/claude/bri-target CARGO_PROFILE_DEV_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`:
- `cargo test -p bri-client-sandbox`: all passed, including the GPU tests on
  llvmpipe (`the_gravity_gun_renders_offscreen`,
  `calibration_measures_the_gpu_quickly`).
- `cargo clippy -p bri-client-sandbox --all-targets -- -D warnings`: clean.
- `cargo clippy -p bri-ui --test minigame_screens -- -D warnings`: clean.
- `cargo fmt --all`.
