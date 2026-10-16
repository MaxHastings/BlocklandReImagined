# 2026-10-16 Contact effect frame design and strict native reproduction

Root granted only design and source-free strict regression after camera fix
`3cf62a6b0229b9df9f3c5b299f8c1fb4a2988a30`. Branch
`codex/contact-effect-frames`; proposed four paths were coordinated with root
and Halo before edits: new client test, architecture note, this progress file,
one seam row. Production stays read-only. No protocol marker is created yet.

Read immutable private Halo PR 34 handoff
`fa09e005e2e7e3ea315f09c5c0c6f2697910c179`. Traced actual native hit producer,
effect emission, Session forwarding, cue validation and existing FX group/plane
mechanisms. The architecture note proposes separate actual travel/normal
transport and a defaulted typed reference on existing Burst groups, with exact
candidate paths and one future wire marker. Source signs, roll/full basis and
contact target/node attachment remain unproved/separate, with no guessed PC
binding. Existing cosmetic particle contact audio is a separate producer.

Public fixtures use invented weapon JSON, native WeaponsWorld with randomized
spread and a Query plane/callback, real effect Event and native Cue serde. No
original content/code is committed. The FX control uses the existing synthetic
effects library and native particle snapshots, not a renderer substitute.

## Evidence

Uses CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4;
CARGO_TARGET_DIR unset. No visible game or interactive playtest.

- First targeted compile reported the omitted EffectsWorld seed argument;
  supplied a fixed invented seed41. Log: target/contact-effect-frames-first.log.
- Corrected `cargo test -p bri-client --test contact_effect_frames --locked`:
  10.87-second compile; two passed, zero failed, two strict requirements ignored,
  tests below 0.01 second. Native randomized-hit and legacy Burst/Plane controls
  pass. Log: target/contact-effect-frames-targeted.log.
- Explicit `cargo test -p bri-client --test contact_effect_frames --locked
  -- --ignored --nocapture`: exit101; two strict failures, tests below 0.01
  second (18.66-second compile). Actual native Event retains only +Z normal;
  the proposed travel/normal context is absent. Native Cue serde also drops
  that context. Both preserve shooter17 separately from contacted Map9 in the
  invented fixture. Log: target/contact-effect-frames-strict.log. No passing
  reliable transport or full Session claim is made by these public boundary
  tests; PR34 is the independent original Session counterexample.
- Added a third strict client consumer requirement during focused validation:
  proposed normal/travel Burst selectors, actual WeaponEffects cue handling and
  native particle positions along two independent axes. Current schema rejection
  remains the first failure; no original sign/roll/attachment policy is asserted.
  Final target compilation and supplemental fmt/Clippy/strict evidence follows.
- `python tools/check.py quick --crates bri-client`: two steps in 110 seconds;
  584 passed, zero failed, 76 existing ignores. Log:
  target/contact-effect-frames-quick.log.
- `python tools/check.py focused --crates bri-client --depth 0
  --tests bri-client:contact_effect_frames`: four steps in 128 seconds;
  586 passed, zero failed, 79 ignored (76 existing client units plus the three
  deliberately red requirements); all-target Clippy `-D warnings` passed before
  the third strict case was added, and the final target compiled all five cases.
  Log: target/contact-effect-frames-focused.log.
- Final explicit strict run compiled in 2m38s and reproduced all three failures
  in 0.41 seconds, exit101. In addition to lost Event/Cue geometry, the native
  Burst parser rejects unknown field `frame`. Its actual client particle
  assertions are therefore ready downstream requirements, not output that ran.
  Log: target/contact-effect-frames-strict-final.log.
- Supplemental `cargo fmt --package bri-client --check` and
  `cargo clippy -p bri-client --all-targets --locked -- -D warnings` both pass
  with the final third strict case present. Clippy completed in 7m29s under
  concurrent Rust builds; no errors/warnings. Logs:
  target/contact-effect-frames-final-fmt.log and
  target/contact-effect-frames-final-clippy.log. No further Rust edits followed.
- `git diff --check` passes. No production changes, full gate or push. Active
  worktree target/cache is retained for root's next assigned work.

## Remaining work

Root reviews the concrete type/frame/roll/attachment/schema choices and assigns
production paths before implementation. The intentionally red strict fixture
must stay outside integrated include-ignored candidates until corrected. Full
actual Session/reliable roundtrip and per-group native output belong to that
implementation, followed by exact-SHA original Halo consumption. Existing
cut-plane/wood geometry, original contact repeat/promotion, mounted authority,
fitting and clock requirements remain open and owned.
