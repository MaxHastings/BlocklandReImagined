# 2026-10-11 Perf waste fix: a resume after a network drop downloads only what changed

Finding 13 of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files): a player whose
connection dropped and came back with their resume ticket received the whole
world again.

## What was wrong

The host answered a resume exactly like a fresh join: `Checkpoint::from_session`
and a `WorldTransfer` of every brick (about 1.2 MB and the full join wait on
a 100k-plate build), although the client still held the world it had been
playing in, current to its replication cursor, and the host had broadcast
every change since as `Delta::bricks`.

## Fix

- The host keeps a bounded backlog of the brick changes it broadcast
  (`crates/net/src/backlog.rs`): at most 20,000 changed bricks or 4,096
  updates, oldest forgotten first, emptied by a map change or by one update
  bigger than the whole backlog (a build loaded). It answers "every brick
  changed after cursor c", or nothing when c is older than what it holds or
  a cursor it never reached.
- A resuming client offers `Hello::kept` (the map and cursor its bricks are
  current to). When the host can match it, the Welcome's `Checkpoint::kept`
  names the bricks removed since and the `WorldChunk` frames carry only the
  bricks added or changed since; the client assembles them over its kept
  bricks (`WorldAssembly::kept`). Everything else in the checkpoint (names,
  tools, poses, mini-games, package state) is sent whole as before, so the
  replica is the host's state at the Welcome's cursor either way.
- Fail safe: without `Hello::kept`, with another map, with a cursor the
  backlog no longer reaches or never reached, or with a ticket the host does
  not know, the join is exactly the old one (the whole world, or a refusal
  that the client retries as a fresh join). A client offers its world only
  when it holds all of it (no join or map change still streaming).
- The game client's connection view carries the kept bricks (an O(1)
  persistent-map snapshot) and the automatic rejoin after `CONNECTION_LOST`
  offers them with the ticket.
- Wire change: `crates/net/protocol-changes/resume-kept-world.md`.

## Evidence

- `cargo test -p bri-net --lib backlog`: merge order (latest change per
  brick wins), floor after forgetting, a map change forgetting its own
  cursor.
- `cargo test -p bri-net --test loopback
  a_resume_over_kept_bricks_downloads_only_the_changes`: two players, one
  drops holding two bricks, the other plants one and hammers one; the resume
  sends one chunk, the replica equals the host's, later updates follow; a
  kept world of another map or a cursor ahead of the host gets the whole
  world; an unknown ticket is refused.
- `cargo test -p bri-net --test bandwidth
  a_resume_over_kept_bricks_sends_no_world_when_nothing_changed`: a 100k
  brick build; the fresh join downloads every brick, the resume sends zero
  world bytes.
- Cloud box numbers (the packed brick codec compresses a flat plate grid
  hard, so the bytes are small either way; the ratio is the point): fresh
  join of the 100k-plate build 30,576 B of world in two frames (head and
  chunk); resume with nothing changed 1,840 B, the head alone, no chunk,
  and the host never walked or encoded the 100k bricks. Host report:
  `kept_resumes` counts the resumes that took the short path.
- `cargo test -p bri-net --test bandwidth`, `--test loopback`,
  `--test hardening_net`, `--test replication`; `cargo clippy -p bri-net
  --tests -- -D warnings`; `cargo check -p bri-client --tests`: pass.
- PC numbers (a real build, both resolutions) go in the audit report's
  status column when the gate queue runs them.

## Next

The resumed client still rebuilds its scene from the replica (the
disconnect clears it), so the saving is the download and the host's encode,
not the client's mesh build. A kept scene across a resume is a separate,
client-side change.
