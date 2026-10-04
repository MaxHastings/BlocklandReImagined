# 2026-10-03 v0.2.3 vehicle wrench Send color

Maxwell reports that changing a vehicle spawn brick's color and pressing Wrench
Send leaves the existing vehicle's old color until Respawn. Root assigned this
lane read-only production ownership, with a reviewable patch handed back for
integration. No shared source, manifests, schemas, original content, commits,
interactive controls or visible game windows were changed by this lane.

## Cause and intended behavior

`Session::tool_action` validates equipment, the actual inspected brick,
inspection freshness, build trust, the tool catalog and item-spawner settings
before applying `Edit::Properties`. It marks the brick dirty but never reapplies
vehicle paint. `reconcile_vehicle_bricks` compares the selected definition to
the existing linked vehicle and immediately continues when those identities
match. `spawn_color` is consequently used only at spawn: toggling Re-Color
Vehicle or sending again after a brick paint change cannot update the current
vehicle. This is a concrete code-path proof matching the user's report;
headless runtime confirmation is pending.

The proposed patch calls a small `color_vehicle_brick` adapter only after a
successful Wrench Send. It uses the existing canonical `spawn_color` function
and updates only the linked vehicle's color entry. It does not call spawn,
remove, recover, mount, physics or pose methods. A change of selected vehicle
kind still follows the existing reconciliation path; it does not transiently
paint the old kind. Invalid/stale/unauthorized requests retain their existing
failure paths and never reach this color application.

Sending Re-Color Vehicle on applies the current brick's palette RGB, opaque.
Sending it off stores `None`, using the same existing semantics as a newly
spawned unrecolored vehicle: the client's original model/material appearance.
This off behavior is inferred from the existing native spawn/render contract,
not a newly verified original-v20 interactive comparison. An unchanged setting
sent again deliberately reapplies the current brick color. Unrelated dirty
brick edits leave independent Fill Can RGB paint intact; changing the generic
reconciliation loop to repaint every dirty brick would break that existing
behavior.

## Replication and proposed regressions

`Session::vehicle_infos` reads the same color entry into `VehicleInfo`. The
network server compares this reliable-on-change vector with its previous value
and includes changes in `Delta.vehicles`. Replica admits finite paint and
updates its vehicle map. Client `ClientVehicles::prepare` reads the current
`VehicleInfo` each frame and derives body/attachment tint with `body_tint`;
`None` uses the original material and destroyed vehicles retain their wreck
appearance. No wire/schema or client production changes are necessary.

Reviewable patch: `/tmp/bri-v023-vehicle-color.patch`. Tests-only extraction:
`/tmp/bri-v023-vehicle-color-tests.patch`. Original/proposed mirrors are under
`/tmp/bri-v023-vehicle-color-{original,proposed}`.

The proposed sim regression runs both the synthetic fixture and the established
ignored native fixture. It boards a rider through ordinary walking/jumping,
opens each wrench through an ordinary delayed click at the spawn's exposed
corner, and submits `ToolAction::SetWrench`. It exercises on -> off -> on,
brick paint followed by another same-setting Send, and independently painted
vehicles through an unrelated dirty edit followed by deliberate Send. It
asserts exact unchanged vehicle identity, definition, occupancy, scale,
destruction and every replicated pose field across each Send. Catalog rejection
must preserve both info and poses. The authoritative info is also checked
through its ordinary serialization boundary.

A client CPU regression reuses one live vehicle and one pose, changes the
reliable paint between red, original material and blue, and inspects the actual
prepared model-instance tint. It loads an existing synthetic fixture and needs
no GPU or window.

## Verification and remaining work

- `rustfmt --edition 2024 --config skip_children=true` completed on all four
  proposed files; `on_both!` macro bodies follow their existing format.
- `git apply --check /tmp/bri-v023-vehicle-color.patch` passed.
- No Cargo command was run; root owns the compute lease and before/after runs.

Root can apply tests alone and run
`cargo test --locked -p bri-sim --test vehicles wrench_send_recolors_the_existing_vehicle_without_respawning -- --nocapture`.
A baseline failure must be the paint assertion, not a mount/inspection fixture
failure, before counting it as reproduced. Then apply the production correction
and repeat, including `--include-ignored` for native packs when available. Run
`cargo test --locked -p bri-client a_live_vehicle_updates_its_draw_tint_without_a_new_pose_or_identity -- --nocapture`
for the CPU presentation boundary. Relevant complete vehicle/fill-can tests,
strict Clippy and the release gate remain root-owned. Maxwell's in-game Send,
recolor toggle and occupied-vehicle acceptance remain open.

## Root before/after runtime evidence

Root's tests-only baseline `/tmp/bri-v023-vehicle-color-before.log` reaches the
intended assertion with the rider occupied: Send requests color `None`, but the
same id-1 vehicle retains its red color. Identity, definition and occupant data
match. This is the reported paint defect, not a fixture setup failure.

After root applied the production correction, the focused run with
`--include-ignored` passed **2/2**, both synthetic and generated native content,
in `/tmp/bri-v023-vehicle-color-after.log` (0.22 s runtime). The scenarios include
on/off, same-setting Send after brick paint, independent RGB preservation through
an unrelated edit, invalid catalog rejection, and exact physical/occupancy
preservation. Client presentation verification is being run separately by root.

Root's focused CPU client regression with `--lib` also passed **1/1** in
`/tmp/bri-v023-vehicle-client-lib.log` (0.01 s runtime). It observes actual
prepared model tint changes on the same vehicle identity and pose. This lane
read the result; no GPU/window was used by this regression.
