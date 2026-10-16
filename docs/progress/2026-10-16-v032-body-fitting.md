# 2026-10-16 v0.3.2 shared body fitting, Stage B

Root reviewed design `d644c0e94e2b8a863e5358ec240be2d9c20d211d` and granted
only content/src/body_pose.rs, client/src/avatar.rs, tests in their existing unit
modules and own docs/seam rows. Work remains on isolated
`codex/v032-engine-seams`, starting from public main
`3117e8c36c460f20bfc571700108890a9d4b7ad4`. No dirty Stage1 files or sampler
candidate were copied; no other checkout was mutated.

## Mechanical extraction

Content now supplies BodyFits/BodyFitInput/build_body_fits and
BodyHolds/BodyFittingInput/apply_body_fitting. It samples the same native
idle/own-hand-ready/level-look stack and uses existing hold fit/reach primitives.
Avatar caches the shared payload and supplies the actual for_mesh rig, resolved
roles, socket indices/offset and completed model frame. Full BodyKind equality,
cache invalidation, phase/channel/transition ownership and source selection stay
at their existing sites. Socket/action methods and their multiplication order
are unchanged.

Generic raising/support retains its early returns, nonfinite-pitch fallback,
corrected or raised right socket basis and left-seat override. Explicit world
seat reaches remain independent of that bypass, run right then left and use the
finite inverse of the completed root/seat/posture frame. Public inputs with bad
hand indices become absent operations; parent-count mismatch leaves nodes alone,
consistent with existing hold primitive bounds. Native validated rig ownership
remains the input contract; no new IK law or arbitrary cap is introduced.

The client keeps the no-IK fast path: stock/authored hands with no seat targets
do not newly allocate parent vectors. Its current thread2 named-action liveness
guard remains unchanged: nonlooping final-key channels may hold while support
becomes eligible again. A fitting helper does not manufacture animation results.

## Native characterization before moving production

Added three source-free tests to the existing Avatar module using its real
own_body package loader and native pose methods. Before any production edit:

- Native thread2 before/at/after the endpoint and looping, Both-hand support,
  nonzero authored right socket offset, and independent mount_node-based support
  target comparison. Final action channels remain distinct from unacted nodes.
- Fitted and authored-hand variants with world seat targets, scales .5/1.7,
  translated feet, seat yaw/roll and explicit fallback root shift. Both seat
  reaches match independent primitive calls in the completed inverse frame;
  nonfinite targets are ignored.
- Same-model actual ready-role alias, missing socket and stand-in changes
  invalidate the fit cache and change native fitting as appropriate.

`cargo test -p bri-client --lib fitting_`: three passed, zero failed/ignored;
initial cold build4m24s, tests .04s. Log `target/fitting-before.log`.
`python tools/check.py quick --crates bri-client`: format and full unit suite,
587 passed/zero failed/76 existing content-dependent ignored,109s total.
Log `target/fitting-before-quick.log`. Existing native outfit continuation,
respawn reset, seat reach/rotation, mount/action and authored-hand controls pass.

## After delegation

The same three strict characterizations pass: three/zero/zero, .05s tests;
log `target/fitting-after.log`. Additional content tests verify malformed hand
indices/parent counts and independent seat reaches with absent/empty fits under
scaled/rotated/translated and singular/nonfinite model frames: two/zero/zero;
log `target/fitting-pure.log`.

`python tools/check.py quick --crates bri-content,bri-client`: four steps73s,
client587/zero/76 existing ignored, content100/zero/zero, both format checks pass.
Log `target/fitting-after-quick.log`. No runtime failure was waived. An early
read of a guessed testing/avatar.rs path failed; resolved through rg to the
existing testing.rs reexport. No original/private data entered these fixtures.

The valid-tree fixtures use other-hand landmarks isolating disjoint arms. Their
seat reaches can commute numerically, so preserving the right-before-left loop
is not falsely reported as an order-discriminating source fixture.

First focused invocation passed both all-target Clippy checks, client587/zero/76
and content100/zero/zero plus doc tests, then failed because the extra targets
were incorrectly passed as one comma-separated `--tests` argument. No test
assertion failed; no integration target ran under that malformed argument.
Log `target/fitting-focused.log` (151s). Corrected invocation uses repeated flags:

```powershell
python tools/check.py focused --crates bri-content,bri-client --depth 0 --tests bri-client:body_layer_plan --tests bri-client:seat_pose_composition --tests bri-client:seat_fallback_anchor --tests bri-client:gltf_body --tests bri-client:world_items
```

Corrected focused passed all11 steps in136s, with warnings denied on all targets
of both changed crates. Client units587/zero/76 existing ignored, content100/zero/
zero, content doc tests zero/zero/zero. Existing integration controls:
body_layer_plan2/zero/zero; seat_pose_composition7/zero/zero;
seat_fallback_anchor4/zero/zero; gltf_body4/zero/1 existing external-file ignored;
world_items14/zero/13 existing generated-content ignored. No assertions changed
in these existing targets. Log `target/fitting-focused-corrected.log`.
`git diff --check` passes. No full gate or original consumer was run here.

Root owns broader integration/gate and original consumers. This is a shared prerequisite, not closure of mounted
Session authority, AA authored-prone/dynamic CameraBone, source reload timing,
package completion hooks, replication or client channel15. v0.3.1 and both Halo
previews are unchanged. Stage C still needs a separate exact-path grant.
