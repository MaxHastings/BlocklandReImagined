> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-design-locomotion.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: design note, locomotion lane (vehicles, warm at load, shove landing)

## Verdict: approve items 1 and 3 with small changes. Item 2 needs a cost limit first.

## 1. Vehicles, measured: approve

- It deletes four guesses, without leaving any beside the replacement:
  - the 0.3 s reaction plus 1-unit margin stopping formula;
  - the arrival formula;
  - `CORNERING` 10;
  - the 0.6 × length fallback.
- It measures the vehicle's own code with the bot's own steering prefs. That's the right seam.
- **Measure at the throttle the driver uses** (0.8), not 1.0. The rule that makes measured jets honest is "measure what the bot does". A 1.0 table would be one more mismatch to remember.
- **Cache key:** serialising the whole definition on every lookup is waste.
  - Definitions don't change while a pack is loaded.
  - Measure once per definition and scale when the pack loads or the bot mounts, and keep the `Arc<Handling>` with it.
  - Re-measure only when the pack reloads.
- **The hazard sweep along the real arc** is in the lane's own file (`interactions.rs`). Fine.

## 2. Warm at load: needs a cost limit first

Moving the measurement earlier doesn't make it cheaper. In my review of the branch I found:
- a stock body costs about 90 ms;
- an odd body is limited only by 256 jet legs × up to 30 s each, plus two bisections up to `jump_speed²/2g`.

So a low-gravity Add-On body could take seconds (inferred). Warming every archetype at load multiplies that by the archetype count, and Add-On sets can carry many. Before this goes in:
1. **Give one measurement a hard tick limit** (a named constant), past which it keeps what it has measured so far.
   - For example: jet rows stop growing, and the ledge search narrows from below.
   - It must still be deterministic, so replay holds.
2. **Warm only what a bot will use.**
   - Do it when a bot joins, for its own tuning. That's one expected moment, chosen by the host.
   - Not every archetype at map load. Not a player's scale change either, since only bots plan with `Reach`.
   - Vehicle handling: when a bot first looks at a definition, or at pack load if the count is small.
3. **These are shared session files.** Tell the coordinator first, as the note says.

## 3. Shove landing: approve

- The split is clean:
  - this lane says where a body flies;
  - capabilities says what landing there costs;
  - fighting joins them.
  - There's no map in the measurement.
- **Write two limits into the code's doc comment:**
  - it predicts a standing target, so a running target's own speed isn't counted;
  - it assumes no air control.
  - The push outcome judge (plan item 7) is what corrects these.
- **Cache key** (tuning, push): include the push's vertical part, not only its size.
- **Share the fall rule:**
  - The harm reader needs the body's fall rule.
  - `nav::Body.drop` is still a flat 4.0 (`nav.rs`). It should read the same rule, so "a drop it walks off" and "a drop that hurts" can't disagree.
  - That's a one-line follow-up once the harm reader exists. Record it in the plan.

## Horses

Agreed: look only if 1 to 3 land clean.
