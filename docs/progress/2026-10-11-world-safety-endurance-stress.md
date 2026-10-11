# 2026-10-11 World safety, endurance and large-build stress tests

Brief P0-06, P1-04 and the "compact but adversarial integration suite and
endurance tests" ask (audit asks 25, items P0-06 and P1-04). All three new
test files are in `crates/net/tests/`, need no game content and no GPU, and
run in the gate by default.

## What is tested

`world_safety.rs`
- A child process writes recovery snapshots of growing worlds forever; the
  test kills it (SIGKILL / TerminateProcess) at nine different moments. After
  every kill the slot decodes, validates, holds exactly the bricks of the
  revision it names, and is at least as new as the last revision the child
  announced. The next write replaces it normally; leftover staging files
  are at most one per kill, named `.<slot>.<pid>.<n>.tmp`.
- A snapshot cut at every length up to 300 bytes and at about 400 more
  points after is refused whole by `build::decode`; `decode_header` never
  reports a count other than the file's.
- A running host with a 200 ms recovery interval: a builder plants six
  bricks, the slot holds them, the builder's connection vanishes without a
  word, a watcher joins and sees exactly those bricks, a clean stop deletes
  the slot, and a new host started from the slot bytes (what a kill leaves)
  gives a joiner the same bricks at the same places.

`endurance_...` (sixty cycles, one test): guests build and have a brick
undone, one hostile batch per cycle (NaN, infinite and absurd positions,
colour 255, 200 quarter turns, unknown and 5000-character brick names, a
4096-character chat line, a replayed sequence number, a player who already
left) is refused without changing a brick, a guest leaves and another
joins, the world is saved and read back exactly, and an administrator
changes map (a fresh session `adopt`s the players). Nothing is lost
(players, bricks), the saved size of the same map stays within 64 bytes,
and the heap after warm-up grows by under 1 MiB over 48 map changes.

`stress_large_builds_...`: 10k and 100k bricks by default, any list in
`BRI_STRESS_BRICKS`. Per size: save, load (every brick compared), host,
first ticks, steady tick, and 400 edits (200 plants, 200 undos), timed in
thread CPU time; heap bytes per brick on the host; ratios between smallest
and largest bound save/load/host/first ticks to three times linear growth
and steady tick and edit cost to eight times plus 5 ms.

## Measured (debug build, 4-core Linux container, CPU time)

| Bricks | Save | Load | Host | First 30 ticks (each) | Steady tick | Edit | Host heap |
|---|---|---|---|---|---|---|---|
| 10,000 | 22 ms | 24 ms | 21 ms | 57 us | 12 us | 42 us | 320 B/brick |
| 100,000 | 224 ms | 297 ms | 238 ms | 515 us | 13 us | 34 us | 307 B/brick |
| 999,600 | 2 s | 4 s | 2 s | 7.8 ms | 11 us | 34 us | 305 B/brick |

Everything is linear or flat. The steady tick and an edit do not depend on
how much is built; the first ticks after a join cost more with a bigger
world (building what is near the player), about 230 ms in total at a million
bricks in a debug build. This says nothing about frame rate or GPU upload;
`bri-client`'s `large_build_perf` still owns that and needs the PC.

## Findings

- A server will not let a player plant past `brick_limit` (default
  256,000), while a save loads up to `MAX_BRICKS` (1,000,000). A million
  brick build can be loaded and played but not added to by players unless
  the host raises the limit; the stress test raises it for sizes above 250k.
  Not changed.
- A killed writer leaves its `.tmp` staging file, and nothing sweeps them.
  One per crash is small, but they accumulate in a recovery folder of a host
  that crashes often. Not changed.
- Nothing broke in the engine: every failure while writing these tests was
  in the test (obstructed spawns, off-grid positions, the 60 actions per
  second limit).

## Not done

- Not run: the 1M case on the PC in release, the real two-client crash and
  rejoin journey with Add-On download (P1-18), and fault injection for
  lost peers and slow downloads. `bri-client`'s own recovery offer for a
  damaged slot (`crate::recovery::keep` on a truncated file) has no new test:
  the client crate does not build in this environment (no ALSA), so a test
  could not be run before pushing.
- The adversarial batch is one session's worth of command abuse inside the
  endurance loop; the existing `hardening_net`, `red_team`, `stress_campaign`
  and `save_fuzz` suites remain the wide adversarial coverage.

Run the larger sizes with
`BRI_STRESS_BRICKS=10000,100000,1000000 cargo test --release -p bri-net --test large_build_stress -- --nocapture`
or `python tools/check.py soak --crates bri-net`.
