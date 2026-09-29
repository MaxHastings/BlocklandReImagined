# Bug patterns and how we catch them (2026-09-28)

Max asked what the last dozen bugs he reported have in common and how to
squash the next ones before he finds them. This is the answer, the checks
that now enforce it, and the audit of every player-reachable hard stop.
Branch `claude/bug-pattern-sweep-h0v7ns` (cloud); the real-screen harness
and the v20 behaviour audit run on the PC (thread "Bug sweep on the PC").

## The five patterns

1. **A screen and the server disagree about what was sent.** The screen reads
   the wrong widget property, or sends a field the server ignores, and the
   tests call the server directly or drive a fake screen, so both halves pass
   alone. Examples: the typed player name never reaching the server; the
   print selector left waiting on a reply the app threw away.
2. **v20 behaviour was guessed, then a test locked the guess in.** The rule
   was written from memory instead of from v20's own scripts and datablocks,
   and the test checks the guess. Examples: the horse camera, the relay
   floor, splash damage.
3. **Only tested as the host, as a Super Admin.** Multiplayer paths were
   exercised from the host's seat, so rules that only bite a guest with
   default trust (or a single player with no server peers) were missed.
4. **A hard stop instead of a fallback.** A refusal, `panic`, `expect` or `?`
   that ends the host, closes the game or leaves a screen waiting forever,
   where a sensible fallback plus a log line would keep the player going.
5. **No budget under abuse or scale.** Loops, big builds and floods had no
   per-tick limit, so one build can stall the host.

Plus one process gap: tests that need generated content skip silently in
GitHub CI and the cloud; only the PC gate (`--include-ignored`) runs them.

## What each brief now asks

- Test through the real screens as a single player, as the host and as a
  guest with default trust.
- Cite the v20 script or datablock a behaviour copies; mark anything
  deliberately different.
- Prefer a fallback and a log line over a refusal or a stop.
- Give every loop, queue and flood a budget, and a fuzz or soak test.
- Budgets inside the simulation count work (rows, bytes, items), never
  wall time, so the game plays the same on any machine; wall time only
  feeds watchdogs and logs. Tests assert counts, not milliseconds; timing
  checks are opt-in benchmarks (`BRI_BENCH`).

## Standing checks added (cloud)

- **Request deadlines in the UI** (`crates/ui/src/ui.rs`): every request a
  screen sends has a deadline (45 s; saves and loads 3 min). An unanswered
  request fails with "The game did not answer this request in time" instead
  of leaving the screen waiting; its late answer is dropped and logged.
  Screens that close while sending (`Core::abandon`) drop theirs too.
- **Command fuzzer** (`crates/chaos/tests/command_fuzz.rs`): one example of
  every `Command` variant, sent by a guest with default trust and by the
  host, then 256 damaged variants (extreme values, dropped keys, cut or
  repeated lists). The host must refuse or handle each cleanly, never fail a
  step, and replicate only finite state. A new variant does not compile
  until it has an example.
- **Event fuzzer** (`crates/chaos/tests/event_fuzz.rs`): random wrench
  programs over the whole catalog (edge parameters, zero delays, relays that
  feed each other, named targets that are missing) on 24 bricks loaded as an
  administrator's build, fired at random for 160 ticks. No tick may take
  longer than one host tick (32 ms, release). `BRI_CHAOS_CASES` runs more;
  `BRI_CONTENT` adds the real v20 catalog on Slate.
- **Host panic fuse** (`crates/net/src/server.rs` `PanicFuse`): a request or
  step that panics is answered with an error and logged, and the host keeps
  going; more than 8 in a minute stops it with an autosave, as before.
- **Event notes in the host log** (`EventNotes`): loop warnings, budgets
  reached and retained rows now reach the host's log, at most 8 lines per
  10 s plus a count. Before, the host never logged them.

## Hard-stop audit

Every player-reachable `panic`, `expect`, `unwrap` on input, `?` in a
frame or tick loop, refusal and wait without a deadline in the client, UI,
host, sim, events and package runtime. Fixed here unless marked routed.

| Where | Was | Now |
|---|---|---|
| UI requests (all screens) | waited forever for a reply | deadline, message, late reply dropped |
| Wrench / event editor | Escape ignored while sending | backs out and abandons the request |
| Print selector | stuck when its reply came from an older dialog | the app answers it; closing abandons |
| Client background jobs (Add-On import, LAN list, firewall fix) | a dead worker closed the game | error message, empty list |
| Client network error in a frame | closed the game | disconnect with the reason |
| Open Saves Folder | folder error closed the game | error on the action |
| Broken Add-On at start or reload | game would not start | left out, "Add-Ons Left Out" message, logged |
| Host request or step panics | host task died, no autosave | answered and logged; fuse stops after 8 a minute with autosave |
| Map load panics | host died | map change fails with the reason |
| Map change cannot place one player | whole map change failed | that player is disconnected with the reason |
| Weapon update over wire limits (70+ owners) | `expect` panic | clamped, rest carried to the next updates |
| Print counter increment at extremes | overflow panic | clamped to ±127 |
| Package value noise at huge coordinates | overflow panic | wrapping arithmetic |
| Kill message with a departed killer | `unwrap` panic | no name |
| Zero-delay event loops (admin builds) | 24 ms event work per tick at 24 bricks | about 6 ms: jobs share compiled rows |

### Round 2 (fixed)

| Where | Was | Now |
|---|---|---|
| Join or rename with a blank, long or control-character name | refused | cleaned as v20's `onConnectRequest` does (ML tags and control characters dropped, cut to 23 characters, trimmed, "Blockhead" when empty), the player told; one rule shared with the client |
| Damaged admin state file | host refused to start | moved aside as `<name>.damaged-<seconds>`, host starts with no bans or saved ranks, logged |
| Admin save the disk could not confirm | host stopped | kept and logged; the next save rewrites the file |
| Administrator's zero-delay event loops | only the engine's per-tick limits | each owner at most 4000 cost units a tick (about 4 ms), everyone 8000; counted, never timed, so play is the same on any machine; the rest waits in order; the owner is logged; ticks over 8 ms are logged |
| Client network worker | a request over 10 s, 64 waiting, or a late answer dropped the connection | that request fails after 200 s, the 65th is refused alone, a late answer is logged and dropped |

### Round 3 (fixed; found by the PC sweep)

| Where | Was | Now |
|---|---|---|
| Avatar screen clan prefix and suffix | saved, never sent; the host ignored them | sent at join and on Done in game, cleaned as `onConnectRequest` does (4 characters, trimmed), shown around the name in chat only, as v20's `serverCmdMessageSent` does |
| Fresh install | asked for the name twice ("Your Name" message, then Choose Name) | Choose Name once per run |

### Routed to the lanes that own them

- **Brick load lane:** Load Bricks stops at a bad brick with a message (its
  milestone 2, citing v20); Load Bricks' colour warning state is not
  cleared on disconnect.

### Closing the rest

| Item | Status |
|---|---|
| Avatar changes refused whole on one unknown part | Fixed: `avatar::Package::repaired` sets each choice the host lacks (part, accent, colour, face, decal) back to the default, keeps the rest, logs it and tells the player (`unknown_avatar_choices_fall_back_to_defaults_and_keep_the_rest`) |
| Large reloads (Add-On changes, map change) freeze the window | Routed to the performance opportunities lane (off-thread reloads) |
| A package script value nested tens of thousands deep overflows the stack | Won't fix: the script engine (rhai) has no data-depth limit, so a fix means forking it. Only an Add-On the host chose to run can do it, and the limit is now in `docs/modding/README.md` |
| Loading screen has no deadline of its own | Won't fix: the connection's own timeout already ends a stalled load with a message, so a second deadline adds nothing |
| v20 event gaps from the PC audit (onBotTouch Client/Driver, radiusImpulse on vehicles/items/corpses, fakeKillBrick 0 s, `/tripOut`, "Do not repeat yourself.") | Fixed; see `docs/audits/v20-behaviour.md` for each test |
| Sandbox `endless_shader_loop` asserted wall time | Fixed: it checks the counted loop limit on each image; the timing asserts only run with `BRI_BENCH=1` |
| Event fuzzer `random_event_programs` wall time | Already fixed in round 4 (no wall-time asserts without `BRI_BENCH`) |
| Loopback `full_event_list_crosses_real_quic_replication_and_native_save_atomically` fails | Routed: the v20 behaviour branch (bug sweep PC, Task B) cuts every brick to 100 rows and 30 s delays, which the alpha contract's event-editor item rules out and which breaks this 1024-row test. Its owner or Gate decides; not changed here |

## Defaults picked

- UI request deadline 45 s, saves and loads 3 min; the network worker
  gives up at 200 s.
- Event work 8000 cost units per tick, 4000 per owner (a row is 1 plus each job it expands into; about 1 us a unit in release); a watchdog logs event phases over 8 ms.
- Hello names up to 1024 bytes are accepted and cleaned; longer is refused.
- Host panic fuse 8 panics in 60 s.
- Event notes 8 lines per 10 s.
- Event fuzzer 4 cases, 1 determinism case and 4 content seeds by default,
  sized to the gate's 600 s limit on a busy PC (a looping case takes seconds
  in a debug build); `BRI_BENCH=1` soaks 16 of each. The command fuzzer 256.
- A broken Add-On is left out, not refused; base content failing to load is
  still an error.
