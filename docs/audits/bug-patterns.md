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

### Routed to the lanes that own them

- **Name lane:** a name over 48 characters is refused outright; truncate
  with a note instead.
- **Brick load lane:** Load Bricks is all-or-nothing on one bad line; a
  `.bls` with an unknown brick or bad colour should load the rest and list
  what was skipped. Load Bricks' colour warning state is not cleared on
  disconnect.
- **Admin lane:** a poisoned admin store stops the host; the admin panel
  can sit pending (now bounded by the UI deadline).
- **Event loop lane:** the event runtime's per-tick budgets
  (`expansions_per_phase` 8192) are the only bound on an administrator's
  zero-delay loop; the host now logs when they bite.

### Open, not fixed here

- Client network worker (`crates/client/src/network.rs`): a request that
  waits over 10 s disconnects, and more than 64 in flight or a full send
  queue drops the connection. Fallback: fail that request, keep the
  connection. Left while the event loop lane works in the same file.
- Avatar changes are refused whole on one unknown part; fall back to the
  default part.
- Large reloads (Add-On changes, map change) run on the UI thread and freeze
  the window for their length.
- A package script nested thousands deep can overflow the stack in
  `to_json` and on drop. Only an Add-On author can write one; document the
  nesting limit.
- The loading screen has no deadline of its own; the connection timeout
  bounds it.

## Defaults picked

- UI request deadline 45 s, saves and loads 3 min.
- Host panic fuse 8 panics in 60 s.
- Event notes 8 lines per 10 s.
- Event fuzzer 8 cases by default (a looping case takes seconds in a debug
  build); the command fuzzer 256.
- A broken Add-On is left out, not refused; base content failing to load is
  still an error.
