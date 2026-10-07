# Platform door-closers audit

Date: 2026-09-27. Code reviewed at `origin/main` c092117, plus
`tools/regenerate_content.py` from commit 3ba01e6 and the uncommitted console
registry in the `console` worktree. Review only: nothing here has been changed.

The principles this grades against are in
[`docs/architecture/platform-principles.md`](../architecture/platform-principles.md).
The question: which decisions in today's code would make the moddable,
agent-driven platform expensive later, and which of them are cheap to fix now?

## P0 progress

Tracked by the Platform door-closer fixes thread. Each line names the commit
that landed it.

- **Package manifest (contract 5, 12):** format landed in `crates/package`
  (`packages.json`, environment, per-package mismatch report), documented in
  [`docs/architecture/packages.md`](../architecture/packages.md). The client
  and `bri-server` now load through `packages.json` (the 17 positional server
  arguments are gone), and a refused join names every differing shared
  package; client-only differences are told to the player instead. The old
  fingerprint chain in `content_identity.rs` is removed.
- **One id grammar (contract 4):** `namespace:kind/name` defined in
  `bri_package::id`, shared with the mod platform lane, with `id::native` and
  `id::Minter` for converted names (including `v20:file/<path>`). New content
  uses it. Renaming the base packs' three legacy spellings (about 400 code
  literals, 12 importers, pack regeneration) is deferred by the coordinator to
  the vanilla-as-packages phase, together with multi-pack loading; the survey
  and plan are in `docs/architecture/packages.md`.
- **Owner identity (contract 3, a):** fixed. `World.owners` maps each owner
  number to the builder's principal and last name; a returning principal gets
  its number back on join, saved builds carry the table instead of an opaque
  session scope, and loads give recorded builders their bricks on any server.
  Trust now also covers offline builders in the table.
- **Partial load (contract 3):** fixed. A world or build brick whose
  definition this server lacks no longer refuses the whole load: it moves to
  `World.unloaded`, kept exactly and saved again (and offered again when a
  build is loaded elsewhere), and the load reports "N bricks were not loaded
  ..." by definition in chat, at map load and in `bri-server`'s log.
- **Avatar part names (contract 3):** fixed. `Appearance.parts` (saved,
  sent and replicated) and the settings file's avatar prefs name the chosen
  part (`hat: "helmet"`, `accent: "visor"`). The avatar pack keeps its
  defaults as v20-style positions in its own lists and names them on load;
  v20 prefs are named once when imported. A saved part the pack no longer
  has falls back to the pack default instead of failing the avatar.

## How to read the priorities

Scope rule from Maxwell: during alpha there is no backward compatibility and no
migration code. Alpha saves may break. So the priorities below are about getting
the *shape* right before beta freezes formats, not about carrying old files.

- **P0**: cheap now, expensive once beta formats and mods exist. Fix before
  vanilla is declared done.
- **P1**: fix while stabilising vanilla, when the system is next touched (the
  10 to 20 percent tax).
- **P2**: needs the rule of two or three first; do it in or after the modding
  spikes.
- **Later**: modding product; written down, not built.

"Side" says where the hardcoding sits relative to the boundary (engine owns
mechanisms, packages own policy). Policy baked into the **bones** (net, render,
physics, persistence machinery) is the worst kind; policy hardcoded in the
**game** layer is normal for now and just needs to stay localised.

## Headline

The foundations are better than a typical game at this stage. There is one
authoritative command path, server-built actors, typed and budgeted events,
string content ids, a fixed 120 Hz tick with seeded randomness, data-driven
weapon state machines, converted UI layouts as data, and no code ever sent to
clients. The door-closers are concentrated in five places:

1. **Brick ownership is a per-session number, not a player identity.** After a
   server restart, returning players no longer own their builds.
2. **Content agreement is one opaque hash over a fixed list of 19 packs.**
   There is no package manifest, so a mismatch cannot be explained, a mod
   cannot be added without a Rust change, and clients cannot fetch what they
   lack.
3. **The wire protocol is the simulation's Rust structs**, with each gameplay
   concept (weapons, tools, vitals, minigames, vehicles) a hand-added field.
   New kinds of replicated state need protocol and server-loop edits. This is
   gameplay baked into a bone.
4. **Vanilla behaviour is selected by matching datablock names** (`name ==
   "skiweaponimage"`, `name.contains("dodgeball")`), and add-on specifics are
   engine enum variants (`HorseTransform`, `FootballCatch`, `StartSkis`).
5. **Loading refuses the whole build if one brick's definition is missing**,
   and several saved formats store indices (avatar parts) rather than names.

## Graded table: Maxwell's twelve contracts

| # | Contract | Grade | Worst door-closer | Priority |
|---|---|---|---|---|
| 1 | Authority model | Solid core, partial permissions | Admin is a boolean; roles are a 3-value enum, not capabilities | P1 |
| 2 | Stable semantic game model | Partial | `Session` is one object owning every concept; damage has three separate paths | P2 |
| 3 | Persistence and migration | Partial | Whole load fails on one missing definition; owners are session numbers; avatar parts are indices | P0 |
| 4 | Stable content identity | Partial | Two id grammars; the importer puts every add-on in the `v20/` namespace | P0 |
| 5 | Content/package model | Missing | Fixed 19-slot `ContentConfig`; no manifest, versions or dependencies | P0 (shape) |
| 6 | Mutation boundaries | Partial | Event intents are semantic ops; weapons, tools and bots mutate directly | P1 |
| 7 | Events/hooks | Solid | Budgets, origins, atomic admission exist; projectile outputs missing | P1 |
| 8 | Lifecycle | Partial | `Session::step` is a hand-ordered monolith; Tutorial is a mode compiled into it | P2 |
| 9 | Budgets and containment | Partial | Strong in events and net; weapons and bots have ad hoc caps; bots use player slots | P1 |
| 10 | Observability | Partial | Rejections are mostly strings; content mismatch says only "does not match" | P0 (mismatch), P1 |
| 11 | Vanilla not special | Partial | Name-matching weapon behaviour; add-on variants in engine enums | P1 |
| 12 | Network content agreement | Missing | Exact hash equality, no protocol/gameplay/cosmetic split, no download | P0 (manifest), Later (download) |

And the coordinator's additions:

| | Addition | Grade | Note | Priority |
|---|---|---|---|---|
| a | Humans, bots, agents and mods as attributed principals on one command path | Partial | Humans use `Command`; bots call `movement`/`weapon_trigger` directly; bricks are owned by session numbers | P0 (owner identity), P1 |
| b | Fixed tick and record/replay | Done 2026-10-07 | Fixed tick, seeded RNG, match recording and headless replay (`bri_net::replay`, `bri-replay`); the wall clock and other outside reads go through the recording. Same build and machine only | P1 |
| c | Data to clients; code only through trust tiers | Changed 2026-09-28 | Sandboxed WebAssembly and WGSL after a per-server trust prompt, elevated capabilities per Add-On, native plugins designed into tier 3 but not built (`docs/architecture/client-sandbox.md`) | Keep the tiers; no silent escalation |
| d | One schema drives saves, packages, replication, cvars | Missing | `Snapshot`, `Checkpoint` and `Delta` hand-maintain the same concepts | P2 |

## Contract by contract

### 1. Authority model: solid core, partial permissions

What is right:
- One typed command enum, `Command` (`crates/sim/src/session.rs:108`), dispatched
  by `command_with_aim_and_admin_persistence` (`session.rs:914`) with sequence
  checks and per-peer rate limits.
- The acting context is built by the server, never decoded from a packet
  (`crates/world/src/authority.rs:28`, and `join_verified` in `session.rs`).
- Durable admin identity is a public-key `Principal` (`crates/admin/src/lib.rs:22`).

Door-closers:
- Permission is `peer.actor.administrator`, a boolean checked inline
  (`session.rs:692`, `974`, `1068`). `Role` is `Player | Admin | SuperAdmin`
  (`admin/src/lib.rs:26`). A mod that may "spawn entities but not clear bricks"
  cannot be expressed. **Cheap fix now (P1):** name the capabilities the checks
  already imply (`build.load`, `admin.teleport`, `world.clear`, ...) and have
  `Role` map to a capability set. The call sites barely change.
- Bots do not go through `Command`. They call `self.movement` and
  `self.weapon_trigger` directly (`crates/sim/src/session/bots.rs:267`, `279`).
  Harmless for vanilla, but an agent or mod actor must use the same path as a
  player. **P1**, when bots are next touched.

### 2. Stable semantic game model: partial

- `Session` (`session.rs:375`) holds every system directly: events, specials,
  tutorial, weapons, vehicles, minigames, bots, trust, undo, loading. Concepts
  exist, but as private fields of one struct.
- Damage has separate entry points per target (`damage_player`,
  `damage_vehicle`, `blow_up_bricks` in `crates/sim/src/session/weapons.rs:386`
  onwards). There is no single "damage operation" a mod could call or hook.
- **P2**: the rule of two or three applies. Extract "damage", "spawn entity" and
  "controllable entity" when the modding spikes show the real shape. The cheap
  part now is to route new code through one function per concept rather than
  adding a fourth path.

### 3. Persistence: partial

Right: worlds store string content refs (`ContentRef`, `crates/world/src/model.rs:17`),
the palette travels with the world as colour values (`model.rs:286`), loading
merges colours by value (`crates/world/src/build.rs:189` onwards), every format
has a `schema_version`, and v20 `.bls` import keeps unresolved bricks, prints and
items as named references instead of dropping them
(`crates/bls/src/bls.rs`).

Door-closers (graded under the alpha rule: shape, not migration):

- **One missing definition refuses the whole build (P0).**
  `preflight_load` calls `definitions.get(brick)?` for every brick
  (`crates/sim/src/simulation.rs:159-163`), which fails on an unresolved or
  unknown id (`crates/sim/src/definitions.rs:105-111`). A v20 save with one
  add-on brick, or a world after a mod is removed, will not load at all. Fix:
  load what resolves, keep the rest as preserved placeholder records in the
  world, report "N bricks of `pkg:brick/x` skipped". This is also the future
  "removing a mod fails predictably" behaviour.
- **Brick owners are session counters (P0).** A joining player gets
  `self.next_owner` (`session.rs:591`). Saved builds keep owner numbers plus an
  opaque `ownership_scope`, and on load other-scope numbers are remapped to
  fresh, unclaimed owners (`build.rs:204-230`). A returning player is a new
  number, so their builds are no longer theirs after a restart, and trust (which
  *is* keyed by principal, `crates/client/src/trust_list.rs`) cannot apply to
  them. Fix: the world stores an owner table (owner number → principal and last
  known name); joining maps the principal to its existing number.
- **Avatar parts are stored as indices (P0).** `Appearance.parts:
  BTreeMap<String, usize>` (`crates/content/src/avatar.rs:30`) and the settings
  file's `AvatarPrefs` store "part indices ... as integers into the pack's part
  lists" (`crates/ui/src/api.rs:270-272`). Adding or reordering a part changes
  everyone's avatar. Fix: store part names; indices stay a UI detail.
- **Effect codes are v20 numbers (P1).** `color_effect <= 6 && shape_effect <= 2`
  (`model.rs:203`). Name them (`pearl`, `undulo`, ...) before beta.
- **Unknown data fails closed (Later, at beta).** Every saved struct uses
  `deny_unknown_fields` (`model.rs:37` and others). That is better than silently
  dropping data, but it means mod-added fields make a world unreadable. At beta,
  add a namespaced `extensions` map that round-trips untouched.
- **Strict schema equality everywhere (Later, at beta).** `WORLD_SCHEMA` is
  compared with `==` (`model.rs:312`), as are settings (`settings.rs:63`), admin
  state (`admin/src/lib.rs:525`) and minigame presets. That is fine for alpha;
  at beta each format needs a migration chain and frozen fixture files.
- **The trust list silently empties (P1).** A damaged or unreadable file loads
  as an empty list (`trust_list.rs:54-60`), and the next save overwrites it. Keep
  the damaged file aside and report instead.
- **Weapon runtime saves pin a pack id (P1, latent).**
  `save.pack_id == pack.id` (`crates/weapons/src/runtime/persistence.rs:29`).
  Unused at runtime today; do not wire it in as-is.
- The 256-colour cap refuses a whole merge (`build.rs:197`). **P2.**
- There is no autosave. When one is added, it uses the world format above.

### 4. Stable content identity: partial

Right: runtime content is named by strings, not indices, and the renderer and
physics use their own handles.

Door-closers:
- **Two id grammars (P0).** Bricks, prints, sounds and maps use
  `v20/brick/brick1x1data`; weapons, vehicles and projectiles use
  `v20.weapon.gunitem` (`session.rs:1034`, and throughout `crates/sim/src`).
  Pick one grammar now, for example `package:kind/name`, while only vanilla
  exists. Renaming later breaks every saved world and mod.
- **The importer stamps every brick into `v20/` (P0).**
  `id: format!("v20/brick/{key}")` (`crates/convert/src/catalog.rs:296`), where
  `key` is the lowercase datablock name. Two add-ons that both declare
  `brick2x2Data` collide, and third-party content masquerades as vanilla. Fix:
  the namespace comes from the source package (`v20` for the base game, the
  add-on's name otherwise).
- Ids are derived from Torque datablock names, not UI names. That is stable and
  fine, but `.bls` files reference UI names, so the alias table (UI name → id)
  is part of the package, not a runtime lookup. It already is for items
  (`crates/net/src/content_identity.rs` `resolve_world_items`).
- A few rules match ids by suffix: `id.ends_with("8x rapids.blb")`
  (`definitions.rs:92`). **P1**: turn into declared properties on the
  definition.

### 5. Content/package model: missing

- `ContentConfig` is a struct with 19 named fields, each a pack directory with a
  sequence number (`crates/client/src/content.rs:49-92`, e.g.
  `map_bundle: "map-bundle-016"`). Adding any new kind of content, or any mod,
  means a new Rust field.
- The dedicated server takes 17 positional directory arguments
  (`crates/net/src/bin/bri-server.rs:53`).
- Each subsystem computes its own fingerprint and they are chained into one hash
  (`content_identity.rs` `with_weapons`, `with_audio`, `with_vehicles` and so
  on).

**P0, shape only:** define a manifest now: a list of packages, each with id,
version, content hash, role (server, client, shared) and the kinds of content it
provides. Vanilla becomes one package (or a few) described by that manifest,
and `ContentConfig` and the server arguments read it. No dependency resolver,
downloader or registry yet. This is the single highest-leverage change, because
contracts 10 and 12 and every scenario below depend on it.

### 6. Mutation boundaries: partial

- The event system already has semantic operations: `Intent::{Brick, Player,
  Client, MiniGame, Projectile}` (`crates/events/src/model.rs:285`), applied in
  one place (`crates/sim/src/session/events.rs:1038`). This is the seed of the
  mod API.
- Weapons, tools, vehicles and bots mutate session state directly through their
  own adapters (`weapons.rs:268-489` matches about 30 weapon event variants).
- **P1**: when touching a system, route its world changes through the same
  operation set the events use. Do not build a generic operation bus yet.

### 7. Events and hooks: solid

- Explicit `Limits` for rows, pending jobs, fanout, steps per phase and origin,
  expansions and state bytes (`crates/events/src/runtime.rs:7-35`); admission is
  atomic; jobs carry an origin; delays are in ticks.
- Gaps: projectile outputs return "not available yet" (`events.rs:1044`);
  outputs are a closed Rust set. That is correct for vanilla. **P1** for
  projectile outputs; the open set is **Later**.

### 8. Lifecycle: partial

- `Session::step` (`session.rs:1306`) runs talking, bots, input, touches,
  triggers, vehicles, weapons and more in one hand-written order.
- The Tutorial is a mode compiled into the session
  (`tutorial: Option<Box<tutorial::Tutorial>>`, `session.rs:380`, 1,414 lines in
  `session/tutorial.rs`), installed only on the Tutorial map.
- **P2**: name the phases (input, simulate, weapons, events, replicate) in
  comments and tests first; extract a game-mode seam after Tutorial plus one
  minigame mode show what it needs. "Engine foundations hardening" is already
  reworking join and map change; its phases should be the named ones.

### 9. Budgets and containment: partial

- Strong: event limits above; the codec bounds every frame and request
  (`crates/net/src/codec.rs`); command rate limits; the brick cap
  (`MAX_BRICKS = 1_000_000`, `model.rs:6`).
- Weak: weapons count dropped work as "gaps" rather than enforcing per-origin
  budgets; the server loop computes hostile pairs for every player pair every
  tick (`weapons.rs:206-216`); bots join as full player peers and count
  against the 64-player cap (`session.rs:586`, `bots.rs:92`).
- **P1**: per-origin budget accounting for projectiles and entities, mirroring
  the event limits.

### 10. Observability: partial

- `Rejection` carries a typed `PlantFailure` plus a message
  (`session.rs:290`); everything else is a formatted string.
- A content mismatch is reported as "Required content does not match"
  (`crates/net/src/server.rs:642`), with no indication of which pack differs.
- Weapon adapter gaps are counted and exposed (`weapon_adapter_gaps`), which is
  a good pattern.
- **P0**: once the manifest exists, the join rejection lists the differing
  packages. **P1**: typed rejection codes for commands, and an attributed change
  log (who changed which brick, from which command or event origin).

### 11. Vanilla should stop being special: partial

- Weapon state machines, projectiles, explosions and damage types are data
  (`crates/weapons/src/lib.rs:104-216`). That part is good.
- But the `onFire` callback picks behaviour by image name
  (`crates/weapons/src/runtime.rs:1064-1190`): `name.contains("spear")`,
  `name == "skiweaponimage"`, `name.contains("keyimage")`,
  `name == "basketballimage"`, `name.contains("dodgeball")`, and the
  `HOST_TOOL_IMAGES` list (`runtime.rs:22`).
- The runtime's event enum carries add-on specifics: `HorseTransform`,
  `StartSkis`, `FootballCatch`, `Touchdown` (`runtime.rs:185-320`).
- Special bricks, emotes and the alarm projectile are id literals in the
  session (`session.rs:1027-1034`; about 46 distinct `v20` literals in
  `crates/sim/src`).
- Vehicle `Family` is a closed enum of ten (`crates/vehicles/src/schema.rs:34`),
  which is reasonable until the rule of two or three says otherwise.
- **P1**: see the pilot below.

### 12. Network content agreement: missing

- The join check is `hello.version == VERSION` and exact `content_id` equality
  (`server.rs:642`). There is no separation of protocol compatibility, gameplay
  content and cosmetic or local content, and a client cannot fetch anything.
- **P0**: the manifest (contract 5) plus a precise mismatch report. **Later**:
  content-addressed download of data-only packages, verified by hash.

### Additions

- **(a) Principals.** Covered under 1 and 3. The owner-identity fix is the P0
  part.
- **(b) Time and replay.** Fixed 120 Hz ticks (`TICKS_PER_SECOND`,
  `model.rs:10`), `BTreeMap` iteration throughout, seeded `spawn_seed`
  (`session.rs:385`) and Rapier's `enhanced-determinism`
  (`crates/physics/Cargo.toml:12`). Gaps: admin commands read the wall clock
  (`session.rs:940`), and time scale changes the number of steps per frame
  (`server.rs:702`). There is no command recording. **P1**: record the typed
  command and movement stream per tick; that alone enables replay debugging and
  headless agent tests.
- **(c) Data to clients.** No client-side scripting exists and all presentation
  is data (cues, UI packs, effect packs). Keep it.
- **(d) One schema per concept.** `Snapshot` (`session.rs:277`), `Checkpoint`
  (`crates/net/src/protocol.rs:166`) and `Delta` (`protocol.rs:229`) list the
  same concepts by hand, and the server loop diffs each one separately
  (`server.rs:723-727`). **P2**: when a new replicated concept is added, add it
  as a generic keyed component rather than another top-level field.

## Gameplay baked into the bones

Sorted by the boundary Maxwell set. These are the priority problems even where
the fix is later.

| Bone | What leaks in | Evidence | Priority |
|---|---|---|---|
| Net protocol | Weapons, tools, vitals, minigames, vehicles, avatars as fixed fields; a 16-variant gameplay `CueKind` | `protocol.rs:166-245`, `crates/sim/src/presentation.rs:9-95` | P2 (no new top-level fields: P1) |
| Net join | One opaque content hash over fixed packs | `server.rs:642`, `content.rs:49-92` | P0 |
| Persistence | Owner numbers instead of principals; v20 effect codes | `build.rs:204-230`, `model.rs:203` | P0, P1 |
| Server binary | 17 positional pack arguments | `bri-server.rs:53` | P0 (reads manifest) |
| Render, physics | Clean. Bricks are a render primitive; no weapon, vehicle or mode logic found | `crates/render/src`, `crates/physics/src` | Keep |

## Readiness test: weapon fire today

The semantic description we want: *player fires → authoritative action →
consumes state → creates projectile → projectile collides → damage operation →
effects replicated.*

What actually runs:

1. The client sends `Command::WeaponTrigger { down }` with an aim
   (`protocol.rs` `Request`). Authoritative, good.
2. `command_with_aim...` checks the mount: riding a non-ski vehicle routes to
   `vehicles.set_fire`; otherwise `weapon_trigger` queues the trigger
   (`weapons.rs:127-158`).
3. `step_weapons` (`weapons.rs:160`) sets each actor's frame (the muzzle is the
   eye as a placeholder), precomputes which player pairs may damage each other
   under minigame rules, and builds query closures.
4. The weapon runtime advances the image state machine loaded from
   `weapons.json`. Data-driven, good. At the `onFire` script state it matches
   the image's **name** to decide what firing means: host tool, skis, key,
   basketball, dodgeball, or else spawn the projectile from its data
   (`runtime.rs:1064-1190`).
5. Collisions emit typed events: `Damage`, `Impulse`, `Contact`, `BrickImpact`,
   `Effect`, and add-on specifics such as `HorseTransform`.
6. The session adapter matches about 30 variants (`weapons.rs:268-489`):
   `Damage` to a player calls `damage_player`, to a vehicle `damage_vehicle`;
   `BrickImpact` calls `blow_up_bricks`; `Effect` becomes a `CueKind::WeaponEffect`.
7. Cues ride in `Delta.cues`; clients match on `CueKind` for audio and effects.

Verdict: closer to the semantic description than the worst case. The state
machine, projectile, damage and effect data are real data, and the path is
authoritative. What fails the test is step 4 (behaviour chosen by name), step 5
(add-on concepts as engine variants) and step 6 (three damage paths, no single
damage operation).

### Smallest pilot

**Declared weapon behaviours**, about one to two days, when the weapons runtime
is next touched:

1. Add `behavior: Option<String>` to `Image` in the weapons pack. The importer
   fills it from a small table (v20 image name → behaviour id: `projectile`,
   `host_tool`, `ski_toggle`, `key`, `ball_shoot`, `dodgeball`).
2. `callback` dispatches on the declared behaviour through a
   `BTreeMap<&str, fn(...)>` instead of `name.contains(...)`.
3. One `apply_damage(target, amount, kind, source)` routes to player, vehicle
   or brick, and the weapon adapter calls only that.

Success test: a new weapon added to a pack as data only, with an existing
behaviour, loads and fires in a headless test with no Rust change. That is also
spike (a)'s first step.

## Maxwell's scenarios

### Creeper mod (a new spawnable, AI-driven, exploding actor)

Needs: a model importer for a modern format, an actor kind that is not a
player, AI behaviour defined by a package and run sandboxed on the server, a
fuse effect, and an explosion operation.

Blocked by today's code:
- Every AI actor is a full player peer: bots join through `join_inner`
  (`bots.rs:92`) and count against the 64-peer cap (`session.rs:586`). Bot AI
  is a Rust `Brain` (`bots.rs:22`).
- New replicated actor state needs new protocol fields (contract 12 and d).
- There is no "spawn explosion at X" operation outside a projectile's data.
- Not a blocker: the runtime shape format is native and importer-neutral
  (`crates/content/src/shape.rs`); only the DTS importer exists
  (`crates/convert/src/shape.rs`), so a glTF importer is purely additive.

Depends on contracts 2, 5, 6, 9, 12.

### Minecraft world (procedural, mineable voxel terrain)

Needs: a world provider supplied by a package, a voxel primitive or bricks that
can act as one, chunk streaming, and mining and inventory hooks.

Blocked by today's code:
- A map is a scene entry in one aggregated converted bundle, looked up by id
  (`crates/sim/src/map.rs:80-86`); there is no generation hook.
- The whole world is sent at join (`Checkpoint.world.bricks`,
  `protocol.rs:166-185`); "Engine foundations hardening" is rewriting this.
- `MAX_BRICKS` is one million (`model.rs:6`), a 256 × 256 × 64 voxel world alone
  is four million.
- Mining is "remove brick plus give item"; the inventory holds tools, not
  materials.

Depends on contracts 5, 6, 8, 9. The streaming rewrite is the moment to keep
the world representation chunk-shaped.

### Minecraft GUI mod (replace the whole Blockland GUI)

Needs: data-driven, skinnable UI; the server pushes UI packages as data.

Today:
- Layouts are already data: converted `.gui` control trees, skins and bitmap
  fonts (`crates/ui/src/lib.rs` layers `schema`, `pack`, `view`). A reskin that
  keeps control names is plausible.
- Behaviour is bound in Rust per GUI name (23 `...Gui` names across
  `crates/ui/src/screens`), HUD logic is Rust (`crates/ui/src/models/hud.rs`),
  and the UI pack is one fixed `ContentConfig` slot.
- Clients cannot receive a UI package from a server.

Depends on contracts 5, 12 and the slot idea (principle 5). **P2**: when a screen
is touched, keep behaviour bound to named slots and actions, not to layout
details.

### Asset provenance

Agents should create original assets or use ones the player may use; the
package manifest records source and license; server owners are responsible for
what they redistribute. This costs nothing now beyond reserving the manifest
fields.

## Legacy Add-On import

Today's importers are an "old Blockland → native" converter, but a vanilla-only
one:

- `bri-convert` takes a whole install and walks exactly `base` and `Add-Ons`
  (`crates/convert/src/main.rs:171`), not one add-on zip or folder.
- Every converted brick lands in `v20/` (`catalog.rs:296`).
- The regeneration script hardcodes which brick add-ons join the catalog
  (`BRICK_ADDONS`) and accepts exactly two known geometry failures
  (`KNOWN_GEOMETRY_FAILURES`), both in `tools/regenerate_content.py:36-42`
  (commit 3ba01e6).
- The colorset is read out of the server script into the UI pack
  (`crates/ui-import/src/data.rs:48`, used at `content.rs:923`), not a content
  type of its own, so a v20 `colorSet.txt` add-on has nowhere to go.
- Each importer is its own binary with fixed positional arguments.
- Good and general already: known-source repairs keyed by file hash
  (`crates/convert/src/brick.rs:264`), `.bls` import that preserves unresolved
  references, and deterministic outputs.

What a general `import <Add-On.zip or folder>` looks like (Later; the P1 part is
only to keep the split below while touching importers):

```text
Torque/v20 readers (.blb .dts .dsq .dif .ter .cs datablocks, prints, colorsets)
        ↓
native semantic representation (namespaced ids, provenance, hashes)
        ↓
native package + manifest + machine-readable report:
  converted | converted with warnings | needs behaviour (TorqueScript X → native primitive?)
```

Agent-friendly means: one CLI, JSON output, precise diagnostics, a headless
"load package and run scenario" check, documented native schemas, and vanilla
shipped as the reference package.

Spike (c) built this as `bri-import-addon` (`crates/addon-import`) and ran it
over three community Add-Ons and a bulk archive; the seams it found are in
[`spike-addon-import.md`](spike-addon-import.md).

## Agent experience today

| Workflow step | Exists today | Blocker | Cheap now |
|---|---|---|---|
| Scaffold | No | No package format | Manifest shape (P0) |
| Validate | Partly: `bri-client --check`, probes, validators per format | Checks are per subsystem, not per package | Make `--check` emit JSON (P1) |
| Test headless | Yes: `bri-server`, `building_probe`, `network_probe`, `perf_probe` | 17 positional args; no scenario runner | Server reads the manifest (P0) |
| Enable on a server | No | Fixed `ContentConfig` | Manifest (P0) |
| Clients auto-download and join | No | Exact hash match, no download | Precise mismatch report (P0); download is Later |
| Modding guide and recipes | No | Nothing to document yet | Keep `platform-principles.md` current |
| Safety | Good base: no client code, bounded codec, verified identities | No sandbox (none needed yet) | Keep data-only clients |

The console registry in progress (`crates/console/src/registry.rs` in the
`console` worktree, uncommitted) is typed, lists commands with usage and help,
and forwards commands owned by another layer. It is a good introspection
surface. **P1** as it lands: every command that changes shared state should map
to an authoritative `Command` rather than a client side effect, names should be
namespaced, and each command should declare the capability it needs.

## Sequencing

1. **P0, before vanilla is declared done** (all small, all about shape):
   owner identity in worlds; avatar part names; one id grammar with the
   package namespace set by the importer; a package manifest read by the client
   and `bri-server`, with a precise join mismatch report; partial load with a
   report instead of refusing a whole build.
2. **P1, while stabilising vanilla**, each when its system is next touched:
   the declared-behaviour pilot and one damage operation; capabilities behind
   roles; bots through the command path; command recording; per-origin budgets
   for projectiles and entities; typed rejection codes; named effect codes; the
   trust-list recovery fix; console commands mapped to `Command`.
3. **P2, in or after the spikes**: generic replicated components; lifecycle
   phases and a game-mode seam; UI slots; world providers.
4. **Later**: everything in the principles doc's "written-down goals".

Nothing in P0 or P1 blocks the alpha, and none of it needs migration code.
