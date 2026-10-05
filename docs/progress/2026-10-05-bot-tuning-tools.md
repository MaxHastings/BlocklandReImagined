# 2026-10-05 Bot tuning tools

Branch `fix/bot-tuning-tools`, from `claude/project-thread-pt64ji`
(e1e2c30c). Max approved the tools; the rule is *nothing dormant, nothing
dominant*. This is test and tooling work: no bot behaviour or shipped
setting changed. How to run each tool is in
[docs/architecture/bots.md](../architecture/bots.md), "Tuning".

## What landed

- **Share report** (`crates/chaos/tests/gauntlet/shares.rs`,
  `tests/data/behaviour_bands.json`). Every gauntlet scenario prints the
  share of bot time per kind (behaviour, `goof`, `vehicle`) and per option
  at the other choice points (`aim:feet`, `route:left`), read from
  `BotThought` and the chooser's candidates, against a band. Offered but
  under the floor is DORMANT, over the ceiling DOMINANT. Enforced today: the
  floors of `fight` and `objective` (a bot offered either takes it). No
  existing gauntlet assertion changed.
- **Off-switch check** (`off_switches`) and **sweep** (`dial_sweep`), both
  `#[ignore = "tuning tool: slow"]` and `[[skip]]` in the push gate. They
  replay the gauntlet's scenario functions with a thread's dials and seed
  applied to every kind (`gauntlet/tuning.rs`), write CSV and text to
  `target/bot-tuning/`. Dials are found by name: every section's
  `strength`, plus the list in `tests/data/bot_tuning.json`.
- **Fair metric** (`fair_hit_rate`, in the gauntlet; `fair_by_dial`
  opt-in): one bot per weapon class against a scripted player who strafes
  and hops; a package takes the target's damage away and counts it, so a
  fight reaches a steady state.
- **All-on run** (`all_dials_on`, ignored by default, run by the push
  gate): every scenario with every dial on.
- **Performance bar** (`bot_think_time_16`, `#[ignore = "timing; run in
  release"]`): 16 bots, all dials on, bot think time per tick. Measured with
  `Session::bot_think_nanos`, a wall-time counter around `step_bots`: a
  diagnostic only, it changes no game state.
- **Live dials**: `/botset <path> [value] [kind]`, `/botreload`, `/botsave`,
  admin only (`crates/sim/src/session/bots/tuning.rs`, dial paths in
  `crates/sim/src/bot_kind/tuning.rs`). Overrides go to
  `bot-overrides.json` in the client state directory (beside
  `settings.json`) and every hosted game applies them as it starts
  (`HostSetup::bot_tuning`). They travel over the existing chat-command
  path; no wire change.
- **F3 why readout**: the bot the host player looks at shows its choice,
  top 3 candidates, biggest terms and what it noticed
  (`BotThought::why`). It is carried host-locally in `ServerPerf` and
  refreshed four times a second only while the overlay asks. No wire
  change, so it shows only for games this computer hosts. The teamwork,
  mood and crowding terms named in the request do not exist on this base;
  the readout shows the terms the chooser has (drift, boredom,
  effectiveness, hold).

## Evidence (debug build, shared 4-core container)

`cargo test -p bri-chaos --test bot_gauntlet`: all green (13 tests plus
the new ones). Share flags on the base:

| Scenario | Flags |
|---|---|
| deathmatch_open_field | fight 93.1% DOMINANT, chase 0.2% DORMANT, goof DORMANT |
| deathmatch_mixed_arsenal | fight 93.8% DOMINANT, chase 0.6% DORMANT, goof DORMANT |
| rooftop_brawl_without_rails | fight 85.7% DOMINANT, chase 0.0% DORMANT, goof DORMANT |
| capture_the_flag | chase 0.1%, search 0.0%, goof: DORMANT |
| weapons_lying_on_the_ground | chase 0.1% DORMANT, goof DORMANT |
| every other scenario | goof DORMANT only |

Goof is DORMANT everywhere because `surprise.strength` ships at 0.
Variety ranges from 0.0 bits (the race, objective 100%) to 2.14 bits
(jeeps).

**Off-switch check** (12 scenarios, 1 seed): `surprise.strength` is OFF
AT BASE (a dormant dial). Turning it on to 1 moves play and breaks 4
scenarios' own bounds: jeep stuck 10.4% > 8%, rooftop idle 1.8% > 1%,
water not crossed, weapons-on-ground 3 of 4 armed.
`behaviours.interact` at 0 leaves the jeeps unused. `behaviours.objective`
at 0 breaks the four objective scenarios. No dial is a cut candidate.
Frame-cost deltas (-28% to -34%) are timing noise from parallel workers
and are not judged.

**All-on run** (`surprise.strength` 0.6, the only dial shipped off): one
failure, zombie_survival switches 12.2/min > 12. It is listed in
`tools/gate-known-failures.toml` with owner `fix/bot-surprise`. Goof
stays DORMANT in every scenario even at 0.6: interrupts need a natural
pause, which these scenarios never reach. This is gap 1 of the NPC
edge-case audit.

**Fair metric**, steady hit rate (band 15-60%): gun 100% at 12-13 units
from the first second (near-perfect aim, as the review expected). Bow
88-90% at 11-12. Rocket 43-50% at 30-35. Shotgun 6% at 34-36 (it fights
from too far). Bouncer 0% at 11-14. Overall 57% steady, 46% in the first
2 s. Reported, not enforced.

**Fair by dial** (`fair_by_dial`; `perception.alertness` is not on this
base, so it moved `aim_error_degrees`): steady 56.4% at 2.5 degrees,
57.0% at 5, 57.1% at 10, flagged NOT MONOTONE. The aim error shrinks to a
third after 2 s of tracking and the steady phase starts at 4 s, so at these
ranges the error stays inside the hit box: the dial does not move the
steady hit rate. Misses come from lead and range (rocket, shotgun,
bouncer), not from aim error. Bounded hit rates need a perception or aim
dial that acts after the first seconds.

**Sweep** (`BRI_TUNING_SCENARIOS=race,runners`, 9 points, 1 seed, 35 s):
`behaviours.objective=0` ranks last (2 scenarios broken). Every other
point ties at 2 bands flagged (goof DORMANT in both). The ranking between
them follows frame cost, which is noise. Output:
`target/bot-tuning/sweep.csv` and `sweep_summary.txt`. Scenario filters
match function-name substrings (`capture`, not `ctf`).

**Perf**, 16 bots, mixed arsenal, all on: bot think time 7528 us a tick
(470 us a bot), whole step 8474 us, in a debug build. The debug bar is
15000 us. The release bar of 2000 us is not measured: there was no disk
for a release build here. Confirm it on a release build.

## Next

- Max: run `/botset surprise.strength 0.6`, play, and `/botsave` what
  feels right. Each lane that adds a main dial names it `<part>.strength`
  or adds its path to `bot_tuning.json` `dials`.
- When the perception lane lands, `fair_by_dial` reads
  `perception.alertness`. Enforce the fair band once aim is tuned.
- Measure the release perf bar on Max's PC:
  `cargo test --release -p bri-chaos --test bot_gauntlet bot_think_time_16 -- --ignored --nocapture`.
