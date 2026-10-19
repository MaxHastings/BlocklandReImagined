# 2026-10-19 Test cost cut

Max asked for the tests that cost more than they prove to go. Ranking and
reasons: `/mnt/project-files/audits/test-cost-cut-list-2026-10-19.md` (from
the PC gate's `gate-timings.json`, 5,843 binary-seconds over 480 binaries).

## What changed

- **Release-only mechanism.** `support/variants.rs` gains
  `synthetic_and_release_content!`: the same pair as
  `synthetic_and_content!`, with the real-content test named
  `release_content`. `tools/gate.py` skips every `release_content` test and
  every `[[release_only]]` entry of `tools/gate-known-failures.toml` unless it
  runs with `--release-content`. 45 client test files moved to the new macro;
  `content_root` and `app_flow` keep `synthetic_and_content!` (real content is
  their point).
- **Opt-in tools** (`[[skip]]`, run by hand): `bot_battle_perf`,
  `bot_combined_perf`, the three `brick_audit` scenes, the eight `night_qa`
  tests, `app_soak`'s five-round soak, the gauntlet's `all_dials_on`.
- **Deleted:** `bot_firefight.rs` (a v0.2.0 crash investigation),
  `net_chaos.rs` (same property as `crossover_soak` and `session_chaos`;
  `docs/crash-hunt.md` updated).
- **Shrunk:** `weapons_fuzz` 64/96 to 16/24 proptest cases (fixed seed, so
  the cases are a prefix of the old run); `session_chaos` 4 seeds to 1 (the
  first seed is the same one); `bot_soccer_match` 3 seeds to 1; `mirror_render`
  split so Slopes is release-only and Bedroom stays.
- **Release-only entries:** `soak_all_features_with_classic_add_ons`, the two
  `bundled_in_game` tests, `stress_sized_join_over_a_throttled_link`, the
  real-content `session_chaos` shards, and four synthetic runs whose proof
  another per-landing test already gives.

## Not verified

Nothing was run. Compile checks only (see the branch notes). The soccer
match is the one change that is not a strict prefix of the old run: its
aggregate checks (goals over all matches, brooms shoved, goof share) now see
three matches instead of nine. If it fails on the first landing, go back to
two seeds.
