# Current context before Add-On camera callbacks

Design checkpoint `936f84a4d8d3a87046705624041963eee5af6767` on
`codex/camera-snapshot-design`, based on
`c8998ac1652b60129ba25c07de22c64404197122`. Root subsequently approved the
bounded implementation on `codex/camera-snapshot-fix`; the investigation and
proposal below preserve the reasoning and original failing requirements.

## Implemented phase contract

`ClientCode::begin_camera_frame(now, stock_world, view)` advances one draw clock,
transfers pending captured look once and binds the batch to existing owners.
`AddOn::camera_context` installs current World/View/controls without executing
Wasm or clearing outputs/inbox/body state. `camera_shot` invokes camera callbacks
once; `run_frame` later uses the same timing/input batch and one ordinary frame
callback. Frame-only callers perform preparation internally. Replacing an
abandoned preparation, stop and device loss discard its batch. Claim resolution
can remove prepared grants but cannot add input for a newly acquired owner.

An additive consumer rerun follows the new App order using the same `now`:

```rust
code.begin_camera_frame(now, stock_world, underlying_view);
let shot = code.camera_shot(stock_eye, stock_forward);
// Derive resolved eye/forward/world from that shot or stock fallback.
code.run_frame(now, resolved_eye, resolved_forward, resolved_world, underlying_view);
// Advance the consumer's monotonic time once, after the draw.
```

`stock_world` is a complete current snapshot with actual synced held frames;
`underlying_view` describes stock mode, not whether the Add-On returned a shot.
The original old-order stale receipt remains historical evidence.

App builds the full stock World before camera selection from actual current
presented actors, vehicles, drawn life/skeletons, entities, state, passages,
synced held transforms and immutable image geometry. Authored/live environment
is resolved once and applied through the same closure to stock and resolved
cameras. The later World retains that current presentation and receives the
resolved camera environment in a separate snapshot; no native pose, item sync,
physics, culling or upload runs twice. View is shared by both phases, using
drawn controls and current local life with underlying stock mode independent
of previous Add-On camera selection. Existing renderer visibility and stock
held placement remain unchanged. App frame.rs/view.rs production is untouched.

The private Source evidence is PR 30, immutable
`0c970aba271039718a89e927aa70b19dac0cf1a0`,
`source/notes/2026-10-16-positive-claims-and-snapshot-gap.md`.
Its real package consumer acquires LOOK only with a live first-person local
actor, matching image, held transform and accessible image geometry. Positive
arbitration does not close the observed same-draw camera freshness failure.
No original package code, recovered assets or Source camera laws are included
in the public fixture.

## Observed order and the dependency

At this base, `app/frame.rs` poses bodies once, computes the stock view with
`view_camera`, and calls `WorldItems::sync` using that eye and the current
body/mount transforms. `held_images` reads those synced transforms;
`held_image_meshes` builds immutable image-local geometry, not camera-projected
geometry. Neither accessor advances image playback. The proposed camera phase
must preserve those real current transforms rather than patch current selectors
onto a retained previous-frame World.

`App::render_frame` then does the following:

1. It selects `third_person` from drawn controls, vitals and the previous
   `self.view.addon_camera`, and obtains stock eye/yaw/pitch/roll.
2. `ClientCode::camera_shot` invokes actual `AddOn::camera_frame`. The sandbox
   refreshes only stock camera eye/forward and clears the previous Shot. World,
   View and Captured remain from the last `AddOn::frame` call; ClientCode time/dt
   also remain the previous step's.
3. The selected shot drives the final camera, projection, environment,
   culling/reflections and GPU body work.
4. It builds a World through `world_view`: current presented actors/vehicles,
   package state, ghost entities, current held transforms/image-local meshes,
   drawn life/skeletons, passages and environment from that resolved camera.
   `ClientCode::run_frame` advances its clock, transfers pending captured input,
   invokes each actual frame callback once, collects sounds/poses/physics
   commands, and resolves resulting claims. The callback's poses affect the
   next normal body pose, as before.

Consequently camera re-querying `view`, `players`, `life`, `held` and
`look_delta` cannot observe this draw's changes. The later frame callback
releases LOOK on death/unequip/mode exit, but the stale shot already selected
for this draw survives. Stop and device-loss paths clear claims immediately.

There are two distinct cycles to avoid hiding: final camera environment and
visibility are resolved after camera control; `addon_camera` feeds the next
render's third-person selection. Moving all of `run_frame` before camera would
change the frame callback's camera/world and pose/sound/physics timing. Calling
it twice would duplicate observable work. A selectors-only refresh would leave
`held` and world queries disagreeing with those selectors.

## Proposed bounded split for review

Retain a pre-camera **stock presentation snapshot** and the existing later
**resolved presentation snapshot**, with their phases explicit in the host API
documentation. These are complete, coherent Worlds, not mixed-age records.
Camera callbacks observe the current stock presentation that their `camera`
argument describes. Frame callbacks retain the current resolved camera
environment and existing timing of their commands. A camera callback cannot
read transforms depending on the camera it has not yet returned.

Before `camera_shot`, construct the complete World from the current presented
state, actual already-synced held transforms, immutable image meshes, drawn
body life/skeletons, entities, passages and public package state. Build its
environment using a stock Camera with the same authored/live environment
application, without culling, uploads, input, simulation or callback execution.
The stock snapshot's held transforms describe the existing `WorldItems::sync`
result; they do not claim to have been reposed through the future Add-On shot.
The later World remains a separate resolved snapshot. If product requirements
instead require final-shot-relative held placement, that needs an explicit
post-camera item presentation split and its own tests; do not manufacture it
in the freshness fix.

Construct current View once from drawn controls, frame size, actual stock mode,
aim/FOV/sway and current local life, and share it with both phases. Explicitly
separate underlying first-person eligibility from an Add-On-selected camera:
the previous `addon_camera` flag must not turn its own current eligibility off.
Death has current View.alive=false even if an interpolated corpse snapshot
still represents its previous life; World players retain the existing
documented drawn-life semantics. Consumers must inspect View and actor life,
not be given an undocumented rewritten corpse. Re-check exact control sources:
the current code mixes `drawn_controls` for stock camera with `self.controls`
for View FOV/aim/sway; the fix must use one current presentation source.

Proposed mutable paths and ownership:

| Path / owner | Bounded change proposed |
|---|---|
| `client/src/client_code.rs`, ClientCode | Add an explicit begin-camera/draw preparation taking `now`, stock eye/forward, full stock World and current View. Transfer pending look to a draw batch once and advance time/dt once. Preserve per-owner grant resolution. Camera callbacks receive this batch; later `run_frame` consumes the prepared timing/input rather than advancing or draining again. Existing frame-only callers retain a single begin+frame path. |
| `client-sandbox/src/host.rs`, AddOn/HostState | Add a non-executing context refresh, or pass a context record into `camera_frame`, to install current World/View/controls before invoking Wasm. This must not clear frame output, consume messages, execute `frame`, reset local bodies or run physics. No import signature or capability changes are needed. |
| `client/src/app/render.rs`, App adapter | Assemble stock snapshot/View before camera, prepare ClientCode once, select shot once, then retain resolved render work and one later frame dispatch. Reserve any shared snapshot-builder extraction explicitly. |
| `client/src/app/view.rs` and `app/frame.rs`, App presentation | Read-only investigation shows the underlying stock-mode and held-pose inputs here. A grant is required if the chosen implementation needs to change these paths; existing body/item stepping must not run again. |

This table records the reviewed proposal. Root's implementation grant covered
render.rs, client_code.rs and host.rs; frame.rs/view.rs remained read-only.

The prepared batch owns draw time, dt and captured input. `now` remains the
existing monotonic avatar animation time, with the same 0..0.25 elapsed clamp.
The draw owns a single time advance; repeated context reads cannot advance it.
Look is a read-only value within that draw, zero on the next draw without new
input. Providing it to two callback phases is not permission to integrate it
twice; the fixture consumes it only in its camera callback and guards its own
application by frame generation. Input held across a control-owner transfer
must retain the current resolver's clearing behavior. Re-resolve after camera
traps/releases, and before dispatching frames, without assigning a captured
batch to a newly acquired owner. No new arbitration or priority rules.

Stop, reconnect, device loss and aborted preparations clear pending draw data.
No next draw may inherit a retired session's timing or input. A preparation
without camera exports still dispatches one frame; no running Add-Ons performs
no callbacks. Stock/conflicting cameras preserve user projection. Physics
advancement remains at its existing App update owner, independent of either
snapshot construction. Frame side effects, inbox delivery, native animations
and rendered avatar overrides still happen once through their current paths.

## Public regression contract

`crates/client/tests/camera_snapshot_freshness.rs` builds invented Wasm bytes in
memory, loads real package manifests through `ClientCode::load/start`, and
executes actual sandbox imports/callbacks in the production preparation,
camera, then single-frame API order. It does not substitute a Rust callback or
manually refresh HostState. The original checkpoint was a boundary reproduction;
the fix also includes a real hosted App/offscreen render case.

At the design checkpoint, green controls covered both unique identities, empty
stock, two claimants in both orders, absent actual mesh, delayed delivery without
replay, stop/reconnect, device loss, eye position and no FOV override. Explicitly
running the two ignored strict requirements reproduced delayed look and stale
death/unequip/first-person-exit shots. That checkpoint remains separate evidence;
its deliberately red state must not be integrated without the correction.

The fix enables both strict requirements without ignores and adds abandoned
preparation, release, trap and replacement-owner controls in both package
orders. An invented telemetry variant reports actual Wasm time/dt and completed
frame count through its controlled camera roll, without a production hook.
The real hosted App/offscreen case uses actual loaded/posed hammer geometry,
consecutive underlying first-person draws, current look, mode exit/reentry,
unequip, death, Eye/held placement, user FOV and single clock/frame advancement.
Scoped evidence and any fixture corrections are recorded in
`../progress/2026-10-16-camera-snapshot-fix.md`.

Existing native vehicle-camera and held-item render targets provide additional
independent regression coverage. The actual Wasm frame counter checks one
callback side effect per draw; physics retains its existing App update owner,
and the sandbox physics/showcase tests remain part of focused validation.
Root supplies an exact fix SHA for the private Source consumer to rerun.
Original GMod camera profile, movement and original repeat/promotion rules
remain separate source-owned work.
