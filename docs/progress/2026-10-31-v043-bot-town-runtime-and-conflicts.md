# Bot_Town runtime and dependency conflict checks

Continued from `2026-10-31-v043-bot-town-native-dependency.md`. The private,
unmodified Bot_Town result and its installed Bot_Hole dependency were used in
`bot_town_runtime`: the host's content-provider discovery found the menu
catalog, and every one of the eight menu entries had a present, hash-matching
icon. A real QUIC host planted each imported brick with ordinary authoritative
commands and varied rotations. A late client with an empty cache downloaded
both actual packages, verified their package identities, loaded all eight
downloaded definitions, and received all eight placed bricks. Native world
save/reload retained them with zero unloaded bricks; a collision ray hit every
reloaded brick. The host uses the actual imported definitions and package
identities on a controlled synthetic map; this does not establish every
original NPC behavior or visual appearance of the menu.

The importer also previously picked an arbitrary directory or ZIP if both
matched a required sibling Add-On. A red/green invented fixture now rejects
conflicting content with an explicit error while accepting byte-identical
folder/ZIP copies deterministically. A native package folder beside a legacy
ZIP is recognized separately, rather than mistaken for a conflicting source;
that distinction also has a red/green invented fixture. Source-only and installed-dependency
imports still pass. This covers sibling providers, not all possible duplicate
providers in an entire explicit legacy reference install; cyclic references
and malformed installed-metadata diagnostics remain separate audit items.

Reimporting the real archive with the current importer and original Bot_Hole
ZIP produced bot definitions, menu catalog, collision recipes, icons and all
eight native meshes equal to the installed-native dependency path. An apparent
bot-setting difference against an earlier output came from comparing different
importer revisions, not the dependency path; the same-revision comparison is
the controlling evidence. `import-source-current` and
`import-installed-native` are preserved privately.

Checks: private `BRI_BOT_TOWN_OUTPUT=... cargo test --locked --profile
playtest -p bri-chaos --test bot_town_runtime -- --ignored --nocapture` passed;
`cargo test --locked --profile playtest -p bri-addon-import --test
borrowed_bricks -- --include-ignored` passed six with the private output set;
the existing `installed` and `bot_holes` integration targets passed eleven.
Scoped changed-crate Clippy passed. The first integration fixture lacked a
second player spawn and failed on join; adding a valid second spawn corrected
the fixture, and the complete test passed afterward. No interactive playtest
or source-installation mutation occurred.
