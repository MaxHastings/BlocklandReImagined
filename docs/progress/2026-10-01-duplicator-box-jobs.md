# Supercut and fill as copy jobs

The New Duplicator's `/SuperCut` and `/FillBricks` no longer cap at 10,000
bricks or run in one tick. Both are copy jobs (`copy_edits/box_jobs.rs`:
`SuperCutWork`, `FillWork`) on the shared per-tick copy budget, with the
original's limits.

- Original limits (read from the original Add-On, not committed): the
  supercut had no brick count limit, only the box size (MaxBoxSizeAdmin
  1024, Player 64, already enforced by the port), and cut it in chunks every
  30 ms with "Supercut in progress... (X%, N deleted, M planted)" and
  "[Cancel Brick]: Cancel supercut"; cancel said "Supercut canceled!".
  FillBricks was admin-only by default and ran at once with no progress.
- Engine cap for both: `MAX_COPY_BRICKS` (1,000,000); a fill also stops at
  the server's brick limit and reports `limit_reached`. `MAX_BOX_EDIT` is
  gone.
- A supercut slice removes its bricks in one batch and plants their plain
  pieces; its undo (`UndoCut::replaced`) clears the pieces, puts the bricks
  back, and restores the pieces when something blocks it.
- `Simulation::hold_settle` lets an undo of a fill remove a slice before one
  collision refresh; a brick tied to bricks outside the group costs the new
  `work::CHAIN`.
- `Progress` gained `placed` and `refused`; the port shows the original's
  supercut line and an added "Filling in bricks... (N%)" line (ours).

Release probe, 500,000 2x1 plates, default copy work:

| Job | Ticks | Slowest tick | Average |
|---|---|---|---|
| Supercut | 650 | 4.8 ms | 1.6 ms |
| Undo supercut | 2,403 | 5.5 ms | |
| Fill (250,000 bricks) | 1,654 | 25 ms (first two ticks; under 6 ms after) | 3.2 ms |
| Undo fill | 2,870 | 6.1 ms | 2.9 ms |

Known: a fill's first two ticks cost 8 to 25 ms as physics first meets the
box (single plants up to 4.4 ms, settle 8 to 10 ms). Not fixed here.

Tests: `a_supercut_puts_plain_bricks_over_what_stuck_out_and_its_undo_goes_over_ticks`
(sim), `new_duplicator_port_supercuts_and_fills_over_ticks` (ports).
