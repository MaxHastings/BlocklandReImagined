# 2026-10-08 v0.2.7: gauntlet seeds and own-side harm as a report

Scope (narrowed by Maxwell's "too biased on fixing tests rather than how the
bots play"): the seed crash, the teamkill whole-match asserts, and only the
asserts blocking today. No wider test rewrite.

## What changed
- **Seeded runs install packages.** `tuning::with_seed(n > 0)` joined `n`
  seed players while `Arena::custom` built the arena, so every scenario that
  installs a package afterwards (zombie survival, capture the flag, the fair
  run) panicked with "Packages are enabled before players join"
  (gauntlet/mod.rs, was lines 107 and 182). The seed players now join just
  before the first builder (`Arena::seed_players_join`): they still take the
  first ids and ticks. Test: `a_seeded_run_installs_its_package_before_anyone_joins`.
- **Team and self kills are a report.** `team_kills == 0` and
  `self_kills == 0` over a whole seeded match were how that match happened to
  go; navigation changed who walks into whose line of fire and they went red
  in v0.2.6. Every gauntlet battle now prints them (`own_side_report`). Still
  asserted: `sane`'s `bad_plans` every tick (no planned shot trades its side
  for less or kills a teammate), and `each_side_hurts_its_own_less` in every
  battle (review: a wide margin today, and it catches a mass friendly-fire
  regression `bad_plans` cannot see, such as melee into allies, unplanned
  presses or aim error). It is new on swords_four_a_side, which passes it. Deterministic
  coverage: `bots/harm.rs`, `combat.rs`'s
  `an_unplanned_press_fires_only_when_it_spares_its_own_side`,
  `bot_tactics.rs`'s close-blast trade test. The leap/hop arc in ally
  prediction gets its own mechanism test with the teamkill fix ("Teamkills,
  Close Quarters, fog" thread).
- The three known-failure entries (capture_the_flag, zombie_survival,
  all_dials_on) are removed.
- **Driving regression as a mechanism test.** `nav.rs`
  `a_chassis_sunk_into_its_suspension_stands_on_the_ground_under_it` (0.4 s):
  the chassis's own cell has its floor when its ray starts on the ground's
  top, and a drive search finds a path. With f1236abd's `SETTLE_GAP` lift
  reverted it fails; that one bug made four gate tests red in v0.2.6.

## Evidence (Linux, debug, cloud)
- `cargo test -p bri-chaos --test bot_gauntlet -- --include-ignored --skip
  off_switches --skip dial_sweep --skip fair_by_dial --skip bot_think_time_16`:
  23 passed, all_dials_on included, 281 s.
- `cargo test -p bri-sim --lib nav::tests::a_chassis_sunk`: passes; fails
  with the fix reverted.
- `cargo clippy -p bri-chaos -p bri-sim --all-targets -- -D warnings`: clean.

## Not done (on purpose)
An inventory found other whole-match asserts (kills > 0, `fell == 0`, the
soccer totals, behaviour bands). None is red or flaky today, so they stay.
The 8th v0.2.6 red, bot_soccer_match's 600 s cap, belongs to the gate-speed
thread.
