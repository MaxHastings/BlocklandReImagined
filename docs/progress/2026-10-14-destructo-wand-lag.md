# 2026-10-14 Destructo Wand lag on large builds

Max reported lag while destroying lots of bricks with the Destructo Wand.
The cause was server tick time, not the network or the client.

## What was slow

Every wand hit (and every hammer swing, undo and `will_cause_chain_kill`)
runs `Simulation::stranded_by`, which searched breadth-first from each
neighbour of the hit brick until it found a grounded brick. Near the top of a
big build that floods a cone of the whole build below the hit, with a stud
query and a ground ray probe per brick. Measured in release on a running-bond
wall of 1x2 plates:

| wall | hit | before | after |
| --- | --- | --- | --- |
| 100 x 100 (10k) | layer 98 | 45 ms | 0.4 ms |
| 200 x 100 (20k) | layer 98 | 65 ms | 0.6 ms |
| 100 x 100 | layer 50 | 22 ms | 0.2 ms |

A tick is about 32 ms, so each hit near the top of a large build stalled the
server for one or two ticks: the "sometimes" lag.

A hit that strands a big build (a 100 x 100 wall on one brick) also emitted a
`BrickKill` cue per falling brick: 9,900 cues against a 4,096-cue queue, so
the wand's own explosion and sound were pushed out and clients got a burst
they could not draw (they fall at most 4,096 at once).

## Change

- `stranded_by` now pops the lowest brick it has found yet (a binary heap on
  the grid bottom, ties by id) instead of breadth-first. The ground is below,
  so the search heads straight down. Results are the same set; stranded
  bricks are listed lowest first instead of nearest first.
- `kill_brick`'s chain kill announces at most `MAX_BLAST_DEBRIS` (2048)
  bricks as debris, spread evenly over the collapse, as blasts already do.
  Every stranded brick is still removed.

A stranded collapse still costs its size once (about 40 ms for 9,900 bricks:
search plus removal); that is a one-off, not per hit.

## Tests

- `crates/sim/tests/tools.rs`
  `a_destructo_wand_collapse_announces_bounded_debris`: a 60 x 60 wall on one
  brick falls entirely, with at most 2049 kill cues, no dropped cues, and the
  wand's explosion and sound present.
