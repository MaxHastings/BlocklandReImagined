# Stress Lab playtest

The Stress Lab is five ordinary mod packages (`content/stresslab/`, enabled by
`content/packages.json`). Everything below is package content; the engine
only provides general seams.

## Start

1. Run `Launch.cmd`.
2. Start Game, pick **Stress Lab Strata**, then Single Player or LAN.
3. You spawn on generated ground: grass, dirt and stone over bedrock, with
   coal, copper and gold deeper down. More chunks appear as you walk.

## Play

- **H** mines the block you aim at (within 8 units). Ore goes into your pouch.
- **G** sells your ore for Bits (coal 2, copper 5, gold 20).
- The **Stress Lab Miner** panel at the top right shows your Bits, ore and
  blocks mined. The server keeps every value; the panel only shows them.
- Creepers appear near players every 10 seconds (at most four at a time).
  They walk to the nearest player, flash white while their fuse burns, then
  explode: they hurt players and blow a crater in the ground. Run to escape
  the fuse.
- **J** spawns a creeper next to you when you are the host (administrator).
  Other players get a refusal: the command is admin-only.

## Things to try

- A friend joins over LAN: both see the same world, holes and creepers.
- Leave and rejoin: your Bits and ore are still there.
- Quit the host and host Stress Lab Strata again: the holes you dug and your
  Bits come back (saved under the client state folder, `packages/`).

## Known limits

- The world stands on Slate's sky and ground; packages cannot provide their
  own sky yet.
- Base-game weapons do not hurt creepers yet.
- Mining uses a key, not the mouse, because a package cannot claim the
  mouse without replacing the base game's tools.
