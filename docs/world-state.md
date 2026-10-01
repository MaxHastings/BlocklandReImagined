# Native world state and authority

`bri-world` contains portable state and rules, independent of rendering, physics
handles, networking and all Torque readers. Schema 2 saves contain stable brick
IDs, native content references, map identity, original palette, placements,
ownership (with each owner's durable principal), names, flags, prints,
light/emitter properties and event rows. Queued event actions are not saved.
Original source records are opaque metadata and never execute.

Native brick transforms use original world units in X/Y-up/-Z coordinates. Angle
IDs are clockwise quarter turns viewed from above. Imported coordinates are not
snapped or rounded to a new grid. The authority API requires a validator
before inserting a brick. `bri-sim` supplies it (`validate_placement` in
`crates/sim/src/simulation.rs`), checking the build grid, collision/support,
reach and map permissions; `bri-world` does not implement those physical rules
itself.

Server-created `Actor` contexts own edit privileges. They must never be decoded
from client commands. Owner zero identifies world-owned imported content; clients
cannot obtain authority by supplying an old Blockland ID. Planting assigns the
server actor's owner, clears imported provenance from the draft and allocates a
monotonically increasing ID. Rejected edits/placements do not mutate state or
consume IDs. Edits check v20 trust levels between the actor and the brick's
owner (`authority::trust`).

## Events

Each brick stores at most 1,024 event rows (`bri_events::Row`). `bri-world` does
not run them. The host session executes them with `bri-events`
(`crates/sim/src/session/events.rs`); see `crates/events/README.md` for timing,
targets and limits.

## Persistence

The native loader checks schema, fields, bounds, finite coordinates/colors and IDs. Unknown fields fail instead of disappearing silently.
JSON uses float-roundtrip support. The file format is editable and versioned;
its strings are authored IDs, never process-local GPU/physics handles.

`save_new` flushes a sibling staging file, then publishes a new revision with an
atomic, no-clobber hard link. Existing revisions are never replaced, and the final
filename is not exposed with partial data. A crash may leave an orphan staging
file. This currently requires hard-link support (verified on this Windows host);
FAT/external-filesystem alternatives and a user-facing revision browser remain work.
No claim of power-loss durability of directory metadata is made.

There is no autosave, as in v20 (removed 2026-09-29 at Max's request; it was the
Autosaver Add-On in v20). Players save with Save Bricks, and leaving with unsaved
changes asks first. The dedicated `bri-server` writes `world-<unix millis>.json`
into its state directory when it stops; `resume` in place of `<world.json>`
starts from the newest one. A crash loses what was built since the last save.

Every world a server accepts can be saved and streamed (stress campaign W7).
`Brick::stored_bound` is an allocation-free upper bound on a brick's JSON save
entry, escapes included, and so on its network encoding. `Authority` keeps the
sum over the world's bricks and refuses any plant, edit, event mutation or
build load that would take it past `MAX_STORED_BYTES` (the 1 GiB
`persistence::MAX_SAVE_BYTES` less 64 MiB for owners and the rest), naming the
budget. Shrinking is always allowed, so a world loaded over the budget can
still be trimmed. Ordinary bricks take about 450 bytes, so `MAX_BRICKS` of
them fit; bricks carrying large event lists reach the budget sooner.

## Original saves

```powershell
cargo run -p bri-convert --bin import_saves -- <v20-saves-dir> <catalog.json> <new-output-dir>
python tools/verify_world_conversion.py <v20-saves-dir> <catalog.json> <output-dir>
```

BLS is read only by the offline converter. The importer preserves descriptions,
all 64 palette entries, exact f32 placements, angle/baseplate/color/effect/visibility
flags, prints, names and every source extension record. Known light/emitter state
and the supported event subset become typed native data. Unresolved resources and
unsupported event inputs/outputs remain explicit diagnostics attached to their
original records. The complete original byte stream is archived by SHA-256.

Stock files use a single-byte degree symbol in ramp names. Valid UTF-8 is accepted;
otherwise the importer uses Latin-1 only when there are no ambiguous C1 bytes.
Ambiguous code pages fail explicitly. Encoding and source hash are recorded.

All 35 supplied saves import: 276,612 bricks and 279,272 source records, with no
missing brick definitions after including Brick_Large_Cubes. `bind_world_events`
(`crates/convert/src/bin/bind_world_events.rs`) types each original event record against the
native events catalog; records it cannot type stay as preserved rows that never
run.
Known print aliases now bind to the converted native print catalog in the client
renderer/selector. The converter binds `+-VEHICLE` and `+-AUDIOEMITTER` records
to native vehicle and music IDs. Saved map IDs are inferred from original
save folders; loading a map not in the current runtime bundle must fail visibly or
offer an explicit map override rather than silently select a different map.

The independent Python verifier compares original source bytes/hashes, descriptions,
palettes, every brick definition/position/angle/flag, source records, and light/
emitter properties. Separate authority tests cover ownership and trust, mutation
atomicity, the storage budget, event-row bounds and save publication.
