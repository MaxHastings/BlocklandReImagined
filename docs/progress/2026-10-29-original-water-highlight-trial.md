# 2026-10-29 Original-water highlight trial

After explicitly deferring the rejected overhaul, Maxwell clarified: keep the
original water and make small improvements. The original texture/material is
the baseline. This supersedes interpreting deferral as a request to leave all
small visual defects untouched. Rejected optics remain reverted/unshipped.

Inspection: existing samplers already support anisotropy (default8) and mipmaps;
stock Environment water scroll values are all zero. Those findings do not
justify inventing a missing-filter or animation correction. No change there.

Selected one narrow, evidenced weakness: the broad pale sun wash visible in
the supplied Slopes original-water shot. Both existing native-water lighting
routes add an identical broad highlight independent of their original texture.
Trial modulates ONLY that added highlight by luminance of the two original
texture samples already fetched by the shader. Pigment/alpha, source images,
UV scale, drift, masks, reflection and all gameplay are unchanged. No new
schemas, material system, depth copy, normal textures or runtime plumbing.
Environment water and brick water do not use this native specular clause;
there is no claim this change addresses Slate's image or unrelated liquids.

Maxwell explicitly answered 'Closed; make the comparison'; root separately
received 'I'm ready-I'll keep the game closed'. Recheck actual process state
before build/render. The warm native probe build and one matched Slopes
capture are next; pending visual verification/acceptance, not a shipped fix.

Validation/evidence: cargo build -p bri-client --bin lighting_probe --profile
playtest --locked passed (5m44). cargo test -p bri-render --test
shader_validation --profile playtest --locked -j2 passed3/3; git diff --check
passed. Bounded native Dynamic MSAA4 capture completed, no GPU validation
error. Matching original/candidate1280x720, H-FOV90, Medium shadows, Soft/AO:
artifacts/liquid-visuals/slopes-matched/before/matching-dynamic-t0.png and
slopes-original-glare/matching-dynamic-t0.png. Supplied camera/yaw/pitch
reused; no claim original diagnostic supplied FOV. Trial reduces pale wash
while retaining original pattern by visual inspection, not human acceptance.
The first capture attempt found one missing baseline referenced PNG after
restoring metadata; copied that unchanged from existing content, recorded
restored-baseline-assets.json, then capture passed. This was a content-repair
failure, not a flaky shader/render test. Three directly attached original
screenshots preserved byte-for-byte under references/05..07 with hashes.
Presented the exact pair and asked whether Maxwell wants a playtest build;
answer pending. No new water package, merge or publication.


## Maxwell acceptance and merge instruction

After seeing the actual same-pose original/after pair, Maxwell answered:
**i accept after just merge into main**. This accepts this exact small glare
change and explicitly replaces the separate-preview request for this change.
It does not accept the rejected overhaul. Release coordinator was sent the
accepted isolated commit1d8ec48b6fa480f84e004b0883be5e886eb7cfb8 for
cherry-pick onto current main under its existing integration lease. Keep
rejected60e ancestry out; local merge/push/platform CI pending coordination.

