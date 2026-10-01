# Trench Digging digs through the host's real loading path (2026-10-01)

Max's v0.1.11 test build (b5d99c948): the Trench Shovel did nothing on a dirt
brick, even inside his mini-game. The cause was the one fixed in
`2026-10-01-companions-follow-their-add-on.md`: his `packages.json` listed
`gamemode_trenchdigging` but not `gamemode_trenchdigging-rules`, so the dig
rules never ran.

New guard `crates/client/tests/trench_dig_flow.rs` follows the path a player's
game takes, content-free:
- A CC0 stand-in is imported with the importer, as Import Add-On installs
  it. It is the port fixture plus 2x/4x/8x Cube Dirt bricks.
- `packages.json` names only the Add-On, as on Max's PC.
- `ClientContent::load` puts 8x Cube Dirt in the brick menu's Dirt tab.
- `packages::load_server` and `HostSetup::session` set up the host as Start
  Game does.
- The host plants the dirt and starts a mini-game whose loadout holds the
  shovel. One shovel click puts a dirt piece in the pocket, and the cube
  splits 7 + 7.

On b5d99c948 it fails at "the rules run with the Add-On". It passes with
24801cfe. Nothing else Trench-specific was broken.

The wrench not opening on the dirt brick is a separate fault. It is
`ToolUi::accept_inspection`'s "Inspected brick definition is unavailable":
`variants` is built from the stock catalog only, so any Add-On brick fails,
portal bricks included. The Portals lane owns that fix.
