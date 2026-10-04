# 2026-10-03 Mounted actor portal travel and driven-view crossing

User reports: riding a horse blocks passage through portals; the Jeep view briefly exposes geometry behind the source portal before its camera arrives. These are separately demonstrated source paths, pending root execution of the proposed regressions. No interactive playtest was automated.

## Source proof and correction

`VehiclesWorld::actor_step` ran `Player::step_in_water` without the host's current passages or merged brick part identities. Actor movement happens during vehicle pre-step, before Session captures the before-centres used by the existing post-Rapier physical-vehicle portal travel. Consequently a horse has neither walking players' portal-aware collision soup nor an observable before/after crossing. Its rider already follows the live mount seat in `follow_seats`.

The proposed patch routes mounts through the same `Player::step_through` motor as walking players, with Simulation's current chunk identities and linked openings. Returned motor carries are recorded once through the existing Session crossings path; physical vehicles retain post-physics travel. Actor travel centres use the actual motor middle, and between-tick centres use the same motor shown feet. Existing identity, seat occupancy and motion persist. No transport/save/content schema change.

In prediction, the same canonical vehicle pre-step observes actual actor carries. Both actor and physical carries move the previous drawn pose and centre to the destination space. Newly recorded driving inputs publish the actual carry and entry opening; correcting and replaying an old authoritative pose does not publish the trip again.

Previously Motion's mounted branch only recorded inputs and never participated in the existing `unshown` crossing delay. Its driven pose was immediately drawn in destination space even while interpolation placed its middle behind the exit plane. The patch keeps the driven pose and canonical interpolated centre on the source side until that centre crosses the entry plane, using the same existing bounded delay as walking. Interim driving inputs use `carried_input`; look carries publish only at the visible crossing. Pending correction offsets and turns change frame with that crossing. Root owns the camera pivot/boom presentation and first-person portal visibility integration.

Independent first-person review found that the original inverse-carry eye suppression named a body copy that AvatarMesh never draws. Root accepted the correction: raw body eye and only its actual current carried copy count. Root added the translated quarter-turn inverse-eye counterexample, recomputes current body_straddle even for hidden local bodies, and retains own held-image groups/shadows appropriately. Root reports seven offscreen mirror checks passed; no extra result is claimed by this lane.

## Requested regression boundaries

`a_ridden_horse_uses_the_same_linked_openings_as_a_walking_player` runs synthetic and ignored native variants: ordinary Session join, spawned horse, jumping to board, ordinary forward movement, linked quarter-turn doorway versus the same unlinked shut doorway. The linked case must retain vehicle ID, seat, occupants, carried running velocity and rider pose/velocity. Test-only baseline patch `/tmp/bri-v023-horse-portal-tests.patch` compiles without new APIs.

Client `ordinary_mounted_portal_prediction_keeps_the_drawn_centre_and_look_together` and ignored native variant exercise both Jeep and Horse through real authoritative Session controls and their actual predictor. Host poses arrive twelve ticks late; correction replay cannot announce another crossing. Alpha 0/.25/.5/.75 samples check the drawn canonical centre, visible crossing announcement and unfolded default third-person camera boom/heading on both sides. This rendering-only sampling is not interactive control automation.

Proposed production and regression patch: `/tmp/bri-v023-horse-portal.patch`. Lightweight formatting and apply-check only; this lane has no compute lease and has not run Cargo. Root should run the baseline Session test before production, then both real/synthetic boundaries after integration:

```sh
cargo test --locked -p bri-sim --test vehicles a_ridden_horse_uses_the_same_linked_openings_as_a_walking_player -- --include-ignored --nocapture
cargo test --locked -p bri-client --lib mounted_portal_prediction_keeps_the_drawn_centre_and_look_together -- --include-ignored --nocapture
cargo test --locked -p bri-sim --test vehicle_prediction
cargo test --locked -p bri-sim --test portals
```

These paths do not attribute or close the unreproduced Windows firefight NaN crash. Normal-death regression remains negative evidence as recorded separately.

## First baseline and ordering

Root ran both native/synthetic unchanged-production Horse tests: both failed the shut-doorway bound (`/tmp/bri-v023-horse-portal-before.log`). Source diagnosis shows this is also the same mechanism defect: linked doorway geometry contains a real hole; the canonical Links mechanism creates `Passages.closed` virtual panes for unlinked doorways, and only portal-aware motor Soup adds those planes. Horse `step_in_water` supplies default empty Passages and therefore walks through the hole without its shut pane. This is not a fixture-bound correction.

Incremental `/tmp/bri-v023-horse-portal-baseline-order.patch` puts the linked case first so a second baseline separately reaches the intended failure to arrive at the partner. The shut case remains intact, with actual pose in its failure message. Root already applied the original test, so `/tmp/bri-v023-horse-portal-production.patch` excludes that test-file hunk.

## Root applied/runtime verification

The linked-first baseline then failed at the intended missing partner arrival for both native and synthetic horses (`/tmp/bri-v023-horse-linked-before.log`). Root applied the portal-aware mount travel and existing driven-view crossing delay. The complete sim vehicles suite passed 55/55 (`/tmp/bri-v023-mounted-vehicles-after.log`), including both ridden Horse boundaries. Both synthetic and native client mounted-prediction regressions passed (`/tmp/bri-v023-mounted-camera-after.log`), exercising Jeep and Horse, twelve-tick delayed authoritative correction, one crossing announcement and all four render alpha samples. The default third-person boom assertion is specifically the Jeep's actual `driver_view` path; Horse uses the actor centre/pose/look assertions, not a claim that the Jeep camera represents the horse player camera. Root's canonical portal and vehicle-prediction suites are queued; no result is claimed yet.

## Bounded rigged-body split follow-up

`ClientVehicles::prepare` previously skipped Horse before computing its straddle. `pose_mounts` baked horse vertices into world space (`instanced=false`), and render added those meshes as direct scenes in ordinary and shadow views. Therefore assigning a straddle alone would have no effect. Separate `/tmp/bri-v023-mounted-split.patch` computes every mount's split before the Horse rig skip, using the existing canonical actor_tuning box for actor middles and retaining physical vehicles' mass centres. actor_tuning gains public visibility and documents actor-family/canonical-scale caller requirements; no serialized schema changes.

The Horse mesh uses its canonical scale and the same clipped two-instance AvatarMesh path as players. Render consumes those GPU/instance pairs in shared models and shadow models, retaining main/portal/mirror/probe/shadow participation while removing duplicate direct scene draws. Rider body_straddle reads the same mount split. Split removal follows current preparation/map passages without retained stale copies. No art/model replacement.

Test-only `/tmp/bri-v023-mounted-split-tests.patch` exercises actual ClientVehicles preparation on synthetic and native Horse packs at scales .75/1/1.5, compares its split with the real spawned motor's observed middle, ensures no static Horse duplicate, and checks removal when passages disappear. This is CPU geometry coverage; no unrun GPU image comparison is claimed. Root can baseline this test before the follow-up:

```sh
cargo test --locked -p bri-client --lib a_rigged_mount_retains_the_same_portal_split_as_its_motor_and_riders -- --include-ignored --nocapture
```

Formatting on temporary non-render sources and apply-check passed; root owns render integration and actual runtime checks.

Root additional canonical checks: vehicle prediction passed 5/5 and sim portals passed 10/10. These supplement the already recorded 55/55 vehicle and 2/2 mounted client checks. The rigged Horse body split follow-up remains separately queued for its before/after check; passing travel/camera tests alone does not establish its render preparation result.
