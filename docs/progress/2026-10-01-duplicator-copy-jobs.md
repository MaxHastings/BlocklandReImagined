# 2026-10-01 Duplicators: million-brick copies as copy jobs

The New Duplicator port now has the original's limits: 10,000 bricks for
players and 1,000,000 for admins, as the coordinator asked once Batch154c
landed. Main bc27f00c5 was merged into the lane first (one merge, unions of
both sides); `Notice::PivotCopy` and the other copy notices are
`crates/net/protocol-changes/duplicator-copy-notices.md` and the compact
copy with its ghost is `compact-copy-ghost.md` (VERSION untouched).

What changed:

- Copy jobs (`crates/sim/src/session/copy_jobs.rs`). Selecting, planting,
  cutting, painting, wrenching, loading and undoing a copy are `CopyWork`
  that does a slice each tick. One pool of copy work per tick
  (`DEFAULT_COPY_WORK`, 10,000 units of a quarter microsecond, about
  2.5 ms) is shared round robin by the players with a job; lit selections
  wearing off take at most half. Costs per brick are in
  `simulation::work`. A command works on its job at once with what is
  left, so a small copy still finishes within the command. One job per
  player; their other copy work and undo are refused as busy. Cancelling
  keeps what was done as one undo step; leaving stops the job.
- Add-On side: `cancel_copy(p)`, `player(p).copy_working`, `on_copy`
  `working` progress four times a second, `ghosted`, `"undone"`, and
  `on_place` `canceled`. The port shows the original's progress lines,
  `[Cancel Brick]`, "X% Ghosted", "Planting canceled!" and "Undo
  finished.".
- Pivot from the coordinator's ask: the copy is not streamed to the player.
  The player's game gets at most `MAX_GHOST_BRICKS` (10,000) bricks of it,
  spread through it, for the ghost, as the original showed at most
  `MaxGhostBricks`; the whole copy stays on the host. Held copies are
  compact (`Blueprint::kinds`, `prints`, `CopyBrick`) and shared (`Arc`).
- `crates/sim/src/id_map.rs`: brick-id maps that grow a page of 256 ids at
  a time. The grid index's id-to-bounds map, a copy job's sets and an
  undone cut's old-to-new ids use it. A hash map of every brick stopped a
  tick for 43 ms when a world passed 917,504 bricks (moving into a bigger
  table); now the slowest plant tick at that point is 3.3 ms.
- An undone cut follows its bricks' new ids through the undo stack and the
  held copy a slice at a time (`Follow`), not all at once in the last tick,
  and frees the cut bricks on a background thread (`drop_later`), as the
  undo stack does with a forgotten big step.

Evidence (release build, 2x1 plates, default copy work; a temporary probe
in `crates/sim/tests/advanced_duplicator.rs`, not committed):

| Job | Ticks | Slowest tick |
|---|---|---|
| Plant 500,000 into a world of 500,000 | 2,202 | 3.3 ms |
| Undo that plant | 1,251 | 5.2 ms |
| Cut 500,000 | 599 | 4.7 ms |
| Undo that cut | 2,453 | 1.4 ms |
| Cut 1,000,000 | 1,199 | 4.4 ms |
| Undo that cut | 4,906 | 6.7 ms |

Before the fixes above, the same probe found 43 ms (index growth), 26 ms
(an undone cut renaming every id in its last tick), 12 ms (freeing the cut
bricks) and 6 ms (growing the cut's list) ticks.

Tests: `cargo test -p bri-sim` (new: a big copy plants and undoes a slice
each tick, a cancelled plant keeps what went in as one undo, a big cut and
its undo go over ticks), `cargo test -p bri-addon-import --test ports`
(new: the New Duplicator port shows progress and cancels a big plant),
`cargo clippy -D warnings` on the changed crates.

Left as they were: `/SuperCut` and `/FillBricks` boxes stay at 10,000 bricks
and run at once. Selecting 1,000,000 through the synchronous `copy_box`
(the engine's own API, not the port's path) takes 0.5 s.
