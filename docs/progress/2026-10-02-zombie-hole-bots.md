# Zombie: hole bricks and Bot_Hole bots on import

Max (v0.1.14): Zombie and Shark bots.

- Engine: a brick catalog entry's `bot` (Bot_Hole's `isBotHole` +
  `holeBot`) makes the brick keep one bot of that kind as soon as it is
  planted (`reconcile_vehicle_bricks`). Guard:
  `hole_brick_keeps_its_own_bot` (sim vehicles), fails on the old code.
- Import Add-On: a hole bot `PlayerData` becomes an archetype (its body)
  and a bot kind in `assets/bots.json` from its h settings
  (`hole_bots.rs`); hole bricks get `bot`. Ports can patch one kind by id
  (merge patch addition) and read Torque colours (`{x:rgba}`).
- Port `bot_zombie` (partial): the Zombie's paint and face, arms out
  (`hug`), and its swipe turning bots of another side at half health
  (`converts_below`). Not ported: "Zombie" name prefix on turned bots.
  Test: `crates/addon-import/tests/bot_holes.rs` on a CC0 stand-in.
- Bundle: Bot_Hole and Bot_Zombie (off by default, like Blockhead Bot).
  The bundled hosted test plants the Zombie Hole and waits for its bot
  (`a_hole_brick_brings_its_bot`). The release build needs Bot_Hole.zip
  and Bot_Zombie.zip in the `--search` folder (both in /mnt/project-files).

Checked: addon-import, content, convert (243 passed); sim, chaos (578
passed); workspace clippy -D warnings clean. Real Bot_Zombie imports with
verdict "converted" (every function ported).

Next: Bot_Shark. Its body is shark.dts, but an archetype's model can only
be a box model or a v20 shape today, so a shark would draw as a Blockhead;
drawing a converted .dts as a player body comes first. Then the swim
speeds (its script sets them all to 0), out_of_water_seconds 9 (3 bot
loops of 3 s), and the 35-damage bite.
