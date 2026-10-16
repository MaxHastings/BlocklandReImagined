# 2026-10-15 Client-local cosmetic contact audio

Maxwell directly requested continued contact-audio implementation, coordination
with root and Halo, and no guessed original repeat/promotion policy. Root assigned
`codex/cosmetic-contact-audio` from immutable `5158ade8` in the existing
`shared-engine-seams` checkout, reserved the listed engine/client paths and wrote
the included registry row. Halo's preview checkout remains unchanged. No agents,
manifest/lockfile changes, wire events, original assets or private research code.

## Mechanism and policy

The private Halo `halo/CONTACT-AUDIO-HANDOFF.md` at `13f0cb4c` separates
recovered material/profile/permutation relationships from unrecovered runtime
repeat/promotion behavior. A promotion count alone does not establish a bounce
ordinal or counting window. Those private relationships are not embedded in the
engine; bindings remain Halo's responsibility.

Optional `Particle.contact_audio` retains the existing version-1 effects policy.
The package must author surface/profile mappings, optional explicit fallback, and
all four eligibility fields: minimum inward speed, cooldown in particle time,
maximum inward-contact ordinal, maximum simulated impact lateness. No original
Halo thresholds, first-hit/repeat behavior, promotion windows/count domains,
zero-distance interpretation, original audibility order or deterioration law are
inferred. Native cooldown history is explicitly defined before surface lookup or
mixer audibility; a package must accept that adaptation or leave the response
absent. Without an explicit response, existing particles retain motion, visual RNG and silence.

FX reports only real inward hits, before bounce or contact death. Support probes,
rest frames, outward/separating hits, expiration and rendering views do not emit
requests. Original sweep endpoints, fraction, normal, point, incoming velocity,
source/emission identity and impact ordinal are retained. Stable seeds do not
consume visual RNG or the general audio variation counter. Eligibility history
advances before report capacity checks; drops cannot shift later variant choices.

At most 64 reports survive each effects advance; a later advance discards
unconsumed reports. The client retraces the exact original sweep against the same
immutable Building snapshot and verifies closest-hit distance and normal before
using the existing surface metadata. It does not shoot a new short ray near the
impact or infer a material from content names. Missing geometry/mapping is silent
and counted. The existing audio adapter queues at most 16 local one-shot starts
across all submissions before its tick, after setting the current listener. App
calls this tick once after world presentation per rendered frame. Unknown/looping
profiles and overflow are silent counted drops. Prepared weighted choices retain
profile gain and ordinary mixer spatialization/voice limits. Queues drain once,
never defer overflow, and session reset clears reports/starts.

These caps are native containment, not claims about original-game audibility.
The host still clamps weapon cosmetic advancement to its existing 0.25-second
frame step. Lateness is simulation time within the supplied advance, including
emission pre-age; it is not an independent wall-clock timestamp. No dynamic-prop,
vehicle, water or moving-support collision mechanism is added.

## Evidence

Cargo commands used process-local `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4`; `CARGO_TARGET_DIR` stayed unset.
All execution was code/headless/offscreen, with no visible game launch or
interactive controls.

- `cargo check -p bri-client --tests --locked`: passed initial implementation.
- Initial synthetic runtime run: six contact-audio tests passed, including
  invented authored cooldown/contact limits and overdue emission rejection.
- First targeted client compile caught test-only assumptions that SoundAsset
  implements Clone. Replaced those fixtures with public SoundAsset::world and
  shared ClipData; no production change was needed for that error.
- Initial runtime test compile caught usize/u32 fixture count mismatch; corrected.
- `cargo test -p bri-client --lib --test contact_audio --locked contact_audio`:
  one offline audio unit test and both client adapter/surface integration tests
  pass. Actual Building geometry rejects a nearer occluder, opposite face and
  absent geometry. The adapter verifies original sweep evidence, unknown mapping
  silence, consume-once behavior and reset of unconsumed reports.
- `python tools/check.py quick --crates bri-client,bri-content,bri-convert,bri-fx-runtime,bri-net`:
  10 steps in 243 seconds; 810 passed, zero failures, 79 existing ignored.
  Client 578/76 ignored, content 92, convert 55, FX 4, net 81/3 ignored.
  Log: `target/contact-audio-quick.log`.
- First focused attempt stopped at warnings-denied Clippy's `collapsible_if`
  in the new eligibility block. Collapsed it to a let-chain without changing
  policy, then `python tools/check.py quick --crates bri-fx-runtime` passed
  (four unit tests, zero ignored; two steps in two seconds). Failure log:
  `target/contact-audio-focused-first.log`; corrective quick log:
  `target/contact-audio-fx-quick.log`.
- `python tools/check.py focused --crates bri-client,bri-content,bri-convert,bri-fx-runtime,bri-net --depth 1 --tests bri-fx-runtime:runtime --tests bri-client:contact_audio --tests bri-client:weapon_effects --tests bri-client:actor_effects`:
  **37 steps passed in 1,975 seconds**. All five changed crates passed
  warnings-denied, all-targets Clippy; changed crates and direct dependents passed
  their tier-defined suites. Summed test executions: **3,549 passed, zero failed,
  396 existing ignored**, including the requested explicit-target repeats.
  Log: `target/contact-audio-focused.log`.
  The explicit FX runtime target passed 34/3 ignored (all seven new contact cases
  passed), client contact target 2/0, weapon effects 17/1 ignored, and actor
  effects 14/1 ignored.
- Independent synthetic checks cover opt-in motion/RNG parity, inward/death hits,
  rest/support/outward/expiry silence, authored speed/cooldown/ordinal eligibility,
  pre-aged emission rejection, explicit fallback versus unknown silence, stable
  choices after compaction/unrelated audio/multiple snapshots, replacement of
  undrained reports and reset. A 1,000-impact burst yields exactly 64 reports and
  936 counted drops; 1,000 pre-drain sound submissions yield at most 16 starts,
  with correct listener order and no next-tick replay.
- `git diff --check` passed. Scope review retained only the assigned implementation,
  constructor defaults, tests, modding section, root-authored registry row and this
  progress entry. No full gate, release package, original-content listening test
  or public push was performed; root owns the integrated candidate.

## Next work

Root owns shared integration/full candidate gate. Halo owns source binding and
original repeat/promotion recovery. Bind only recovered policy or an explicitly
labelled native adaptation; exact original repeat/promotion acceptance remains
open. Maxwell owns all interactive listening/playtesting. The active checkout
cache is retained per root's instruction.
