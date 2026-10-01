# Duplicator gaps, world shapes and the 1M copy probe

The Gate's real-copy check on a130a36af found both duplicator ports
incomplete (New Duplicator 95/363 behaviours, 19 unsupported, datablocks
20/24; Duplorcator 9/38, 22 unsupported). This entry closes the
Duplorcator, adds the seams the New Duplicator's boxes need, adopts honest
counting, and adds a timing probe for million-brick copy jobs.

## Honest counting

A port's `handles` map (port.json) names what it carries out without a
rules function of the same name: original functions, `call:`, `new:`,
`set:`, `file:` and `datablock:` keys (`docs/modding/porting.md`). The
report lists them under "Carried out by the port" (Tier's counting, same
names: `ported`, `PortRef::how`, `Summary::ported`), and a handled
datablock as `ported`. Only what the port fully does is listed; a part
behaviour stays unported. Deliberately skipped things say "not run: why".

## Duplorcator: 38/38, 0 unsupported

- `plant_wait` now runs from the last plant (it ran from the wait's start).
- `plant_error(p, error)`: v20's `MsgPlantError_*` icon and sound to one
  player (overlap, float, stuck, buried, too_far, limit, flood).
- The rules: a flood wait after big plants (over 50 bricks, as the
  original), mount/unmount bottom prints, [Cancel Brick] drops the copy
  ("Normal Mode"), the arm swing on fire, `/clientLoad` and `/reloadDup`
  answers. Client upload is not run: the original's Client Loading
  preference defaults off, so a default host takes no uploads.
- `player(p).copy` is `#{ addon, bricks }` or `()`.

## New Duplicator: 313/363, 1 unsupported, datablocks 24/24

`show_shapes(owner, key, shapes)` / `hide_shapes(owner, key)` (effects):
translucent boxes every player sees, each face coloured from outside and
from inside, per-axis outside colours, and a label drawn like a name. The
host keeps sets by `package/owner/key`, replicates the sets that changed
(`Checkpoint::world_shapes`, `Delta::world_shapes`, protocol change
`world-shapes`), and drops a player's sets when they leave. The client
draws them with `bri_render::world_shapes` (alpha blended, no depth
write) and their labels as name tags. The port draws `ND_SelectionBox`
(inner/outer faces, edges scaled `7/1024 * longest + 1`, the selected
corner in blue, shaded corner cubes, "Name's Selection Box", the grey
disabled look) and `ND_HighlightBox` round a selection with them.

The other 50 behaviours are not carried out yet; they need: brick extras
in copies (names, events, lights, emitters, items, vehicles, music), the
ghost relayed to every player and the blue box round it, an untimed
highlight, per-brick cut trust, stack-owner trust, mirroring a plain
brick ghost, the image's loaded flag, a palette query, fuller progress
lines, cancel on put-away, and Add-On preferences. The port is listed
`partial` until they land.

## 1M copy timing probe

`cargo run --release -p bri-net --example copy_job_timing` builds a
deterministic 1000 by 1000 grid of 1x1 plates (1,000,000 bricks), then
selects, cuts, plants, undoes both, supercuts and undoes that, each as a
copy job, and prints each job's ticks, total, first and worst tick against
the 2.5 ms copy budget (10,000 units), then the same as one JSON line.
`COPY_TIMING_SIDE=200` runs a smaller grid.

It fails (exit code 1) when any job's worst tick, the starting command
included, passes 8 ms: a quarter of the 32 ms server tick, leaving the
rest to physics, events and replication while a million-brick edit runs.
The copy work itself is 2.5 ms; the bound covers the parts one slice
cannot split (a bucket of bricks, a chunk rebuilt). A selection's copy
is reserved once (`CopyBuilder::reserve`), not doubled and copied a tick
at a time.

## Per-tick outliers (Windows, c674fd44b)

The Gate's Windows run of c674fd44b had worst ticks of 4.6 ms (select),
40 ms (cut), 42 ms (plant), 49 ms (undo plant), 24 ms (undo cut),
17 ms (supercut) and 74 ms (undo supercut). Causes and fixes:

- **The allocator's purge.** An allocation spy (every alloc or free over
  0.5 ms, with its caller) found single 4 KB frees of the world's brick
  map nodes (`imbl` B-tree leaves dropped in `Authority::remove`) taking
  5 to 20 ms. mimalloc 3 decommits freed memory a second after it is
  freed, inside whichever later `free` or `malloc` notices, so a tick
  after a big cut or undo paid for decommitting hundreds of megabytes.
  `bri_net::allocator::tune()` turns purging off (`purge_delay = -1`)
  at the start of the game, the dedicated server and the probes. Freed
  pages stay committed and are reused; the process keeps its peak working
  set instead of returning memory between big edits.
- **A box selection's last layer joined in one tick.** A layer of
  buckets is gathered by rows, then joined to the selection; a floor of
  a million plates is one row, copied in one go (6 ms). Rows are now kept
  in runs of 16,384 (never one big growing vector) and join the selection
  a slice at a time, charged to the copy budget (`work::MOVED_PER_UNIT`,
  32 ids a unit). The selection is reserved once at the scan's start.
- **A copy's pivot move at the end.** `CopyBuilder::finish` moved every
  brick round the pivot at once (4 ms for a million); selection and load
  jobs now do it a slice at a time (`CopyBuilder::center`,
  `center_copy`) before finishing.
- **The finished selection's ids copied for the report.** The job's end
  reports counts only; the ids stay with the held copy (3 ms saved).

Linux container after the fixes (`MIMALLOC_ALLOW_THP=0`; the container's
transparent huge pages add their own compaction stalls): worst ticks
select 2.2 to 3.4 ms, cut 1.6 to 2.0, plant 2.6 to 3.6, undo plant 2.3 to
3.0, undo cut 2.4 to 3.2, supercut 1.8 to 2.6, undo supercut 2.0 to 2.8.
One run in four had one stray tick (5 to 18 ms) at a different point
each time with no page faults in it: the virtual machine, not the copy.
