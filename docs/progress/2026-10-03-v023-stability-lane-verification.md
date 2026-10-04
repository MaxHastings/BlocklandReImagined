# 2026-10-03 v0.2.3 Stability/creator lane integration review

Root owns shared engine integration, compute, release gate and publishing. This lane supplied reviewable temporary patches, read-only source reviews and its owned notes. No commits, pushes, new worktrees, original-install writes or interactive playtest automation were performed by the lane. Maxwell explicitly authorized all current work for v0.2.3 on three platforms; that authorization is not evidence that the release gate or platform builds have passed.

## Root-reported runtime evidence

| Boundary | Result and receipt |
| --- | --- |
| Saved declared per-mini-game state, ordinary build/load hooks | script_api 24/24, `/tmp/bri-v023-saved-state-after.log` |
| Wrench Send paints existing live vehicle | native/synthetic 2/2, `/tmp/bri-v023-vehicle-color-after.log`; client presentation 1/1, `/tmp/bri-v023-vehicle-client-lib.log` |
| Fresh release retains explicit Add-On choices with current dependencies | 5/5, `/tmp/bri-v023-preferences-final.log` |
| Gravity Gun icon exact clockwise rotation | unit 1/1, `/tmp/bri-v023-icon-unit.log`; native/synthetic 2/2, `/tmp/bri-v023-gravity-icon-native.log`; root viewed comparison image |
| Actual Gravity Gun Full trust / linked-spawn mini-game owner | showcase 32/32, `/tmp/bri-v023-gravity-trust-after.log` |
| Real joining content preparation before player admission | package_sync 15/15, `/tmp/bri-v023-join-final-2.log` |
| Horse linked/shut openings and live vehicle lifecycle | vehicles 55/55, `/tmp/bri-v023-mounted-vehicles-after.log` |
| Jeep/Horse predicted crossing and delayed authoritative correction | client native/synthetic 2/2, `/tmp/bri-v023-mounted-camera-after.log`; root also reports vehicle_prediction 5/5 and sim portals 10/10 |

The original saved-state fixture contained no bricks and failed LoadBuild before the intended boundary; it was corrected to an ordinary authored brick before after-fix verification. The final join fallback fixture originally used a fetch-only server with no game Environment; it was corrected to the existing canonical join server pattern with every exact expected ID/count unchanged. Both initial fixture failures remain recorded in their detailed notes.

## Final source review and bounded costs

Saved state mutates only declared per_minigame keys for the selected game, validates the complete resulting value/budget before admission, distinguishes absent from saved empty and rejected values, and retains other packages/scopes. Wrench paint uses the existing live vehicle state/replication/material path without respawn or new identity. Full trust remains ordinary policy; the narrow linked-spawn owner fix reuses existing brick-group ownership rather than granting a Gravity Gun exception.

Preferences add one bounded per-user IDs/booleans document with atomic file replacement and canonical Library plan/apply. Unknown IDs remain retained preferences; absent first-run overrides preserve shipped defaults, and explicit defaults reset follows future releases. Existing settings/selected colorset IDs already use the stable user directory. A selected release-local colorset's missing data is separate from the selection, and no old-release scanning or save migration was added.

Icon work adds an optional authored clockwise-quarter-turn field and exact pixel permutation in the existing icon renderer; original models/art remain unchanged. Joining adds one bounded authenticated probe and prepares current fetched content through the existing loader before Hello, with a typed local deferral and returned prepared identities. Genuine loader errors, package refusals and departures remain visible. No new network message, protocol file or schema semantics are introduced.

Mount work adds portal-aware actor pre-step through the same motor/Soup/part identities as walking players, observed canonical centres and bounded Drive crossing/centre accessors. Actor carries are observed once before Session's post-physics centres; physical vehicles retain their existing post-physics carry. Existing unshown interpolation/input/look machinery owns mounted visible crossing and correction framing; replay does not announce a new trip. The separate rigged-body split uses the canonical actor_tuning API rather than a duplicate box formula and the existing AvatarMesh clipped two-instance renderer.

Independent NPC and Admin Refresh reviews found no remaining actionable integration defect within the reviewed changes. Root's admin helper deliberately uses the newer already validated UI state only in the same Session; reset/correlation guards remain the authority boundary. Root owns its committed fix and its tests/notes.

## Remaining evidence boundaries

The new standard crouch collision boundary and separate rigged mount split before/after check were queued at this note's creation. Their source reasoning alone is not runtime acceptance. See their dedicated notes for exact pending commands.

The normal authoritative explosion/fall/corpse/respawn regression passes unchanged production (`/tmp/bri-v023-death-lifecycle-before.log`). This is negative evidence, not a reproduction or causal cure of Maxwell's death disconnect. Fourth explode argument is brick_radius, not impulse; canonical knockback is nonzero in the fixture. Compact authoritative correction diagnostics preserve rejection rather than clamping values. The Windows firefight NaN crash remains unreproduced by this lane; the already fixed late atlas reload defect was not reimplemented.

All-platform release builds, Windows CI, the mandatory full gate and Maxwell's human playtests remain root/user verification. No result in this note reduces those requirements.
