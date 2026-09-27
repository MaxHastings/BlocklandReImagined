# Progress entry for root integration

2026-09-26: Native `Brick.item_spawn`/`WrenchProperties.item_spawn` now persist
item ContentRef, original position/direction selectors and millisecond respawn
timing. Defaults NONE/Up/North/4000 ms are source-backed; native validation bounds
position 0..5, facing 2..5 and timing 1000..300000 ms. Missing fields on old schema-1
save objects default compatibly. BLS `+-ITEM` converts to native configuration
with original setter clamping and unresolved display-name references; exact raw
records remain preserved, including malformed/unsupported extensions. Trusted
native alias resolution never interprets source records.

ToolCatalog and ToolUi expose atomic item installation APIs. Ordinary wrench
edits now admit only validated native Item fields in addition to previous native
properties; music/vehicle remain explicit pending adapters. Native pack003 plus
four core tools supplies all 21 choices. Corrected the source-backed wrench text
unit to seconds while keeping typed/persisted values in milliseconds; this
supersedes the older UI audit's millisecond-text description.

Verification: world 7, converter 37, sim tools 11, ToolUi 8, explicit ignored
all-21 native pack test 1, and UI wrench 10 tests passed. Combined five-crate
Clippy with tests and `-D warnings` passed after fixing test-module placement.
See [README.md](README.md) for commands, source line evidence, integration APIs
and limitations; source hashes and all IDs are in source-inventory.json.

Root still owns catalog installation, world-load resolution and actual item
appearance/pickup/respawn/equip/network integration. No spawner gameplay acceptance
item is checked off by property persistence alone. No original writes, game
windows, OS input, audio playback or child agents were used.

## Follow-up: startup integration

Catalog installation and startup world resolution are now connected: client
ContentConfig/Paths defaults to weapons-pack-003, App installs all21 item choices,
local host installs Session weapon definitions, and join/dedicated use the same
native content identity8 (independent of protocol7). Native WeaponContent loader
validates bounded pack/resources and hashes actual converted bytes; original DTS
hashes remain provenance. Cached App catalogs reject disk replacement until restart.
Dedicated CLI and docs/networking.md now accept weapons-dir after foliage-dir.

Verification: identity10 + actualpack1 + content6 + headless App host1 tests passed;
all-target check and Clippy -D warnings passed. Dedicated smoke completed240 ticks,
zero dropped ticks/cues and advertised21 items/version8. See
[startup-integration.md](startup-integration.md) and startup-evidence.json. Later
build-append alias resolution remains root integration; live gameplay and mount
presentation remain root-owned. Provisional eye-origin muzzles are not a fidelity
acceptance claim. All assigned integration paths are returned to root.

## Follow-up: checked authored bounds

ContentConfig/Paths now defaults item_presentation to pack003. Shared native
ItemPhysicsContent checks presentation schema2 pins, schema1 metadata, the exact
item catalog and authored model bounds, plus contained native model and texture
checksums. Host/join/dedicated append content identity9; dedicated CLI adds the
presentation directory after weapons directory. Host installs bounds immediately
after weapon pack and before joins. No original reader or renderer is required.

Identity 13, actual physics pack 1, content 7 and headless App host 1 tests passed;
all-target Clippy -D warnings passed. Dedicated copied-world smoke initialized 21
static items across all catalog choices, resolved imported aliases, completed 240
ticks with zero drops and preserved all 21 IDs/source records in the save. See
item-physics-startup.md and item-physics-startup-evidence.json. Bounds startup is
verified; contact/pickup feel and mount/fade fidelity remain separate acceptance.
