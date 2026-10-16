# 2026-10-16 Accepted contact geometry and authored Burst references

Root reviewed diagnostic `27678b2fbcf87cbaae7540b5e6b242a8c7e5191b` and granted
the listed production footprint, one protocol marker, corresponding tests and
own docs. Branch `codex/contact-effect-frames-impl` retains that diagnostic on
camera fix `3cf62a6b`; integrate only the corrected chain, never the red design
alone. Root additionally reserved client app/fx.rs constructor contact:None,
the directly affected cfg-test constructors and mechanical test defaults.
No manifests/dependencies or direct VERSION edits.

ContactGeometry retains actual incoming unit travel and actual normal from one
accepted hit, before bounce/stick/redirect or special hitscan explosion-projectile
velocity changes. Unrelated image effects and non-contact expiry carry None;
legacy direction and shooter attribution retain their meanings. Session forwards
the optional record without deriving it from camera aim. One protocol marker
records the reliable cue change.

Existing Burst gains a Source-default typed reference: contact normal/travel,
opposite travel, reflected travel or world down. Native minimal arc is an explicit
engine convention, not verified original PC marker roll. Offset/local ejection
direction compose through the same selected reference exactly once. Existing
Source lowering, seed/count/radius and Plane/right/spin mechanisms are retained.

Pure composite preparation validates provided geometry and every authored group
before emission. Client preparation validates the whole incoming batch and caches
pose results before cursor/host-request/emission mutation. Missing contact skips
and counts only that explicit group; legacy siblings remain eligible. Only eligible
groups reserve the existing source/light and conservative count-max emission
budgets. Capacity rejection emits nothing and the client consumes the cue once.
This admission is per composite, so earlier successful cues remain accepted;
malformed provided data instead rejects the whole batch before any mutation.
No retry/replay. New contact-reference groups requesting unavailable contacted
target attachment are separately omitted/counted; the shooter is never borrowed
as that target. Default Source follow behavior is unchanged.

## Evidence and corrections

CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4;
CARGO_TARGET_DIR unset. Native/headless tests only; no visible game or interactive
playtest. Original installations remain unchanged.

- First `cargo test -p bri-client --test contact_effect_frames --locked
  -- --include-ignored`: five passed, zero failed/ignored after a 2m33s rebuild.
  All three formerly red requirements pass, including actual client independent
  normal/travel particles. Log: target/contact-effect-frames-impl-first.log.
- Expanded first run: 13 passed/one failed, no ignores, 2m26s rebuild, 0.01s
  tests. Actual ordinary Session forwarding, per-reference offset/axis,
  Source preservation, missing/mixed eligibility, atomic invalid-batch/pose,
  capacity/cursor and unsupported attachment controls passed. The flying fixture
  failed to produce its expected bounce. Log:
  target/contact-effect-frames-impl-expanded-first.log.
- Holding its trigger for the same two-tick entry sequence as other native tests
  did not change that failure. Recorded Contact and Effect showed the native
  projectile correctly exploded: its default arm_ticks was zero. Set the invented
  bounce/stick fixture's arm_ticks to120, leaving non-contact expiry independent.
  This is a test-policy correction, not a runtime bounce-law change. Log:
  target/contact-effect-frames-impl-flight-second.log.
- The corrected bounce/stick paths passed, then the same combined fixture reached
  expiry and correctly removed without an effect because explode_death was false.
  Explicitly enabled explode_death only for that invented expiry case. Log:
  target/contact-effect-frames-impl-expanded-final.log. This does not change the
  original repeat, promotion or projectile-expiry rules.
- Reliable native Message::Update codec roundtrip/rejection test passes: 2m35s
  compile, one passed/no failed/ignored, below0.01s tests. It uses actual bounded
  MessagePack/zstd encode/decode, malformed shape rejection and Replica's atomic
  invalid-geometry/cursor/world validation. Log:
  target/contact-effect-frames-impl-net-first.log.
- Added a source-free zero-normal adapter control: existing hitscan legacy facing
  fallback remains, but the new geometry record uses the observed raw hit normal
  and remains absent if that normal is unavailable. It does not promote a facing
  fallback into contact evidence. ContactGeometry also rejects unknown fields,
  so an unverified basis cannot silently disappear during decoding.
- Corrected complete client target, `cargo test -p bri-client --test
  contact_effect_frames --locked -- --include-ignored`: 15 passed, zero
  failed/ignored; 2m07s compile, 0.01s tests. All originally red requirements are
  enabled. Log: target/contact-effect-frames-impl-expanded-corrected.log.
- Full `cargo test -p bri-net --test replication --locked`: 18 passed, zero
  failed/ignored, 0.06s tests. This includes normal optional geometry roundtrip,
  a valid named-shape control, malformed/unknown context field decoding rejection
  and invalid finite/nonfinite geometry rejection before Replica mutation.
  Log: target/contact-effect-frames-impl-net-full.log.
- Final six-crate quick and focused evidence follows; production/test Rust is
  frozen except for concrete failures. No broader gate is run by this worker.
- First quick stopped at net formatting: the final unknown-field codec assertion
  needed rustfmt line wrapping. Applied cargo fmt across the six changed crates
  and restarted the scoped tiers. Log:
  target/contact-effect-frames-impl-quick-first.log.
- Corrected `python tools/check.py quick --crates
  bri-content,bri-weapons,bri-sim,bri-fx-runtime,bri-client,bri-net`: twelve steps
  in359s; 1,183 passed, zero failed, 83 ignored (76 client, three net, four sim).
  Log: target/contact-effect-frames-impl-quick.log.
- Final `python tools/check.py focused --crates
  bri-content,bri-weapons,bri-sim,bri-fx-runtime,bri-client,bri-net --depth 0`
  plus explicit targets client:contact_effect_frames, client:weapon_effects,
  fx-runtime:runtime, weapons:impacts, weapons:tactical_seams, weapons:sticky,
  sim:projectile_events, net:replication and net:protocol_changes passed:
  27 steps in1,036s, 160 result summaries, 2,283 passed, zero failed, 256 ignored.
  All six all-target Clippy checks passed with `-D warnings`. The tier runs full
  non-client integration suites and then the explicitly added targets; totals
  therefore include those planned repeated targets. All fifteen contact cases
  are enabled with zero ignores. Log:
  target/contact-effect-frames-impl-focused.log.
- `git diff --check` passed. No full gate, push, rebase or main integration was
  performed here; root owns the integrated candidate and exact-SHA handoff.

## Exact committed footprint

Root received this 24-file list before commit. Semantic production changes are
limited to content/effects.rs, weapons/runtime.rs, sim/presentation.rs,
sim/session/weapons.rs, fx-runtime/world.rs and client/weapon_effects.rs. Other
listed native constructor edits only supply contact:None; corresponding tests,
one protocol marker and own architecture/progress complete the scope.

```text
crates/client/src/app/fx.rs
crates/client/src/app/tests.rs
crates/client/src/avatar.rs
crates/client/src/brick_debris.rs
crates/client/src/explosion_debris.rs
crates/client/src/explosion_shapes.rs
crates/client/src/weapon_effects.rs
crates/client/tests/contact_effect_frames.rs
crates/client/tests/world_items.rs
crates/content/src/effects.rs
crates/fx-runtime/src/world.rs
crates/net/protocol-changes/contact-effect-frames.md
crates/net/tests/replication.rs
crates/sim/src/presentation.rs
crates/sim/src/session.rs
crates/sim/src/session/breakables.rs
crates/sim/src/session/packages.rs
crates/sim/src/session/spray.rs
crates/sim/src/session/tools.rs
crates/sim/src/session/weapons.rs
crates/weapons/src/runtime.rs
docs/architecture/contact-effect-frames.md
docs/architecture/seams.md
docs/progress/2026-10-16-contact-effect-frames-impl.md
```

## Remaining acceptance and ownership

Root owns integrated candidate/gate/release and exact-SHA original Halo rerun.
Original PC sign/roll/full-basis, contacted target/node attachment, cut-plane and
wood collision/render separation remain open. Mixed authored body/vehicle surface
properties need their own actual hit identity seam; Halo private5bf47c3/PR34 and
Source36 are evidence, never model/class/part-name heuristics. Original contact
audio repeat/promotion remains source-owned. Mounted authority/fitting and AA's
host-owned generation/completion clock remain active queued work; no timer or
expanded-slot workaround is introduced here.
