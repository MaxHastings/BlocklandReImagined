# 2026-10-29 Large-collapse gameplay review

Maxwell reports large chunks losing environmental/player interaction, floating
until disappearance, and oversized rigid assemblies that never break apart.
Reviewed the recent destruction lane at `f099209b7`, including the capacity
retry correction `93ee85794`. The chat's main-directory checkout is older
(`591ff3c72`); it is not the reviewed implementation. No exact current report
was reproduced, and no visible game, GPU suite, benchmark or cargo build ran.
Other threads' changes and original installations remain untouched.

## Findings, in correction order

1. **Player proximity ignores chunk extent.** `BrickDebris::push`
   (`crates/client/src/brick_debris.rs:959`) tests player/vehicle distance to
   body centers against `PUSHER_REACH + p.half.max_element()`. It includes
   chunk handles but not their bounds. An actor touching the far end of a
   long chunk can therefore be excluded even with no crowd or capacity
   pressure. Use actual swept actor/chunk bounds for candidate admission,
   retaining compound narrow-phase gaps. Add a large-chunk endpoint regression;
   the ordinary small-debris player test cannot cover this failure.
2. **Temporary pressure becomes permanent coarse geometry.**
   `Section::partition` (`sections.rs:440`) chooses target member count from
   available slots at planning time. One free slot can plan a 1,500-member
   connected section as one body. Once chunks exist, the partition guard
   prevents later refinement; impact has no further breakup path. Even without
   pressure, the 64-body ceiling gives a 10,000-member connected section a
   target of 157 members per body at quality 2048. Member count alone also
   places no bound on physical span. Preserve a spatially bounded breakup plan
   independently of transient body occupancy; schedule physical activity rather
   than permanently welding more material to fit current slots.
3. **Representation overflow still permanently removes physical interaction.**
   `keep_shared` (`sections.rs:412`) discards chunk assignments and sets a
   terminal shared-only flag. More than 64 disconnected groups, a chunk proxy
   exceeding 64 boxes, or intrinsic global limits can enter this state.
   `proxy.rs` preserves gaps correctly; the fallback loses dynamics instead.
   The overload test at `sections.rs:1765` explicitly requires 100 visible
   members, zero bodies and shared-only state. Source-box projectile recoil
   does not supply gravity, environment response or player pushing. Define a
   recoverable local-interaction lifecycle for retained visible material.
4. **Freezing and collision preparation can produce visible non-interaction.**
   `Chunk::freeze` removes colliders and permanently marks a chunk released;
   quality eviction and source-member retirement can use it. Collision-window
   preparation separately disables a live body when coverage is unavailable
   (`sections.rs:792`). The first is terminal; the second can recover. Shared
   first-contact motion stops the whole section at the earliest sampled
   contact, so waiting/fallback can leave overhanging material suspended. These
   are code-supported mechanisms, not proof of which caused Maxwell's scene.
   Preserve distinct preparation, sleeping, suspended, retired and terminal
   states in diagnostics; sleeping Rapier bodies already retain interaction.

## What the latest fix actually establishes

`93ee85794` correctly distinguishes zero temporary slots from Physics Off and
retains a completed plan when capacity changes before admission. Its recorded
two regressions show eventual body allocation and ordinary player displacement
after capacity clears. It does not fix center-based pusher exclusion, coarse
planning with one available slot, proxy/disconnected terminal fallback, later
impact breakup, or collider removal by freeze. Earlier receipts remain valid
within their stated scope; this review does not claim they prove large-collapse
gameplay acceptance.

## Validation and next supported increment

Read the current design, constraints/lifecycle audits, capacity-retry receipt,
partition/proxy/motion/pusher implementations and existing tests. A standalone
PowerShell arithmetic check of the actual grouping formula yielded targets
500 (1500 members, quality8, eight slots), 1500 (1500 members, quality2048,
one slot), and157 (10000 members, quality2048, 64 slots). An illustrative
12-unit endpoint distance with actor max half-extent1 fails the current7-unit
center threshold. These are deterministic code/formula checks, not integrated
physics tests or a reproduction of the reported save.

Next: correct extent-aware pusher admission, then implement one cohesive
recoverable breakup/activation lifecycle. Acceptance must include actors at
chunk ends and holes; one-slot planning followed by capacity recovery; more
than64 disconnected groups; irreducible proxies; airborne budget eviction;
support removal under settled debris; repeated blasts; and meaningful breakup
at ordinary landings. Keep exact visible material, gaps, finite cost, existing
server authority/save/respawn, and player-owned interactive testing. Do not
close this report with increased caps, stronger impulses, or an allocation-only
test. The current architecture document accepts several fallbacks that this
feedback now rejects as satisfactory gameplay; its acceptance wording needs
to change with implementation, without discarding the valid resource bounds.
