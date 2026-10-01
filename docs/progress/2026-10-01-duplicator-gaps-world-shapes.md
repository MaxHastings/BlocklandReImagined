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

Linux container results vary with two allocator stalls outside the copy
code: transparent huge page compaction (THP set to madvise and mimalloc's
`allow_thp`; 9 ms ticks, gone with `PR_SET_THP_DISABLE`) and mimalloc's
purge on large frees (8 to 35 ms). Windows has no THP. Both are
allocator settings for the whole engine, not changed here.

A selection's copy is now reserved once (`Blueprint::reserve`), not
doubled and copied a tick at a time.
