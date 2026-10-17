# 2026-10-17 Ordinary Zoom presentation diagnostic (red-only grant)

Root granted a diagnostic only on new codex/v033-weapon-handling from published
v0.3.2 e4d9bb05df3c26c95a410cb9872990d99b37b54d. Saved engine31f and docs722/4b
remain on codex/v032-engine-seams at4b2898145873e33cea7193fd96772b2a5b519535.
Verified clean checkout and no own Cargo/client process before changing branch.
This machine's older git lacks switch/show-current; git checkout -b was used.
No original files, private source, main branch or published artifacts changed.

Only crates/client/src/app/tests.rs is edited. The latest root instruction
supersedes host-ADS-intent as the first predicate: test local sight placement
through ordinary Zoom, not server eligibility. The fixture uses ContentRoot's
made-up valid gun/model/presentation, adds an asymmetric probeSight native node,
keeps distinct muzzle/eject nodes, and supplies an invented expected eye-frame
translation outside all runtime metadata. Existing Zoom.on_jet+jets=false gives
ordinary RMB zoom; source-free rules give the gun through normal loadout.
Actual App hosts/equips/samples, presses/releases RMB, fires one ordinary shot
(12->11 on real authority), loses focus, swaps weapon and dies. Only after all
positive controls does it assert the expected pose, with the intentional marker
ADS_PRESENTATION_MISSING. No echo, direct Session mutation or new runtime API.

A held pose is sampled from actual WorldItems.mounted_transform/mounted_node;
effects/muzzle still share that existing sampled pose. The reference eye frame
is reconstructed from the known static hip offset; this is not screen-pixel
projection, original-AA sight alignment, AA source timing, or gameplay ADS proof.
No source-specific profile, scope projection or authoritative firing changes
are inferred. Remote/profile/same-ID reset implementation is not attempted.
MountedImage on e4 has image/state/hand/paint but no mount generation; an
unobserved server-forced same-ID remount remains an explicit identity caveat.

Targeted command only: CARGO_BUILD_JOBS=2, RUST_TEST_THREADS=8,
CARGO_TARGET_DIR unset; cargo test -p bri-client --lib --locked
 diagnostic_ordinary_zoom_changes_fov_but_not_authored_sight_pose -- --nocapture.
Log outside target: artifacts/v033-ads-diagnostic-red.log. Driver patch outside
target: artifacts/v033-ads-diagnostic-driver.patch. No full tiers/gate or failing
shipping test commit. Targeted run is still pending; compile/setup/lifecycle
failure is not the intended red. Root notes slot0 swap must be a genuinely
different equipped item; if the fixture ever places the gun there, correct the
fixture rather than mislabel a timeout.

Frozen app/tests.rs SHA256: 92fd59646c1e0b04cfcf06fba746fb751e77777f9403f3434949914acee96990.
Next: retain actual exit/output, report the precise predicate to root, and await
AA endpoint/red evidence and a reviewed bounded production grant.

## Actual targeted checkpoint

Actual exit101, 0 passed/1 failed/670 filtered, build4m44s and test1.41s.
The intended ADS_PRESENTATION_MISSING assertion was reached after every positive
fixture/host/lifecycle control. FOV90->40; hip sight eye x0.4699998 and aimed
x0.4699998 instead of expected approximately0. Host ordinary aimed shot12->11
and fired_tick present; RMB release and focus loss clear local aiming; a genuinely
different tool was sampled after slot0 swap; dead aim=false. The reported Y
shift0.002528 is small existing frame/body drift, not a proof of bit-identical
placement; the missing requested lateral move0.47 is the actual red predicate.
Muzzle is distinct: hip[0.35,-0.17,-1.26], aimed[0.35,-0.172528,-1.26].
No original AA profile, screen projection or tween interpolation was tested.
There is no authority ADS claim and no remount-reset implementation.

Existing synthetic content emits twelve missing optional sound/effect health
warnings; the valid catalog, native model, host, shot and ammo controls still
pass. Compiler also reports one redundant diagnostic import; retained unchanged
with the frozen first-run driver, not waived/hidden or mistaken for a pass.
Parsed evidence retained as artifacts/v033-ads-diagnostic-evidence.json.
Log SHA256: bb7fbe38b0290c7913ef1299e86f5f602aca0dec274eb19ead4da3356ffad19a.
No full tier, repeated run, runtime changes, commit or push was performed.
Root receives this exact checkpoint before any further changes.

## Parked after actual AA existing-API success

Root reports private b1ecaf7ee6cfd6b3122e75c25671f0553b7affbd: actual ordinary
AA existing Zoom.overlay consumer green as original-artwork preview adaptation,
not original 3D/interpolation parity. No aim-pose runtime seam is authorized.
Verified saved log/evidence/driver patch are present and that the patch exactly
matches the frozen working diff, then restored ONLY own app/tests.rs bytes from
e4d9bb05. Working source diff is empty. Synthetic expected-pose red remains
retained evidence of an optional capability, not proof the selected AA scope
requires it. No whole-tree reset/clean, new test run/tier or production edit.

## Existing held-bind sprint path handoff (read-only)

Root routed this finding to AA's bounded existing-API trial; no new Sprint
input/protocol/production seam is justified before an actual consumer defect.
Verified exact published e4 paths:

- docs/modding/content-kinds.md29-53 and package-runtime/src/content.rs Binds:
  configurable binds.json, hold:true, registered command args:[bool], not
  tool_only. Defaults bind only if the key is free; desired Shift may conflict
  with existing Crouch, so distinguish user remap/default conflict from transport.
- ui/src/ui.rs1096 run_command emits GameAction::Package pressed=Some(down).
  key_down remembers the physical bound command. release_all987 invokes that
  command with false; FocusLost2576 calls release_all. Ordinary key-up also
  releases the remembered command. PackageKey HUD fallback is press-only.
- client/src/app/actions.rs611 forwards pressed as PackageArg::Bool in the
  ordinary ordered Command::Package path to Session rules. This needs no
  input.capture module, sandbox net.message bridge, Sprint enum or wire change.
- Existing set_speed_scale and set_image_state provide authoritative movement
  multiplier/image state; package-authored native states can express lower,
  hold, raise and fire restrictions. This does not qualify AA timings or policy.

First ordinary consumer: free configurable held key, no cooldown, real App key
press/release/focus loss -> server bool counters and native speed/image state.
AA supplies qualified standing-forward eligibility, speed, priorities and clip
phases. Prove ordinary firing/ammo/projectile restrictions in each phase and
restoration after release, not just a package busy boolean. Then swap/holster,
reload interruption, death/respawn, observer, disconnect/rejoin and vanilla
no-bind controls. Use actual App/QUIC, not direct Session replacement or an echo.

Caveats: release may meet dead eligibility/cooldown; try a zero-cooldown bool
command and explicit source-owned death/unmount/loadout cleanup rather than
widening dead-player authority. Actor-global speed_rule must not leak to another
weapon/life. Original image mount/unmount callbacks are reusable but not proof
of generation-bound state resolution. MountedImage has no generation; an
unobserved same-ID remount is still unproven. If source forward-intent facts are
unavailable, isolate that actual observation failure instead of rewriting motor
physics. Any later defect gets only its demonstrated owning path/test grant.

No new runtime source, tests, build, gate or broad research for this handoff.
Hold for actual AA consumer evidence; saved31f/722/4b and ADS artifacts preserved.
