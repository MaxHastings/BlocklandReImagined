# Splitting the client `App`

Status: the move is done on `claude/code-health-audit-eui8bq` and lands last
in v0.1.11, after the Tier and Adventure lanes, so no lane merges against it.

## Why

`crates/client/src/app.rs` is about 10,400 lines. `App` has about 157
fields, 59 of them `Option`, and four functions do most of the work:
`tick` (1,250 lines), `render_scene` (1,160), `pump` (880) and
`poll_network` (870). Each of them can write any field, so two systems can
quietly undo each other within one frame. That has already happened twice:

- `follow_control` calls `Controls::follow(Player)` every tick, which drops
  the Fire that `pump` recorded. `update_held_weapon` then never sees Fire
  held, so the Gravity Gun's scroll reel broke (`controls.rs:455`,
  `app.rs` tick step 7 before step 10).
- Switching into the Tank's gunner seat aims the turret, and later in the
  same tick `set_ride(None)` and `set_vehicle_view(None)` overwrite that aim
  (`controls.rs:309`, `controls.rs:340`).

A third bug comes from the same shape. `poll_network` takes `attempt` out of
`App` for its whole run, so anything it calls that asks `App` for the view
gets `None`. `queue_weapon_cue` asks for the listener's position to apply
the 80-unit caption earshot, so in practice every caption shows at any
distance (`app.rs` `caption`, called from `poll_network`).

## Target shape

`app.rs` becomes `app/`. Each system is a struct that owns its own fields.
It gets the other state it needs as explicit borrows (`&View`, `&mut Ui`,
its outputs), never `&mut App`. `App` keeps the systems and one
`frame()` that calls them in a fixed order. Only the system that owns a
field writes it. Another system that wants a change returns a request
(a small struct or enum), and the owner applies it at a named step.

| Module | System | Owns |
|---|---|---|
| `app/mod.rs` | `App` | the systems, `frame()`, `reset()` (calls each system's `reset`) |
| `app/session.rs` | `Session` | `attempt`, reconnects, pending actions, steering prefs sent, abilities; host/join/command/replies |
| `app/net_events.rs` | | `poll_network`, which returns a `NetFrame` (cues, notices, map change, scene ready, failure) instead of writing other systems |
| `app/scene.rs` | `SceneState` | CPU scene, chunks, world log and revision, query mirror, liquid cache |
| `app/gpu.rs` | `GpuState` | renderer, GPU scene, terrain, palette, chunk uploads, `gpu_ready`/`gpu_stopped` |
| `app/lighting.rs` | `Lighting` | light volume, reflections, environment probe |
| `app/view.rs` | `ViewSystem` | the only writer of `controls`; `drawn_controls`, observer eye, rendered camera and roll, crosshair and tool wheel |
| `app/mounts.rs` | `MountSystem` | `seated_on`, `mount_heading`, `tumble`, rider rotations, rider eye; returns a `SeatView` decision that `ViewSystem` applies |
| `app/avatars.rs` | `AvatarSystem` | avatar instances, actions, gestures, previews |
| `app/building.rs` | `BuildingSystem` | building, brick hand, ghosts, selection and hidden lines |
| `app/fx.rs` | `EffectsSystem` | weapon, actor and world effects, debris, fades, cue queues, world items |
| `app/addons.rs` | `AddOns` | package catalog, client code, server packages, import |
| `app/saves.rs` | `SavesSystem` | saves, file jobs, old saves, previews, save pictures |
| `app/lobby.rs` | `Lobby` | LAN hosts and query, update check, firewall fix |
| `app/perf.rs` | `PerfSystem` | frame stats and log, net sampler, auto quality, frame limit |
| `app/hud.rs` | `CombatHud` | combat presentation |
| `app/actions.rs` | | `pump`'s dispatch, one function per action group |
| `app/render/` | | `render_scene` as uploads, ghosts and lines, camera, reflections, bodies, Add-On frame, effects, passes |

Existing modules (`controls.rs`, `motion.rs`, `vehicles.rs`, `avatar.rs`,
`building.rs`, `effects.rs`, ...) stay where they are. The systems wrap them.

## Frame schedule

These are today's call sites, in today's order. The first move keeps this
order exactly, so behaviour does not change.

1. `perf.begin`
2. `lobby.poll_update_check`
3. clock advance, animation time
4. `session.poll()` returns a `NetFrame`; effects queue its cues; building
   and view apply its notices
5. `saves.poll`
6. `addons.poll_hud`
7. `view.follow_control`
8. `view.advance_prefs` (fly, FOV, invert)
9. `session.send_steering`
10. `view.update_held_weapon`
11. `view.advance` (zoom, view, head)
12. local motion step: move input, `motion.advance`, send movement, carries, impact
13. `mounts.present`
14. `mounts.predict_and_update_vehicles`
15. `mounts.seat_view()` returns a `SeatView`, and `view.apply_seat` applies it
16. `mounts.pose_mounts`
17. `mounts.seat_riders` (vehicle riders, then riders on players)
18. vehicle models and loose models
19. audio loops and music
20. `hud.update`
21. `perf.update`
22. Add-On import, LAN, firewall polls
23. macro playback
24. first-person flag
25. weapon-effect session reset
26. In game only: ghosts, liquids, avatar animation, rider eye, world effects
    sync, `view.camera` (listener), world items, effects advance
27. `audio.tick`
28. `view.snapshot` (what render reads)

`pump` stays its own event-driven system and runs after every input event.
Render reads only the snapshots taken at step 28.

## Order hazards: kept by the move, fixed after it

The move must not change behaviour, so it keeps these orders. Each one is
then fixed in its own commit with a test that drives the real path.

- **Steps 7 then 10.** Fire was cleared before it was read. This is fixed on
  main: `follow` clears Fire only when the player comes back from a
  camera (`ab3c8140`).
- **Step 15.** The turret aim was overwritten by the seat reset. This is
  fixed on main by the turret lane (`7ec124a5`).
- **Step 4.** Captions were filtered against a listener that was always
  `None`, because `attempt` is taken out. This is fixed: the presentation
  events take the listener from the attempt that `poll_network` holds.
- **Steps 4 and 25.** A session reset can drop cues queued in the same frame.
- **Step 26.** Add-On poses lag one frame (they are set in render and
  applied in the next tick).
- Render reads the live `controls` for the Add-On frame instead of the
  snapshot.
- `disconnect` did not reset `seated_on`, `mount_heading`, `tumble`,
  `posed_eye`, `rider_rotations`, `observer_eye`, `liquid_cache` or
  `drawn_controls`. This is fixed on main: `disconnect` clears them
  (`9360b544`). `crosshair_hidden` and `tool_wheel` are
  mirrors of what the UI was told, and `update_held_weapon` settles them
  every frame, so they are left alone.

## Where it stands

Done on the branch:

1. `app.rs` is now `app/`. Every `impl App` method moved by name into
   the module of the system that owns it, and the code inside did not
   change. `PlatformApp`'s `tick`, `pump` and `render_scene` call
   `frame`, `dispatch` and `render_frame`.
2. Most of App's fields are now 13 system structs: `SessionState`
   (`net`), `SceneState`, `GpuState`, `Lighting`, `ViewState`, `Mounts`,
   `Effects` (`fx`), `Avatars` (`avatar`), `AddOns`, `Saves` (`files`),
   `Lobby`, `Perf` and `BuildState` (`build`). App keeps one field per
   system plus the state several systems share: `ui`, `controls`,
   `content`, `audio`, `motion`, `vehicles`, `world_items` and others.
3. The two hazard fixes (`disconnect` resets the seat and the game's
   eyes; captions get the real listener) landed on main first
   (`9360b544`).

4. The giant functions are broken into steps, with no code changed inside
   them:
   - `frame` calls `advance_local_game` (schedule steps 12-19),
     `poll_background_jobs` (22) and `advance_world_presentation` (26).
   - `dispatch` runs each action through `intercept_action` (spectator,
     dead player, admin, macro and building input) and `dispatch_action`.
   - `poll_network` calls `drain_events`, `take_prepared_scene`,
     `enter_when_ready` and `present_session`. The early returns that drop
     the attempt stay in `poll_network`.
   - `render_frame`'s pre-draw work is `prepare_render`.

Steps 1 and 2 were scripts, run on main `39a1cbbe` and then deleted
(see the branch history for `tools/app_split/`).

Still open:
- `render_frame` (1,130 lines) holds the renderer, view and camera borrows
  across its whole body, so it splits only once those live in a per-frame
  context struct. `dispatch_action` (770, one arm per action),
  `advance_world_presentation` (650) and `advance_local_game` (485) split
  further the same way.
- Moving behaviour onto the systems, so that writes go through their
  owners.
- A session reset that drops cues queued in the same frame.
- Add-On poses lagging one frame.
- Render reading the live `controls`.
- Three vehicle camera implementations.
- Looping weapon sounds that the client's `is_looping` drops.

## How it lands

1. **Now:** this document, plus the refactors that need little of
   `app.rs`: host setup in `bri_net::host_setup`, vehicle drive state in
   `Motion`.
2. **On the final main** (the coordinator says when): the move, as a
   series of mechanical commits, one system each. Each commit moves the
   fields and the code that uses them, changes `self.x` to `self.sys.x`, and
   passes `cargo check`, clippy and the client tests. No logic changes.
3. **After the move:** the hazard fixes above, one commit each, each with a
   test that drives `App::tick`/`pump` (or the system it lands in) and
   fails before the fix.

Performance: the move only changes field paths. It adds no allocation,
locking or indirection to the frame, so the 1M-brick headline is
unaffected. The gate's perf probes confirm it.
