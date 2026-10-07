# 2026-10-07 Capabilities: timed throws (grenades)

Branch `claude/project-thread-uoh3ap`, on fighting `b6280a24` plus loop 3
`498d7f58`. Design and review conditions:
`docs/plans/v0.2.6-design-grenades.md` (§8 and §9 say how the code meets
them).

## What changed

- Bots throw grenades from any pack: a projectile that bounces before it
  can burst, or a fused image that throws on the press, is a
  `Delivery::Timed`. The bot follows the throw past each bounce with the
  host's own rules to where it goes off, and counts it only when that is
  within its blast of the enemy where the enemy will be by then.
- One bounce rule (`bounce_velocity`) and one set of burst rules
  (`expires`, `hit_bursts`, `rebound`) in `bri_weapons::runtime`, used by
  the host, bots and the client's projectile prediction.
- Fragments (`children`) no longer make a weapon unusable: they count no
  damage, and widen the danger zone a throw keeps clear of the thrower and
  allies by how far they fly plus their own blast. Harmless fragments
  (sparks, smoke, a trail) widen nothing. Fragments of fragments, lingering
  or never-expiring fragments, or ones that hurt in flight are refused.
  No stock v20 projectile has fragments, so no stock weapon became usable
  by allowing them.
- The planned flight casts one ray per chord of the arc, as long as the arc
  sags under a plate off it (`fall_per_tick * k^2 / (8 * HZ)`). Review
  caught the first version missing the `HZ`, which spent about 11 times
  the rays needed.
- A timed throw aims at the feet.

## Evidence

- `a_planned_burst_matches_the_host_flying_it` (`bri-sim` lib): planned
  burst tick equals the host's for an armed hit, the last bounce, a cooked
  fuse, the end of life, a stuck throw and a rolled one.
- `a_bot_throws_an_unfamiliar_grenade_that_goes_off_near_its_enemy_not_itself`
  (`bri-chaos` `bot_tactics`): a synthetic canister with shards, three
  distances: 14 thrown, 13 went off near the enemy, none near the thrower,
  the thrower unhurt.
- Tactics unit tests for the timed delivery, the cooked fuse and the
  fragment reach.
- `a_timed_throw_spends_one_ray_a_chord`: the chord is the longest whose
  sag stays under a plate, and an open-air throw spends exactly one ray
  per chord.
- `bot_think_time_16` (release): 672 us/tick as shipped; 507 us/tick with
  the arsenal's bouncer made a grenade (a blast and `explode_death`,
  measured locally and not committed). Both under its 2000 us bar.
- `cargo clippy --workspace --tests -- -D warnings`: clean.
- `bri-chaos` `bot_tactics` has two failures that also fail on fighting
  `b6280a24` by itself (`a_close_ranged_bot_backs_out_from_under_a_low_roof_and_delivers_a_safe_blast`,
  `replacing_the_charged_equipment_cancels_without_throwing`); reported to
  the coordinator for the fighting lane.

## Next

The Add-On grenades (`weapon_hegrenade`, `weapon_package_explosive1`)
checked headless on regenerated content; my half of the acceptance run.
