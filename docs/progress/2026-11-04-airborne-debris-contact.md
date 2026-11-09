# Airborne debris contact and solver admission — 2026-11-04

Maxwell finished preview.3 playtesting. NPC acceptance remains unchanged;
destruction remains subject to a new interactive playtest. This correction is
local cosmetic presentation, with no wire/save, authoritative damage or crushing
change. Existing architecture/seam ownership is retained.

Preview.3 contact tests forced `landed=true`, so they did not cover the actual
reported airborne player crossing. Complete sections also waited for first
static impact before publishing collision-aware clumps. Prepared clumps now
publish during flight, preserving the current shared transform and material
velocity field. Incrementally prepared proxies remain hidden from render,
attachment and moving-rubble queries until the complete publication.

The Pirate World repro has 147 clumps. A second 64-body cap admitted only 63
of them alongside another section, despite High's available body budget. The
remainder held angular orientation in the checked translation fallback. Removing
that duplicate cap retains the existing quality/body and 2048 proxy-part limits;
all 147 now use the solver in this replay. No random spin, render hinge, larger
blast or additional material is introduced.

Validation and evidence are private under
`E:/Downloads/BlocklandReImagined-Release-Evidence/v044-release-prep-2026-11-04`.
`cargo test --locked -p bri-client --lib brick_debris::` passes all 70 cases.
The initial section run exposed an obsolete assertion that planning must remain
pending until landing. The capacity regression now occupies the body slot before
admission and retains its checks for material, fallback, limit and later recovery;
its contract is preserved. The new regression uses ordinary airborne release,
then a crossing actor at zero and nonzero body budgets, without forcing landing.

The external-save `destruction_repro` runs pass for JetPuff's Towers and Pirate
World using the supplied views, two actual stock rockets, normal authority cues,
client motion and bounded offscreen capture. At High, Pirate update milliseconds
in debug changed from p50/p95/max 3.27/8.43/27.36 with the early-flight correction
alone to 0.55/3.33/6.53 with quality-owned admission. These are local replay
observations, not a general performance guarantee. Compared frame-240 captures
show more varied rotating contact/breakup; they do not establish accepted feel.
The saved rocket replay does not include Maxwell flying through the sail; the
synthetic airborne-contact regression covers that previously absent mechanism.

`cargo clippy --locked -p bri-client --all-targets -- -D warnings` passes on
the final correction. Seat fallback-anchor and pose-composition consumers pass
four and seven cases respectively. Cargo fmt for
the entire workspace hits Windows command-length error 206, so changed Rust
modules are formatted directly with rustfmt edition 2024.

Retained limits: complete delivery/GPU readiness and bounded preparation can
still delay onset, overflow is translation only, and the shared static witness
trajectory still supplies the section-level landing cue. Cosmetic motion does
not push/damage players. These must be represented honestly in the handoff;
no final destruction approval, release gate or public release is claimed here.
