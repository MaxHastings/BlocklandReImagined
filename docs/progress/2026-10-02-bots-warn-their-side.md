# Bots warn their side

Bot redesign, step 1 of the plan in
`2026-10-02-bots-spear-gravity-gun-and-kinds.md`: shared battlefield
awareness, starting with Bot_Hole's `hAlertOtherBots`.

## What changed

- `BotKind::alerts_allies` (default off). A bot of such a kind that first
  sees an enemy, or is hit, warns its side. Allied bots (one `side`, or no
  side and one builder) within its sight that have no target and no memory
  remember where the enemy was and go to search there. Warnings are heard
  after every bot has stepped, so the order bots step in does not matter.
- `bot_allies` is the one test for "same side"; `bot_enemy` uses it.
- The importer maps `hAlertOtherBots` to `alerts_allies` (Zombie and other
  hole bots that set it).
- Blockhead Bot turns it on.

## Evidence

- `crates/chaos/tests/bot_brain.rs`
  `a_bot_that_sees_an_enemy_warns_its_side`: two bots of one side, the far
  one out of sight of the human. Warned, it closes in by more than 6
  units in 3 s; unwarned, it stays. Fails with `hear_alerts` removed
  ("43.13 to 43.13").
- `cargo test -p bri-chaos --test bot_brain` (14 pass),
  `cargo test -p bri-addon-import --lib hole_bots`, clippy `-D warnings`
  on bri-sim, bri-chaos, bri-addon-import.

## Next

Step 2: scored behaviours in place of the fixed order in
`session/bots/behaviour.rs`.
