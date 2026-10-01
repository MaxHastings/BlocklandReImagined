# Bundled originals ship their port's host rules (2026-10-01)

Max's test build of d6152ab58 loaded the bundled originals, but their
scripted behaviour never ran: the Hookshot and Grapple Rope fired without
pulling, and Trench didn't dig. The cause was in `tools/addon_bundle.py build`.
It imported each copy into a dot-prefixed work folder and then moved only that
folder into `addons/<id>`. The port's host-rules companion, which Import writes
beside the import at `<out>-rules`, stayed behind in
`.<id>-0-rules`. So every ported original shipped without its rules: Hookshot,
Grapple Rope, Trench, the Duplicators, Fill Can, Throwing and Slayer.

Fix:
- `build` imports each copy straight to `addons/<id>`, so the rules land at
  `addons/<id>-rules`, the layout an in-game Import gives a player. It then
  checks that the import's `companions` name exactly the rules its ports
  wrote.
- `problems()`, which every step runs, requires each named companion to be
  whole beside its original and to depend on it. As a result `sources`,
  `install`, `fetch`, `from-release` (Mac/Linux) and `verify-release` all
  carry the rules and refuse an original without them.
- `sources` lists the rules right after their original, on or off with it,
  so all three packagers ship them and turn them on after an original that
  starts on. `verify-release` checks this.
- Bundle schema 2. A bundle built before this is refused with a "rebuild"
  message. The draft `addon-bundle` must be re-uploaded.
- `bri_package::defaults`: a default that starts on now starts with its
  installed companions, in both a root with no list and a checkout list
  that predates the rules (they are inserted after it). A default the
  player turned off keeps its rules off.

Tests:
- `bundle.rs an_originals_host_rules_ship_beside_it_and_load` uses the CC0
  stand-in Player Throwing. It covers build, sources, install, Library
  turning the original on (the rules follow), `Catalog::load` of both,
  verify-release (refused when the rules are not on), and from-release
  (refused when the rules are missing). It fails on the old tool.
- `defaults.rs an_original_starts_with_its_host_rules`.
- `Test-PlaytestPackaging.ps1` gives the Duplicator stand-in host rules and
  expects them on after it, as a server package.
