# Playable alpha contract — complete vanilla v20 scope

**Current handoff:** [STATUS.md](STATUS.md) records the public release and
current decisions; [v022-delivery-contract.md](v022-delivery-contract.md) records
the v0.2.2 delivery scope. This document remains the complete vanilla roadmap. The
[first-playtest contract](playtest-contract.md) records the earlier building
handoff, rather than the current release gate. Unchecked roadmap items do not
prevent a separately authorized alpha release.

Scope expanded by Maxwell on 2026-09-26. This supersedes earlier minimum slices
(one weapon, Jeep only, limited events, omitted minigames), including those still
quoted in the persistent goal description or historical research/coordination.
The goal stays active; this is an expansion, not completion of the earlier scope.

## Outcome
A packaged Windows build ready for Maxwell's thorough interactive playtest.
Preserve v20's recognizable controls, menus, building interactions, character,
maps, bricks, textures, music and sounds. Modernization should feel natural and
improve reliability, performance, portability and future modding.

The handoff must include the full verified vanilla v20 content and gameplay set:
core engine/script features plus original shipped stock add-ons, including all
default-enabled packages. Installed community add-ons and B4v21-only changes do
not become vanilla merely because they are present in the reference installation.
Confirm other shipped-but-disabled vanilla content during the inventory pass.
Examples below are minimum coverage reminders, not an exhaustive allowlist.
Maxwell's designated read-only content reference is now
`E:\Downloads\B4v21Launcher\versions\Blockland v20`. The checked map list and
source comparison are recorded in `vanilla-reference.md`; Tutorial is present.

This is an engineering handoff, not a declaration that subjective feel has
already been accepted. The goal remains active until this contract is met.

## Required experience
- [ ] Launch outside the development environment with explicit content setup.
- [ ] Recognizable main menu, settings, map selection, loading, pause and HUD.
- [ ] All verified vanilla maps/environments, including Bedroom, Kitchen, Slopes, Slate and Tutorial: geometry, textures, collision, sky/fog, water, snow and authored environment effects where present. Cover all 14 map missions in the designated reference; resolve additional provenance/availability gaps before handoff or obtain Maxwell's explicit scope exception.
- [ ] Original customizable character, animations, walking, jumping, crouching, jets and first/third-person views.
- [ ] Stock brick catalog, favorites, ghost placement, original numpad shifting/rotation, planting/canceling, paint and removal.
- [ ] Complete vanilla building workflows: tools, undo, favorites, ownership/trust, stock brick behaviors and build macros where supplied by vanilla.
- [ ] Hammer, spray can, printer and wrench property/event dialogs, including special sound and vehicle-spawn brick variants.
- [ ] Every vanilla print pack/image, correct aspect/brick compatibility, selector/letter shortcuts, multiplayer application and native save/reload.
- [ ] Every vanilla brick event input/output, including stock add-on registrations, with correct target classes, parameter types, delays, relay/named-target behavior, enable/cancel semantics and persistence. Implement dependent Player, Client, Projectile, Bot/Driver and MiniGame behavior rather than displaying inert event choices.
- [ ] Event-editor quality of life: substantially more than 100 rows per brick with practical editing; reliable ordered zero-delay chains and relays without an imposed 33 ms per-hop wait. Explicit bounded execution, loop diagnostics, cancellation and fair handling of event bursts must prevent a runaway build from monopolizing the server. Delays requested by the author remain meaningful; budgets must not silently discard work or disguise throttling as normal timing.
- [ ] Import representative BLS worlds and preserve supported state on native save/reload; retain/report unsupported original data.
- [ ] All vanilla weapons/items/projectiles: equip/use, mounts and animations, firing/charging where applicable, collision/damage/impulses, audio, effects and multiplayer behavior. Includes Gun/Akimbo, Rocket Launcher, Bow, Spear, Sword, Horse Ray, Push Broom, keys, skis and the stock special projectile types.
- [ ] All vanilla vehicles and mounts: Jeep, Flying Wheeled Jeep, Tank, Magic Carpet, Horse, Ball, Pirate Cannon, Rowboat and any other verified stock entries. Correct spawning, mounting/seats, driving/flying/steering, weapons, damage, destruction/respawn and player/world interaction as applicable.
- [ ] All vanilla player types, avatar parts, animations and emotes, including alternate jet/player configurations and their movement rules.
- [ ] All vanilla sounds/music and particle/light effects, including brick emitters, animated lights, weapon/projectile, tool, vehicle, player, death/spawn and environment effects. Preserve original resources and authored behavior; list conversion or runtime gaps explicitly.
- [ ] Create/configure/join/leave/invite/reset/end vanilla minigames. Implement stock settings, player types/loadouts, scoring, damage permissions, stock death/manual-respawn rules (unlimited lives) and associated event behavior. Verify interactions between players inside and outside a minigame. The source audit found no vanilla configurable lives limit; do not invent one to satisfy the earlier shorthand “lives/respawn.” See research/minigames/.
- [ ] LAN/direct-IP host/join, headless server, two-player building/chat, ownership enforcement and late join.
- [ ] Complete vanilla player/host workflows identified by the inventory: trust management, player list, stock chat commands, server configuration, stock Add-Ons/Music selection and host administration. Preserve familiar layout while adapting unavailable legacy online services explicitly.

- [ ] Original Admin/Super Admin menus and workflows, including verified role differences, player administration, granting/removing privileges, kick/ban/unban and server settings. Server authority must enforce every permission independently of UI visibility; test denied remote requests as well as allowed actions.

## Engineering acceptance
- [ ] Fidelity gaps explicitly reaffirmed by Maxwell are resolved before handoff: authored sky and fog; map decorations/static objects; water and snow/weather; terrain detail and streaming/LOD sufficient for normal traversal; original brick surface textures and every vanilla print; brick color/shape FX; and correct interpretation of legacy vertex-color sentinels. Diagnostic warnings, white development materials, a finite terrain patch, retained-but-undrawn scene nodes, or a working menu/client do not satisfy these requirements. Any exception requires Maxwell's explicit scope change.
- [ ] Checked vanilla inventory covering packages, maps, bricks/behaviors, player types, vehicles, weapons/items/projectiles, prints, sounds/music, effects, events, minigames and remaining user-facing workflows. Every required entry has conversion/binding, implemented behavior, integration and verification evidence; no uncategorized omissions.
- [ ] Repeatable conversion manifest including failures and manual adaptations.
- [ ] Parser/transform/placement/persistence/event tests at meaningful boundaries.
- [ ] Automated multiplayer agreement, late-join and disconnect/rejoin checks.
- [ ] Bounded load/offscreen render checks for representative maps/content.
- [ ] Measured performance on reference saves with hardware/configuration recorded.
- [ ] Event/bot scalability evidence: an eight-client scenario with independent active areas, event bursts and representative vanilla bots; record simulation/event/AI/physics/network timing and deferred-work diagnostics. Establish a measured operating envelope, including sustained and overloaded cases, rather than promise unlimited bots/events. Bot activity reduction must preserve gameplay visible to participants and server authority.
- [ ] Windows package with no compiler/development-tool requirement.
- [ ] Original Windows 10/11 target. Maxwell expanded release platforms to Windows x86-64, Apple-silicon macOS and Linux x86-64 on 2026-10-02; see STATUS.md for current platform evidence.
- [ ] Versioned build, launch instructions, reference worlds, test guide, logs and known issues.
- [ ] Once the full contract is satisfied and Maxwell's playtest package is ready,
  create a new private GitHub repository, commit the project and push it (Maxwell
  explicitly authorized this on 2026-09-26). Keep original assets, recovered
  scripts and ignored generated content out of Git; include source, conversion
  tools, documentation and reproducible local content setup. Verify remote
  visibility and the pushed commit. Do not publish early to substitute for readiness.
- [ ] Complete vanilla content is usable through normal solo and multiplayer workflows in the packaged build. Parsed files, isolated previews, disabled menu entries, placeholder behavior and preserved-but-unimplemented vanilla events do not satisfy this requirement.

## Boundaries
Do not alter the original installation or use desktop/game input automation.
Maxwell performs interactive playtests. A quick bounded screenshot/render check
is allowed. Preserve original content outside the repository. Do not silently
remove required features to complete the goal.

Excluded: modding support, a public mod API/SDK, mod distribution/loading and
modding-language/framework selection. Maxwell will decide these after the base
vanilla game is satisfactory. Maintain sensible internal boundaries without
turning speculative extensibility into alpha work. Vanilla wrench events and
the agreed event-system improvements remain normal gameplay requirements.

Since lifted: the game now ships Add-On support, the modding guide
(`docs/modding/README.md`), Add-On downloads on join and v20 Add-On import.
See [STATUS.md](STATUS.md) for the current scope.

Also excluded: structural collapse/fragmentation, arbitrary legacy-script execution,
community add-on compatibility, public identity/matchmaking services, obsolete
purchase/key/authentication/update backends, extensive mod-editor tooling, and original
network protocol compatibility. Original numeric IDs must not confer authority
on new network clients.

Reimplement vanilla brick destruction, fake-kill/respawn and damage behavior where
present; the structural-collapse exclusion does not exclude those familiar toys.
Native adaptation of stock scripted behavior is required; a general TorqueScript
VM is not. Modern host authority remains intentional, with original LAN trust
differences clearly documented. Unavailable original online services do not justify
silently removing local gameplay features. Any other exclusion needs Maxwell's
explicit approval; uncertainty remains an open requirement, not an assumed waiver.

## Evidence standard
Successful parsing does not prove render/collision correctness. Successful
decompilation does not prove behavior equivalence. Platform claims follow the
verified releases recorded in STATUS.md. Each completion entry needs a test, artifact,
log, measurement or explicit user playtest result.
