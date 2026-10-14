# 2026-10-13 v0.3.0 Tune changes apply live, every time

Max's playtest of the v0.3.0 preview: "the tuning stuff seems a bit glitchy
sometimes my changes take hold sometimes they don't sometimes i have to save
to addon sometimes i don't". Every part of the Tune window was traced from
the slider to the body that moves.

## What applied when, before

- Looks, hands, eye, camera, seats, animation speeds and health already
  applied at once: the client reads them from the replicated archetype
  table every frame, and the host's grips read the table live.
- Movement numbers and the body's size did not. A host player keeps the
  motor constants (`PlayerTuning`) it got when it spawned or switched body,
  so a tuned speed waited for the next respawn, body switch or new game.
  Saving to the Add-On seemed to help only because people then re-hosted.
- The client's predicted body kept its old constants too: even after the
  host moved, the local player's own movement was predicted with the old
  speed and corrected each frame, which felt like a change that half took.
- The slider jumped back to the old number between sending and the host's
  answer, and a value typed without Enter was never sent.
- Save to Add-On straight after a change wrote the number before it.
- A refused number said why only in chat, behind the window.

## Changes

- `Session::tune_archetype` gives every living player of that body its new
  numbers at once (`set_player_archetype` onto the same body, which keeps
  the controller's state, place and motion), and puts the old body back if
  that fails. `Session::player_tuning` exposes a player's current numbers.
- `Predictor::set_archetypes` restores the predicted player with the new
  tuning when it differs; a failure there warns and keeps predicting.
- The Tune screen holds a sent number on its row until the host's tuned body
  shows it (2 s at most, for a refused one), and sends a typed box when it
  loses focus, on Save and on Done.
- `TuneWindow` remembers sent numbers until the table shows them, so Save
  writes them; `save` checks that the edited file still reads as an
  `ArchetypeDef` before writing it.
- The host's "Not tuned:" answer (`session::NOT_TUNED`) also goes to the
  window's status line, and what was waiting on it is dropped.
- A body with its own movement controller shows a note on the Movement
  part: the controller may set its speed and jumps itself.
- Max also found `/tune` clunky. What is saved is now plain: every number
  played but not in the file is marked `*` on its row and its part, the
  status line names them, Done with unsaved numbers says which once (Done
  again closes; they keep playing for the game), and reopening the window
  in the same game still shows them unsaved instead of forgetting them.
- Max saw sliders snap back or jitter while dragging. The old window put
  the replica's number back on a slider as soon as its value was sent, even
  under the mouse; now a sent number holds its row, and a slider the mouse
  holds is never moved by an update.
- `docs/modding/tuning.md` says what applies when.

## Evidence

- `crates/sim/tests/body_switching.rs`
  `the_host_tunes_a_body_live_and_clients_get_it`: a living Runner's
  `forward` goes 10 to 20 right after the tune, and a predictor given the
  tuned table takes 20. With the live retune disabled the test fails
  (`10 -> 10`).
- `crates/ui/tests/tune_window.rs`
  `a_sent_number_holds_its_row_and_a_typed_one_needs_no_enter`.
- `crates/client/src/tune.rs` unit test: sent numbers saved, refused and
  settled; an unreadable result is never written.
- `crates/client/tests/modder_screens.rs` renders the window offscreen with
  marked rows and after Done with unsaved numbers (1024x768, 1920x1080).
- `python tools/check.py quick --base origin/v0.3.0-experimental` passed;
  `cargo clippy -p bri-sim -p bri-ui -p bri-client -p bri-net --all-targets
  -- -D warnings` clean. `cargo test -p` for bri-sim, bri-ui and bri-net
  passed; bri-client passed except `launch.rs`
  `a_startup_failure_tells_the_player_and_points_at_the_logs`, which fails
  in this Linux container because `libxkbcommon-x11.so.0` is not installed
  (unrelated to this change).

## Next

Max to retune a body in the next preview: each change should take hold
within a moment of the slider stopping, with no save or re-host.
