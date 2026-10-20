> **Historical note. Written 2026-10-09 (the file's date in the cloud project folder, `v027/punch-list.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# v0.2.7 punch list

v0.2.7 is the last release before a long break: a stabilization release, and
nothing waits for a v0.2.8. Every open item from the v0.2.6 release notes, the
gate's known failures, the repo's notes, docs/progress (2026-10-05 to 10-08),
the audits and bot reviews in project files, the threads and the branches is
below, with its owner thread. "Sweep" is the v0.2.7 stabilization sweep thread,
which also owns the release. An item marked **Dropped** is not being done, with
the reason, so nothing is silently parked. Each thread keeps its own section
further down up to date; edit only your own section, and re-read the file
just before writing so nobody's section is lost.

Owners: Teamkills, Close Quarters, fog (TCF) · Tests that check mechanisms (MT)
· Faster release check (FRC) · Bot sims same on every OS (DET) · How bots really
play (HBP) · v0.2.6 regression check (REG) · Code reviewer (CR) · Sweep.

**2026-10-08 14:15Z, Max: 8% of the weekly usage left.** v0.2.7 is now TCF,
MT, FRC and the sweep's reviewed fixes (already queued for the gate). How bots
really play, the regression check and the cross-platform work are stopped:
every item owned by HBP, REG or DET below is **Dropped (usage budget, Max
2026-10-08)** and ships as a known issue where players can hit it.

## Bugs players hit

| Item | Owner | State |
|---|---|---|
| Bots occasionally kill a teammate (v0.2.6 known issue; capture_the_flag, zombie_survival, all_dials_on) | TCF | In progress. MT turns the whole-match teamkill asserts into reports. |
| A save naming an item from an Add-On that is off fails to load (Close Quarters, HE Grenade off; dedicated server) | TCF | In progress |
| Far-edge fog dither crawl; pale tall far terrain (v0.2.6 known issues, review-fog-sky.md) | TCF | In progress |
| A jeep driver runs over its own teammate | TCF | In progress |
| Bots stuck on awkward stairs and on drops deeper than 4 units (v0.2.6 known issue) | Dropped: known issue in notes | Real-match behaviour on Max's saves |
| A bot whose spawn brick is boxed in kills itself about every 11 s forever (review-2026-10-07-final.md #2) | Dropped: known issue in notes | Respawn trap clock |
| Bots stand on a player's head and stay; bots jam on jeep roofs | Dropped: known issue in notes | |
| Leap think time (2.1 ms a tick, Afghanistan, 7 bots); frame-time spikes with many bots | REG | Measuring v0.2.5 against v0.2.6 |
| Ragdoll, recording overhead, Windows crash capture and Linux startup risks | REG | Measuring |
| Looping weapon sounds, an orbit camera's focus and Add-On client code use the standing eye for seated players (2026-10-06 seated-eye entry) | Sweep | Fixed (7fa22d8) |
| Windows crash capture: on a dump timeout the snapshot is freed while the dump thread may read it, which can leave a second, spurious crash report (crash/src/windows.rs:199) | Sweep | Fixed (ea5b07e): the stalled dump keeps its snapshot; the free never ended the clone anyway (measured for v0.2.6) |
| Mini-game script errors reach only the diagnostics log; the player sees nothing (2026-10-05 Slayer team bots entry) | Sweep | Already fixed: problems are told to admins in chat and the log (`tell_package_problems`) |
| An admin kick on a bot is announced but the bot stays (review-round3 L7); picked as quality of life, Max 15:16 | Sweep | Fixed (0c1b594, on main): a kicked bot leaves at once |
| Ranged fighters' two sideways movers merged into one (bot polish) | HBP | **Parked** by Max 16:44: not in v0.2.7, listed as a known issue; branch kept |

## Tests and the gate

| Item | Owner | State |
|---|---|---|
| Windows CI on main has been red since v0.2.5: `--ci-test` ignored the known-failure list | Sweep | Fixed (9948a92) |
| bot_soccer_match two_against_two runs past the 600 s cap on CI and the PC | FRC | Optimized bot crate (f5543d6) |
| bri-render/mirrors and bri-client/brick_fx time out on CI's software GPU (run 37736068374; passed in earlier runs) | Sweep | Watch the first CI run after landing; fix if it recurs |
| Gate entry `steady_play_builds_and_uploads_nothing_whole::content` (soak rocket no longer knocks out its test wall); owner "v0.2.5 bot/test cleanup" is finished | Sweep | Stays a known failure: c362281 left out of v0.2.7 (reviewer: a count can mask a real failure) |
| Gate entry `the_bundled_soccer_field_loads_whole_from_load_bricks` (needs the regenerated worlds pack); same stale owner | Sweep | Kept as a known failure: the gate reads the main checkout's old worlds pack |
| Seeded tests that need a chance event before checking (bot_extras, bot_windup, tuning goof) | MT | Looked at, left as is with the reason in MT's section |
| App and GPU test bottleneck in the gate | FRC | Dropped by Max to save weekly usage (FRC section) |

## Code and docs tidy

| Item | Owner | State |
|---|---|---|
| Bot systems that fight each other: three stuck detectors, two hop-when-stuck paths, idle fun in three places, two carry controllers, `bot_live_blasts` beside the dodge's `bot_incoming`, two ally-shot readers, dodge's own flight model (next-version.md, v0.2.6 reviews) | Dropped (usage budget) | "One owner per job" |
| Split the 3,748-line `sim/src/session/bots.rs` | Dropped (usage budget) | Coordinator 14:45: no split before the break |
| docs/STATUS.md, KNOWN-ISSUES.md, FEATURES.md and docs/README.md still call v0.2.3 current; STATUS says the known-failure list is empty | Sweep | KNOWN-ISSUES, FEATURES, docs/README fixed (3870ee0); STATUS rewritten at release |
| Two weapon image lookups by datablock name (no_datablock_names.rs: emote/teleport, emote/pain/burn) | Sweep | **Dropped:** both work correctly; moving the names into importer data needs a pack schema change and a content regeneration for no player-visible change |
| Release smoke: a second `--check` with every default Add-On on (review-release-addon-check.md) | Sweep | **Dropped:** the bundle is imported and packed in one go, so a stale bundle already fails the smoke through the two Add-Ons on by default; a single off-by-default port bug is what its port tests are for |
| `restore_group`'s doc said nobody may stand in an undone cut (review-plant-limit.md #4) | Sweep | Fixed (142bed9) |
| Stale thread: "CTF and minigame setup" shows blocked, but its three fixes shipped in v0.2.6 | Sweep | Close it |
| Remote branches: only main and the four v0.2.7 thread branches | Sweep | Deleted after release, with a backup bundle |

## Dropped, with the reason

- **Bots play the same on Linux and Windows (DET):** Max kept it out of v0.2.7 to save weekly usage (DET section).
- **Out-of-process crash dump:** a new helper process and IPC, which can't be tested here or on CI; the bounded fix above covers the visible symptom.
- **New bot skills** (lining up a push, push harm to allies as its own slot, grudges, confidence, horses/planes/boats, one mood, k-best routes, ladders, Add-On goal grounding, jet climb): new behaviour, not stabilization. A skill comes back only if it is the fix for a real bug.
- **Recording on by default, and its prerequisites** (writer thread, size cap, fsync, seeking): recording ships off, so nothing a player hits.
- **Pipeline cache across map changes, unbounded shader compile:** a renderer change with no failing case to test against.
- **Visible Add-On status in the Add-Ons list, invite URL handler, terrain LOD, interior subobjects and alarm lighting:** new features.
- **Gravity Gun at 0 kills, vehicles and CTF flags rarely used by bots, Shark/Zombie full fidelity:** feature depth, already listed in KNOWN-ISSUES.


## Tests that check mechanisms (thread "Tests that check mechanisms")

Branch `claude/v027-mechanism-tests-kjrsa7` (6465c04), reviewed. It lands after
"Faster release check" puts the optimized bot crate on main: gate run 2 was
green except bot_soccer_match's 600 s cap.

**Taken on (done, waiting for the gate):**
- Seeded gauntlet runs no longer crash when a scenario installs a package
  (gauntlet/mod.rs; seed players join just before the first builder).
- Team and self kills over a whole match are printed in a report, not
  asserted. "Each side hurts its own less" and the per-tick `bad_plans`
  stay asserted. The three teamkill known-failure entries are removed.
- nav.rs test: v0.2.6's driving bug as a 0.4 s mechanism test.

**Looked at and not taken on, with the reason:**
- T.
- Whole-match "kills > 0", "fell == 0", "captures > 0" and the soccer
  totals and behaviour bands only fail when play is bizarre, with wide
  margins. None has been red or flaky, and each still catches real
  regressions.
- These seeded tests need a chance event inside their window before they
  check an invariant:
  - bot_extras: idle ball push, dodge jets, opening a door for fun, goofing
    up close
  - bot_windup: the knife goof
  - tuning_dials_and_seeds: goof > 0

  They are deterministic and have never flipped. Making them independent
  of the event would need a "force this choice" hook in surprise.rs, which
  is bot decision code that "How bots really play" owns. If one ever flips
  after a brain change, the fix is that hook, not a retuned threshold.

- **App and GPU test bottleneck in the gate (Faster release check).** Not done: dropped to save weekly usage (Max, 2026-10-08). GPU and whole-app test binaries run three at a time, and their sum on the PC (about 3,100 s per gate run, app_soak alone about 520 s, with its tests taking turns on one GPU) now sets the gate's test time. Next steps: split app_soak's tests into separate processes and raise HEAVY_JOBS if the PC's GPU allows it. The new log line ("phases: ... N test jobs") gives the before/after.

- **Bots play the same on Linux and Windows (Bot sims same on every OS).** Not landed: kept out of v0.2.7 to save weekly usage (Max, 2026-10-08); it touches all game maths and needs its own review and PC gate. Branch `claude/v027-cross-platform-determinism-sxsapn` (621c962, dee0ee0, 4e1d66f) routes all float maths (ours, glam, Rhai's sin/atan/exp/ln/log) through libm, with clippy banning the std versions. The determinism workflow on that branch proves it: before, every gauntlet arena parted between Linux and Windows within about 30 ticks; after, 40 of 40 arenas are identical tick for tick. Not checked: the full engine test suite with the change, and whether it fixes the Linux-only soccer 2v2 seed 2 failure.

- Regression check (v0.2.5 vs v0.2.6, stopped early for usage): Windows crash capture passed 20/20 runs on both tags on Max's PC, no hangs. Nav, CPU, recording and Linux startup measurements were not run. One ragdoll finding (Linux, offscreen, Ragdoll Add-On probe): v0.2.6 Add-On physics is 4-5x cheaper (mean 0.7-2 ms vs 4-11 ms a frame), but whenever `AddOnPhysics::settle_oldest` fires (only when a frame takes over 4 ms, so it depends on machine speed), later contact can blow a ragdoll apart: with a player/vehicle-sized pusher sweeping the pile, a limb fell through the floor to y -204 and joints stretched 259 units; in a 3,240-brick build bodies were flung at 90 u/s and 456 body-frames went under the floor. Runs where settle never fired were clean (stretch under 0.2, nothing through the floor). Likely cause: `settle_oldest` puts a whole jointed group to sleep while touching or jointed bodies stay awake. Not yet confirmed with forced settles. Repro probe: `regress_ragdoll_probe` in the thread's scratch worktree (not committed).

- **Bot brain ownership cleanup (How bots really play): landed** on main 65dc06e (2026-10-08) through one green PC gate, with the Linux crash fix and a gate self-test fix (e0a6b84). One owner each for blast escape (a Dodge in `bot_extras`; safety only vetoes), vehicle detour (before safety), idle goals (`Brain::idle`, consumed by Wander), and v20 spawn placement (feet never inside the spawn brick: the soccer regression). The 0.5 s glimpse memory was deleted: it faked sight of a hidden target, duplicated the hold rule and was the grenade regression; the step-out strafe ban that compensated for it went too. Headless loop counts vs main: stuck time down on Soccer, Close Quarters and Afghanistan (10.5% to 3.4% on CQ), self kills down on CQ and Afghanistan, flips at main's level. Open: fight/search flicker belongs in the hold rule; a vehicle detour can still bend a blast-escape dodge; head snaps; CQ fall deaths; the jeep driver running over a teammate (Windows only).- **v0.2.7.1: HE Grenade wind-up loop + Slayer 32 bots: landed** on main f11138c (2026-10-09) through one green PC gate, first try. Bots holding an HE Grenade wound up and cancelled over and over because a target dropping out of view a moment made the fire gate abort the held button ("no plan") and `bot_act` drop the native wind-up; both now follow the one hold rule. 4 HE bots on CQ, 40 s: wind-ups 155 to 72, cancel remounts 139 to 57, kills 7 to 8, self kills 0. Tests: a lost-sight throw test in `bot_tactics` (fails before the fix) and an imported HE Grenade duel in `bot_windup`. Also 93ee6c2: Slayer's Bots (free-for-all) max 16 to 32. Entry: `docs/progress/2026-10-08-he-grenade-windup.md`.

- **v0.2.7.2: HE Grenade bots really throw + paint name bar: landed** on main 3c8ed1f (2026-10-09) through the PC gate (third run; first stopped on a clippy dead-code error, second on `acceptance_unfamiliar`). The v0.2.7.1 note measured spears, not grenades. Three real causes, each fixed at its owner: the shot planner refused the HE Grenade's data scripts as an unknown tool (`plain_scripts`), `attack_clear` counted teammates the blast cannot hurt so bots never released, and a grenade lying still flipped in and out of the live-blast list so a thrower walked into its own blast. HE bots on CQ, 60 s: 0 to 3 throws, 4 kills, no grenade self kill; Afghanistan 1 throw. Also 917ad0f (paint name bar hides with the box). Side effect, test adjusted: enemies now step around a grenade lying in sight, so the "grenade near an enemy" check counts an enemy coming within the blast at any point of its life (old: at the burst only). Other test changes listed in the progress note. Open: bots throw only a few times a minute (the planner declines moving targets and anything inside the 17 unit blast); raising the 32 bot ceiling needs sight/planning budgets re-measured (card to Max unanswered, default keep 32). Entry: `docs/progress/2026-10-09-he-grenade-throws.md`.
