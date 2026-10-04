# 2026-10-04 Never lose the world

Branch `fix/never-lose-world` (from `53f05cf`, v0.2.3). Four lost-work
findings from a code-reading review, each verified against the code before
fixing.

## 1. A failing host threw its world away (verified)

`server::run` ended with `outcome?;` before `map_loader.outgoing` and before
the report took `session.saved_world()`, although the `PanicFuse` comment
said a blown fuse "saves the world on the way out". The fuse errs past 8
faults a minute, so a tick that panics every time stops the host in about
nine ticks, and so does any other `?` in the loop ("Replication sequence
exhausted"). `bri-server` then left through `server.stop().await?` before
`save_new`, and the menu host only logged "Host stopped with an error".

Now the loop's end always builds the report: the outcome becomes
`ServerReport::failure` (a panic outside the guarded work too: the loop
future runs under a small `CatchUnwind`, and the session lives outside it).
`ServerHandle::finish` returns the report either way; `stop` keeps failing
on a failure so the existing tests and chaos runs still catch host errors.
`bri-server` saves with `finish` before it exits non-zero, and also notices a
host that stopped by itself while it waits for a signal. The menu host's
worker uses `finish`: a failure writes the final world into the recovery slot
(below), the game ends with "The game you were hosting hit an internal error
and stopped. Your unsaved build was kept, so you can recover it.", and the
failure screen asks Recover Unsaved Build? (Keep / Discard).

## 2. No autosave; SIGTERM, SIGHUP and closing the console lost the world (verified, redesigned)

Max removed the earlier autosave (`df90261`, "v20 never auto-saved")
because it kept adding save files. So this is not an autosave: each host
keeps exactly one crash-recovery snapshot (`bri_net::recovery`), in its state
folder, never in the saves Load Bricks lists. The loop looks once a minute
and writes only when the world revision or the mini-game snapshot changed;
the snapshot (`Session::recovery_snapshot`) is the world (a persistent brick
map, so taking it copies no brick) and one mini-game, and capturing,
encoding and the atomic `bri_files::replace` run on a blocking thread, never
on the tick. A clean stop deletes it; only a failure, a crash or a kill
leaves it.

- Menu host: `<state>/recovery/hosted-world.build`. On the next start (or on
  the failure screen) the game asks once: Keep puts it in its map's saves as
  "Recovered <date> <time>" (a save the player chose to make), Discard
  deletes it. Hosting again before answering keeps the old one as a save
  first, so a new game never overwrites it.
- Dedicated: `<state-dir>/recovery.json`. The next start renames a left one to
  `world-<its time>.json`, the world `resume` carries on from, and says so.
- `bri-server` watches Ctrl+C, SIGTERM and SIGHUP from the start of `main`
  (so a stop asked for while loading is not missed), and on Windows the
  console closing, log-off and shutdown through its own
  `SetConsoleCtrlHandler`, which waits for the save before returning (tokio's
  handler returns at once and Windows then ends the process). Not compiled
  here: this container has no Windows target; the Windows CI check will
  build it.
- The shutdown save is now a build (`SavedBuild` with the mini-game), not a
  bare `World`; `load_startup` returns the build. A restarted server holds
  the saved mini-game (`Session::hold_minigame`) and sets it up again for
  the player who ran it (their principal, recorded only in host recovery and
  shutdown saves) when they join, telling them.

`SavedBuild::capture` with events and ownership both kept no longer rebuilds
the brick map.

## 3. Bricks placed during a save were marked saved (verified)

`poll_files` set `saved_revision` from the view when the file write
finished. The client revision is a local counter, and the app refreshes its
view from the watch before draining events, so even the view at reply time
can be ahead of the answer. The network task now attaches its own
`world.revision` to every `Event::Reply`; Save Bricks carries it through
`saves::Request::revision`, and a finished write marks exactly that.

## 4. Damaged servers.json / trusted-hosts.json were wiped; copies wrote without fsync (verified)

`read_small_json` turned any parse failure (an unknown field under
`deny_unknown_fields`, a torn edit) into the default, and the next write
replaced the file. It now moves an unreadable file aside as
`<name>.damaged-<seconds>.json` (the name the settings loader uses for a
damaged `settings.json`, now one shared `settings::damaged_copy`) and the
menu shows "Saved List Problem" naming the kept copy. `copies.rs` wrote a
`.json.partial` and renamed it without flushing; it uses `bri_files::replace`.

## Evidence

Guard tests, each run on `origin/main` (with only a test seam where the API
did not exist) and on the branch:

| Test | origin/main | branch |
|---|---|---|
| `bri-net` lib `server::tests::a_host_that_keeps_failing_keeps_its_world` | FAILED: "the host threw its world away: The host kept failing (9 faults in a minute)" | ok |
| `bri-net --test dedicated_shutdown` SIGTERM / SIGHUP | FAILED: "bri-server ended with signal: 15 (SIGTERM)" / "signal: 1 (SIGHUP)", no world saved | ok |
| `bri-client` lib `app::tests::a_save_covers_the_world_as_the_host_took_it::synthetic` | FAILED: "the brick placed while saving counts as saved" | ok |
| `bri-client` lib `servers::tests::an_unreadable_server_list_is_kept_before_it_is_replaced` | FAILED: no `servers.damaged-*` copy (servers.rs:229) | ok |

New tests without a main counterpart (new API): `recovery::tests` (schedule
by an injected clock: on time, only when changed, mini-game changes count),
`server::tests::a_clean_stop_deletes_the_recovery_snapshot`,
`dedicated_shutdown::a_left_recovery_snapshot_becomes_the_world_resume_continues_from`,
`bri-sim --test host_recovery`, and the client's
`recovery::tests::a_left_snapshot_is_kept_once_as_a_save_or_discarded`.

Commands: `CARGO_TARGET_DIR=/home/claude/bri-target CARGO_PROFILE_DEV_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test -p <crate> --lib|--test
<file>`; clippy `-D warnings --all-targets` on the touched crates. The
origin/main runs used an exported copy of `53f05cf` with the same tests
(adapted only where the new API is absent) and, for finding 1, only the
`start_stepping` test seam added.

The shared target dir is also written by other worktrees of this repo, and
a path dependency's artifact hash does not include the checkout's path, so
two checkouts overwrote each other's `bri-*` artifacts (a client test binary
built from another tree's sources). Runs here passed `--config` with a
per-package `codegen-units` override to give this tree's crates their own
hashes.

## Next

- Max: on Windows, host a game, build, kill `bri-client.exe` from Task
  Manager, start again: Recover Unsaved Build? should offer the build; Keep
  lists it in Load Bricks as "Recovered ...". Close a `bri-server.exe`
  console window: a `world-*.json` should be written.
- The recovery snapshot keeps one mini-game (the server's, else the oldest).
