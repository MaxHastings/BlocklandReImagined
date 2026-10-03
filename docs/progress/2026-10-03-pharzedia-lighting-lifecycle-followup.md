# 2026-10-03 Pharzedia lighting lifecycle follow-up

This source-only follow-up to `9f38c828` addresses concrete findings from the
coordinator's independent Sol High review. **It has not been compiled or run.**
Root requested a Cargo/offscreen/CPU-heavy validation pause for exclusive NPC
performance work; only source edits, direct rustfmt and static diff inspection
were performed. The earlier rendering handoff's passing counts describe the
previous commit, not this follow-up. Root will run integrated validation after
the explicit performance release, including the two parent-fixed baseline
client tests without exclusions.

## Findings and changes

1. Replacing `LightVolumeState` on every source-mode transition dropped the
   receiver of a detached compatibility bake. Repeated mode flips could spawn
   duplicate fitting jobs. The same map now retains its compatibility ticket,
   receiver, completed volume/map data and fixture ownership across source
   changes. Fresh compatibility scenes reapply retained leak fixes and equip
   retained switchable sheets; modern scenes do neither. Dynamic does not drain
   queued legacy completions or invalidate its bindings because they arrived.

2. Map changes could still orphan expensive work, and `Baker::new`/`Bake::new`
   constructed BVHs and texel inputs synchronously from `prepare_render`. There
   is now one lazy process-wide compatibility worker and at most one pending
   input. A new pending request cancels/replaces the preceding pending request.
   A map-owned drop ticket cancels obsolete work between expensive stages; a
   currently running stage is allowed to finish. Superseded current legacy
   requests can retry from their retained input; Dynamic never submits/retries
   them. Constructors, baking and cache work all run on the single worker.
   Initial source snapshot cloning occurs in the existing map/source loading
   threads, not in `prepare_render`. Retaining one compatibility snapshot and
   up to two completed stage messages is a bounded cost of switching modes.

3. A pending/failed Dynamic-to-Classic/Unified source reload kept shader mode 3,
   but requested graphics disabled cubes and omitted modern map/terrain/brick
   casters. This could indefinitely erase recovered lamp illumination if legacy
   images were missing. `Graphics::with_lighting` derives cube settings from the
   effective shading mode without changing the saved request. Pipeline creation,
   rebuild comparisons, shadow caster selection and cleanup/equip guards use
   that mode. Failed compatibility source loads therefore keep current modern
   descriptors and geometry/shadow resources; a successful reload applies the
   compatibility policy together with its source.

4. Sidecars permitted radii at/below the fixed 0.05 shadow near plane, producing
   invalid projectors. Both descriptor validation and renderer light upload now
   use the shared radius contract: finite inner/outer, inner >= 0, outer > inner
   and outer > 0.05. Valid normal light parameters retain their visual behavior.
   The generator calls `Parameters::write_atomic`, which uses the reader's field
   validation and checks encoded size before atomic replacement. Invalid fields
   or an oversized encoding leave the previous sidecar intact. Existing
   `Parameters::read`, `.lights(id)` and generator CLI interfaces remain usable.

Dynamic still starts no compatibility preparation of its own and samples no
legacy illumination. Classic/Unified retain their previous required inputs and
output model. There are no manifests, wire/content schema changes, original
installation writes, gameplay automation or generated content changes here.

## Authored regressions awaiting root execution

Client library tests under `app::lighting::tests`:

- `mode_flips_keep_one_compatibility_ticket_and_queued_result`: 100 source flips
  retain a single ticket/receiver and fixture ownership, modern ignores queued
  completion, compatibility consumes it, completed data does not restart a job.
- `compatibility_queue_has_only_the_latest_pending_input`: 100 replacements
  cancel preceding tickets, release preceding queued snapshots and disconnect
  their senders; dropping the remaining map ticket blocks another stage send.
- `failed_compatibility_source_keeps_modern_lights_and_shadow_policy`: repeated
  failed Classic/Unified requests preserve modern descriptors, cube settings,
  map/terrain/brick caster policy and avoid compatibility/reload retry loops.
- `returning_to_legacy_reapplies_retained_cleanup_without_rebaking`: retained
  cleanup patches a fresh image to its exact original values; switchable shares
  remain available for re-equipping; no second bake is started.
- Existing `modern_state_has_no_compatibility_preparation` and
  `latest_source_selection_discards_stale_completion` remain in place.

Renderer library tests:

- `scene::tests::shadowed_light_parameters_reject_tiny_or_equal_near_radii`.
- `shadow::tests::validated_light_radii_have_finite_shadow_faces`: rejected
  tiny/equal-plane inputs never reach projection; accepted just-above-plane and
  normal ranges produce finite face matrices.
- `lighting_parameters::tests::generator_validation_preserves_existing_file_on_bad_radii_or_size`:
  valid write/read succeeds; rejected tiny/equal radii or oversized encoding
  preserve the preceding valid file.

Direct rustfmt completed and `git diff --check` passed. No Cargo command,
headless test, GPU render, content preparation, large archive/hash operation or
new build output was run during the coordinator's pause. Integrated build,
strict lint, all client/renderer suites and runtime/offscreen lifecycle checks
remain required. Root owns integration, final package checks and publication;
Maxwell owns interactive acceptance. The full alpha contract remains open.
