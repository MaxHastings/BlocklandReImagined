# 2026-10-27 Blockhead F directional gait correction

Maxwell reported rapid footstep retriggering and visible animation trouble while
holding W+A or W+D and turning the camera. He requested autonomous correction,
2–3 dB more volume (chosen +2.5 dB), and less repetitive variation while retaining
the selected F sound. Later reference notes do not authorize copying Torque
datablock code, example contact phases or broad random pitch into this engine.

Latest scope: Maxwell requires this in the **base game**, in the same installed
build, and a remote branch/PR. He confirmed the four F recordings are cleared
for public distribution. The repository's main README must stay unchanged.
After the first base-game playtest he requested **-1.5 dB**, retaining the four
recordings and existing repetition behavior, then a remote PR. This is applied
at native profile gain: contact gain remains +2.5 dB and profile gain is -1.5 dB,
so net playback is +1 dB over initial F. No further sound-design expansion.

## Inputs and cause

- Branch `codex/footstep-turning-f`, based on main
  `e1eece1fad0c40e2e58f1235ec189fbe551faf85`, matching the installed v0.3.7
  client. Main checkout's unrelated local changes remain untouched.
- Existing local playtest:
  `E:\Downloads\BlocklandReImagined-v0.3.7-windows`, Add-On
  `Blockhead Footsteps F (Playtest)` / `blockhead_footsteps_f` 0.1.0.
- Private sound sources and accepted processing remain under the main checkout's
  ignored `artifacts/footstep-audition-20261027`. `selected-F/recipe.json` records
  hashes and the D/E midpoint. Original source installations are read-only.
- `AvatarMesh` and the host's `Gaits` restarted their clock when the directional
  action changed. The installed side/crouchside policies mark phase zero;
  `ClipSpan::passes` correctly covers the initial instant of a restart, so these
  repeatedly played without a completed stride. Repeated transition snapshots
  also postponed the pose blend's completion.
- The new client regression failed before correction: **600 restarts** in ten
  seconds at 60 fps. This is a deterministic boundary-crossing reproduction,
  not an automated interactive playtest or a trace of Maxwell's exact inputs.

## Correction and scope

- Shared `gait::ground_phase` transfers normalized progress between looping
  directional gaits within the same posture, accounting for reverse playback.
  Both host and client advance the transferred phase during the changing frame;
  the change is not a fresh clip-start event. Client ground blends finish before
  another source pose is frozen. No arbitrary audio cooldown or marker offset.
- This intentionally improves the original v20 restart/flicker quirk. Source
  evidence stays in `docs/audits/torque-quirks.md`; it does not establish source
  fidelity for the new behavior. Idle, airborne, posture and non-looping action
  starts retain their existing semantics. Physics, controls and wire schemas
  are unchanged.
- Direct footstep pools use a shuffled bag per walker and pool identity. Every
  distinct profile is heard once before refill; a refill avoids the previous
  sound. Duplicate IDs are not new variations. Surface/posture pool changes,
  singleton/empty pools and owner separation are covered. Existing weighted
  choices inside sound profiles keep their own mechanism.
- Keep the same **four F WAVs**, byte-for-byte. The prepared private Add-On
  revision 0.1.1 applies +2.5 dB through event volume (1.333521432), retaining
  half-gain crouch contacts. No extra layers, recordings, pitch or timing jitter.
- That temporary Add-On delivery is superseded by base integration:
  `assets/blockhead-footsteps/` holds the cleared WAVs, recipe and authored
  `sounds.json`. Existing stock archetype defaults read this policy for
  Blockheads; Horse keeps its own sounds. `bri-weapons-import` ships the WAVs as
  ordinary first-party sounds/resources in the existing native catalog. Bootstrap
  tracks the authored folder; no runtime Torque/importer dependency or new sound
  playback system is introduced. The catalog-only warm refresh is idempotent.
- Offline revision builder:
  `artifacts/footstep-audition-20261027/prepare_f_revision_02.py` in the main
  checkout. Prepared package:
  `integration/revision-02/blockhead_footsteps_f`. Dry steady-gait overlap across
  every alternating pair remains below clipping (worst -7.55 dBFS). This is not
  a multiplayer mixer or perceived-loudness acceptance result.

## Validation

- Failing-before/passing-after command:
  `cargo test -p bri-client --lib turning_across_a_diagonal_keeps_walking_instead_of_restarting -j 2`.
  After: all four standing/crouched, left/right cases pass, exactly one start and
  advancing phase over ten seconds. Host regression covers 30/60/144 fps, reverse
  playback, stopping and jumping; phase-transfer checks exclude non-ground and
  posture changes.
- `python tools/check.py quick`: pass; final repeat after shuffled-pool edge-case
  handling also passes: client 695 tests, sim 428 tests, no failures. Ignored
  content/GPU tests remain distinct. Logs:
  `artifacts/footstep-turning-f/{quick,quick-final}.log` in this worktree.
- Installed exact-main `bri-addon-check.exe` accepts revised package 0.1.1,
  no diagnostics. The four exported F hashes match the original recipe.
- Focused checks, converted-body regression/render checks, full candidate gate
  and optimized playtest packaging are in progress. No human acceptance claimed.
- Initial focused Clippy passed. Subsequent test compilation was deliberately
  stopped when the base-game scope changed; exit 4294967295 is that interruption,
  not a failing test assertion. Final checks cover the expanded candidate.
- `cargo build --profile playtest -p bri-client -p bri-weapons-import --locked`
  passes (8m32s), optimized without LTO. Warm refresh was exercised twice against
  a copied native catalog; four profiles remain, no duplicate resources.
- First installation check caught item presentation's whole-file weapon
  checksum. The installer restored the previous executable/config/Add-On.
  Correction proves the complete non-Foley weapon data and resources are
  unchanged before rebinding only that source fingerprint. Derived geometry,
  physics and images remain unchanged. Retried actual installed
  `bri-client --check` passes. This is not an unchecked checksum bypass.
- Base candidate unit checks: client 695, motor 14 and sim 428 pass. The new
  importer test initially compared `Pack` without a `PartialEq` implementation;
  switching its idempotence assertion to serialized values fixes that test-only
  compile error. All 12 importer unit tests then pass. Expanded focused checks
  (`--depth 1` plus avatar animation integration) have clean Clippy and are still
  running; the remote checkpoint is a draft until validation is complete.
- Requested -1.5 dB trim is installed in the same game folder; actual startup
  check passes. Only four native profile volumes and the dependent source hash
  change; recordings, cadence, selection and executable remain unchanged. The
  new gain takes effect at the next restart. `FOOTSTEPS-F-GAIN-RECEIPT.json`
  records it; immutable pre-adjustment metadata is under `integration/revision-03`.

## Acceptance handoff

Maxwell bounded this pass: dependable triggering/animation, F at +2.5 dB and
balanced selection, then one ordinary listening session without repeated
distraction. If it passes, park further footstep polish and audit missing action
feedback before expanding an already audible sound. Footsteps are an addition,
not automatically the most important audio work; an existing toy-like
source sound is not by itself a missing behavior or a mandate to redesign it.

Maxwell explicitly requested updating the **same installed build**, then pushing
the changes to a remote branch and preparing a PR. Update
`E:\Downloads\BlocklandReImagined-v0.3.7-windows` in place, retaining an immutable
backup of its original client, package selection and F Add-On under the main
checkout's `artifacts/footstep-audition-20261027/integration/revision-02/backups`.
The ignored `install_base_foley.py` refuses a running client, records source
identities and performs headless startup validation with rollback on failure.
The temporary Add-On is archived outside discovery, and its explicit selection
is removed. F is now in the base game, without an enable checkbox. Four sounds
and their resource hashes are added to the native catalog. Unrelated package
choices stay as installed. `FOOTSTEPS-F-RECEIPT.json` in the game folder records
the binary hash, parent commit and candidate source-file hashes. This is a local
candidate, not a new published release. No second game folder is being created.

Maxwell alone performs gameplay acceptance: both diagonal directions while
turning both ways; straight/back/side movement; start/stop; crouch; jump/fall/jet;
sit and vehicle seats. Then build/navigate normally for 10–15 minutes and judge
cadence, repetition and mix comfort. Other-player attenuation and busy-scene mix
remain listening checks; existing spatial audio is reused. All non-forward gait
contacts remain initial authored, source-pose-based estimates from the earlier
F playtest, not recovered vanilla footsteps. No footstep production-readiness
claim, new surface library or additional Foley is warranted before that test.
