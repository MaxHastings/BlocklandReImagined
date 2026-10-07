# Gameplay capabilities and vanilla cleanup: phase 1 plan

Branch: `claude/v0.2.6-capabilities-8gived`, off main 2cf0b3a4. Nothing here is built yet.
This is the catalogue plus the plan for Max's OK.

## 1. What exists today

**Already judged by capability (keep and build on):**
- Weapons: `tactics::Capability` (`bots/tactics.rs:307`) reads attack family, delivery,
  trigger, reach, damage, splash, cadence and push from native image/projectile data.
  Unknown scripted images return `DescriptorRequired` and stay unsupported.
- Declared bot data: `Image::bot: BotUse { fire: Tap|Hold, reach, near, manipulation: Hold{..} }`
  (`weapons/src/lib.rs:466`).
- Seats: `Seat { controls, weapon }` and `SeatRole` on vehicle data; the `shove`, runover and
  `smash` flags. Doors: a brick whose swap pair reverses itself.
- Objectives: package `DesiredState::CarryReturn`, rule-derived `RoundWin` / `ScoreRise`,
  the GOAP planner in `bots/planning.rs`. No game-mode names anywhere in bots.
- Events, package-runtime, render and world: no content-name recognition.

**Still recognised by name (the work):**

| Where | What it decides | Bucket |
|---|---|---|
| `bots/surprise.rs:1649`, `bots/team.rs:530` | spray can image id: hold the trigger, and "that player is spraying" | 1 (trigger style) |
| `bots/combat.rs:169`, `physical_objectives.rs:497,855` | `native_hammer` (image id + `"hammerImage"`): hammer melee, hammer-a-vehicle method | 3 (declared host tool) |
| `bots/combat.rs:331`, `bots/fire.rs:440` | `CORE_TOOLS` item ids are worth 0 | 3 |
| `bots/combat.rs:196` | `"onfireakimbo"` stands in for the akimbo gun | 1 (state data) |
| `session/tools.rs:326-363` | `NativeTool` enum chosen by image id | 3 |
| `session/tools.rs:539-711` | hammer/wand damage 10, explosion and sound names in code | 2 |
| `session/tools.rs:1063` | wrench dialog needs `PRINTER`/`WRENCH` held | 3 |
| `weapons/runtime.rs:27-43` | `CORE_TOOLS`, `HOST_TOOL_IMAGES` (copy of `NativeTool`) | 3 |
| `client/building.rs:82-93,926-970` | third copy: `Equipment {Hammer, Wrench, Printer, Wand, Paint}` by item id | 3 |
| `weapons/runtime/stock.rs` | sanctioned name table: spear, key, balls, skis, sword... | 3 (declared behaviour) |
| `weapons/runtime.rs:2318,3151,3727,3799,3920`, `runtime/sports.rs`, `lib.rs:77` | stock names outside stock.rs | 1 (fold into the same) |
| `session/tools.rs:282` | paint effect chosen from projectile name | 2 |
| `sim/definitions.rs:360` | water, checkpoint, teledoor, chest, pumpkin, spawn chosen by brick id | 2 (`special_kind` already exists) |
| `session/special.rs:7-14` | pumpkin carved only by the sword projectile | 2 |
| `session/combat.rs:2459`, `session/events.rs:1212` | two copies of "holding the admin wand = immune" | 3 |
| `session/weapons.rs:576` | brick-deploy projectile skips the hit hook | 2 |
| `session/rules.rs:1049,1207` | Rule Workshop finds the soccer ball by "steel"+"ball" | 1 (`Family::Ball`) |
| `tools.rs:160,183`, `client/tool_ui.rs:616` | three copies of the `Letters` print rule | 1 (`Print::compatible`) |
| `client/tool_ui.rs:984` | second copy of the special-brick kind mapping | 1 |
| `minigames/model.rs:199`, `ui/api.rs:1243` | two copies of the default loadout | 1 |
| `presentation.rs:42` `CueKind::HammerHit/WrenchHit` | never sent | 5 (delete) |
| `packages/perform/world_edit.rs:264` | `starts_with("v20/")` but stock items are `v20.weapon.*` | likely bug: packages can't put a stock item on a brick |

**Justified, documented and left (bucket 4):** the v20 command contract (spray-can colour
commands, `/bsd`), emote and pain images, the `Letters/A` default print, save-folder map names,
the Tutorial map-script port, GUI skin and `$Pref` names, the `v20.` id spelling.

**Duplicates inside my area:** three weapon descriptions (`tactics::Capability`, the private
`bots::Weapon`, `BotUse`) merged by `image_weapon`; `objectives::View` and
`physical_objectives::Directive` carry the same fields; "Capability" also means package
permission (`crates/package/src/capability.rs`). `bots::Weapon::band` has unnamed numbers
(0.8, 1.2, 0.7, 6, 40, 0.75).

**Movement assumptions, handed to the locomotion thread:** bots only take `Family::Wheeled`
vehicles (`interactions.rs:153,347`) because route prediction only models wheels;
`Family::Ball` push extent (`interactions.rs:459`).

## 2. The shared vocabulary

Three layers. No universal enum, and two of the three layers already exist.

1. **Capability: what can be attempted.** Small typed fields on the definitions we already
   have (image, projectile, brick, seat), filled by the importer for stock content and by the
   Add-On for its own. Runtime code reads the field, never a name. New fields, only as many as
   stock content needs:
   - `Image::trigger`: Tap, Hold or Charge. Derived from the image's state machine where the
     data says it; declared otherwise. Replaces `BotFire` and both spray-can checks.
   - `Image::host_tool`: which built-in building mechanism the trigger runs
     (break, inspect, print, destroy, admin destroy), plus its hit damage and effects as data.
     Replaces `NativeTool`, `HOST_TOOL_IMAGES`, `native_hammer`, the wrench-dialog item check
     and the client `Equipment` mapping. `CORE_TOOLS` stays only as default-loadout data.
   - `Image::behavior`: the stock script behaviour (spear charge, key, ball, skis...), the
     pilot already written in `docs/audits/platform-door-closers.md`. The name table moves
     from `stock.rs` into the importer, outside the runtime.
   - Projectile: `paint` effect, `carves` (pumpkins), `skips_hit_hook`. Image: `immune_while_held`.
   - Brick: declared `special_kind` for every special brick (the field exists; only spawn
     points use it today).
2. **Attempt: what was tried.** The existing `Command`, `MoveInput` and vehicle `Controls`.
   No new type. Bots already send these; the few direct calls that skip `Command` belong to
   the replay thread's input boundary, so I list them for it and don't touch them.
3. **Outcome: what happened.** One small typed per-tick record,
   `Outcome { tick, actor, target, effect }` with `effect` one of damaged, pushed, held,
   released, brick edited, activated. It is built in the one place all of these already pass
   (the weapons-event adapter in `session/weapons.rs`). Bots read it to see whether their
   attempt worked; it replaces the ad-hoc `push_contact` progress. Replay records commands,
   not outcomes, and can checksum outcomes for free. I build this only when the first loop
   needs it, and I tell the coordinator first because it touches a shared session file.

**Stability:** every new capability field is marked "not stable yet" in the docs and the Add-On
schema notes until the modding API freeze after v0.2.6. Modders are warned it may be renamed.

Industry standard vs experimental: declared affordances on objects (The Sims "smart objects"),
GOAP planning (F.E.A.R.), the command pattern and per-tick event logs are all standard.
Deriving trigger style from an authored state machine is our own idea and needs checking
against real stock data before I rely on it.

## 3. Plan: four loops

Each loop: land the smallest capability, move stock content onto it, delete the special cases,
prove nothing changed. Once replay core is on main, the proof is a before/after replay;
until then, the existing headless tests.

1. **Weapon reading (bots only, no data change).** One weapon description; trigger style
   derived from the image; name the `band` numbers; akimbo by its state data.
2. **Host tools read once (done).** `bri_weapons::HostTool`, decided only in the sanctioned
   `stock.rs` table; sim, bots and the wrench/printer dialogs read it. No content change.
3. **Stock behaviours declared in data.** The `stock.rs` table, host tools included, moves to
   the importer; stray names in `runtime.rs`, `sports.rs` and `lib.rs` fold in; the client
   `Equipment` mapping reads the declared host tool. Needs a content regeneration on Max's PC.
4. **Bricks, projectiles and copies.** Declared special bricks, paint, pumpkin, brick-deploy,
   admin-wand immunity once, one `Letters` rule, one default loadout, delete dead cues, fix the
   `world_edit` item check.

## 4. Finish line

Done when, in runtime code outside importers and tests:
- no bot code compares a content id or name;
- no engine code chooses behaviour by image, projectile or brick name;
- each interpretation in the duplicates list exists once.

**Not in this version** (one-line reasons): the default loadout stays written twice (minigames and ui) because the only crate both depend on is `bri-package`, which holds package identity, not stock content; vehicle `Family` and the stock-specific weapon
events (`HorseTransform`, `FootballCatch`...) wait for a second Add-On that needs them; the
Tutorial port is a faithful copy of one map's script; client presentation names (horse camera,
player emitters) don't change gameplay.

**What a player notices:** nothing; that's the test. **What a modder notices:** an Add-On item
can say "works like the hammer / wrench / printer", "hold the trigger", "carves pumpkins" or
"reuses the spear charge" in data, and bots use it with no code change.

## 5. File overlap

- Content packs change schema in loops 2-4, so packs regenerate once on Max's PC at merge time
  (no migrations in alpha).
- I don't reshape `ToolCatalog` or `session/packages/host_data.rs`, so the replay thread's
  edits there stay clean.
- Shared session files I would touch: `session/tools.rs`, `session/weapons.rs`,
  `sim/definitions.rs`, `session/special.rs`. I tell the coordinator before each one.
- I don't touch `nav.rs`, `route.rs` or the vehicle prediction in `interactions.rs`.

## 6. Convergence test (after all three merge)

Max's synthetic package: an unfamiliar body, an unfamiliar item that declares a hold or push
capability, an objective that needs that item, and no code that knows any of their names.
Record a bot solving it and replay it. I own the item and objective half.

## 7. Loop 1 notes (what changed from the plan, and why)

- **Trigger style is not derived from the state machine.** On a closer read, deriving "hold"
  for every image that keeps firing while held would make bots hold the trigger on automatic
  weapons and the hammer instead of clicking. A player would see that. The spray-can checks
  ask "does its shot paint" instead (`tools::image_paints`, built on the existing paint
  rule). That also counts the FX cans, which the old id check missed.
- **Akimbo is justified (bucket 4).** `onFireAkimbo` is a native engine state script (it pulls
  the left hand's trigger), listed with `onFire` in `NATIVE_STATE_SCRIPTS`. It is not a content
  name.
- **The three weapon descriptions aren't true copies.** `BotUse` is declared data, `Capability`
  is the full attack model, and the bot `Weapon` is the fight-distance view built from either.
  The real duplicate was the ranged standoff rule written in two places; it is one function now.
  Folding the structs together would be churn with no behaviour gain.
- Unnamed numbers in the weapon band and reach rules are named constants now.
