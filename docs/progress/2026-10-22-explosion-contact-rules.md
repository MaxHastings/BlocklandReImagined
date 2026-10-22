# 2026-10-22 Explosion contact for package damage rules

`on_explosion` now receives optional `nx/ny/nz` components from the current
projectile collision. A fuse/lifetime explosion without a current contact
receives `()` for all three. The visual point, native radius damage and
perception stimulus retain their existing positions. Package policy chooses
any separate damage origin using the actual normal and its authored dimensions.
The existing explosion hook owns this seam; no second collision query is added.

The underlying `bri_weapons::Event::Blast` is an internal event, not a network
message. Checkpoint/Delta projectile state and presentation cues are unchanged,
so this change does not add a protocol marker.

## Evidence and checks

- `cargo test --locked -p bri-sim --test item_hooks on_explosion -- --nocapture`:
  both tests pass. A real floor impact supplies its upward normal to rules;
  a bouncing projectile that later expires supplies no current contact normal.
- `python tools/check.py quick`: format checks and 412 sim / 14 weapons unit
  tests pass; four existing sim long cases remain ignored in this tier.
- The private ordinary Pipeline Grenadier consumer initially produced an armed
  ground explosion with an exposed enemy still at 100 health. Applying its
  source-backed collision-height offset through this seam changes the same
  impact to a splash kill. Visual explosion position remains unchanged.
- The extended private consumer passes launcher switching/reload/Use exclusion,
  an early unarmed bounce, preserved rifle ammunition, a next-round Grenadier
  kit and splash damage to a normally admitted defender NPC. The private rules
  also correct human-only victim enumeration and a stale class-body predicate.
  `kit-class-reset-fixed.log`: one test passes in 79.14 seconds. The fixture
  arranges positions only; actual class/trigger/reload/reset/rules cause results.

- `python tools/check.py focused`: passes in 1,061 seconds, including Clippy
  with warnings denied, changed crates and dependent integration tests, and
  661 client unit tests (77 existing ignored cases). No failures or retries.

The integrated push gate is required before this candidate lands. Logs are
local under `.research/pipeline-integration/explosion-*.log`.
The private handoff records its matching Add-On revision and full content check.

This is authoritative gameplay verification, not rendered/audio acceptance or
source-fidelity completion. Projectile collision volume, permanent dud resolution
and bounce damping remain disclosed private conversion gaps. The next Pipeline
correction targets allied head-on passage avoidance: the ordinary trace shows
both soldiers choosing a sideways pass where neither can fit.
