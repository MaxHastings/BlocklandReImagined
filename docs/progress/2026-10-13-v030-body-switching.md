# 2026-10-13 v0.3.0 W4: switching bodies without a mini-game

Branch `claude/character-switching-39vhwt`, into `v0.3.0-experimental`,
stacked on W2 (#109). Max asked that a brick turn a player into any loaded
body, for example a spawn room of class bricks for the Blockhead, Mario,
Master Chief or a Goomba. Any body is both a player type and a bot body.

## What changed

- **One transition** (`Session::set_player_archetype`, `crates/sim/src/session/combat.rs`).
  Every route goes through it: the brick event, `/body`, a mini-game or team
  rule, an Add-On's `set_archetype` and respawn. It does these steps in order:
  - swaps the archetype; the motor restarts the controller when it differs
  - keeps the share of health, so a player at half health stays at half of the new maximum
  - resizes a seated occupant and re-seats riders
  - ejects a body that cannot ride
  - puts the held item away for a body without hands, keeping the item in the inventory
  - re-holds the brick ghost

  Client prediction takes the authoritative body whenever the archetype
  changes, so each switch costs one correction.
- **Room check** (`Player::room_for`). A brick or `/body` switch to a wider
  or taller body is refused when static geometry is in the way. The player
  sees "No room to become X here". A switch while seated skips the check,
  and so does an Add-On's own switch.
- **Choice and precedence** (`crates/sim/src/session/bodies.rs`). The
  latest choice wins.
  - An Add-On's choice is always kept.
  - A player's own choice (brick or `/body`) is kept through respawns
    outside mini-games and into any game that keeps chosen bodies.
    `keep_chosen_body` is on by default, so a dialog-made game keeps
    picks. This follows Max's playtest script, where he is Mario, his
    friend becomes Master Chief from a wrench brick, and they fight in one
    mini-game. A game with `keep_chosen_body: false` ends the pick, and
    leaving that game does not bring it back.
  - Otherwise the team's body applies, then the game's player type.
  - Per-team bodies chosen in the dialog are parked; Max said to work
    with what exists.
- **Brick event**. `Player → ChangeDatablock` lists the v20 player types,
  then the server's Add-On bodies. The client receives the list with the
  server's content (`tool_ui::offer_bodies`).
- **Mini-game rules**. A mode may set `bodies` (allowed for `/body`) and
  `keep_chosen_body`. A team may set `body`, through rules `teams()` or a
  `set_teams` spec. The settings dialog keeps the rules an Add-On set.
  Playground allows the Blockhead and keeps chosen bodies.
- **`/body`**. With no argument it lists the bodies you may pick. With a
  name (an id, a v20 datablock or a display name) it switches you. The
  command has a cooldown of 60 ticks and says when you hit it. A dead
  player is told whether the choice applies at respawn.
- **Defect fixed**. A body with `can_ride: false` boarded a vehicle by
  landing on it (`vehicle_contacts` ignored the archetype). Now it cannot.

## Evidence

- `cargo test -p bri-sim --test body_switching`: 13 tests pass. They cover:
  - class bricks round-trip a builder through Add-On bodies
  - the Carbine fires from each body's own hand
  - a mid-air switch keeps momentum
  - a seated switch in the buggy stays seated or steps out safely
  - a body without hands stows the Carbine
  - the room refusal
  - the health share
  - respawn precedence
  - `/body`
  - a mid-match Runner switching to a Blockhead
  - two players switching independently
  - bodies picked from bricks coexisting in a dialog-made mini-game,
    through a late join and respawns
  - a predicting client at 18 ticks of latency
- `cargo test -p bri-net --test playground players_switch_bodies_mid_match_armed_and_seated`:
  two QUIC clients switch armed, mid-match and seated, and both see every switch.
- `cargo test -p bri-minigames --test teams`: covers team bodies and the dialog keeping the Add-On rules.
- `cargo test -p bri-client tool_ui`: `ChangeDatablock` lists the server's Add-On bodies.
- Matrix rows: `docs/audits/interop-matrix.md` (W4).

## Notes and next

- The Stomper (#110) is not on experimental yet. Its switching rows are
  added once it lands.
- The client's vehicle-prediction occupant size is not refreshed on a
  seated switch. It corrects on the next authoritative update. This is minor.
- In the net test harness a jet dismount needs one input per batch, or the
  ack wait hangs. This happens without a switch too, so it is a harness
  artifact.
- Wire: `crates/net/protocol-changes/body-switching.md`.
