# Native world state and authority

`bri-world` contains portable state and rules, independent of rendering, physics
handles, networking and all Torque readers. Schema 1 saves contain stable brick
IDs, native content references, map identity, original palette, placements,
ownership, names, flags, prints, light/emitter properties, events and pending
actions. Original source records are opaque metadata and never execute.

Native brick transforms use original world units in X/Y-up/-Z coordinates. Angle
IDs are clockwise quarter turns viewed from above. Imported coordinates are not
snapped or rounded to a new grid. Runtime planting still needs the build-grid,
collision/support, reach and map-permission validator; the authority API requires
that validator before inserting a brick. It does not yet implement those physical
rules itself.

Server-created `Actor` contexts own edit privileges. They must never be decoded
from client commands. Owner zero identifies world-owned imported content; clients
cannot obtain authority by supplying an old Blockland ID. Planting assigns the
server actor's owner, clears imported provenance from the draft and allocates a
monotonically increasing ID. Rejected edits/placements do not mutate state or
consume IDs. Removal cancels queued actions involving the removed brick.

## Events

The implemented native subset is activation and player touch, enabled state,
delays, self/named targets, color, rendering, collision, raycasting, light and
emitter changes. Color FX state changes are also represented. Event timing uses
120Hz integer ticks; nonzero milliseconds round up, so actions never run early.
Inputs and due actions are processed at the start of each tick. Equal-time actions
retain authored order. Pending actions persist through save/reload.

Named targets are captured when the input fires, compared without case and scoped
to the source's owner. Execution rechecks source/target ownership. A single input
can schedule at most 4,096 actions; the pending queue holds at most 20,000. Exceeding
these bounds rejects the trigger atomically. This subset cannot recursively fire
relays. Editing event definitions does not retroactively change captured actions.

The server simulation must validate activation reach and originate touch events
from physical contacts. These hooks, visual/physics updates after property changes,
and light/emitter resource resolution remain integration work. Passing state tests
does not prove an in-game event workflow or visual effects are complete.

## Persistence

The native loader checks schema, fields, bounds, finite coordinates/colors, IDs and
pending-event ordering. Unknown fields fail instead of disappearing silently.
JSON uses float-roundtrip support. The file format is editable and versioned;
its strings are authored IDs, never process-local GPU/physics handles.

`save_new` flushes a sibling staging file, then publishes a new revision with an
atomic, no-clobber hard link. Existing revisions are never replaced, and the final
filename is not exposed with partial data. A crash may leave an orphan staging
file. This currently requires hard-link support (verified on this Windows host);
FAT/external-filesystem alternatives and a user-facing revision browser remain work.
No claim of power-loss durability of directory metadata is made.

Hosts checkpoint the authoritative world while running (`ServerOptions::autosave`,
stress campaign W2). Every interval the host loop hands a snapshot to a save
callback on a blocking thread, with at most one save in flight, and saves once
more if the loop ends with an error, since the stop report and its world are lost
then. `persistence::autosave` publishes `autosave-<unix millis>.world.json` with
`save_new` and keeps the newest revisions. The dedicated `bri-server` autosaves
every 60 s into its state directory and keeps 3; passing the newest as its
`<world.json>` resumes after a crash. The snapshot is a clone of the world taken
on the tick thread; bricks are a persistent map, so that clone shares them
rather than copying. The windowed client host does not autosave yet.

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
missing brick definitions after including Brick_Large_Cubes. There are 302 adapted
events; the remaining original events are preserved, not claimed compatible.
Known print aliases now bind to the converted native print catalog in the client
renderer/selector. Item/vehicle/music attachments and many event behaviors still
need resource binding or native adaptation. Saved map IDs are inferred from original
save folders; loading a map not in the current runtime bundle must fail visibly or
offer an explicit map override rather than silently select a different map.

The independent Python verifier compares original source bytes/hashes, descriptions,
palettes, every brick definition/position/angle/flag, source records, and light/
emitter properties. Separate authority tests cover permissions, mutation atomicity,
delay/order, named-target ownership, deletion, save/reload with pending events and
save publication. These are backend checks; the alpha handoff remains outstanding.
