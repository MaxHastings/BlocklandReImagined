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

## Follow-up: no steering out over the edge in the air

Windows CI failed the new test with 2 falls (1.25 and 2 at z 55.5) where
Linux had 0. Shifting the file and its enemy a little on Linux reproduced
falls, and a per-tick trace showed the cause: a bot held at the edge (its
footwork pointing out over it) jumps in place, and in the air the edge check
measured the drop from its feet in the air, with the depth of a fall that
hurts; a 6-high deck does not hurt, so the footwork steered it out over the
edge. Off its route in the air the check now uses the same rule as on its
feet (no drop it could not walk back up), measured from the highest floor
under any part of its body.

Evidence: 12 shifts (0 to 0.41) x the 4 layouts went from 2 falls to 0. The
test adds the two shifted layouts that lost a bot each on Linux before (it
fails without the fix: 2 falls). `cargo test -p bri-sim --lib` and every
`bri-chaos` `bot_*` suite pass, except `bot_soccer_teams`' known Linux
bound (PR #65).
