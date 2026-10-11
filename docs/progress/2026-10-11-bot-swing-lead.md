# 2026-10-11 The fire gate judges a swing from its wind-up

The 3x gauntlet soak had team kills in `swords_four_a_side`. One was traced
per tick: bot 4 pressed its sword at tick 1500, judged then as sparing its
side (planned harm to allies 0). The swing's shot left after the sword's
PreFire, and by then ally 6, closing on the same enemy, had stepped into it
and died at 1508. The gate had predicted bodies moving only over the shot's
flight, as if it left at the press.

Fix: `combat::launch_lead` reads the image's own states, from the state its
trigger leads to through to the one that fires (a swing's PreFire; nothing
for a gun). `validate_fire` adds it to every chord's time, so the bodies
about, an ally among them, are grown by how far they move before the shot
leaves as well as during its flight.

Evidence:
- `combat::tests::a_swing_is_judged_from_its_wind_up_and_a_gun_from_its_press`.
- `swords_four_a_side` at 1, 2, 4 and 5 rounds: team kills 1, 1, 1, 1 on main
  and 0, 0, 0, 0 with the fix. Team damage at 5 rounds went from 130 to 60.
  Enemy kills at 3 rounds went from 105 to 115.
- `cargo test -p bri-sim --lib` and every `bri-chaos` `bot_*` suite pass,
  except `bot_soccer_teams::a_bot_scores_in_its_own_teams_goal_after_save_and_reset`,
  which already fails on Linux main (PR #65).

Not asserted in the scenario: team kills there depend on who steps where in
a seeded match and vary between machines, which is why
`own_side_report` prints them rather than asserting them.
