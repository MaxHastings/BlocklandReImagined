> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-whole-picture.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Whole-picture review: v0.2.5 (2cf0b3a4) → final main + pending stack, as one change

Tree reviewed: main d068f06e + ledge pushes ec7db637 + acceptance ed0680c0 + plant limit 92501ea + minigame e59b354 + Set Favs 97b5fc1, merged locally (46a32d3). Merges clean; bri-sim builds with tests. Diff vs v0.2.5: 183 files, +14.5k / −2.4k (crates).
Method: two independent read-only sweeps (bot module; everything else) plus my spot checks (✔ = verified by me at file:line).
Deltas still to add: teammate-harm build, rooftop fix, ragdoll final (one must-fix open), load-flakes 43ce22e9 (passed).

## Part 1: the change since v0.2.5

### Replaced cleanly (checked gone)
Stock-weapon runtime table (`Stock`, HOST_TOOL_IMAGES, NativeTool), route::Chassis, the old analytic Jets model, guard_fall, multibody ragdoll joints, the hardening test's own cpu timer. 20 lines left gate-known-failures. Every new #[ignore] has a hardware or content reason. The gate is stricter on crashed processes. One replay writer (sim/replay = frames, net/replay = file); all mutating server calls go through Recorder.

### Left behind (should go with the change that replaced them)
1. ✔ **Runtime name table for arm poses.** `bri_weapons::scripted_arm_pose` (weapons/src/lib.rs:79) matches "lefthandedgunimage", "basketballshootimage", "dodgeballimage", "basketballimage", "footballimage", called from client/app/frame.rs:1056. The importer now writes arm data from onMount/onCharge (weapons-import/src/stock.rs:24-41). The table predates v0.2.5, but it is the same job loop 3 moved into data, and it breaks the project rule (no datablock names in runtime). The name-ban test (weapons/tests/no_datablock_names.rs:8-17) only scans 3 runtime files, which is why it survived. **v0.2.6 if small:** client reads the imported arm data, delete the table and its test. Widening the name-ban test to the whole runtime would also flag older debts (below), so widen it in v0.2.7 together with those.
2. ✔ **Dead `route::landing`** (route.rs:545): only its own test calls it; bots use reach::shove_landing. Delete (one-line-class change).
3. **Jet leg numbers.** route.rs:382 brake rate 6.0, :366 speed cap 10.0, :324 thresholds 0.9 / 0.3 have no named source, and reach::fly's measured times inherit them. v0.2.7 (name them from tuning or measure).
4. **Stale owners in gate-known-failures.toml.** `steady_play...` and `the_bundled_soccer_field...` are still owned by "v0.2.5 bot/test cleanup". Update the owners when the flake fixes land (steady_play should leave the list once 43ce22e9's gate proves it).
5. **cpu_time returns ZERO on OS failure** (sim/cpu_time.rs:33/:49). A perf bar would then pass without measuring. Small: make the test that uses it assert the reading is nonzero. v0.2.7.
6. **Damage ring buffer only read by the acceptance test** (`Session::damage_results`). Fine as the canonical per-hit source for teammate-harm. If teammate-harm doesn't read it, say in its doc that it exists for tests.
7. **Reach::of cache never cleared**, one full measurement per distinct player scale (reach.rs:96-118). Fine at "a handful of tunings"; v0.2.7 bound or key note.

### Older debts found on the way (not from this change, v0.2.7 with the wider name-ban test)
Name checks in runtime: session/weapons.rs:581 "brickdeployprojectile"; presentation.rs:362-377 emote/pain images (+ client actor_effects); spray.rs:10 / tools.rs:279-326 spray, wand and paint projectile ids (`paint_edit`, now also used by bots' `image_paints`); weapons lib.rs:97 "bluepaint". Sport pass/throw velocities still runtime constants per ball (runtime/sports.rs); only Sport::throw moved to data.

### Deliberate and documented (no action)
Stock behaviour written three times (importer table, test fixture copy, frozen parity reference) as loop 3's proof; launch_playtest.sh's own dialog (runs before the binary); gate refreshes content/addons; client tests' 300 s STALL waits.

## Part 2: is the bot foundation done?

**Short answer: the shape is done, the consolidation isn't.** The architecture Max asked for is in place and holds across the diff: perception → one score per option → near-best chooser → hold → boredom → act, items judged by capability, no item/map/mode name gates in the bot module (✔ sweep found none). But inside that shape, a few jobs are still done two or more ways. Each is a place where a future change silently disagrees with itself. Until they are one mechanism each, I would not call the foundation finished.

Structural gaps, most important first:
1. ✔ **Two readers of "is that enemy armed".** contest.rs:80 and team.rs:471 (plus extras.rs:672, interactions.rs:1372) use `item_attacks` (capability-only). The planner uses `attack_of` (combat.rs:318), which also covers script-fired weapons. **Players would hit this:** an enemy holding a scripted Add-On weapon reads as unarmed to contest/team, while the same weapon in a bot's hands is an attack. **v0.2.6:** small, route all four through attack_of.
2. **Push valued four ways.** tactics::rate ignores push (tactics.rs:650), worth uses the simulated push_harm, the spots bound uses whole health, item_worth falls to UNKNOWN_WORTH. → v0.2.7 (already listed: rate ignores push, tighter pusher bound, Harm.push).
3. ✔ **Third flight model in the dodge.** extras.rs:300 integrates with a bare `9.81 * d.gravity` and raw damage, next to tactics::flight_position and runtime::fall_per_tick. → v0.2.7: dodge reads the same prediction as the planner (teammate-harm's shot_harm is the natural home).
4. **Clearance checks with margins that disagree.** UnsafeBlast / safe_blast / bot_fire_clear / bot_miss_spares_allies / BLAST_CLEARANCE 3.0 vs blast_margin 1.0. Teammate-harm deletes the first three. **Delta check when it lands:** bot_miss_spares_allies (perception.rs:675) and BLAST_CLEARANCE (bots.rs:818) must also go, or be shown to answer a different question.
5. ✔ **Second fire decision.** bots.rs:2878-2892 (`reach*1.1+0.5`, aim tolerances 0.1/0.12 rad, cadence::bursting) runs alongside hand_combat::Decision. → v0.2.7.
6. **Timers outside chooser/hold/boredom.** ✔ crouch_until re-extends itself while hit (extras.rs ~427); DODGE_REST, CLICK_REST, HAND_REST, next_wander (bots.rs:758, bare 240 + random·480), the strafe leg timer, give-up memories in three styles (Cooldowns, interactions GIVE_UP_TICKS / VEHICLE_GIVE_UP, extras rests). → v0.2.7, folds into the mood work (step 1-2).
7. **Two scales for weapons.** item_worth (pickups) vs tactics::suitability (fight). Documented, but one quantity. → v0.2.7.
8. ✔ **Variation sources.** Idle look drift has its own two-sine signal (perception.rs:593) next to cadence::drift. Walk speeds as constants (contest WALK 4.0, surprise 3.0) vs tuning().forward elsewhere. 35 bare `120.0` tick-rate literals in bots. → v0.2.7 mood consolidation + a mechanical tick-constant pass.
9. **Nav** (from the todo list): less of a gap than listed. nav.rs already covers walk, jump, crouch, swim, jets and portals. v20 has no ladders (inferred). Remaining: moving bodies are left out of the grid (stuck, then replan) and the ball-off-platform planner gap. v0.2.7, not structural.
10. **Pusher bound** and **in-process minidump**: as before, v0.2.7.

### Recommendation
- v0.2.6: fix gap 1 (armed reader), Part 1 items 1-2 (arm-pose table, dead landing), and check gap 4 on the teammate-harm delta. All small, in changed code, or a rule-breaking leftover.
- v0.2.7 "foundation consolidation" pass: gaps 2, 3, 5, 6, 7, 8 as one item, plus the older name debts with the wider name-ban test. After that pass the honest answer to "is the foundation done?" is yes.

## Delta slots
- Teammate-harm: (pending) check gap 4 deletions, shot_harm as the one shot prediction, Session::damage_results single stream.
- Rooftop fix: (pending)
- Ragdoll final: (pending; one must-fix: floor-only contact)
