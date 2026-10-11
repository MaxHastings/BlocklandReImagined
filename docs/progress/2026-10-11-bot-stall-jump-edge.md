# 2026-10-11 A stall jump on the move is judged where it lands

Bots on a 6-high deck still went over its edge in layouts the deck-edge
scenario (PR #56) left out. Traced per tick: the stall judge, which hops a
bot that has got nowhere for a second, also fires on a bot wobbling between
two spots at walking speed. Its jump was pressed as a jump in place, which
safety passes as it is, so the bot flew on at 5 units a second for the whole
jump and came down past the edge.

Fix: the stall judge's jump stays a jump in place only while the bot is
nearly still (under 1 unit a second across). Faster, it is a hop, which
safety lets through only where it comes down on floor (`hop_lands`).

Evidence (`crates/chaos/tests/bot_hardening.rs`):
- New `a_stalled_bot_on_the_move_does_not_jump_off_a_deck`: layouts
  (0.75, 54.5), (0.75, 55.5), (1.25, 55.5) and (2, 55.5) lost 1, 1, 1 and 2
  bots before the fix and 0 after.
- A 20-layout sweep (spacing 0.75 to 2, z 54 to 55.5) went from 5 falls to 2.
  The two left come from a different cause: wander goals the route grid puts
  right on the rim, so a bot stands with half its body over the edge and a
  jump or a nudge takes it off. That is not fixed here.
- `cargo test -p bri-sim --lib` and every `bri-chaos` `bot_*` suite pass,
  except the soccer scoring test that already fails on Linux main (PR #65).

This branch includes PR #56 (the scenario's harness); merge that first.
