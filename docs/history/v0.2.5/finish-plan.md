> **Historical note. Written 2026-10-06 (the file's date in the cloud project folder, `v0.2.5/finish-plan.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# v0.2.5 finish plan

Goal: "Same game, simpler and solid inside."
Ship bar (Max): only bots we're confident delivering. That means low-hanging fruit, solid design and solid gameplay.
Everything ships together, and the release waits for the bots.

## 1. How we make close calls
Apply these in order. When two conflict, the earlier rule wins.

1. **Core design fix?** That is, it removes a duplicate mechanism, a special case, something on its own timer or an unbudgeted cost, or it makes problems easier to see. → Always in v0.2.5, with no time limit on coding.
2. **Never a special case** by map, mode, item name or save. Items are judged by what they can do, not by their name. One mechanism per job.
3. **A behaviour players can see that's badly wired inside** → keep it and fix the wiring. Remove it only if fixing it is a big job. Then it's queued for v0.2.6 and we say so.
4. **A brand-new behaviour** → v0.2.6. Exception: a small change that reuses an existing mechanism and is driven by what the bot itself sees or feels (for example, the human-variation picks) → v0.2.5 if small, otherwise v0.2.6.
5. **Tests are sanity checks for absolutes only:** no goals at all, no kills at all, never goofing, stuck all match, or something bizarre. Number targets don't gate, and one run's swing isn't a regression. Act only on a wrong design or an absurd result. Never loosen a real test. A fragile test becomes an absolute or is deleted with a one-line reason.
6. **Frame rate must not drop:** every sight ray goes through the shared budget.
7. **Code quality:** no leftover patches, duplicated logic, dead settings or one-off hacks.
8. **Who decides:** the coordinator decides, under Max's full autonomy. Max is asked only about a real blocker, a change of direction, or something only he can see (visual checks on his PC).

## Finish line (Max agreed 2026-10-06 21:22Z)
- Section 3 is the full list for v0.2.5.
- A core issue found after this still goes in only if at least one of these is true:
  1. A player would actually hit it: stuck, broken behaviour, a crash, or a frame drop.
  2. It's in code already being changed for a list item, so fixing it now is cheaper than coming back later.
  3. Leaving it would mean shipping a patch or a hack around it.
- Anything else goes into the v0.2.6 section with a one-line reason.
- The final review before merge checks the work done. It doesn't hunt for new to-dos.
- Share numbers (stuck, idle and so on) are read against main once before release. Only a big jump, such as stuck time doubling, gets a closer look. Nothing blocks on a number.

## 2. What's done
On the bot branch `claude/v0.2.5-s4s5bn`:
- The act stage: one place decides walk, jump, crouch and jet (a53f8884).
- Threat scoring replaces the hidden-enemy filter. This is the general soccer fix (66871759).
- Go explore (2d452247).
- Hops are judged on the walk actually taken, and the goof's trigger goes through the one trigger (41287b01).
- Earlier: the ledge fix, the heard fight, door and goof fixes, and the soccer limits (those limits become absolutes in C4).

Elsewhere:
- On main already: the seated aim fixes and the LAN fix.
- The portal far-shadows fix is pushed on its own branch (`claude/portal-far-shadows-99fyjw`).
- The stuck root fix in the route planner (`crates/sim/src/nav.rs`) is coded and being tested before it's pushed. It only plans a drop where the body clears the edge.

## 3. What's left, in order
The bot thread does these one after another.

### A. Current items
1. Finish and push the stuck fix.
2. Move aim and trigger into the act stage.
3. Turn goofs, dodges, hand-offs and door clicks into options the chooser scores.
4. Emotes only from the bot's own signals, and only if that comes naturally.
5. Brick bots stroll using the explore scorer.

### B. Cleanup (the review's verdicts)
Source: `removal-candidates.md`, as corrected by the addendum at the end of `review-2026-10-06-round3.md`. Nothing a player can see is removed. What goes is the second copy of each mechanism.

**Keep, and rebuild on the shared mechanisms:**
1. Shoving an opponent off the ball and standing behind a teammate. These become an ordinary Fight choice and team spacing. The separate swap after the choice ("clear and cover") is removed, so there's one way to go after an opponent at the ball. The broom's shove counts as an attack.
2. Four goofs, each kept:
   - the flashlight flick;
   - dropping a weapon (it only ever drops a spare);
   - the aimless detour, which now just picks an explore spot (its own walk code goes);
   - walking to a door to click it, which loses its own timer and holds until the click lands.
3. Idle ball-kicking outside a game. It runs through the one shared push, and the second push code goes.
4. The Gravity Gun grab and throw. It goes on the one shared hold (the same hold the objective uses), becomes a scored option with no 6 s timer, and its open-sky scan goes on the ray budget and is remembered. If this grows past folding two controllers into one, remove it and queue it for v0.2.6.

**Remove:**
5. The objective stopgaps: pausing delivery when hurt, the "peaceful objective" arming gate, and the filter that hides enemies. They go only after C1. Arm when an enemy is about and the bot has nothing to fight with, whatever its objective.
6. The strafe's own ledge guard. The act stage's edge check already covers it.
7. Settings only tests can change (13 bot-kind knobs), the hold margin that never changes, and the spawner name that's only counted.
8. Tests that pin old behaviour: covered in C4.

**Merge duplicates (keep one of each):**
9. Weapon readers: keep one. This also stops unknown modded weapons being guessed as melee, which was a round-3 must-do.
10. Give-up lists: keep the shared one.
11. Progress judges on one objective step: keep one.
12. Rename the two constants both called `RETRY` for what each one does.
13. Drop the extra random stream used for dodges.
14. Repeated budget numbers: read them from the one shared limits list.

### C. Core fixes (round 3, plus today's decisions that broke the rules)
1. An enemy is worth chasing or fighting only if the bot can hurt it. This is one factor beside "spared", and it retires the unarmed stopgap.
2. Every sight ray goes through the shared budget. This includes go-explore's rescan when it's boxed in: remember a failed scan, and charge its rays to the budget.
3. One F3 line shows what the act stage did: which mover won, any edge veto, and the stuck count.
4. Tests (rule 5):
   - Convert 33 to absolutes and delete 6, each with a one-line reason.
   - Soccer and gauntlet gate on absolutes only. That includes the tight soccer limits and gauntlet limits restored today. Shares become a report that doesn't gate.
   - The water test's stuck limit (loosened to 35 %) becomes "never stuck the whole run".
   - Rewrite the bedroom mixed-firefight test to today's design, or delete it with a reason.
   - Fix the watch-tool test so it prints "skipped" when no save is set.
   - The `bot_firefight` and carryable-CTF known failures check real absolutes. Keep them, and fix the cause if it's in the bot work.
5. One way to hold an objective: the hold rule alone. Commitment comes from the hold, not from fixed higher scores. (The opponent at the ball is handled in B1.)
6. The stuck hop moves into the act stage as a press, instead of being written after it.
7. If small: the same first-come rule for the line-of-fire cost (F9), and aim scaled by the target's size (P5).

### D. Human-variation picks
These are in v0.2.5 under rule 4 only if each one is small and reuses existing mechanisms. Otherwise they go to v0.2.6.
1. When searching, it looks one way, then back, with uneven pauses, favouring where you were last seen heading, instead of spinning like a turret.
2. Hit while strafing, it turns the other way.
3. It stares back at someone who is staring at it.
4. Its head turns lazily when relaxed and snaps round in a fight.

### E. Before review
1. Merge the latest main.
2. Play quick sanity matches on Max's saves:
   - Soccer Field Goo and slot 1;
   - Close Quarters DM;
   - A TDM;
   - Afghanistan DM;
   - Slopes;
   - My Vehicles;
   - CTF.
3. Write one report:
   - proof by search that no mode or item names are special-cased;
   - net lines removed;
   - what was kept and what was removed.

### F. Final review
An independent review thread re-checks the work done on the final tip before the merge. It doesn't hunt for new to-dos (see the finish line).

## 4. Release steps (after the bot tip passes review)
1. Merge the bot branch and the portal branch through the PC push gate. The portal branch waits for Max to look at it on his PC: shadows through far portals, frame rate, and how the mirror room looks.
2. Regenerate the weapons pack and what depends on it (ball sounds). Re-import the bundled Add-Ons (coffin).
3. Remove the temporary known failure `steady_play_builds_and_uploads_nothing_whole::synthetic` if possible. The CTF flag test stays a known failure unless it gets fixed along the way.
4. Check the changelog against the merged code: a short New / Fixes / Known issues list, plus the download table.
5. Publish for Windows, Mac and Linux. Upload the content and the Add-On bundle. Clean up the merged branches.

## 5. Not in v0.2.5 (v0.2.6 queue)
- goal defending;
- passing;
- strafing that reacts to being tracked (beyond D2's turn when hit);
- emotes after kills;
- more callout lines;
- sloppiness under pressure;
- lamp shadows through far portals (unified lighting);
- the jet climb to high decks, which the planner thinks it can make but the controls can't (parked by Max);
- anything rule 3 or rule 4 pushes out.

## 6. Only Max can do
- Look at the portal fix on his PC: shadows through far portals, frame rate, and the mirror room.
- A final play on his saves before publish, if he wants.

## Corrections
- The removal list's first draft put the four goofs and idle ball-kicking under "remove". Its addendum (21:10Z) reverses that: keep all of them, rebuilt on the shared mechanisms. Weapon drop only drops a spare, so it can't strand a bot unarmed.
- Round 3 lists 6 must-dos, not 5. The watch-tool test is folded into C4, and the unknown-weapons fix is done by B9's merge.
- "Strafing that reacts to fire" partly overlaps D2. v0.2.6 keeps only the larger part (reacting to being tracked).
- Round 3 says the `bot_firefight` and carryable-CTF known failures are real and should be fixed. Release step 3 keeps CTF as a known failure only if the fix doesn't come along the way.
