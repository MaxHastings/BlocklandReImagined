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

## New Duplicator gaps, batch 1

Carried out from the original's scripts, each through a generic seam:

- Every progress mode's `onCancelBrick`: a cancelled job's `on_copy` has
  `error` `"canceled"` with what it did (cut, paint, wrench, load,
  select, supercut). The port says "Selection canceled!" and drops the
  selection, goes back to selecting after a cut or load, says "Supercut
  canceled!", and stops paint and wrench quietly.
- `onKillMode` / `ndKillMode`: putting the duplicator away cancels its
  job (`cancel_copy`, allowed now for an administrator's own) with
  nothing said.
- `ndStartDeHighlight` / `deHighlight`: `highlight_copy` with a negative
  time lights the selection until it is let go, lit again or taken up;
  0 puts it out. The port lights selections this way and puts them out
  going to fill colour, plant or wrench.
- `startCutting` / `tickCutting`: `cut_copy(p, #{ each: true })` cuts each
  brick the player may and counts the rest in `refused`.
- Progress lines: `on_copy` gives `queued` (a stack's queue) and
  `searched` (a box's buckets, a plant's later passes, in percent), for
  the original's Selecting, Searching and Finding Next Brick lines; the
  plant line shows the failed count.
- `PlayMenuSounds`: the upload start and end and process complete sounds.
- `ndGetPaintColorCode`: the fill colour swatch from the palette.
- `NDM_PlantCopy::onPlantBrick`: `float_copy(p, f, #{ admin_only })` is
  checked at each plant; `on_place` has `float_refused`.

Recount on the real copy (scratchpad import, not committed): New
Duplicator 342 of 363 behaviours, 1 unsupported (`ndRegisterPrefs`);
Duplorcator 38 of 38.

Saved mini-games from main (`SavedBuild::minigame`): duplicator copies
are kept in their own format (`SavedCopy`), so they are unchanged; a new
test plants a copy while its player runs a mini-game, saves the build,
encodes and decodes it, loads it into a fresh game and checks every
brick, the mini-game, and that the duplicator copies and plants the
loaded build again.

Tests: `cargo test -p bri-sim --test advanced_duplicator` (new: a planted
copy saves and loads back with its mini-game; a cut of each brick leaves
what its player may not cut; a copy floats admin-only for
administrators alone), `cargo test -p bri-addon-import --test ports`
(new: the port cancels each job its own way and glows until let go).

Left: stack-owner trust (`ndTrustCheck*` with `stackBL_ID`), the mirrored
plain-brick ghost, the ghost shown to other players with its box, brick
extras in copies, the save progress line, prefs through the settings
seam, `ND_Item::onAdd`, `ndSetMode`'s image flag.

## Stack ownership (New Duplicator trust rules)

`ndTrustCheckSelect` and `ndTrustCheckModify` let a player select and
change a brick when they own its stack (`stackBL_ID`: whoever owns the
bricks it was built on) or have the trust in that stack's owner. The
engine now keeps this as `Simulation::stack_owner`: a brick planted,
copied in or restored takes the stack of the lowest-numbered brick under
it, else of one on top, else is its owner's, as the original's own plant
set `stackBL_ID`. Only bricks in someone else's stack are noted, and it is
not saved (v20 did not save it either). A copy rule's `stack` option
counts it when copying and when cutting, painting or wrenching through
the copy; `may_copy(p, brick, options)` asks the same before a box corner
is taken, which the port uses for `ndTrustCheckMessage`'s refusal.

The 1M probe after the change (three runs, Linux container): worst ticks
select 2.2 to 2.3 ms, cut 1.8 to 3.1, plant 3.0 to 4.4, undo plant 2.4 to
3.3, undo cut 3.2 to 3.4, supercut 1.7 to 1.9, undo supercut 2.7 to 3.4;
totals as before.

Tests: `a_stack_owner_copies_and_cuts_what_others_built_on_their_stack`
(bri-sim), `new_duplicator_port_refuses_a_box_corner_without_trust`
(ports; fails without the rules' check). New Duplicator: 345 of 363.

## Mirroring a ghost brick

`FxDtsBrick::ndMirrorGhost`: outside plant mode, the New Duplicator's
`/MirX`, `/MirY` and `/MirZ` mirror the player's own ghost brick. The
engine's `mirror_ghost(p, axis, asymmetric)` finds its image the way a
mirrored copy places each brick (`Mirrors::mirror_brick`: the image's
turn less the brick's across x, half way round across z, the image's
turn added upside down) and sends `Notice::MirrorGhost` (wire change,
`protocol-changes/mirror-ghost.md`); the client swaps its ghost where it
stands. A brick with no exact image stays and the player is told the
Add-On's line (the original's "asymmetric" and "not vertically
symmetric"). `player(p).ghost` says whether a ghost is out. After a
mirrored plant with inexact bricks, the port now says the original's
"Some bricks were probably mirrored incorrectly" line.

Tests: `a_ghost_brick_mirrors_into_its_twin_where_it_stands` (bri-sim:
the wedge's twin and turn match a mirrored copy of the same brick; twice
is the original; upside down has no image), the client's
`a_mirrored_ghost_takes_its_image_where_it_stands_and_plants_it`, and
`new_duplicator_port_mirrors_a_ghost_brick`.

## The ghost box others see, and what a copy carries

Other players saw nothing while someone placed a copy; the original drew
a blue box round the ghost and sent the ghost bricks to everyone. The
client now reports where its copy ghost stands (`Command::CopyPose`,
wire change `protocol-changes/copy-pose.md`, sharing the ghost brick
rate bucket and sent at most every 100 ms). The host works out the box
from the copy's size alone (`Blueprint::ghost_box`) and tells the rules
through `on_copy_ghost(p, #{box})`, and `()` when the ghost goes away.
The port draws the original's blue box there. The ghost bricks
themselves stay with their owner (`spawnGhostBricks`,
`ndUpdateSpawnedClientList` are marked not run): the host spends no
bandwidth on cosmetics.

Copies now keep each brick's name, light, emitter, item, sound, vehicle
and events (`Blueprint.extras`, wire change
`protocol-changes/copy-extras.md`; saved copies keep them too). They
turn and mirror with the copy: emitter and item directions, `fireRelay`
directions, direction list params (from the event catalog) and vector
params. A row aimed at a named brick turns only when the copy carries
that name, as the original's `ndTransformDirection` did. Planting
applies them one at a time through the same checks as the wrench
(quota, tool catalog, item spawners, event review, relay delay clamp
for non-admins). v20 duplication files dropped in the Duplications
folder are bound the same way as saves, so their lights, emitters,
items and events load too. The New Duplicator item now idles with its
original spin (`ND_Item::onAdd`, read by a cover).

Save progress (`NDM_SaveProgress::onCancelBrick`, `getBottomPrint`,
`ND_Selection::cancelSaving`) is not run: the copy store writes a save
off the tick, so there is no save job to cancel or show.

Tests: `a_copy_carries_its_bricks_settings_and_turns_them_with_it`
(bri-sim), `a_ghost_box_holds_the_copy_however_it_is_placed`
(blueprint), the New Duplicator port test's blue box checks, the
client's copy report serial test, and the content test
`a_dropped_duplication_keeps_its_bricks_names_and_events`. New
Duplicator: 362 of 363 (left: `GameConnection::ndSetMode`, the item
spinning while a job runs; prefs wait for the settings seam).

## The duplicator spins while it works

`GameConnection::ndSetMode` gave each progress mode a `spin`, applied as
`setImageLoaded(0, !spin)`; the image's states (`stateTransitionOnLoaded`,
`stateTransitionOnNotLoaded`, `stateSpinThread`) then spin it up, keep
it turning and slow it down. Image states now carry `loaded`,
`not_loaded` and `spin` (the importer reads the three v20 fields), the
weapon runtime checks the loaded transitions before ammo as
`ShapeBase::updateImageState` does, and an image put in hand starts
loaded. Rules unload the held image with `set_image_loaded(p, loaded)`.
The spin is presentation: each game turns the image's `spin` sequence
from the state the image is in (a speed per state, kept between
states), so it costs no bandwidth. The port's `set_working` unloads the
image while a job runs and loads it when the job ends.

Tests: `a_spin_speeds_up_and_slows_over_its_state_and_keeps_otherwise`
(bri-weapons), the New Duplicator supercut test (the held image is in
its spinning state during the job and back to its idle state after;
fails without the rules' `set_image_loaded`), and the port's image
state checks. The real copy's `ND_Image` imports with spin up, full
speed and spin down states and no diagnostics. New Duplicator: 363 of
363; its prefs (`ndRegisterPrefs`) wait for the settings seam.
