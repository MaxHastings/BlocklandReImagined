# Demo Pong events audit

Date: 2026-09-28. Branch `claude/pong-events`.

v20 ships a default Bedroom save, `saves/Bedroom/Demo Pong.bls` in the
reference install: a two-player Pong game built only from wrench events. It
has 148 bricks, 43 named bricks and 277 event rows. We ship it converted as
`content/worlds-pass-005/8a3130ab...c05.world.json` ("Demo Pong"). This audit
uses it to stress the event engine end to end.

**Result:** on `origin/main` the game did not work. Reset and serve worked,
but the ball vanished one tick after it was served, so nobody could score.
After the fixes below, a headless match plays like v20. Paddles rally, each
side scores, the first to 10 wins, the bumpers stop play and reset starts a
new game.

## How the build works

The court is a vertical plane. Each side has a column of six 1x1 paddle
cells. One cell per side is white (the paddle). The other five are black,
and a ball hitting a black cell scores for the other side. Black cells have
rows 0-4 on: `onProjectileHit` increments the opponent's score, explodes the
ball and re-serves 33 ms later. White cells have rows 5-6 on instead, so the
ball just bounces. `onRelay` on a cell toggles rows 0-6 and recolours it.

Each side's `+` and `-` print bricks relay (33 ms) to five `_pong_AU*` or
`_pong_AD*` bricks. Exactly one of them has its rows on. It toggles the two
cells the paddle moves between, then turns its neighbours on and itself off.
That state machine remembers the paddle position. The reset ramp zeroes the
score counters, disarms the bumpers, turns the win lights off and serves
after 2 s. A counter going past 9 wraps to 0 and fires
`onPrintCountOverFlow`. That lights the winner's alarm light and arms the
floor bumpers, which delete the next ball.

## Event table

Every row parses, imports and binds: 277 of 277 rows are runnable in the
conversion report, and installing the world logs no diagnostics.
"Registry" means `content/events-pack-002/catalog.json`.

| Event | Rows | Targets | Parameters | Registry | v20 behaviour | Status |
|---|---|---|---|---|---|---|
| input `onActivate` | 52 | Self, Named | none | yes | Click fires rows in order. Zero-delay `cancelEvents` runs first as a prepass. | OK |
| input `onRelay` | 181 | Self, Named | none | yes | Every row is `schedule`d, even at 0 ms, so it runs after relays that are already due. The AU/AD state machine depends on this. | OK |
| input `onProjectileHit` | 38 | Self, Named, Projectile | none | yes | Fires on every contact, including bounces of the unarmed ball. | Fixed (Explode, below) |
| input `onPrintCountOverFlow` | 6 | Named | none | yes | Fired by `incrementPrintCount` past 9, after it wraps to 0. | OK |
| `fireRelay` | 85 (73 Named, 12 Self) | Named group, Self | none | yes | Relays to every brick with that name in the owner's group. v20's 15 ms relay flood gate is intentionally not modelled (see `crates/events/README.md`). No Pong relay comes close to it. | OK |
| `setEventEnabled` | 83 (63 Named, 20 Self) | Named, Self | row list `0 1 2 3 4 5` or `0`; bool | yes | Applies to later activations, not jobs already captured. | OK |
| `toggleEventEnabled` | 12 | Self | row list `0 1 2 3 4 5 6` | yes | Same. | OK |
| `setColor` | 24 | Self | palette 15 (white), 16 (black) | yes | Server-wide brick change. | OK |
| `setColorFX` | 36 (32 Self, 4 Named) | Self, Named | 0, 3 | yes | Server-wide. | OK |
| `cancelEvents` | 4 | Self | none; delay 0 (3 rows) or 100 ms (B's `+`) | yes | Cancels the brick's pending delayed rows. | OK, authentic quirk below |
| `incrementPrintCount` | 12 | Named | 1 (clamped 1-9) | yes | v20 `getPrintCount` starts from the digit the brick shows. | Fixed |
| `setPrintCount` | 2 | Named | 0 | yes | Sets the counter and its digit print. | OK |
| `setLight` | 4 | Named | `AlarmLightA`, `-1` (none) | yes | Server-wide light change. | OK |
| `spawnProjectile` | 1 | Self (`_pong_Serve`) | velocity `0 5 1`, `pongProjectile`, variance `0 1 0.5`, scale 1 | yes | Spawns at the centre of a thin brick, with uniform variance. | Fixed (ball escape) |
| Projectile `Explode` | 12 | Projectile | none | yes | Explodes the ball on the spot with `pongExplosion` and its score sound. | Fixed |
| Projectile `Delete` | 2 | Projectile | none | yes | Removes the ball silently. | OK |

The build uses four delays: 0, 33, 100 and 2000 ms. All are honoured.

**Per-client vs server-wide.** Every Pong output targets a brick or the
ball, so every change is server-wide: colours, prints, lights, row enables
and the ball are shared by all players. No row uses Client or Player
targets. One difference is kept on purpose. v20's `ProcessInputEvent` needs
a client, and the ball inherits the client of whoever pressed reset. If that
player leaves, v20 stops running the ball's hit events, so the game
freezes. Ours keeps running the game.

## What was broken and what changed

1. **The ball vanished right after the serve.** `spawnProjectile` starts
   the ball at the centre of `_pong_Serve`, a brick in the court floor. Our
   projectile ray began inside that brick. That gave a zero-length hit with
   no normal, and the weapon runtime deleted the ball as an invalid
   collision. Projectile rays now ignore a brick they start inside
   (`crates/sim/src/weapon_query.rs`). This is like Torque, which ignores
   the projectile's `sourceObject` (the serve brick). The ball now bounces
   off the neighbouring bumpers' side faces, rises out of the floor and
   crosses the court. The exact Torque ray behaviour inside bricks is
   inferred from the fact that v20's shipped demo plays. It is not read from
   engine source.
2. **A miss never scored.** Projectile `Explode` from a zero-delay
   `onProjectileHit` row was ignored, so the ball kept bouncing off black
   cells. `bri_weapons::ContactResponse::Explode` now explodes the
   projectile at the contact.
3. **Paddle moves reached the ball a tick late.** The projectile responses
   of a brick were rebuilt only on the next tick after a toggle. They are
   now rebuilt when the event phase changes the brick's rows.
4. **Delays started one tick early.** Inputs fired inside a tick, such as
   ball hits and touches, took their start time from the previous tick. A
   33 ms re-serve therefore ran after 25 ms. `EventWorld::set_clock` now
   moves the event clock to the current tick before gameplay fires inputs.
   The re-serve is now 4 ticks (33 ms), as in v20.
5. **Loaded score counters restarted at 0.** A converted print such as
   `Letters/4` is stored unresolved, and the counter read only resolved
   digit prints. It now reads both, like v20's `getPrintCount`.

## Colours that stayed changed (2026-09-28, `claude/pong-colour`)

Maxwell reported that using the paddle buttons changed brick colours and
some changes stayed. In v20 they always came back. Headless, the server
state was right for clicks at least a tick apart. It broke when two clicks
landed in one 120 Hz tick, as queued clicks do after a server hitch. Both
clicks then had the same timestamp, and so did everything they scheduled.
A `+` and a `-` relay reached the `_pong_AU*`/`_pong_AD*` state machine
together. Both relays used the rows as they were before either one
switched them. Both moved the paddle, and a shared cell was repainted twice
but toggled back. The result was two white cells, or a white cell with
black rows. The same happened to two B `+` clicks in one tick, because B's
cancel is late. The old scheduler also gave origins round-robin turns. That
could run a later click's row ahead of an earlier click's due row.

v20 stamps every input with its own millisecond and runs every `schedule`
from one queue in time order. The event engine now does the same, with one
rule for every brick event (`crates/events/src/runtime.rs`):

- Jobs run earliest due first, then by the activation's order inside its
  tick, then by scheduling order. Origins are only budget and deferral
  boundaries.
- Each `trigger` is its own instant. Everything it schedules keeps that
  instant, directly or through relays, prints and chains. Chained rows are
  scheduled from their parent's due time, not the later phase clock.
- A `cancelEvents` at instant *t* removes the source's delayed rows that
  were scheduled by *t* and are due at or after *t*. A late cancel no
  longer reaches rows scheduled after it. The zero-delay prepass no longer
  cancels rows that v20 had already run.
- A zero-delay chain past `loop_warning_depth` queues behind everything
  else at its instant. Independent origins still interleave with a runaway
  loop.

Nothing was wrong in saves, `setColor` or the revert rows themselves. A
timed revert of each output (`setColor`, `setColorFX`, `setRendering`,
`setColliding`, `setRayCasting`, `setLight`, `setEmitter`) on one brick,
clicked in overlapping bursts with a late `cancelEvents`, always landed,
before and after the change. The fault was ordering between activations.
Job checkpoints gained two defaulted fields (`scheduled`, `order`) and the
world gained `next_order`. Older checkpoints still load.

Tests:

- `hammered_paddle_buttons_restore_every_colour` makes 60 rounds of 1 to
  13 clicks on all four buttons. Clicks are a tick to 133 ms apart, and a
  quarter of them are doubled into one tick. After every step each cell's
  colour, glow and rows must agree. After each round no button glows. The
  paddles are then walked home, and every brick must be on its loaded
  colour. It fails on the old scheduler (round 3: a B cell goes black with
  white rows) and passes now.
- `timed_reverts_of_every_brick_output_always_land` covers the seven
  outputs above.
- `activations_in_one_tick_keep_their_order_through_relays` and
  `late_cancel_and_revert_run_in_time_order_across_activations` are in
  `crates/events/tests/runtime.rs`. The first fails on the old scheduler.

## Authentic quirks kept

- B's `+` button has its `cancelEvents` row on a 100 ms delay. Every other
  button cancels immediately. So a second `+` click on B within 100 ms is
  cancelled by the first click's late cancel, and the paddle moves once.
  v20 behaves the same.
- The save was made just after B won 10-4. B's counter shows 0, B's win
  light is on and the bumpers are armed. Pressing the reset ramp starts a
  new game.
- `pongPaddleHit.wav` ships in `Projectile_Pong.zip`, but no datablock uses
  it. Paddle hits play the wall sound, as in v20.

## Evidence

`crates/sim/tests/pong.rs` loads the converted save into a full `Session`
and plays it headless. It needs the local content packs, like the other
ignored content tests. It checks the following:

- **`demo_pong_plays_like_v20`:** the loaded post-win state; reset; a serve
  at 2 s; the ball leaving the floor. A 12 s rally with both paddles blocking
  gives no points and at least 3 paddle hits per side. When A stops
  blocking, B scores, the ball explodes with the score sound and the
  re-serve comes 33 ms later. B stops blocking and A scores. A reaches 10:
  the counter wraps, the win light turns on, the bumpers arm and the next
  ball is deleted. Reset then serves again.
- **`paddle_buttons_move_one_cell_and_stop_at_the_ends`:** each click moves
  a paddle one cell and it stops at both ends. A double click inside 33 ms
  moves it once.
- **`b_up_swallows_clicks_inside_100_ms`:** the quirk above.
- **`loaded_counter_counts_from_its_print`:** the counter continues from 4.

Commands (`CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`):

```
cargo test -p bri-sim --test pong -- --ignored
cargo test -p bri-sim --test events_native -- --ignored
cargo test -p bri-events
cargo test -p bri-events -p bri-weapons -p bri-sim
cargo clippy -p bri-events -p bri-weapons -p bri-sim --tests
```

All pass. Maxwell's interactive check is to load Bedroom, load
"Demo Pong", click the ramp by the court and play with the `+`/`-` buttons.
