# 2026-10-01 Bots grab, carry and throw with the Gravity Gun

Branch `claude/gravity-gun-rework-ainb6j` on main 08dfdbf01. Max: a
Blockhead Bot with the Gravity Gun only clicked rapidly; it should hold
you, take you out into the open and throw you.

## Engine (generic, `crates/sim/src/session/bots.rs`)

- A bot whose tool reaches or holds (`reach`, `hold`: any Add-On's, not
  just the gun) keeps its trigger down instead of pulsing it, until it
  catches.
- Holding something, it carries it (`Carry`, goal `Goal::Carry`): the
  nearest open spot (sky 16 up over it and 3 round it, 6 of room all
  round) in rings out to 24, pathed like any goal; it holds its catch up
  at least 0.75 s, and after 6 s it throws wherever it is.
- The throw: it turns at twice its turn rate, aim rising, and lets go
  while still turning, so the hold's lead flings the catch; then no grab
  for 1 s.
- `Session::is_reaching` (movables).

## Evidence

`crates/sim/tests/showcase.rs`:
`a_bot_with_the_gun_grabs_holds_and_throws` (old: held 28 ticks at most)
and `a_bot_carries_its_catch_out_into_the_open_to_throw` (old: let go
under the roof) fail on main and pass. bri-sim clippy `-D warnings`
clean; showcase, portals, unlike_modes, carry_rules, script_api,
vehicles, v20_events pass.

## Jet chase (Max: bots only jetted straight up and down)

- `Session::air_chase` (bots.rs): an enemy (seen, or last seen) at least
  2.5 above and within 30 across, where its plan does not walk up to
  them, is flown to: straight up first (v20 jets lift hardest with no
  move), out from under anything overhead, then steering over once above
  them and gliding down by them. Jet physics are unchanged.
- Guard: `a_bot_jets_over_to_someone_above_it` (a floating platform 7
  up, 11 across): fails before (the bot stood under it), passes now.

## Behaviours (Max: a sensible long-term design, not stitched on)

- The brain picks one behaviour a tick (`session/bots/behaviour.rs`:
  Carry, Fly, Fight, Chase, Search, Return, Wander) in a fixed order of
  urgency, with leeway at the fight band's edge and on the walk home.
  Each sets the goal and movement; aim and trigger stay shared.
  Perception, memory and the walk grid are unchanged.
- Weapon images say how bots use them (`Image::bot`, `BotUse`: `fire`
  tap/hold, `reach`); the Gravity Gun's says hold. Protocol file
  `bot-weapon-use.md`.
- Design: `docs/architecture/bots.md`.
- Evidence: arbitration unit tests (order, band leeway, walk home);
  `bot_use_is_read_and_limited`; every existing bot test passes unchanged.
