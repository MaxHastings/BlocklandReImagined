# Bots pick behaviours by score

Bot redesign, step 2.

## What changed

- `behaviour::choose` no longer walks a fixed if-chain. Each behaviour
  scores itself from the `Situation` (`Behaviour::score`); the kind's new
  `behaviours` weights in `bots.json` scale the scores, and the highest
  wins. The base scores keep the old order, so stock bots play as before.
- A weight of 0 turns a behaviour off (`"chase": 0` is a guard that holds
  its post); more than 1 puts one ahead. Names are listed once, in
  `bot_kind::BEHAVIOURS`, and validated (0 to 10).
- Search now applies only to an enemy out of sight (the old order already
  meant that); otherwise a guard with chase off would chase by "searching".

## Evidence

- `crates/chaos/tests/bot_brain.rs` `a_bot_whose_kind_never_chases_holds_its_post`
  (chases: 20.6 to under 14.6; chase 0: stays). Before the Search fix it
  failed: "it held its post: 20.56 to 3.10".
- `behaviour.rs` unit `weights_reorder_or_turn_off_behaviours`, and the
  old order tests unchanged.
- chaos bot_brain 15 pass, sim lib bots 5 pass.

## Next

Step 3: a richer walk map (`crate::nav`).
