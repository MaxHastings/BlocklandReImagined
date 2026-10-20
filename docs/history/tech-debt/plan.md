> **Historical note. Written 2026-10-01 (the file's date in the cloud project folder, `tech-debt/plan.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Tech debt: what keeps slowing the lanes down, and the plan (v0.1.11)

Thread: Tech debt hot spots. Main at survey time: 312e912f5 (protocol 69).
Owner of the app.rs split: Code health lane (not repeated here).

## Evidence

**Merge conflicts, measured.** Of the 118 merge commits on main since 2026-09-28,
these files were changed on both sides of a merge. Correction: docs/progress.md
was already `merge=union` in .gitattributes, so its 95 rarely became real
conflicts; app.rs and the shared lists below are the real hot spots.

| File | Merges |
|---|---|
| docs/progress.md | 95 |
| crates/client/src/app.rs | 59 (Code health owns) |
| docs/modding/README.md | 24 |
| crates/sim/src/session.rs | 11 |
| crates/render/src/scene.rs, scene.wgsl | 11 each |
| crates/package/src/defaults.rs | 11 |
| packages/default-addons.json | 10 |
| crates/package-runtime/src/script.rs | 10 |
| crates/sim/src/session/packages.rs | 9 |
| crates/package-runtime/src/ops.rs | 8 |
| crates/client/src/items.rs | 8 |
| crates/client/tests/default_add_ons.rs | 7 |
| docs/modding/torque-equivalents.md | 6 |

After batch151d alone, 10 lane tips conflicted (Tier, Portals, Code health,
turret, GG, Knife, Sniper, Trench, Adventure, Throwmod). Fill Can hit 13 files
at once and Sniper 15.

**Protocol number.** `pub const VERSION: u32` in crates/net/src/protocol.rs is
one integer and one growing doc comment. Every network change edits both.
Throwmod took 69 (Environment already had it), Grapple went 69, 70, 71, turret
and Throwmod both held 70, and the Gate hand-numbers every batch (70 to 73 for
the current queue).

**Default Add-On list kept in three places.** packages/default-addons.json, the
expected id list in crates/package/src/defaults.rs (twice, in tests) and
crates/client/tests/default_add_ons.rs. Every new Add-On edits all three.

**Central script-op and session lists.** A new script operation means editing
the 45-variant `Op` enum plus `capability()` and `op_name()` in ops.rs, a
binding among about 87 `register_fn` calls in script.rs, the handler in
session/packages.rs (3,300 lines), and often a field on `Session`, a `Notice`
variant and a `Delta`/`Checkpoint` field. Throwmod conflicted in all of these.

**The same seam built twice.** Tier and Adventure both started datablock reading,
magazines and the ammo HUD; Adventure and Trench both needed own-model loading
and textured icons; Tier, Grapple and Duplicators each began host-rule wiring
that Fill Can owns; Tier was about to build a second importer base-datablock fix.
Owners were assigned only after lanes had already started.

**Importer needs `--core`.** Base v20 datablocks (brick1x1Data, default
projectiles, damage types) only resolve when the decompiled core scripts are
passed with `--core`. Eight lanes needed a Gate re-run with `--core`; players'
in-game Import Add-On has no such flag, so Add-Ons built on base bricks fail
there (Trench's 11 dirt bricks, Slayer's spawns).

**Tests that only run on Max's PC.** 281 `#[ignore]` tests plus 9 whole test
targets skipped on CI need generated v20 content. Lanes cannot run them in the
cloud, so failures appear a round late at the Gate (turret failed twice;
Code health broke the Tank Turret spawn-list test). The opposite also happened:
the Tier loader's tests read Max's real Steam Add-Ons folder.

**More from the coordinator (2026-10-01 03:02Z):** adding a required field
to `bri_addon_import::Options` broke four test files in three crates (public
option structs with no `Default`); Adventure's branch carried a stale copy of
the hit-region seam that clashed with main's; Tier's loader and Trench's
importer both edit `start_import` in client add_ons.rs. Cloud containers run a
newer Rust whose clippy fails on untouched code (Knife lane).

**Also seen:** the modding README hooks table and torque-equivalents.md are
edited by most Add-On lanes; software-GPU test timeouts on Windows CI (fixed by
Fill Can in 500a975, one re-run still needed since); everything queues behind
one Gate batch while Max's game is open.

## Plan, ranked by lanes helped per unit of risk

Each item is the long-term shape, not a patch. "Now" items touch nothing a
lane is editing. "Final batch" items land with or right after the app.rs
split on final main, so no in-flight lane has to rebase twice.

1. **Progress journal as one file per entry (now).** New entries go in
   `docs/progress/<date>-<topic>.md`; docs/progress.md keeps its history and
   current state and links the folder. Two lanes never write the same file.
   AGENTS.md updated to say so. In-flight lanes' appends to the tail of
   progress.md still merge, because that tail is not touched.

2. **Protocol version without a shared number (next Gate batch, right after
   the current protocol queue lands).** Each network change adds one file
   under `crates/net/protocol-changes/` describing it. The version is the base
   plus the number of those files, computed at build time, so two lanes'
   changes add up instead of colliding, and the changelog lives in those
   files. The Gate's "never goes backwards" check becomes "change files are
   only added". No more hand renumbering.

3. **One default Add-On list (final batch, on top of the bundle lane's
   schema 2).** Each package declares whether it ships on or off in its own
   folder; the list is discovered, sorted by id. Tests check properties
   (unique ids, every entry loads, on/off matches the package) instead of
   restating the list.

4. **Features register their own script ops, messages and session hooks
   (final batch).** One module per op family declares name, capability,
   limits, script binding and host handler together; the engine collects them
   with a static registry instead of central match arms. Replicated state an
   Add-On feature adds (map lights, environment and the like) goes through
   one keyed replication channel instead of a new `Delta` field each time.
   The modding op reference is generated from the same registry, so docs and
   code cannot drift. The perf headline is guarded: dispatch stays a table
   lookup, measured against the Kitchen and 1M-city numbers.

5. **Modding docs split by topic (final batch).** README becomes an index
   over rules, HUD, weapons, content kinds, importing and trust pages; the
   hooks table is generated (item 4).

6. **Importer knows v20's base catalog by itself.** The base datablocks are
   read from the player's own v20 install when content is generated (as every
   other pack is), so neither the Gate nor the in-game Import Add-On needs
   `--core`. Nothing derived is committed. Trench is already building the
   general fix; I review its sha and close any gap (in-game import path)
   rather than build a second one.

7. **Content-free test fixtures (start now, grows over time).** A small
   original fixture pack (CC0, made in code) for the vehicles, items and
   weapons the ignored tests need, with the stock sizes tests depend on taken
   from the existing written tables. Convert the ignored tests that only need
   shapes and numbers, starting with files no lane is editing, and drop
   targets from the CI skip list as they pass. Plus a guard so no test can
   read a real user folder (tests run with a scratch user directory).

8. **Seam owners written in the repo (now).** docs/architecture/seams.md lists
   each engine seam and the lane that owns it, so a new lane checks before
   building. The coordinator keeps it current.

## Done so far

- Items 1, 2 and 8, plus a pinned Rust 1.93.1 (`rust-toolchain.toml`), on
  branch `claude/project-thread-9ttn9s` (e66f800). Lands right after
  batch153; I then turn the four queued protocol bumps (70 to 73) into
  change files.
- Item 6 is Trench's 0114cf0 (`--installed CONTENT_ROOT`, passed by the
  in-game Import with the content root). Checked: it covers the in-game path
  and its test is hermetic. No second fix.

- Parked cleanup branch claude/project-thread-3pq3wl (4cabd5195) folded in:
  two commits on local branch `cleanup-fold` (backup cleanup-fold.bundle),
  rebuilt on main ab08f69a: shared workspace dependency versions, unused deps,
  dead code and probes, old-format defaults nothing on disk needs. Kept what
  main now uses (Linux packaging scripts, macOS state folder, LAN minigame
  argument). Its docs commit is redone during the modding docs split. Lands
  in the final batch with the rest.
  Old-file check (against tags v0.1.0 to v0.1.10): every removed default is
  on a field that existed and was always written since v0.1.0 (world saves,
  wrench properties), or on data the game never writes to disk (weapon
  saves, event checkpoints) or sends only between matching peers. Settings,
  Add-On state and vehicle pack loading are untouched. No converter needed.

- Final batch todo: docs/progress.md (around line 8752) still names the deleted
  prediction-vehicles.json; fix it with the stale-docs pass (fixtures lane note).

## Item 4 design: one line per op

A new script op today edits five places in three files. After:

- `crates/package-runtime/src/ops/<op>.rs` holds the op's struct, its name,
  capability, limits and script binding.
- `crates/sim/src/session/packages/ops/<op>.rs` holds what the host does.
- One line in `crates/package-runtime/src/ops/list.rs` names it. That file
  is only those lines, merged with git's union driver (`.gitattributes`), so
  two branches adding ops never conflict. The `Op` enum, dispatch, capability
  checks and bindings are generated from it; an op missing its host side does
  not compile.
- Public option structs (importer `Options` and similar) get `Default`, so a
  new field never breaks other crates' tests.

## More items from the Code health lane (folded in 03:05Z)

- **Re-export lists in sim session.rs** (`pub use vehicles::{...}`): one-line
  lists every lane appends to; batch151d conflicted there. Fix in the final
  batch together with the rustfmt pass: one item per line, sorted.
- **Struct literals copied across tests** (`DriveSpawn {..}` in three files,
  hand-built empty weapons and vehicle packs). Fix: a shared test-support
  module with constructors (`Pack::empty()`, a `DriveSpawn` builder), and
  `Default` on public option structs. Same root as the importer `Options`
  break.
- **About 14 `Session::set_*` setters** each host path must call (Change Map
  and the dedicated server lost bot kinds and Add-Ons this way). Code health's
  `bri_net::host_setup::{SessionContent, HostSetup}` (d5142617, batch153) is
  the one route; new content kinds go through it. Documented as a seam.
- **server.rs main loop is minified one-liners**; and rustfmt drift across
  many files. Fix: one fmt-only commit on final main (after the app.rs split),
  then a CI `cargo fmt --check` so it stays.
- **Content-only test targets** failing in the cloud ("Run the documented
  importer first"): bri-sim tools (32), vehicle_prediction, vehicles;
  bri-vehicles native, skis, flying_jeep, flying_vehicle, impacts; bri-weapons
  runtime; bri-client brick_fx, item_rendering. Fix is item 7: synthetic
  fixtures through bri-chaos (as `gun_seat_fire` does), real-content variants
  kept for the Gate.
- **Cloud disk**: a full test build reaches about 29 GB. Test builds already
  use line tables and dependencies have no debug info, so the size comes from
  202 integration test binaries each linking the whole engine (60 in bri-client
  alone). The real fix is one test binary per crate (`tests/main.rs` with a
  module per file), which also cuts link time on the PC; it touches how the
  gate names, skips and serializes test targets, so it goes after the final
  batch, measured first.

Later, after the above: renderer feature passes registered like item 4
(scene.rs and scene.wgsl conflicts), and items.rs after the app.rs split.
Not mine: Gate throughput while Max plays.
