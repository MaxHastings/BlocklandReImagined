> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `bots/review-2026-10-07-final.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Final check of the released bot branch (ba8af8c), 2026-10-07

What I did: a read-only check of `claude/v0.2.5-s4s5bn` at **ba8af8c** against main (dfc143d). Nothing was built or run. I checked only the work in finish-plan sections 3A-3F and Max's two playtest items, using the finish-line rules: a v0.2.5 fix means a player would hit it, it's in code that was changed, or leaving it is a patch or hack. Paths are under `crates/sim/src/session/` unless they name a crate or a test. I re-checked items marked **(checked)** myself; the rest come from three readers and are each backed by file:line. **(inferred)** means reasoned from the code, not run.

## Verdict

**Not clear.** The cleanup work is real and mostly right:
- one weapon reader;
- one give-up memory;
- the stopgaps gone;
- the stall hop as a press;
- F3 "Acts" line;
- explore and throw scans on the budget;
- no map, mode or item names added.

Seven things a player can hit need fixing right after the release. Most of them are in the two newest pieces of work: self-respawn, and the "can it hurt" change.

## Fix right after release (a player would hit these)

| # | What a player sees | Cause | Fix direction |
|---|---|---|---|
| 1 | **A bot kills itself where players can't, and loses a point each time.** | Self-respawn sends Ctrl+K (`Command::Suicide`, `bots.rs:2432-2437`), but the game's "suicide allowed?" policy returns early for bots (`packages.rs:2868`) **(checked)**. Slayer's `enable_suicide` is skipped, and each one scores `points_kill_self` (-1) and burns a life. | Ask the game's suicide rule for bots too. Respawn scores 0 when it says no. |
| 2 | **A bot whose spawn brick is boxed in (pen, cage, pillar top) kills itself about every 11 s, forever.** (inferred) | The trap clock (`bots.rs:3066-3088`, constants `:189-205`) counts unreachable stroll goals as tries. Respawn needs only to beat Wander's 0.1. The bot respawns in the same box. The new sealed-in test seals the bot only *after* it has left its brick, so this case isn't covered. | No Respawn when its respawn point is inside the trap. Better: build Respawn on the existing stuck judge's give-ups at one spot (`route::Headway`), not a second clock with its own numbers. |
| 3 | **A bot riding as a passenger can Ctrl+K in the seat** when the driver parks. (inferred) | The only seat check is `driving.is_none()` (`bots.rs:3080`). Passengers aren't "driving". | Also exclude seated. |
| 4 | **A flag carrier hit by a new attacker stops and duels** instead of delivering. | `HURT` = 1.25 (`bots/contest.rs:83`) **(checked)**, so Fight = 0.8 × 1.25 = 1.0, which is above a committed carrier's 0.92. A new hit interrupts the hold. | `HURT` below 0.92/0.8. For example, 1.1 still lets Chase (0.66) beat the plain objective (0.65), as its own doc intends. |
| 5 | **Bots no longer kick a loose ball outside a game.** This was fixed on this branch (f4766c9), then broke again. The test went back on the known failures (`tools/gate-known-failures.toml:69`) **(checked)**. | Not run. The likeliest cause is 4797179: the idle mark now uses `bot_sees_player`, the ball blocks the view, and a miss is cached for 30 ticks (`bots/extras.rs:252-255`). | Find the regression. If that's not quick, turn idle ball play off rather than ship bots that walk up to the ball and stand there. |
| 6 | **Broom kits no longer shove an opponent off the ball.** Plan B1 said keep it as a Fight choice. | `has_possible_attack` skips anything without damage (`bots/combat.rs:245`) **(checked)**, so a broom-only bot "can't hurt", and menace gives it 0. There are three attack checks with different rules: this one, `item_attacks` (`combat.rs:280`) and push worth. The soccer test now *fails* a broom kit that fights (`crates/chaos/tests/bot_soccer_match.rs:1078-1081`) **(checked)**, while its doc at `:1217` still says brooms clear. | One attack check that counts a push, and drop the broom no-fight assert. Or record that the behaviour was dropped on purpose. |
| 7 | **A bot whose only attack is an unknown modded item walks up and never fires.** | The new single weapon reader returns nothing for an unknown scripted item (`bots/fire.rs:252-256`), but "can it hurt" still says yes (`combat.rs:258-262`). | "Can it hurt" reads the same reader (same root as 6). |
| 8 | **The tool goof can pull out a sword and stare at the floor.** | The melee filter (`bots/surprise.rs:1359-1369`) doesn't ask whether the target can be hurt. A hurtable target then takes the "look down" branch (`:1598-1603`). | Filter on "strikes up close and can't hurt this target". One line. |

**Release check.** Three known failures (`gate-known-failures.toml:74, 79, 84`) say the main checkout's content has no `blockhead_bot` install. Make sure the packaged release content does include it, or no bot spawns. If the claim is true, these entries' owner should be the content/bootstrap step, not "v0.2.6 bot work".

## Rule breaks in code that was changed (cheap; do with the fixes above)

- **Tests still gate on numbers (C4 half done).** 18 of 33 were converted and 2 of 6 deleted. Nothing was loosened instead. Still to do:
  - `shares::check` in the gauntlet's `finish()` (`crates/chaos/tests/gauntlet/mod.rs:540-545`) **(checked)**, and `all_dials_on` (`bot_gauntlet.rs:936`);
  - `bot_think_time_16` wall clock (`:988`), which isn't skipped;
  - the `fair_hit_rate` 15-60 % band (`:1347`);
  - the 16-bot profile still a known failure, not skipped;
  - the nav-spike limitation test (`bot_navigation_spike.rs:347`) **(checked)** and the aim-low share (`bot_tactics.rs:1080`) still present;
  - 11 single-number tests not converted (brain 449/996, routes 143/892, tactics 580/1043, interactions 1369, soccer_teams 672, slayer 4540).
- **Dead "clear" callout.** `TERMS` still has "clear" (`crates/sim/src/bot_kind/team.rs:11`), and `bots.json:22` still has "I got him, go!". Nothing reads either **(checked)**. Wire it to the Fight choice or remove it.
- **Rays still off the budget.** These were listed in round 3:
  - hand sight (`bots/extras.rs:750-752`, a one-line swap to `bot_sees_player`);
  - the goof aim check (`bots/surprise.rs:1638-1640`, edited in 669215b);
  - `surprise_surface` (`:1022-1067`) and `surprise_flanks` (`:1089-1092`).

  The report's "every sight scan on the shared ray budget" overclaims.
- **Second stuck judge.** The trap clock is a hand-rolled tuple (`bots.rs:351`) with its own numbers, beside the stall judge and `route::Headway`. See fix 2.
- **The F3 "Acts" line hides the winner.** `walk = controls.by.filter(|_| direction != 0)` (`bots.rs:3168`) shows "walk none" when the Stand mover wins, or when the edge held a walk back. Show the mover unfiltered.
- **Docs that now say the wrong thing:**
  - `docs/architecture/bots.md:133-134` says the stall hop is judged like any hop; 98f6859 reversed that.
  - The progress report says three known failures were removed because they pass, but ba8af8c re-added them.
  - It also says "every sight scan" is on the budget.
  - Cover is still a swap after the choice (`bots.rs:2314-2317`). That was kept on purpose, since soccer "ignoring ball" goes from 10 % to 37 % without it, but it isn't listed as kept or queued.
  - Stale comments at `bots.rs:1896`, `bots/behaviour.rs:24` (peaceful objective), `:73` (a `bot_trapped` that doesn't exist) and `:66-70` (`spared` now goes to -0.25).

## v0.2.6 notes (one line each)

- **13 test-only kind settings are still fields that bots.json refuses** (`bot_kind.rs:57-64, 125-145`, `bot_kind/team.rs:32-40`). Five are never set by any integration test (`glance_seconds`, `cooldown_seconds`, `view_degrees`, `mood_cap`, `mood_human`) and could become constants now. Players can't hit this.
- **Looks are chosen by an if-chain** (`bots.rs:2649-2781`), then one winner is ordered against Hold, Gesture and Down. The priority is written twice. It's correct today.
- **The seat adapter writes yaw, pitch and jump after the act stage** (`bots/interactions.rs:1454-1592`). It's older code.
- **The search sweep reads drift at the absolute tick**, so the first look is a random bearing (`bots.rs:2773-2782`). Cosmetic.
- **Unarmed bots with nothing at stake still walk up to an enemy** (`contest.rs:48-50`). This is by design and unchanged.
- **The spray can is still told by image id** in two places (`bots/surprise.rs:1645`, `bots/team.rs:530`), and only the blue can matches.
- **Soccer gates goals over all 9 matches, not per kit.** Rockets/spears/jeeps is still one seed.
- **The sealed-in test never checks that its walls were placed.**

## Share numbers against main (from the bot thread's 120 s matches, read once)

Stuck time:
- Nothing doubled at a meaningful size.
- Soccer (1.5 vs 0.8 %) and A TDM (1.5 vs 0.6 %) about doubled, but they're tiny.
- Close Quarters with swords is up about half (11.5 vs 7.4 %).
- Afghanistan has no main number to compare with (14.5 %).

Other numbers:
- Idle time dropped sharply everywhere (for example A TDM 5.4 vs 89.4 %).
- Soccer kills halved (36 vs 82), which fits threat scoring keeping bots on the ball.
- Nothing here needs a closer look under the finish-line rule.

## Clear

- **Stopgaps:** gone, with the arming rule in place.
- **One weapon reader:** `movement_weapon` gone, apart from the seam in fix 7.
- **Duplicates removed:**
  - one give-up memory (`Cooldowns`);
  - `RETRY` renamed;
  - `reflex_rng` gone;
  - limits from `objectives::limits()`;
  - `BotHold.margin`, `spawner_name`, `gunning` and `clearing` gone;
  - the strafe air guard gone.
- **Act stage:** the stall hop is a press, with its exemption from the landing check documented. The trigger has one owner a tick.
- **Rays and scans:**
  - explore's failed-scan memory is sound and charged first;
  - the throw's open-spot scan is on the budget and remembered.
- **Goofs:**
  - they end through the chooser, with no leftover private timer;
  - the door goof gives up within 8 s;
  - the detour picks an explore spot.
- **Hammer goof:** reads `melee` from data, not a name, and can't hurt anyone.
- **Search sweep:** no rays.
- **Watch tool:** prints "skipped" when unset.
- **No new map, mode or item names.**
