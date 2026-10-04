# Engine seams and who is building them

Add-Ons change the game through generic engine seams (see
[platform-principles.md](platform-principles.md) and
[../modding/torque-equivalents.md](../modding/torque-equivalents.md)). When
two threads need the same seam, one builds it and the other builds on it.
Twice in v0.1.11 two threads started the same seam before anyone noticed
(datablock reading and the ammo HUD; own-model loading and textured icons).

**Before you build a seam, look for it here.** If it is listed, build on the
owner's commit or ask the coordinator for it; do not start a second one. If
it is not listed, add a row in the same commit that starts it, and tell the
coordinator. Once a seam is on main, its row says "on main" and anyone may
extend it.

Owners are named by thread title. "On main" records integration of the
mechanism, not complete v20 fidelity or human playtest acceptance; those limits
belong in the feature/known-issue pages and dated evidence. Historical lane
labels are retained so earlier progress notes remain traceable.

| Seam | What it covers | Owner | State |
|---|---|---|---|
| Add-On content preparation | One worker and latest pending choice; owned UI schema crosses threads, caches and audio device stay on the UI thread; host/join continuations preserve cancellation and explicit tool choices | Content reload cleanup | on main |
| Native framework dependency ports | `handles` dependency declarations paired with manifest removal; only applied ports settle source requirements, with evidence retained | Content reload cleanup | on main |
| Click swaps | `CatalogEntry::swap` (front/back brick ids), `ToolCatalog::swaps` and `swap_sounds`; swaps replace bounds and collision, including wider open doors, with optional positional sound, at most every 250 ms | Bundled originals (doors, coffins) | on main |
| Hit regions | `info.region` on damage: head, torso, legs | Bushido's Adventure Pack | on main |
| HUD "holding" line | per-gun HUD text while an item is held | Bushido's Adventure Pack | on main |
| Scope zoom | zoom, `hide_nodes`, `both_arms` | Kaje's Sniper Rifle | on main |
| Image scripts | `Image.scripts` state-machine hooks | Butterfly Knife and Grenade | on main |
| Voxel bricks and own models | `place_voxel`, `*.shape.json` loading, textured item icons, `mode.json` | Trench Warfare game mode | on main |
| Gun behaviour | hitscan, spread, shake, `left_image`, `set_speed_scale`, projectile bounces, children and aura, damage direction, Add-On settings, HUD format, magazines, explosions | Tier Tactical weapons | on main |
| Classic Add-On loader | loading imported originals, datablock reading (Discovery injected) | Tier Tactical weapons | on main |
| Host rules companion | rules that ship beside a weapon or tool | Fill Can Add-On | on main |
| Item looks | `item_appearance`, `looks.json` skins, reach while holding | Gravity Gun rework | on main |
| Mounting and look limits | `mount_object`, `look_limits` | Modder's weapon hook ideas | on main |
| Tethers | tether and winch | Grapple rope Add-On | on main |
| Passages | `Brick::stretched`, `EffectsWorld::set_passages`, clip planes | Portal bricks Add-On | on main |
| Seat-tagged moves | moves tagged with the seat they steer | Turret aim flickers for watchers | on main |
| Importer base datablocks | base v20 bricks, projectiles and damage types without `--core` | Trench Warfare game mode | on main |
| Client app split | `crates/client/src/app.rs` into modules | Code health audit | on main |
| Registries, protocol changes, progress entries | how features add ops, messages and notes without editing shared lists | Tech debt hot spots | on main |
| Brick values | `set_brick_field`, `brick_field`: values rules keep on bricks, readable by every Add-On | Capture the Flag (Slayer) | on main |
| Drop key with empty hands | `Command::DropKey`, `on_drop_key` | Capture the Flag (Slayer) | on main |
| Dropped item names | `Drop::name`, `name_drop`, name tags over drops | Capture the Flag (Slayer) | on main |
| Image lights | `Image::light`, drawn at the mounted image in its paint | Capture the Flag (Slayer) | on main |
| End-of-round report | `show_report`, `report_column`, `Notice::Report`, the client's Report window | Capture the Flag (Slayer) | on main |
| Kept worn images | `mount_image(..., #{ keep: true })`: only the Add-On that put a worn image on changes it | Capture the Flag (Slayer) | on main |
| Orbit camera | `orbit_camera` with a body that acts (Throwing) or is frozen (`watch`, spectating), `ControlObject::Orbit::body` | Capture the Flag (Slayer) | on main |
| World shapes | `show_shapes`/`hide_shapes`: translucent boxes and labels every player sees, replicated by key, dropped with their player | Duplicators | on main |
| Bot behaviours | the bot brain's behaviours and their order (`docs/architecture/bots.md`), a weapon image's `bot` use (`fire`, `reach`) | Gravity Gun (bots lane) | on main |
| Rules bots | `add_bot`, `remove_bot`, `rest_bot`, `bot_tool`, `bot_kinds`, `bot_limit`, a player's `spawner`; capability `bots`; the engine brain roams, fights and respawns them, sharing the 16-bot cap with spawn-brick bots | Capture the Flag (Slayer) | on main |
| Message box | `message_box(p, title, text)` (v20 `MessageBoxOK`), capability `chat` | Capture the Flag (Slayer) | on main |
| Saved mini-games | a build keeps its saver's mini-game (settings, Add-On settings, teams, `per_minigame` state keys); loading sets it up again and sends `on_minigame` `loaded` | Capture the Flag (Slayer) | on main |
| Copy jobs | big copy work a slice each tick (`CopyWork`, `cancel_copy`, `copy_working`), copy ghost subset, `IdMap` | Duplicators | on main |
| Stack ownership | `Simulation::stack_owner` (v20's `stackBL_ID`, not saved), `CopyRule::stack`, `may_copy` | Duplicators | on main |
| Copy pose | `Command::CopyPose` (client reports where its copy ghost stands), `Blueprint::ghost_box`, `on_copy_ghost(p, #{box})` hook | Duplicators | on main |
| Copy extras | `Blueprint.extras` (names, lights, emitters, items, sounds, vehicles, events), turned with the copy and planted through the wrench checks (`give_copy_extras`) | Duplicators | on main |
| Image loaded and spin | `State.loaded`/`not_loaded`/`spin` (v20 `stateTransitionOnLoaded`/`NotLoaded`, `stateSpinThread`), `set_image_loaded`, client spin clock in `world_items` | Duplicators | on main |
| Slayer game rules | `remove_body`, `setting_info`, `setting_text`, `data_lines` (rules data files), `on_pick_spawn` answering `"map"`, `teams` events with `by`/`quiet`, `on_minigame_request` `info.teams`, mini-game event chat charged to the acting player | Slayer and CTF port | on main |
| Experimental event guards and rule observations | Optional IF fields on existing event rows, bounded transient state, region/object inputs; existing event scheduler and package vocabulary remain the execution path. No lasting schema commitment. | Rule Workshop | integrated for v0.2.0 alpha |
| Detection region authoring and outlines | Direct wrench dimensions, shared authoritative bounds, live creator preview and replicated outlines with existing building-tool visibility | Release coordinator | on main (v0.2.2 refinement); human acceptance remains separate |
| Environmental bot interactions | Capability-based ground seats and loose hazards; bounded advisory claims, live occupancy, dated allied evidence, ordinary seat controls, finite-mass walking contacts; `docs/architecture/bot-interactions.md` | Bot NPC coordination | integrated for v0.2.0 alpha |
| Bounded NPC objective planning | Utility-selected plans over observed scoped facts and truthful action descriptors; deterministic search/memory budgets, canonical rule context/permission, live revalidation and ordinary-control execution; `docs/audits/bot-objective-spike.md` | Sol High NPC runtime; root integration/release | on main (v0.2.2); bounded real-control evidence recorded; unsupported mechanics and human acceptance remain open |
| Grouped native popup choices | Optional UI-only families and search aliases retain authored IDs, keyboard/mouse behavior and unavailable selections; shared by Wrench and MiniGame screens | v0.2.2 root integration | on main (v0.2.2); human acceptance remains separate |
| Custom actor body fidelity | Imported square collision dimensions and contiguous authored mount nodes; existing avatar-part selection reveals matching own-model accessories; authored swim loop in liquid; read-only script body/mount/brain-kind relationships | v0.2.2 root integration | on main (v0.2.2); Shark remains package policy and fidelity remains partial |
| Provider-owned bot rest | Existing `rest_bot` additionally accepts kinds owned by the caller or its explicitly declared host-rule companion; deletion and equipment ownership remain restricted to rules-spawned bots | v0.2.2 root integration | on main (v0.2.2); human acceptance remains separate |
| Host colorset selection | Bounded local text palettes and imported `colorSet.txt` data initialize a fresh world's existing palette; ordinary replication and brick-save color matching remain the mechanisms | v0.2.2 root integration | on main (v0.2.2); human acceptance remains separate |
| Definition-owned visibility | Existing `set_brick_shown` additionally admits a brick definition's provider or its declared host companion; capability and foreign-brick trust checks remain in place | v0.2.2 root integration | on main (v0.2.2); physically verified |
| Objective causes and affordances | One desired-state/causal-input/grounded-action lifecycle over the existing bounded planner; Brick, physical, combat and typed package adapters use ordinary executors and actual observations | v0.2.2 Sol High performance/foundation; root brain integration | on main (v0.2.2); local real-control evidence recorded; one Windows repeated-hold journey failed and v0.2.3 revalidation remains open |
| Read-only package objective discovery | Opt-in bounded `bot_objectives` query shares the Rhai sandbox, rejects attempted state/operation/output writes, and returns typed gameplay state rather than bot controls | v0.2.2 Sol High NPC foundation | on main (v0.2.2); bounded read-only session discovery and real pickup/return verified |
| Declared physical hold affordance | Optional image metadata describes native command-backed hold limits; actual hold geometry comes from the existing movable executor, not duplicated grip physics | v0.2.2 root and Sol High performance | on main (v0.2.2); native-hold objective integration has local evidence; Windows repeated-entry revalidation remains open |
| Canonical round result observations | Read-only bounded history records real MiniGame RoundEnded player/team/owner identities for diagnostics and acceptance, without commands or replication | v0.2.2 root integration | canonical-effect and retention unit test verified |
| Canonical death observations | Bounded read-only victim/life/killer/game/round results after the canonical life transition; planners observe these rather than infer death from attempted damage | v0.2.2 root combat integration | on main (v0.2.2); observation is distinct from the unresolved human death/disconnect report |
| Modern map lighting | Offline typed source-light descriptors beside native bundles; independent runtime source loading, current environment and geometry shadow maps; compatibility bake remains isolated | Pharzedia v0.2.2 rendering; root integration/release | on main (v0.2.2); v0.2.3 authored-unlit refinement verified offscreen; broader lighting and human acceptance open |
| Resource choice identity and draft request lifecycle | Shared Wrench/MiniGame resource rows preserve missing authored IDs; recovery gates submission, while ordered correlated settings requests freeze mutation controls and report actual completion | v0.2.3 root integration; creator cohesion review | integrated in local v0.2.3 candidate; full gate/Windows/human acceptance pending |
| Controlled portal frame reconciliation | Authoritative cumulative PassageFrame in own-player/vehicle motion; own pose pairs live mounted vehicle/body frames at one tick. Prediction and camera compare accepted travel, retain stationary admission baselines, and settle walking/mounted transitions without geometry guesses | v0.2.3 root integration | integrated in local v0.2.3 candidate; focused checks and release evidence pending |
