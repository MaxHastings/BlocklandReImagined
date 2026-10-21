> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `bots/next-version.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot ideas for after v0.2.4

Scope for v0.2.4 was frozen on 2026-10-05 (Max). Ideas go here unless they fix something that looks broken on camera.

- Grudges: whoever killed a bot recently is a slightly juicier target, and it fades over time.
- Confidence: bots that keep winning push further, and bots that keep dying hang back.
- Full inventory weapon swap: drop the worst weapon for a clear upgrade.
- Bots ride horses, planes and boats. Today they only use wheeled vehicles (second-opinion review, 2026-10-05).
- Raise the 16-bot server cap (MAX_BOTS in package-runtime ops) if bigger bot crowds are wanted.

From the bot review thread (not v0.2.4 blockers; moved here at the 13:14Z hard freeze):
- Shotgun and bouncer bots barely hit (5-10% and 0% in the fair table). They fight from too far away for spread or bounce weapons.
- One bot's error skips the other bots for that tick (`step_bot(...)?` in `step_bots`). Contain the error per bot.
- A bot jittering back and forth in one spot is never counted as stuck.
- `team_mood` runs every tick for every bot and is outside the shared sight budget. This is fine at the current speed, so tidy it later.
- Cut the bot settings toward ~6 dials (about 72 leaf settings today, behind four main strength dials).
- Many Add-On weapons (scripted, spread, thrown, cooked) use the basic fallback: the first item in inventory, with no blast safety.

## Redundancy check on final merge (b986d6bb, 2026-10-05)
Gone and confirmed: the old Fly, per-behaviour dwell timers, the separate tell pause (now perception's switch delay), team mood every tick (now once a second). bots.json really has 4 dials.
Overlap left, ranked:
1. Idle fun in three places: goofs (their own play-vs-goof pick, surprise.rs ~588), idle play (Interact IDLE score, interactions.rs:20), Wander. Extras crouch, dodge, door-for-fun and hand-weapon use their own coin flips (extras.rs chance()) outside the chooser.
2. Three stuck detectors (progress.stalled, stuck, wedged; bots.rs ~3676-3697) and two hop-when-stuck paths, besides the known 8 give-up timers.
3. Two carry controllers: the old grab-and-fling Carry and the objective body hold. A guard (objective_hold_control, bots.rs:2303/2453) lets the objective win.
4. Claims (hard seat/body reservations) and intents (soft overlap cost) both keep bots off the same thing. They complement each other; low priority.
5. Hidden knobs: the loader still accepts ~60 other fields, and behaviour weights read "fly", "arm" and "chase", which are absent from bots.json (code defaults). Three RNG seeds share one draw().
6. Teamwork dial: at 0 it turns off only allies' intents; team mood, pressure (score deficit) and copy stay at their fixed values (16 / 0.3 / 0.15). Docs now say so (149696d); decide whether teamwork should scale them.
7. bots.json "fixed in code" check sees only numbers; non-number fields (mounted.anchor, fights_bots) still load. Slayer keeps its own first-names.txt beside bots.json first_names.

## Parked during v0.2.5 (Max 2026-10-05: "simpler and more well rounded", no new layers)
- Add-On goals bots cannot read. Slayer's `Team(Brick) IncScore` goal (and any Add-On output) is "unsupported rule semantics" to the planner, so team-fill bots in a Slayer soccer field never score and drift onto a parked tank. A general fix was drafted and parked because it adds a layer: run the output's own rules without committing and read the engine ops (team points up, round end), plus a "raise our score" goal when the win itself cannot be planned. Patch: /mnt/project-files/bots/parked-addon-goal-grounding.patch (against 09c8f1e). First try still scored 0 goals, so it needs more work too. Test: team_fill_bots_play_slayer_soccer_with_or_without_weapons (still a known failure).
- Jetpack climb to a high deck stalls near 4 units although the flight estimate says it reaches (a_bot_jets_over_to_someone_above_it, still a known failure). The search half was fixed in v0.2.5; the climb itself is parked.

- **One mood instead of many dice** (Max said yes to the write-up 2026-10-07 00:27Z): replace the separate variation sources with one per-bot mood (alert, bored, frustrated, confident) read by perception, the chooser and the executors. Fewer knobs and connected human behaviour. Design: /mnt/project-files/bots/mood-design-note.md. Max decides whether to build it.
- **Measured moves instead of formula × fudge** (review thread with Max 2026-10-07 00:46Z): jump height (nav.rs:82-89, ×0.8), jet reach (route.rs:55-93, caps 30/32 + 0.5 s), hop flight (bots.rs:878, 0.8 s), vehicle stopping (interactions.rs:1466, straight line) are ideal-physics formulas plus fudge. Measure each once per body or vehicle setup in the real physics, cache it, re-measure when the data changes, then delete the fudge. Start with jets: this fixes the parked jet climb. This is industry-standard (jump links measured by simulation), not experimental.
