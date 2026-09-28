# Platform principles

Status: proposed 2026-09-27, awaiting Maxwell's approval. These are the
constraints we keep while building the game so that the modding platform stays
possible later. The alpha and the first building playtest come first; nothing
here blocks them.

The evidence for each principle, what the code does today, and what to fix first
are in [`docs/audits/platform-door-closers.md`](../audits/platform-door-closers.md).

**Mod-ready foundations now, mod platform later.** The engine may specialise for
today's game, but it must not unnecessarily make today's game the only game its
foundations can support.

## North star

**Make the imagination the limit, not our engineering.**

A player tells their agent "I want X". The agent builds it. The player enables it
on their server, friends join, their clients download what they need, and
everyone is playing. There is no folder management, no load-order archaeology, no
compiler and no list of frameworks to install first. Modding should be easier
and more fun than old Blockland, and reach further than Torque ever allowed:
new kinds of actors, worlds, interactions, UI and game modes, not just data
tweaks to fixed engine classes.

**Vanilla is the proof that the platform is expressive enough, not a privileged
exception to it.** Vanilla does not have to ship as a mod. But if a mod cannot do
something because vanilla uses a secret internal path, that is a defect.

## The boundary

**The engine owns mechanisms; packages own policy.**

The bones are ours and are not moddable: rendering, networking transport and
replication machinery, physics, IO, threading, asset loading, persistence
machinery, scheduling, sandboxing and security. The bones expose stable
primitives that packages compose, so a package never needs to reach inside them.

Everything above the bones is policy, and policy is moddable: what a world or map
is, which entities exist, how players move and interact, what an item or weapon
does, what the HUD and menus look like, what a game mode is, what gets spawned or
mined, and what winning means.

Gameplay, UI or world concepts baked into the bones are the most expensive
mistakes, because every mod would have to work around them.

## Principles

1. **Never strand a world (from the first beta).** Anything a player or server
   owner creates (worlds, builds, autosaves, prefs, trust and admin lists,
   avatars, presets, mod state) stores semantic identity and intent, never
   implementation details. Every format carries an explicit version and, from
   beta on, migrates forward so old files always load. Missing or renamed
   content degrades and is reported clearly, and the rest of the world still
   loads. Unknown data is preserved, never silently dropped. Server owners
   decide what loads on their server. Imported v20 saves (`.bls`) are always
   supported.

   **During alpha there is no backward compatibility and no migration code.**
   Alpha designs are disposable; full rewrites and breaking format changes are
   allowed, and alpha saves may simply stop loading. We still keep a schema
   version on every format and get the semantic identity right, so beta can
   start migrating from a sound base.

2. **Durable, namespaced identity.** Content is named by stable ids such as
   `v20:brick/1x1`, owned by the package that declares it. Saves, packages and
   the wire never depend on file paths, display names, array or palette indices,
   Rust enum order, pack directory numbers or transient handles. Players are
   identified by their durable principal (public key), not a per-session number.

3. **One authoritative command path.** The server owns shared truth. Humans,
   bots, agents and mods are all principals with attribution, submit the same
   typed commands, and are checked against the same capabilities. There is no
   agent backdoor. Permissions are named capabilities, not a single admin flag.

4. **Semantic operations are the mutation boundary.** Systems change the world
   through a small set of operations (spawn entity, damage, give item,
   plant or remove brick, set property, start game mode, emit effect, schedule
   action), not by reaching into each other's state. This set becomes the heart
   of the mod API. The event system's typed intents are the seed.

5. **Composition over override.** UI, world generation, damage rules and similar
   policy expose named slots and hooks. A package adds to a slot or declares an
   exclusive replacement, so conflicts come from declarations and are reported,
   rather than emerging from load order.

6. **A defined lifecycle.** Server start, package load, world create and load,
   player join and leave, tick phases, map change, save and shutdown are named,
   ordered phases that packages can later participate in.

7. **Fixed time, reproducible runs.** Simulation runs on a fixed tick and
   schedules in ticks, randomness comes from seeded streams, and the
   authoritative command stream can be recorded and replayed for debugging and
   agent testing.

8. **Budgets and failure containment.** Every origin of work (player, package,
   behaviour) has budgets for CPU, entities, events, projectiles, network and
   storage. Pathological work is throttled, rejected or quarantined; the server
   degrades rather than falls over.

9. **Observable and explainable.** Rejections carry typed, precise reasons.
   Changes and events are attributed to a principal and package. The engine can
   be asked what exists (entity kinds, events, UI slots, packages, owners) and
   why something happened.

10. **Trust tiers: safe by default, further with trust.** (Maxwell,
    2026-09-28: allow as much potential as possible, keep people reasonably
    safe by default, and let them go beyond safety for more capability if
    they trust the other person.) Servers send joining players:
    - **data** (assets, declarative UI, effect and content definitions)
      without asking;
    - **sandboxed code** (WebAssembly and WGSL shaders that use only declared
      sandbox capabilities, under CPU, memory and GPU budgets) after the
      player trusts the server once, asked again when the code changes;
    - **elevated code** (capabilities beyond the sandbox: URLs, a folder of
      its own, even native plugins) only after a separate, stronger
      per-Add-On choice that spells out the risk and can be revoked; a
      native plugin also needs the server's name typed.

    Nothing escalates silently: a grant covers exactly the code and
    capabilities the player saw. Native plugins are designed into tier 3
    but not built yet. Client code is presentation (and prediction the server
    corrects); gameplay truth stays on the server. Server behaviour from
    packages runs sandboxed with capabilities and budgets. Packages are
    hash-verified. Design: [client-sandbox.md](client-sandbox.md).

11. **The server environment is explicit.** Game build + platform API level +
    packages + versions + hashes + server configuration = the server
    environment, published as a manifest (a lockfile). Protocol compatibility,
    gameplay content agreement and cosmetic or local differences are separate
    checks. Clients fetch missing data packages from the server,
    content-addressed and hash-verified.

12. **A versioned platform API.** Packages declare the platform API level they
    need (`api >= N`), independent of the game build and the wire protocol, so
    internals can be rewritten without breaking mods.

13. **One schema per concept.** A concept's typed definition drives its save
    format, package format, replication and console/cvar exposure, instead of
    being hand-maintained in several places.

14. **Agent experience is a product requirement.** One documented workflow
    (scaffold, validate, test headless, enable on a server, clients
    auto-download) is available as CLI commands with machine-readable output
    and precise errors. The game ships a modding guide with rules, templates and
    working recipes. A fixed set of "I want X" prompts is run by agents against
    the platform regularly, and success rate and time-to-playable are the
    platform's headline metric.

15. **Provenance and licensing travel with content.** Package manifests record
    authorship, source and license. Agents create original assets or use ones
    the player is allowed to use; server owners are responsible for what they
    redistribute.

Principles 1 to 4, 7 to 9 and 11 are things we enforce in code now; 10's
client sandbox has a working prototype (`crates/client-sandbox`). The rest
(5, 6, 10, 12 to 15) are shapes we preserve now so they can be built later; see
the scope table below.

Some "Do later" items in that table now exist as prototypes: Rhai server
scripts and chunked world providers (`package-runtime.md`), dependency
ordering when an Add-On is turned on (`mod-manager.md`) and fetching a
server's missing Add-Ons on join (`packages.md`).

## Scope: what we build now

Build a great Blockland implementation that is hard to accidentally make
unmoddable. Do not build the general-purpose platform yet.

| Do now | Preserve the seam | Do later |
|---|---|---|
| Server authority | Gameplay separate from engine machinery | Public scripting runtime (Luau, Wasm or other) |
| Stable authored, namespaced IDs | A world/map abstraction, not `if map == "Slate"` | Generic world-provider API |
| A schema version on every save (no migrations during alpha; they start at beta) | Behaviours keyed by capability, not by vanilla IDs or names | Arbitrary behaviour components |
| Content hashing | Data-driven vanilla definitions | Mod SDK and public API design |
| Headless validation and testing | Package-shaped content boundaries | Dependency resolver and conflict detection |
| Precise, typed diagnostics | UI logic separate from the renderer | Fully replaceable mod UI |
| Resource limits | Client/server responsibility boundaries | Automatic package download and content-addressed cache |
| Correct vanilla systems | An importer separable from v20 assumptions | Registry, workshop, marketplace |
| Semantic operations instead of ad hoc state mutation | Controllable entities not limited to player-shaped Blockheads | AI mod-creator UX, live AI runtime |

**The moddability tax is about 10 to 20 percent.** When implementing a vanilla
feature, spending that much extra to keep the boundary clean is worth it. For
example, a checkpoint *behaviour* that the stock checkpoint brick uses, rather
than checkpoint logic spread through unrelated systems. Spending weeks on a
generic behaviour runtime, registry, loader, capability system and Wasm host
because a future mod might need it is not.

**Rule of two or three.** Do not generalise from one hypothetical. Build the
concrete vanilla things, then extract the real common concepts once two or three
exist: gun, bow, rocket launcher, spear and sword teach what a weapon is;
Blockhead, bot, horse and vehicle occupant teach what a controllable entity is;
Bedroom, Slate, Slopes and Tutorial teach what a world is.

**Torque is an input format, not the architecture.** Conversion should
increasingly produce native semantic content: Torque/v20 understanding, then an
intermediate native representation, then the current vanilla package. Not a
script that specifically produces today's vanilla game.

### Written-down goals for later

So nobody thinks they are forgotten, and so nobody builds them yet:

- A universal package format (weapon, bot, world generator, GUI replacement,
  game mode, asset pack, colorset or overhaul) declaring identity, version,
  dependencies, permissions, client/server/shared pieces, persistent state,
  hashes, and license and provenance fields.
- Content-addressed caching, so a shared asset pack downloads once.
- Dependency resolution (`creature-framework >= 2`) and conflict detection.
- Lifecycle hooks for packages, including enable and disable.
- Namespaced, versioned mod-owned state with its own migrations.
- Failure isolation: throttle, disable or quarantine a misbehaving package.
- Structured `check`, `test`, `profile` and `explain` diagnostics.
- Reference recipes: new weapon, new bot, new world type, replace HUD, new game
  mode, new interactive object, import a legacy Add-On.
- Introspection: which entity kinds, events, UI slots and packages exist, and
  what owns a resource.
- Provenance and explain: which package changed this screen, why this NPC did
  that.
- Reversibility: enable, test, roll back, restore the world.
- Server lockfile / world manifest published to joining clients.
- Fast iteration: hot reload or very fast headless reload.
- A separate, unsupported native-extension tier outside the safety guarantees
  (native plugins: tier 3, behind the strongest per-Add-On prompt).
- A versioned platform API level, separate from the game build (`api >= N`).
- Composition over override through named slots and hooks.
- Plain-language capability consent when a server owner enables a mod ("can
  spawn entities, change damage, read chat").
- A recorded, attributed event log powering explain, provenance and replay.
- An agent eval suite of "I want X" prompts, tracking success rate and
  time-to-playable.
- Also deferred: scripting language choice, generic plugin runtime,
  generalized procedural-world API, mod browser, cross-game portable intents,
  and a perfect "everything is a component" architecture.

## Definition of done for vanilla, and when modding starts

Vanilla is done when:

1. Every major vanilla family works end to end in one of Maxwell's playtests
   with no new blocking bugs: building with hammer and wand, events, players,
   weapons, vehicles, maps, UI and console, multiplayer, and saves.
2. The gate on main and the v20 behaviour comparison are running and green.
3. The P0 items in the door-closer audit are fixed.

Then run deliberate spikes, using only intended extension surfaces and no engine
code changes:

- (a) A new creature, a new item, a small gameplay behaviour and a client asset
  package.
- (b) A simple procedural world type.
- (c) Importing one ugly community Add-On.

Wherever a spike needs engine surgery, it has found a missing seam while it is
still cheap to fix. Those seams become the requirements for mod platform v0.

## Roadmap

Each phase starts when its entry condition holds and ends when every exit
criterion is met.

**Phase 0: stop the bleeding.**
Entry: now.
Exit:
- The gate on main runs on every push and is green.
- The v20 behaviour comparison runs and its report is green or has only
  accepted, documented differences.
- The a10 playtest package is handed off.

**Phase 1: finish vanilla.**
Entry: phase 0 done.
Exit:
- Every major vanilla family works end to end in one of Maxwell's playtests
  with no new blocking bugs (the definition of done above).
- The P0 items in the door-closer audit are fixed.
- P1 items were handled as their systems were touched, or are listed as open.

**Phase 2: vanilla as packages; first beta.**
Entry: phase 1 done.
Exit:
- Vanilla content is described by package manifests in the format mods will
  use, and the client and dedicated server load it through them.
- A join mismatch names the differing packages.
- The world, build, settings, avatar, trust, admin and package schemas are
  frozen as beta version 1, with fixture files checked in.
- From here on, format changes ship with a forward migration and a test that
  loads the previous fixtures.

**Phase 3: spikes.**
Entry: first beta.
Exit:
- Spike (a), a creature, item, small behaviour and client asset package, runs
  headless and in a hosted game, or its blocking seams are written down.
- Spike (b), a generated world type, does the same.
- Spike (c), one messy community Add-On, imports with a machine-readable report
  of what converted and what needs behaviour.
- The seams found are written up as the mod platform v0 requirements.

**Phase 4: mod platform v0.**
Entry: phase 3 requirements agreed with Maxwell.
Exit:
- The server publishes its environment manifest, and joining clients download
  missing data-only packages, content-addressed and hash-verified, with no
  manual steps.
- Server-side package behaviour runs in a sandboxed runtime, chosen from spike
  evidence, with capabilities and budgets.
- `check`, `test` and `explain` commands give machine-readable, precise results.
- Agent recipes for the common asks ship in the repo, and the "I want X" eval
  suite runs and reports success rate and time-to-playable.

**Phase 5: open up.**
Entry: phase 4 done and the eval suite shows agents can ship the recipes.
Exit:
- Bulk legacy Add-On import works on a representative set of community
  add-ons, with reports.
- A package can replace the GUI through declared slots.
- A package can provide a world generator.
- Hot reload, or a headless reload fast enough for iteration, works for data
  packages.
- Players can share packages with each other.

## Readiness test

We are ready to design the public mod API when most vanilla gameplay can be
described without implementation details. For example: "player fires weapon →
authoritative action → consumes state → creates projectile → projectile
collides → damage operation → effects replicated", rather than "a Rust function
reaches into five private maps and matches on the weapon's name".

The ultimate test: could we rewrite the renderer, physics backend, networking
implementation or large parts of internal Rust without forcing every mod to be
rewritten? When the answer is mostly yes, design the modding surface.
