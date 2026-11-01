# Release-gate fixture inputs and clean bot performance

The first attempt at restoring stock-named packs into the isolated gate content
used older generated files from `content/`; the fresh-checkout shared-door test
then rejected their item-physics checksum against the newer weapons pack. The
three private stock-named directories were refreshed with byte-identical copies
of the corresponding current release package files instead. Package IDs,
versions and roles match; the release's `packages.json` still selects the
unchanged `-v043-r2` directories. This supplies the default-path test fixture
without modifying the published bundle. Afterward the gate binary's generated
weapon-debris test, fresh-checkout shared-door test, and imported weapon hosted
beside v20 test all passed. More default-content cases remain to be rerun.

A separate clean `python -u tools/gate.py --perf-only` run used the isolated
release content and the user's real saves, with no concurrent local builds:
32 bots and 3,600 ticks each. Crazy Collage Madness averaged 6.20 ms (p99
12.5 ms) under the approved 8.00 ms budget; TDM Afghan averaged 4.74 ms (p99
8.3 ms) under its 5.50 ms budget. This run covers those two workloads only and
is not a substitute for the still-failing full release gate.

The open-door stall was recorded as GitHub issue #419; the vehicle TDM case's
low fight activity and an actor reported far below the map were recorded as
#420. Neither is asserted fixed. Original content installations and saved
worlds were read only.
