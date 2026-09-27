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
| W3 | Budgets keyed to a transient handle: a per-connection budget resets when the same origin reconnects. | Budgets that bound a person key on the durable principal (`Administration::login_strikes`); anonymous connections get no credential attempts at all. | Admin password guesses (H-F1); the same key choice already bounds bans and auto-roles, and must bound per-player package budgets. | Fixed |
| W4 | Command preconditions are hand-written per handler, so a policy gate (alive, mini-game build rules) is silently missing on some paths and the client UI is trusted to respect it. | `Command::preconditions()`: an exhaustive per-command declaration (no wildcard, so a new command must decide) checked once before dispatch. | Dead players planting (H-F4); `enable_building = false` not enforced on Plant (H-F3). Package commands (Stress Lab's serverCmd seam) need the same declaration. | Fixed for alive and Build; hammer, wrench and undo inside mini-games still to audit |
| W5 | Expensive per-element validation runs before cheap admission, and element counts are bounded late. | Admission order: sequence and rate first, then declared preconditions, then count bounds, then per-element validation. | Event rows validated before the stale-sequence check (H-F6); SaveBuild cloning the world before per-player limits (H-F5). | Fixed |

## Experiments

| # | Cat. | Experiment (test) | What broke | Result |
|---|---|---|---|---|
| E1 | 6 | `idle_handshakes_from_one_address_cannot_lock_out_real_players`: 96 QUIC connections from one address that never open a stream. | 80 of them took all 80 shared connection permits (joined players also held one for life), so the host refused every new player; each idle connection held its permit 10 s per handshake step and could simply reconnect. | NEW CLASS W1. Fixed: 64 pending slots in total, 8 per address, one 10 s pre-join deadline, stateless Retry under load, joined players release their slot. |
| E2 | 6 | `guests_reserving_huge_request_frames_cannot_stall_other_players`: two joined guests each declare a 64 MiB command frame and send 1 byte. | The frame length reserved the shared 128 MiB body budget before the body arrived, so every other player's commands waited 9.8 s (baseline 1 ms), repeatable indefinitely. Guests never need more than ~2.4 MB. | NEW CLASS W1. Fixed: 4 MiB player frame limit, per-peer 4 MiB allowance (8 commands in flight), 64 MiB bulk frames only for administrators from a separate bulk budget. |
| E3 | 6 | `a_command_flood_does_not_starve_other_players`: a joined peer writes `ToggleLight` requests back to back for 3 s and never reads replies. | Nothing: session rate limits reject the excess, the flooder's replies back up its own stream, and the victim's worst latency stayed interactive. | none |
| E4 | 6 | `many_clients_building_at_once_converge_without_dropping_ticks`: 24 clients each pipeline 40 plants at once. | Nothing: all 960 planted in 0.48 s (debug build), zero dropped ticks, every replica converged. The test first tripped over plant reach and grid rules, which are policy working as intended. | none |
| E5 | 7 | `a_crashed_host_loses_at_most_one_autosave_interval`: plant, then read the disk while the host still runs. | The dedicated server wrote the world only after a clean stop (`bri-server.rs`); a crash or a host loop error lost the whole run, and the windowed host never saves its world. | NEW CLASS W2. Fixed: host autosave hook, 60 s rotating autosaves in `bri-server`. |
| H | 4, 5 | Hardening suite by a separate agent: `cargo test -p bri-sim --test hardening_session` (31 tests) and `cargo test -p bri-net --test hardening_net` (9 tests), covering ownership and trust, every admin action as a player, inventory, seats, credentials, movement validation, request shapes and malformed frames. 33 checks held on first run. | Six findings: H-F1 failed logins reset on reconnect; H-F2 the fourth failed login's disconnect was dropped because the locked sender's reply failed first; H-F3 mini-game `enable_building = false` not enforced on Plant; H-F4 dead players could plant; H-F5 guests' SaveBuild used up the administrator's shared save/load budget; H-F6 event rows validated before the replay and rate checks. | NEW CLASSES W3 (F1), W4 (F3, F4), W5 (F6), W1 again (F5); F2 ordinary bug. All fixed; the tests now pass un-ignored. |
| E6 | 3 | `package_sync::a_clean_client_fetches_verifies_and_reuses_the_servers_packages`: a clean client fetches a modded server's 3 client-side packages (2 share a 1.5 MB texture) and fetches again. | Ordinary bug: the progress total counted the shared texture twice (the download itself fetched it once). Fixed. Server-only package never offered; second fetch downloads nothing. | ordinary |
| E7 | 3, 5 | `package_sync::downloads_reach_only_offered_files`: ask for a server-only package's listing and file, an arbitrary hash, a range past the end, an oversized range; download from a host without packages. | Nothing: all refused with a reason. | none |
| E8 | 6 | `package_sync::download_connections_are_bounded_per_address`: hold 2 download connections, open a third. | Nothing: refused with "Too many package downloads" (the W1 seam applied to a second kind of connection without change). | none |
| E9 | 3, 5 | `package_sync::corrupt_or_interrupted_downloads_fail_safely_and_resume`: a lying server corrupts one chunk, then another drops the connection mid-file; then an honest server. | Nothing: the fetch fails naming the file ("do not match their hash"), nothing unverified is installed, and the honest retry downloads only what had not verified. A client-side budget per fetch was added up front (a hostile server offering valid but huge packages), an instance of W1 on the client. | none |
