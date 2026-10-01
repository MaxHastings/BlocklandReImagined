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
