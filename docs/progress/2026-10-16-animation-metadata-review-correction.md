# 2026-10-16 Animation metadata review containment and identity correction

Root's bounded review of immutable Stage1 94bae5632d1a19d16d9c663e146bcc5034522d55
identified two possible loader failures. Scope is only existing
net/src/content_identity.rs, net/tests/animation_metadata.rs and own docs. No
Stage2, schema, wire, policy or additional production path. The Stage1 commit
remains unchanged; this is a separate correction on top.

Characterization first used native fixture files and actual optional-extra
animation_assets loading, then the existing dedicated HostSetup recipe to create
a fresh Session and admit a verified stock clip. Two new cases failed on Stage1:
malformed optional presentation JSON propagated its parse error; a presentation
entry for another_model incorrectly admitted metadata under Actual_Model's
image identity. Four other cases passed, including mandatory malformed base
presentation rejection through the existing dedicated/ItemPhysicsContent path.
Log: target/animation-metadata-review-red-corrected.log (2.69s compile/.78s tests,
four passed/two failed/zero ignored). An earlier test fixture referenced the
nonexistent common::simulation helper; corrected to existing simulation_with
before obtaining those red results (target/animation-metadata-review-red.log).

The stock presentation remains mandatory: existing item physics loading parses
and validates it before animation catalog construction. Optional extra animation
metadata now warns and omits malformed presentation JSON, preserving verified
stock clocks and ordinary hosting. A presentation image's declared model must
agree case-insensitively with the actual weapon Image.model before its native
file can supply clock metadata. A separate positive case proves normalized case
agreement remains supported. Existing catalog/byte validation is unchanged.

The previous ambiguity/budget test fixtures now declare actual_model consistently
in the weapon and presentation bindings. This preserves their intended conflict
and resource-limit coverage under the corrected identity check.

Commands use CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0 and
CARGO_BUILD_JOBS=4; CARGO_TARGET_DIR remains unset. Root owns the integrated gate
and publication. No interactive game session, original content changes or new
mechanism is involved.

Validation:

- Targeted metadata after production correction: six passed/zero failed/zero
  ignored, 24.84s compile/.82s tests, target/animation-metadata-review-green.log.
  The mandatory-base test was then strengthened to exercise ItemPhysicsContent
  directly, as well as dedicated host loading; final focused includes that check.
- `python tools/check.py quick --crates bri-net`: two steps/26s, 81 passed,
  zero failed, three ignored. Log: target/animation-metadata-review-quick.log.
- `python tools/check.py focused --crates bri-net,bri-client --depth 0 --tests
  bri-net:animation_metadata --tests bri-net:host_setup --tests
  bri-client:trench_dig_flow`: nine steps/442s; both all-target Clippy checks
  passed, 810 passed/zero failed/96 ignored across 43 summaries. The six
  metadata, five HostSetup and one client trench-flow cases all pass with zero
  ignored. Log: target/animation-metadata-review-focused.log.
- Final whitespace diff check passed. Exactly three changed paths: the granted
  loader, its metadata tests, and this progress entry. No full gate/rebase/push.

Separate correction parent is immutable94bae563; root receives the committed
head and owns integration. Optional malformed/mismatched entries remain explicitly
unsupported, while mandatory stock validation and verified catalog checks remain
in force. This does not establish any original animation timing law.
