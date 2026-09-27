# Native brick item properties

Implemented 2026-09-26. This slice persists and validates ordinary wrench item
configuration. Actual item appearance, pickup, ownership, respawn scheduling and
equipped gameplay are host integration work; property tests alone do not establish
usable item spawners. No original installation was modified or executed.

## Representation and integration

`bri_world::ItemSpawn` is carried by `Brick.item_spawn` and
`WrenchProperties.item_spawn`:

```rust
pub struct ItemSpawn {
    pub item: Option<ContentRef>,
    pub position: u8,
    pub direction: u8,
    pub respawn_ms: u32,
}
```

Default is no item, position 0 (Up), direction 2 (North), respawn 4000 ms.
Position accepts 0..=5, direction 2..=5, and respawn 1000..=300000 ms. Clearing
the item preserves the other selectors. `validate()` rejects invalid native data;
`respawn_ticks()` rounds milliseconds upward to 120 Hz ticks. An absent entire
`item_spawn` field defaults on schema-1 saves and old wrench property JSON. New
objects require every field and reject unknown fields. This additive save change
does not establish cross-version network compatibility; the host owns its protocol.

Load the validated native weapons pack, collect each item's `(id, ui_name)`, and
append these four core tool choices:

| ID | Display name |
| --- | --- |
| `v20.weapon.hammeritem` | Hammer |
| `v20.weapon.wrenchitem` | Wrench |
| `v20.weapon.printgun` | Printer |
| `v20.weapon.wanditem` | Wand |

The 17 pack items plus these four tools are listed in
[source-inventory.json](source-inventory.json). The core Hammer source name has a
trailing space; display-name aliases deliberately normalize surrounding whitespace.

Call `ToolUi::install_items(rows)` before `server_catalog()` and
`catalog_updates()`. It validates and atomically installs the server allowlist and
the sorted `ItemData` UI choices. Alternatively install the authoritative IDs using
`ToolCatalog::install_items(ids)` before accepting joins. Both reject duplicates,
invalid IDs and more than 1024 entries without changing the old catalog. UI rows
also reject blank/control-containing/overlong names and ambiguous normalized names.
Neither API loads original content or executes source code.

For converted worlds, construct a trusted `BTreeMap<String, String>` from trimmed,
ASCII-lowercase original display names to validated native IDs, then call
`brick.item_spawn.resolve_item(&aliases)` during world loading. It only converts
the `item_ui` unresolved namespace, returns whether it changed the field, and
leaves unknown names unresolved. Report those unresolved names rather than silently
removing their configuration. This operation never reads retained source records.
The wrench also resolves known aliases on a display clone; its original inspection
snapshot and source records remain intact for stale-edit checks.

Full wrench edits retain existing ownership, reach and inspection checks. Native
item validation occurs before publication together with all other properties, so
an unknown item or invalid selector cannot partially apply a color/name/etc. edit.
Music and vehicle controls remain explicitly rejected by this adapter. Item event
execution is a separate host integration; source event records remain preserved.

## Source behavior and conversion

Primary evidence is recovered v20 core, with exact SHA-256 values in the inventory:

| Recovered source | Lines | Evidence |
| --- | --- | --- |
| `server/scripts/allGameScripts-Vanilla.cs` | 2846–2848 | Respawn default/min/max 4000/1000/300000 ms |
| Same | 2239–2267 | BLS `+-ITEM` name and position/direction/respawn loading |
| Same | 11035–11055 | Wrench item setters; IRT is floored seconds multiplied by 1000 |
| Same | 11334–11495 | Item creation and selector/timing setters |
| Same | 11801–11811 | Inspection defaults and respawn milliseconds divided by 1000 |
| Same | 17392–17394 | Registered item, direction and position choices |
| Same | 10408, 10748, 11900, 23935 | Hammer, Wrench, Wand and Printer definitions |
| `client/scripts/allClientScripts-Vanilla.cs` | 11079–11084 | BLS item and NONE serialization retains selectors/timing |
| Same | 15660–15662, 15891 | Respawn field receives/sends integer seconds |

These files live under `.research/v20-dso/`, remain ignored, and are evidence only.
The wrench displays **seconds**, with floor/clamp to 1..=300; the typed UI API,
native save and BLS records use milliseconds. This corrects the earlier UI audit's
millisecond-text description in `docs/research/ui-ux/01-screens-and-flows.md`.
Thus a displayed `4` sends 4000 ms, and `2.9` sends 2000 ms. Non-numeric input is
bounded to the source minimum rather than becoming an unbounded timer.

The original position selectors are world axes: 0 Up (+Z), 1 Down (-Z), 2 North
(+Y), 3 East (+X), 4 South (-Y), 5 West (-X). In the native X-right/Y-up/-Z-forward
basis these become +Y, -Y, -Z, +X, +Z, -X. Original placement uses the brick's
world-box half size plus the rotated item's world-box half size, correcting for
item pivot relative to box center. Facing is North=2, East=3, South=4, West=5;
the host should apply existing Torque-to-native rotation conversion rather than
assuming selector numbers are native yaw angles. This module stores the authored
selectors; geometry placement remains host-owned.

The converter recognizes `+-ITEM <display name>" <position> <direction> <ms>`.
`NONE` is case-insensitive. Other names become unresolved `item_ui` references.
It mimics source setters: an out-of-range position retains the prior value,
direction clamps to 2..=5, and milliseconds clamp to 1000..=300000. Native live
edits instead reject invalid ranges. Every original extension line remains an
exact source record, including unresolved, malformed and unsupported lines.
Malformed item lines leave the last valid configuration unchanged and produce
diagnostics. Native save/load and build capture/append preserve item fields and
source records, including NONE selectors and builds with ownership/events omitted.

## Verification

Headless commands run successfully:

```powershell
cargo test -p bri-world -p bri-convert --lib
cargo test -p bri-sim --test tools
cargo test -p bri-client --lib tool_ui::tests
cargo test -p bri-client --lib tool_ui::tests::native_weapon_pack_and_core_tools_expose_all_21_item_choices -- --ignored
cargo test -p bri-ui --no-default-features --lib wrench
cargo clippy -p bri-world -p bri-convert -p bri-sim -p bri-client -p bri-ui --lib --tests -- -D warnings
```

Results: world 7 passed; converter 37 passed; sim tools 11 passed; client ToolUi
8 passed and 2 ignored by default; explicit native pack test 1 passed; UI wrench
10 passed; Clippy clean. The native pack test reads the generated JSON, installs
all 21 choices and round-trips every choice through wrench conversion. Other tests
cover legacy defaults, unresolved aliases, exact source-record retention, NONE,
malformed/clamped conversion, save/build round trips, invalid/forged catalogs,
atomic property rejection and source seconds behavior. An initial Clippy
`items_after_test_module` failure was fixed by moving the added build tests after
the implementation, then the full command above passed.

No visible windows, interactive input or audio playback were used. Root must still
connect the item catalog, converted-world alias resolution and gameplay lifecycle;
Maxwell must evaluate appearance, placement and pickup/respawn feel in the packaged
build before these properties can be called complete gameplay.
