> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-design-acceptance.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: acceptance run design note (db76ec7c, doc-only on c424c8b0)

Verdict: **approved with the spec below.** The frame is right: the package is built in code, its names differ per variant, the checks are absolutes only, and each run is replayed. The capabilities checks stand with two sharpenings. The locomotion/fighting checks and the runtime budget are below.

## Answers to its questions

1. **Keep two variants by three seeds.** The variants are what catch a name check, since any name check passes one and fails the other. The seeds catch numbers that only work by luck. More seeds with one variant would lose the first proof.
2. **Zone entry, not just "moved".** Anything moves a loose body when a grenade lands next to it, so "moved" proves nothing about the rules. Two conditions:
   - The body starts outside its zone.
   - An entry counts only when a bot was the last thing to touch the body before it entered: its body, a kick, or its push shot. If the session has no last-touch reader, use "a bot touched it within the previous second", measured from contacts. One second is the replay's FULL_CHECK_TICKS, so it isn't a new number.
   - Once per variant.

## Capabilities checks (sharpened)

- Thrown at least once in each run. Goes off within its blast radius of an enemy at least once per variant. Both stand.
- **"Never within the danger zone of the thrower or an ally":** judge it on what the bot could know.
  - The thrower: never within the danger zone at the burst (it can move away).
  - An ally: never within the danger zone of the actual burst point **at the tick of release**. An ally who walks in afterwards is not the thrower's mistake.
  - The danger zone is blast radius plus harmful shard reach (`fragment_reach`), the same reader the bot uses. The harmless trail adds nothing.
- Push weapon: fired at least once, and moves an opponent at least once. "Moves" means its hit changes an enemy's velocity. Per variant.

## Locomotion / fighting checks (my spec, absolutes only)

All counts are per variant across its three seeds, unless marked "every run".

1. **Up the platforms.**
   - Build the platforms so the top can only be reached with the odd body's jump or jets: each step is higher than a stock jump and within the odd one.
   - At least one bot stands on the top platform at least once. A bot that planned with stock numbers can't get there; one that measured can.
2. **Through the barrier.** At least once, a bot knows of an enemy whose eye-to-body line passes through the see-through barrier, read from the bot's own sight or knowledge, not a fixture.
3. **Fires from a chosen spot.** At least once, a bot arrives at a place it chose (the `spots` arrival) and fires from there.
4. **Pushed off the drop.**
   - At least once, an enemy lands below the drop within a second of a bot's push (the push shot or a shove).
   - **Never, across all six runs**, does a teammate land below the drop within a second of a bot's push.
5. **No walking off the drop.** Never, across all six runs, does a bot fall below the drop with no push, shove or blast on it in the previous second. This is the "bot fell off the deck" bug, as an absolute.
6. **Nothing stuck, every run.** No bot spends a whole life stuck, using the gauntlet's own stuck reader (`gauntlet::Metrics` stuck), not a new definition.
7. **The odd vehicle.**
   - Its measured handling exists and is finite (`reach::Handling`, `every_installed_wheeled_vehicle_measures`' reader).
   - If a bot boards it, it never leaves the slate's top or ends a drive below the drop.
   - Boardings are printed, not gated. Bots on a small slate may have no reason to drive, and adding one would be a fixture.
8. **Sanity, every run.**
   - `scan::ensure_finite` on every tick.
   - No body below the world.
   - No panic.
   - The replay matches tick for tick.

Printed, not gated: weapon shares, throws and pushes per bot, platform time, barrier sightings, boardings, first zone entry, worst think time per tick.

## Who builds what

- **Locomotion (idle, cloud) builds every part but the capabilities three:**
  - the odd body;
  - the odd vehicle;
  - the platforms and drop;
  - the barrier (a solid see-through brick: just geometry plus its flag).
  
  Each part is a builder function plus the reader for its check, in its own file `crates/chaos/src/unfamiliar/ground.rs`, so it never edits capabilities' `unfamiliar.rs`. Capabilities only calls them.
- **Fighting** builds nothing. It reads checks 2 to 5 when the code is up, a short look.
- Builders and readers get unit tests that need no content. The full run needs generated content, so it runs on the PC.

## Runtime (10-minute rule and the gate)

- The gate runs `cargo test` in its own build with a 600 s per-binary deadline, and the bedroom test already takes 366 s there. So:
  - Three bots a side.
  - The six runs on parallel threads, as `tuning::run_all` does.
  - The whole binary, replays included, must measure **≤ 300 s in the gate's build** on the PC once before the code review.
- If it's over, shorten each run before cutting a seed or a variant. A run shorter than 30 s game time is too short for the zone check to mean anything; if it can't fit at 30 s, say so and we decide.

## Small points for the code

- The fuse, bounce and blast draws per seed must come from the run seed, not wall clock, and be printed with each run so a failure can be reproduced.
- The test must not read a single name it built, beyond handing the package to the session. Grep the test for the variant words outside the builder and fail if any are found, as the loop 3 `native_id(` guard does.
- A failed check prints the variant, the seed and the tick, and keeps the recording file.
