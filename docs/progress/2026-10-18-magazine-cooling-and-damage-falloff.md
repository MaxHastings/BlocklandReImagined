# 2026-10-18 Magazine cooling and projectile damage falloff

Two small generic weapon seams from the Halo lane (`codex/halo-covenant`), for
the plasma guns. Both are inert unless a pack sets them.

Before: a Halo plasma gun's heat could only leave a gun by venting an empty
magazine, so six pistol bolts, a pause and one more bolt overheated it, and
a bolt did its one authored damage at any range.

After: `magazine.cool_ticks` returns a round every that many ticks while the
gun is held and its magazine is neither full nor empty (nothing refunded from
the reserve), and `projectile.falloff` (`near`, `far`, `far_damage`) moves a
projectile's direct damage in a line between `near` and `far` units flown.

How: `WeaponsWorld::step` runs `cool_magazine` after `finish_reload`, keeping a
host-only next-round tick per magazine; the contact damage reads
`Falloff::damage_at` from the distance to where the projectile was made.
Tests: `crates/weapons/tests/cooling_and_falloff.rs` (the three damages, the
cooling timeline and its stop at full, the schema limits). No wire change.
