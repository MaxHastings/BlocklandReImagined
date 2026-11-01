# Extended bot-test lifecycle follow-up

The `authored_tool_loss_releases_the_method_and_accepts_only_real_native_outcomes`
fixture records a genuine item removal, then the bot is killed by the author
at tick 386 and respawns at 658 with the game's normal loadout. Its former
assertions required the removed tool to stay absent for the entire 40-second
scenario and the author's score to stay zero, even across that credited death.

The fixture now enforces removal, image/hold release, and non-reacquisition
throughout the affected life. If the spawn tick changes it requires an actual
recorded death; it continues checking real goal entry, winner and guard outcomes
through the rest of the scenario. The author's expected score equals the
number of their recorded credited kills, so an unrelated objective award still
fails. All five `bot_creator_adversarial` cases passed with the playtest profile;
`cargo fmt -p bri-chaos -- --check` and changed-target Clippy passed.

The release gate's door test is tracked as #419. Two tentative changes
(centered crouch sampling and nearby settled-route invalidation) did not fix it
and were reverted. The relaxed-perception test currently expects wandering after
three seconds, whereas the bot can search the last known area. A trial waiting
up to 60 seconds for a true wander reached that state and confirmed the
reported relaxed reaction interval, but the final wound-time comparison remained
unstable (285 vs 289 ticks). The trial was reverted; this test and the related
behavior need further supported investigation.

A release-profile `--perf-only` run produced collage 6.70 ms (8.00 ms budget)
and Afghan 5.01 ms (5.50 ms budget), 32 bots/3600 ticks each. Compilation
overlapped part of this run, so it is diagnostic and must be repeated without
that interference for clean release evidence. A full gate is still failing;
no release or main update has been made.
