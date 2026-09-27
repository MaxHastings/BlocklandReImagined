# Weakness ledger

The stress campaign's running record (spec: the "Mod platform stress campaign"
thread). Each experiment attacks one category, records what broke, and
classifies it:

- **NEW CLASS**: a general platform weakness that other systems or packages
  would hit too. Fixed with the smallest general seam, justified by two real
  systems or an engine boundary.
- **ordinary**: a one-off bug, polish, balance or content issue.
- **none**: the platform held.

Saturation is reached when 5 or more consecutive experiments across different
categories find no new class.

Categories: 1 creation, 2 composition/conflicts, 3 multiplayer distribution,
4 authority/cheating, 5 sandbox/security, 6 resource pressure, 7 persistence,
8 package lifecycle, 9 radically different assumptions.

Every experiment is a test whose body is the replayable command stream. Run one
on Windows with `cargo test -p bri-net --test stress_campaign <name>`.

## Classes found

| Id | Class | Seam | Systems justifying it | Status |
|---|---|---|---|---|
| W1 | Shared admission pools have no per-origin share: one source can take a whole pool that every player needs (principle 8). | Per-origin bound in front of each shared pool; privileged bulk traffic in its own pool gated by capability. | Pre-join connection slots (E1), command body budget (E2). | Fixed |
| W2 | Authoritative state is durable only on a clean shutdown path: a crash, or a host loop ending in an error, loses everything since start. | Host-owned `ServerOptions::autosave`: periodic snapshot to a save callback off the tick thread, plus a final save on the error path; `persistence::autosave` rotates crash-safe revisions. | Dedicated server world (E5); windowed host world (discarded at stop); package-owned state needs the same cadence. | Fixed for the dedicated server; windowed host MISSING |

## Experiments

| # | Cat. | Experiment (test) | What broke | Result |
|---|---|---|---|---|
| E1 | 6 | `idle_handshakes_from_one_address_cannot_lock_out_real_players`: 96 QUIC connections from one address that never open a stream. | 80 of them took all 80 shared connection permits (joined players also held one for life), so the host refused every new player; each idle connection held its permit 10 s per handshake step and could simply reconnect. | NEW CLASS W1. Fixed: 64 pending slots in total, 8 per address, one 10 s pre-join deadline, stateless Retry under load, joined players release their slot. |
| E2 | 6 | `guests_reserving_huge_request_frames_cannot_stall_other_players`: two joined guests each declare a 64 MiB command frame and send 1 byte. | The frame length reserved the shared 128 MiB body budget before the body arrived, so every other player's commands waited 9.8 s (baseline 1 ms), repeatable indefinitely. Guests never need more than ~2.4 MB. | NEW CLASS W1. Fixed: 4 MiB player frame limit, per-peer 4 MiB allowance (8 commands in flight), 64 MiB bulk frames only for administrators from a separate bulk budget. |
| E3 | 6 | `a_command_flood_does_not_starve_other_players`: a joined peer writes `ToggleLight` requests back to back for 3 s and never reads replies. | Nothing: session rate limits reject the excess, the flooder's replies back up its own stream, and the victim's worst latency stayed interactive. | none |
| E4 | 6 | `many_clients_building_at_once_converge_without_dropping_ticks`: 24 clients each pipeline 40 plants at once. | Nothing: all 960 planted in 0.48 s (debug build), zero dropped ticks, every replica converged. The test first tripped over plant reach and grid rules, which are policy working as intended. | none |
| E5 | 7 | `a_crashed_host_loses_at_most_one_autosave_interval`: plant, then read the disk while the host still runs. | The dedicated server wrote the world only after a clean stop (`bri-server.rs`); a crash or a host loop error lost the whole run, and the windowed host never saves its world. | NEW CLASS W2. Fixed: host autosave hook, 60 s rotating autosaves in `bri-server`. |
