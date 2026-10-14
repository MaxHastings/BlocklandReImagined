# 2026-10-14 A host stall left a player's moves refused until they respawned

Max, hosting v0.3.0, found his own player laggy after a while (most visible
jetting). Respawning (Ctrl+K) or dropping into the admin orb and back made
it smooth again.

## Evidence

His live game's recording (`recordings/match-29.brimatch`, 136 minutes,
read frame by frame with a scratch reader over `bri_net::replay::open`)
holds 1,105,286 moves the host refused with "Movement input rate exceeded".
His own moves were refused 2,000-17,000 times a minute through minutes
60-110, and none in the first 29. While refused, his accepted moves trailed
his newest by 3-10, and when the trail outgrew the 12 moves a datagram
repeats they were lost outright: 2,481 of his moves in minutes 60-61, 132
in minute 89.

## Cause

`Session::movement` takes a move only while the player's allowance
(`input_budget`, 48) has one; it refills one per tick the host runs. When
the host stalls, `TickClock` runs at most eight ticks to catch up and skips
the rest, but the client kept making one move per real tick through the
stall. Each skipped tick used up one move of allowance and never came
back. At tick 431163 a stall delivered 34 moves at once and took the
allowance from about 26 to 1. From then on the client and the host ran at
the same rate, so the allowance stayed empty. Each move was refused on
arrival and accepted only from a later datagram's copy, so it ran late, and
was sometimes lost.

Only the client falling behind real time (a hitch of its own: 21 ticks with
no moves at 791880, then 12) refilled the allowance. That is why respawning
and the orb seemed to fix it: switching views is the likely hitch, though
this is inferred, not measured.

## Fix

The host loop passes the ticks its clock skipped to
`Session::skipped_ticks`, which adds them to every player's allowance, up
to its cap. The allowance now tracks real time rather than ticks the host
ran, so it still holds a flooding or speeding client to real time. Replays
record the call (`Call::SkippedTicks`).

## Tests

- `prediction::a_host_stall_does_not_leave_a_players_moves_refused`: a
  half-second host stall while a client jets around.
  - Without the credit: hundreds of moves refused in the last five seconds.
  - With it: none refused, and a shorter trail.
- `replay::a_recorded_match_replays_tick_for_tick` records a skip.
- `cargo clippy -p bri-sim -p bri-net --all-targets -D warnings`, and
  `cargo test -p bri-net --lib` and `-p bri-sim --test prediction --test replay`:
  all pass.
  - `tools/check.py quick` stops at `bri-client`, which needs ALSA, not
    installed in the cloud container.
