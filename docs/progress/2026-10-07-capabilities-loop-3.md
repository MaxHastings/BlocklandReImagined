# 2026-10-07 Capabilities loop 3: stock script behaviour as data

Branch `claude/project-thread-uoh3ap`, off main after PR #29. Design:
`docs/plans/v0.2.6-design-capabilities.md` §1, with the review's decisions
(sports move to data and `stock.rs` goes entirely; `OnFire` wraps
`HostTool` and is the only stored field).

## What changed

- `weapons/src/runtime/stock.rs` is deleted. The runtime reads declared
  fields and never an image's, item's or projectile's name. New schema
  fields (weapons `SCHEMA` 3 to 4, all marked not stable):
  - `Image::on_fire: Option<OnFire>`, `OnFire { Tool(HostTool), Skis, Key,
    Mount(image) }`. `bri_weapons::host_tool` reads it, so PR #29's callers
    are unchanged. `Mount` is the basketball's swap to its shooting image,
    which the runtime named by id before (`native_id("image",
    "basketballShootImage")` in two places); it is the one variant beyond
    the design note's three.
  - `Image::sport: Option<Sport>` (throw, aimed throw, spawn grace, thrown,
    keys, ball kind).
  - `ProjectileDef::sport_hit: Option<SportHit>` (`KnockOut`, `Catch`) and
    `ProjectileDef::turns_into` (the horse ray's player type).
  - `Image::riding_image`: the image a ball's catcher holds instead on a
    horse (`passBallCheck`'s `"horse" @ %image`), which the runtime built
    from the name before.
  - Existing fields now filled for vanilla: each scripted state's `arm`, and
    `Image::left_image`.
- A resting sports ball becomes the item that holds its ball's image (the
  pack's own item to image link), not "the football's item, else the soccer
  ball's". A pass, lateral, pop, drop, steal or fumble throws the held ball
  image's own projectile, not a stock projectile by name, so an Add-On ball
  can be passed and popped.
- The importer fills the fields (`weapons-import/src/stock.rs`):
  - read from the scripts: the arm move a state's script plays
    (`script_arm`, `playThread(2, X)`) and the left hand `onMount` mounts
    (`left_hand_image`, `mountImage(X, 1)`). The Add-On importer now calls
    the same two functions (`addon-import/src/ports/shots.rs`); the left
    hand read is new there, so an Add-On akimbo pair gets its left gun from
    its own script. The building tools' scripts come from the recovered base
    script, which the importer now also reads for function bodies.
  - from the one remaining name table, for the vanilla import only. Before,
    the runtime table matched any image by part of its name (an Add-On
    `hawkeyeImage` contains `key`); none of the generated Add-On images had
    a state the leak actually reached (checked over `content/addons`).
- The runtime plays `root` on `onAbortCharge` and `onStopFire` only when
  the state has no arm of its own, as it already did for the arm after a
  shot, so a script's own `playThread(2, root)` is not played twice.
- Client: `Equipment::{Hammer, Wrench, Printer, Wand}` become
  `Equipment::Tool(HostTool)`, read from each tool item's image
  (`ItemAssets::host_tools`, `Building::set_host_tools`). The item-id match
  is gone. **For the release notes:** the admin wand now counts as a
  building tool, so holding it shows non-rendering bricks as box outlines,
  as v20's `AdminWandImage` (`showBricks = 1`) did.
- The hammer's bot capability and the host's hammer use named constants
  (`tools::TOOL_RANGE`, `tools::HAMMER_DAMAGE`, from `hammerImage::onFire`
  and `hammerProjectile.directDamage`) instead of bare 5.0 and 10.0. The
  shared `session/tools.rs` change is those constants only.
- The name ban (`weapons/tests/no_datablock_names.rs`) compares sources
  without whitespace, so a call split over lines counts, and bans
  `native_id(` in the runtime outright.
- A weapons pack of another schema is refused with the bootstrap command
  that rebuilds it.
- The synthetic test pack (`bri_weapons::testing`) declares the same data by
  hand, as a port would.

## Evidence

- Parity with the old table, `weapons-import/tests/stock_parity.rs` (needs
  the v20 install and `.research`): every vanilla image and projectile
  matches what the old table made the runtime do, except eight arm cues now
  read from v20's own scripts. The ball images' `onFire` plays
  `playThread(2, root)`; only `redKeyImage` defines `onPreFire`
  (`blueKeyImage : redKeyImage` copies fields, not the namespace); the
  horse's football has no charge or throw script. These are presentation
  cues, outside the match state.
- Before and after, same play: `sim/tests/stock_digest.rs` tours all 21
  stock items on the generated content (pick up, tap, charge and release,
  hold through jet and crouch, a second player in front), asserting each is
  in hand and hashing the match state's part digests every tick. Main
  (bfdf854e) on the main checkout's content and this branch on its
  regenerated content print the same 22 lines (`whole 7821e428c676410a`).
  The tour notices a change: a dodgeball throw edited from 30 to 31 changes
  the dodgeball's line. A replay recording cannot carry this proof, since
  `bri-replay` refuses a content folder other than the recording's.
- The three known failures owned by "v0.2.6 bot work" pass on this
  branch's content (regenerated weapons, item presentation and bundled
  Add-Ons; default Add-Ons installed with `BRI_INSTALL_DEFAULT_ADD_ONS=1`):
  `bedroom_mixed_firefight_survives_spawn_loadout_changes`,
  `imported_ctf_bot_takes_the_actual_flag_and_returns_it_for_its_team`,
  `profile_sixteen_bots_in_the_real_bedroom`.
- `cargo test -p bri-weapons -p bri-weapons-import -p bri-addon-import`;
  `bri-sim` tests `sports`, `tools`, `item_hooks` and the `session::bots`
  and `session::tools` unit tests; `bri-chaos` `bot_physical_objectives`,
  `bot_tactics`, `weapons_fuzz`; `bri-client` `building` and `items` unit
  tests: all pass. `cargo check --workspace --tests` is clean.

## For the PC thread at merge

The main checkout's content needs the weapons pack, the item presentation
pack and the bundled Add-Ons rebuilt (weapons schema 4):
`python tools/bootstrap.py --rebuild weapons --rebuild item_presentation`
(bootstrap ends by rebuilding and installing the bundled Add-Ons). The
copied weapons pack had no stamp, so bootstrap kept it until told. Then the
three known failures above come off `tools/gate-known-failures.toml`.

## Not in this loop

`bri_weapons::scripted_arm_pose` (`lib.rs`), the thread-1 ready pose of the
akimbo and ball images, is still chosen by image name. It is client
presentation (the audit's "not in this version" list); it is the next
read-from-script case (`playThread(1, armReady*)` in `onMount`).
`runtime/sports.rs` still names the stock ball projectiles it spawns for a
pass, pop or lateral, as references, not as a match on a name.
