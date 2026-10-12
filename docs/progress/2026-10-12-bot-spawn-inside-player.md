# 2026-10-12 Bodies spawned inside another player walk out of it

Found by the v0.3.0 Mixed Add-On Playground (`a_blockhead_bot_fights_the_runners`
on `claude/v030-interop-playground-e14qg7`): with one map spawn point, the
mode's mini-game respawned the Blockhead Bot on the spot the Runner stood on.
The bot stayed in `fight`, its brain asked to walk every tick, and its feet
never left the spawn point; it never fired either, since it never got a line
on the player from inside them.

Cause: the motor's Torque sweep (`torque::update_pos`) treats other players
as solid polygons. A standard body started around a shorter Runner body
(1.0 x 1.9) met the Runner's faces at time 0 in every direction, so each
tick's velocity was removed completely (four hits, `impact` equal to the
velocity) and the body never moved. The smaller player was equally stuck.
Same-sized bodies on exactly one spot happened to slip out, which is why the
earlier motor tests never saw it. Not a brain or spawn-choice bug: v20 places
spawned bodies wherever the point is, and the server keeps that rule
(`Player::spawn_overlapping`, `respawn`).

Fix (`crates/motor/src/player.rs`): at the start of each Torque tick, the
players a body's box is already inside by more than 0.02 on every axis
(`players_inside`) are dropped from that tick's sweep polygons. Players
otherwise never overlap (each closes half its gap per tick), and the
existing player push still parts the two along their least-overlap axis.
Only player bodies are released; bricks, vehicles and other actors still
block as before. Client prediction runs the same motor, so it agrees.

Evidence:
- New `crates/sim/tests/player.rs`
  `bodies_spawned_inside_another_player_walk_out_of_it`: fails on main
  (`walker 0 at yaw 0 moved only [0, 0, 0]`), passes with the fix, for the
  big and the small body walking out in several directions.
- The Playground test with a single spawn point (`vec![SPAWN]`) passes with
  the fix: the bot walks off the spawn point in its first 120 ticks and hurts
  the Runner within 3 rounds. Without it the bot's feet stay at
  `(0, 0.05, 8)` for all 60 rounds.
- `python tools/check.py quick` and `focused` (see the PR).

Next: the Playground thread can drop its second spawn point workaround once
this is on main.
