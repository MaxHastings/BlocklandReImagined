# 2026-10-22 Casing contact separation

Maxwell reported continuously repeating casing impact sounds during the private
Interdiction preview. A native mesh casing remained exactly on a two-sided map
triangle after its first bounce. The next surface ray returned another contact
at distance zero while it moved away, reversing it and emitting another sound.

The existing test's toy floor required start.y > 0 and hid this condition. It
now uses the client's Building::surface_at query on actual triangle collision,
including inherited horizontal motion that keeps the casing above its rest
speed through the first bounce. Before correction it failed: an energetic
casing never rose from the floor after impact. Reuse the existing initial-clip
contact margin on each resolved hit. No audio cooldown, content-specific
branch or change to package-authored bounce/rest/lifetime was added.

All nine source-free weapon_debris unit checks pass; four generated-content
variants await the integrated gate. Baseline and passing logs are under the
integrator's ignored `.research/pipeline-integration/`, named
`casing-real-surface-moving-baseline.log` and `casing-contact-correction.log`.
Quick passes in 64s (client and net); focused client checks pass in 149s,
including clippy with warnings denied and 661 unit checks. The integrated gate
is pending. Full-workspace cargo fmt hit the
Windows command-line length limit; rustfmt was run on the changed source and
the normal tiered formatter remains required. Interactive audio acceptance
belongs to Maxwell. Original AA emitter transform corrections remain private
Add-On work, independent of this generic contact defect.
