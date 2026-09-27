# Blockland v20 rewrite assessment

> Historical first-pass assessment. The user subsequently selected one-time content migration rather than ongoing unmodified TorqueScript add-on compatibility. The interpreter recommendation below is therefore superseded. See [second-pass-audit.md](second-pass-audit.md) for new verified findings, including successful decompiler output from all 16 local DSO files and the current scope recommendation.

Investigated 2026-09-26. Recommendation: build a Rust game/runtime and wgpu renderer around a measured v20 compatibility target. Preserve Blockland2's format research and focused tests. Keep Jolt behind a narrow adapter. Make original construction, movement, events, and multiplayer the acceptance criteria.

## Evidence and limits

Read the supplied v20 installation's `base`, `Add-Ons`, and `saves` content, including ZIP members. No game scripts were executed, and the installation was not changed. Configuration, keys, and personal account data were not inventoried. Created a local research clone of Blockland2 under `.research/Blockland2`.

Reviewed Blockland2 main at `d77e33112c3f5c4fad72ae601743825474d420a7`, its import/save code, architecture/material documents, tests, and branch-tip differences. The remote exposed three heads:

| Branch | Commit |
| --- | --- |
| main | d77e33112c3f5c4fad72ae601743825474d420a7 |
| experimental-failed-physics-destruction | 1a249b44e285a406221a689bbd9c55290feba6f8 |
| codex/metal-loading-pr | bf609853aa6d2ebcf6de460dbba02f6642e89fe5 |

The clone is shallow at all three tips; this was not an exhaustive historical audit. Neither game was built or played during this investigation. Compatibility conclusions below distinguish source evidence from proposed behavior. Inventory success is not proof that an asset renders, collides, or behaves correctly.

The supplied installation is a patched reference: `base/server/scripts/allGameScripts.cs` includes B4v21 compatibility globals and custom master-server/auth fixes. Blockland2's rendering documents also cite B4v21 shaders. Freeze this installation as a reproducible reference, then explicitly distinguish stock v20 behavior, launcher patches, and intentional modernization. Do not assume the old prototype's visual rules are proven v20 rules.

## What is actually in the installation

The census read 3,304 physical loose-file/ZIP-member entries. Counts include duplicate logical resources; unreadable content is excluded. There are 91 ZIPs in Add-Ons. See `v20-inventory.json` for entries/errors and `reference-fingerprints.json` for SHA-256 fingerprints of seven reference files.

| Content | Entries read | Required preservation |
| --- | ---: | --- |
| BLB bricks | 174 | Geometry, dimensions, collision, coverage, stud/side/ramp overlays, print surfaces |
| DTS shapes | 154 | Nodes, rigid/skinned geometry, materials, animation, mount points, collision details, LOD |
| DSQ animation | 61 | Node association, timing, looping, blends and channel semantics |
| DIF interiors | 76 | Geometry, UVs, collision, lightmaps, transforms, multiple instances/details |
| TER terrain | 13 | Heights, materials, holes, triangulation, repeat behavior, collision |
| MIS missions | 27 | Object hierarchy, datablock references, spawn, environment, water, precipitation |
| BLS saves | 38 | Brick state plus events, names, attachments, and unresolved metadata |
| CS scripts | 147 | Language semantics and the Blockland host API |
| GUI files | 24 | Layouts, profiles, actions, keyboard focus and interaction |
| DSO compiled scripts | 16 | Separate bytecode-compatibility investigation or behavior reimplementation |
| PNG/JPG images | 2,177 | Color space, alpha, tiling, filtering, source resolution |
| WAV/OGG audio | 123 | Cue timing, looping, spatialization and profile parameters |

Also present: DML sky/material lists, IFL animated material lists, GFT font caches, ML files, help documents, editor/source files, and other caches. Classify each as runtime input, generated cache, authoring source, or irrelevant data; do not promise to treat every file as a runtime asset.

Header probes found DTS v24 in 153 entries and v18 in `base/data/shapes/markers/octahedron.dts`; DSQ v24 in all 61; DIF outer resource header 44 in all 76; TER first byte 3 in all 13; DSO first word 190 in all 16. These are header observations, not complete format validation, and DIF outer versions do not prove matching interior layouts.

`Map_BiomeRacing.zip` is rejected as non-ZIP by Python's ZIP reader. Ten members of `Map_Slate_Death_valley.zip` have differing central-directory/local-header path spellings. These require diagnosis or an explicit compatibility policy, not silent failure or unchecked path normalization. Two logical paths occur more than once. ZIP/folder precedence, case folding, slash normalization, extensionless names, and script-relative paths belong in a virtual filesystem specification.

The 38 saves contain 1,507 EVENT records, 723 NTOBJECTNAME records, 304 EMITTER records, 134 LIGHT records, 37 ITEM records, nine VEHICLE records, and seven AUDIOEMITTER records. A city that looks correct after loading may still have lost its gameplay.

## What to preserve from Blockland2

Keep as reference and selectively port after testing:

- `TorqueFormats.cpp/.hpp`: DTS/DSQ decoding, material flags, node/detail information, binary bounds checks.
- `BrickAsset`, `BlocklandCoordinates`, `BrickTransform`, and related tests: unusual brick geometry, axis conversion, rotation and ramp-name lessons.
- `BedroomAsset`: recovered DIF/TER parsing, lightmap and collision knowledge; separate those facts from Bedroom assumptions.
- `AvatarPose`, mount/socket code, and asset probes: animation, tools and vehicle attachments.
- The separation of authored brick IDs, physics IDs, and render snapshots.
- Material/alpha/color-space investigation, with original-source provenance and visual revalidation.
- Focused fixtures and tests that encode an observed format or gameplay invariant.

Concrete limitations verified in code:

- `TorqueFormats.cpp:108` accepts only DTS v24; `:645` accepts only DSQ v24. `:310` rejects mesh types other than Standard/Skin, apart from Null.
- `TorqueFormats.hpp` says the current renderer displays only the first vertex frame. Parsing fields is not full animation support.
- `BedroomAsset.cpp:502` opens `bedroom.mis` explicitly; `:449` requires four glassA panes. It also requires an interior/spawn/sun arrangement, opens Bedroom-specific data, and rejects DIF sub-object collision. This is not a general mission loader.
- `BlocklandSave.cpp:132` skips every `+-` extension record. The main brick parser consumes only a subset of the base brick fields.
- Source/dependency inspection did not identify a multiplayer transport/replication implementation or a general TorqueScript execution engine. Treat both as new work unless a broader history audit finds otherwise.
- `BrickDestruction.cpp` has 10,275 physical lines, `BrickStructure.cpp` 7,644, and `PhysicsScene.cpp` 7,589. Line counts are not a quality metric, but these systems contain a large amount of work outside the requested core experience.

Main versus the failed-physics branch changes 111 files; main versus the Metal-loading branch changes 47. Neither branch tip should be presumed to be a clean minimal foundation based on its name. Use the old repository as a reference implementation, not the architecture to translate wholesale.

## The largest scope decision: add-on compatibility

Four different claims must be tracked separately:

1. **Resource compatibility:** read the textures, geometry, animations, sounds and layouts.
2. **Data compatibility:** instantiate missions, datablocks, brick definitions and complete saves.
3. **Behavior compatibility:** run callbacks, events, state machines, schedules and script APIs with matching semantics.
4. **Protocol/service compatibility:** interact with original clients, servers, authentication and external services.

The requested core experience needs the first three in a defined, expanding corpus. A new multiplayer protocol can satisfy modern networking without promising connection to old servers. Original service compatibility is a separate requirement.

The stock Teledoor demonstrates the difficulty. Its script uses datablocks, dynamic fields, named target groups, `onPlant`, `onLoadPlant`, trust completion, player touch callbacks, package overrides, `Parent::`, transforms, velocity, and events. Loading `teledoor.blb` only creates its appearance. The Gun also needs weapon-image states and projectile callbacks; the Jeep needs script inheritance/includes, mount behavior, tire/spring data, and vehicle rules.

Recommended approach:

- Implement a bounded TorqueScript source compatibility layer in Rust, backed by explicit native host APIs. Grow a documented support matrix against actual add-ons.
- Begin with declarative datablocks plus a small executable script subset, but label that honestly. Arbitrary scripts can compute values and change behavior; regex extraction is not an interpreter.
- Preserve namespaces, dynamic fields, string/numeric conversions, object identity/lifetime, inheritance, package stack/Parent calls, scheduling order/cancellation, file lookup and client/server separation as first-class requirements.
- Reimplement missing core game behavior against observed v20 results. Keep the Blockland game API separate from generic language execution.
- Investigate DSO 190 independently. Do not assume modern Torque3D documentation or bytecode loads this variant. Core behavior can be recreated without a DSO interpreter, but DSO-only third-party add-ons remain unsupported until a compatible execution path exists.
- Native engine hooks/DLL add-ons and vanished external services need explicit ports or replacements. A sandboxed Rust runtime cannot promise binary ABI compatibility with the original executable.

Progress should be reported as parsed / resolved / rendered / collision-verified / behavior-verified / multiplayer-verified for each fixture. Never report merely 'add-ons supported.'

## Proposed core architecture

```text
Original installation and add-on ZIPs
              |
      Virtual filesystem + provenance
              |
 Bounded format readers + script/data compatibility
              |
 Versioned intermediate assets + dependency graph
              |
 Validated content cache and immutable asset catalog
              |
 Authoritative game simulation + brick/event state
       |               |                  |
 Physics adapter   Network replication   Save/load
       |
 Render/audio/UI presentation snapshots
              |
       wgpu client + shared game UI
```

Suggested ownership boundaries: `legacy_content`, `content`, `world`, `game`, `physics`, `net`, `render`, `ui`, and small client/server/import-tool executables. Do not begin by building an extensible general-purpose engine framework.

Keep legacy file structures out of renderer/game internals, but retain semantic metadata such as mount nodes, material flags, collision details, event records and original identifiers. glTF can be useful for inspection or new art; using it alone would lose Torque/Blockland semantics.

Use content hashes, importer/schema versions and dependency fingerprints for cache invalidation. Derived GPU data and physics meshes are rebuildable. Preserve source assets and unknown save records, with explicit unsupported-state diagnostics. A missing brick must not silently disappear on resave; retain a placeholder and its metadata.

Define units, handedness, winding, angle conventions, UV orientation and alpha/color semantics once. Keep the legacy gameplay API in Torque coordinates/units. Bricks should use integer grid coordinates where possible; any renderer/physics basis conversion happens at a specified boundary. Do not copy Blockland2's 0.48 world scale as an unexplained constant. Include asymmetric ramps, nonuniformly scaled shapes and rotated interiors in tests.

## Rust, wgpu, physics and UI

Rust + wgpu is a reasonable fit for the stated rewrite. wgpu still uses native Vulkan, Metal and DirectX 12 backends; its benefit here is one application-facing renderer and shader pipeline rather than maintaining separate native renderers. It does not supply gameplay, terrain import, UI behavior, physics or networking. [wgpu documentation](https://wgpu.rs/)

Keep the game custom and small. Evaluate reusable window/input, ECS/scheduling, text and UI components at a narrow integration milestone; avoid spending the first phase writing those utilities from scratch. Bevy is a candidate if its engine conventions help rather than hinder the compatibility model, but no framework eliminates this import/behavior work. [Bevy introduction](https://bevy.org/learn/quick-start/introduction/)

Jolt can remain a dependency of a Rust application. It provides virtual characters and vehicles and has language bindings. Rust wrappers need an explicit Windows/macOS/Linux compilation and API-coverage test: the examined `jolt-rust` repository describes itself as early work, and its safety layer is best-effort. A small owned FFI surface around a pinned Jolt version is an alternative. Rapier is a Rust-native fallback to evaluate if the boundary proves costly; switching merely for language purity is not a fidelity improvement. [Jolt](https://github.com/jrouwe/JoltPhysics), [Rust wrapper](https://github.com/SecondHalfGames/jolt-rust), [Rapier](https://rapier.rs/)

Player movement remains a tuned game controller: acceleration, stopping, air control, crouch clearance, step height, jump/jet transitions and camera response must be measured. A virtual controller provides collision facilities, not Blockland movement automatically. [Jolt character architecture](https://github.com/jrouwe/JoltPhysics/blob/master/Docs/Architecture.md)

Authored construction should be stable by default. Use spatially partitioned static collision and batched/instanced render data for bricks; dynamic bodies for vehicles and bounded temporary effects. Specify ordinary brick removal/support behavior against v20. Structural fatigue, material fracture, automatic collapse and debris persistence stay outside the initial release scope.

Use a shared UI/action model on all platforms. Import GUI layouts/profiles/skins where practical, recreate callbacks through the game API, and retain recognizable workflows. Preserve numpad building and keybinds, tool/brick/paint modes, wrench dialogs, brick favorites, avatar selection, chat, player/trust lists, hosting/joining and save/load. Add DPI scaling, readable text, remapping and optional mouse-friendly shortcuts. Debug UI must not become the shipped interface by default.

## Visual modernization without losing identity

Establish a reference rendering path first, then an enhanced profile using the same authored content. Match texture/face/print selection, palette colors, silhouette, proportions, animation timing, fog and environment mood before adding effects.

Modern improvements should target stable filtered shadows, clear contact shading, consistent exposure, restrained plastic highlights, antialiasing, good mipmaps and less particle sorting/intersection trouble. Preserve low-resolution art where it carries the visual identity. Avoid automatic shiny PBR conversion, exaggerated bloom, or global color changes used to hide import mistakes.

DIF baked lighting must be understood before combining it with new lighting. Distinguish authored diffuse coloration, embedded lightmaps, cached lighting, and new illumination; otherwise surfaces get lit twice. Multiple rendering modes are useful for debugging, but production should have a small number of coherent profiles.

Particles need imported curves, lifetime/variance, ejection rate/cone, inherited velocity, gravity/drag, blend/depth behavior, attachment points and sound timing. Reusing the texture alone does not preserve an effect. Bound particles/lights per scene and keep gameplay effects independent of client cosmetic budgets.

Three map fixtures are mandatory from the beginning. Bedroom tests interiors/glass/static shapes; Kitchen tests a second interior with different transforms and scene composition; Slopes tests terrain, repeating world behavior, snow, water and fog without relying on a room. Their shared loader must not require a Bedroom window arrangement or even an interior object.

## Networking and persistence must arrive early

Use an authoritative server with a fixed simulation step, predicted local movement, reconciliation and interpolated remote entities. Implement single-player through the same game authority. The server executable must start without a window or GPU.

Separate reliable edits/events/inventory/permissions from disposable movement snapshots. Version the protocol and content manifests. Support late-join snapshots followed by ordered deltas, stable brick/object IDs, map transitions, reconnects, content mismatch diagnostics and per-client interest management. Test edits during initial world download. Choose transport only after these semantics are specified; a transport library does not implement them.

Do not depend on all clients running identical full-world physics. Jolt's deterministic guarantees have build/order requirements; server authority with selective prediction is a more bounded initial design. [Jolt determinism documentation](https://github.com/jrouwe/JoltPhysics/blob/master/Docs/Architecture.md#deterministic-simulation)

Preserve ownership/trust, named brick targets, event inputs/outputs/delays, minigame rules, inventory, damage, spawns and quotas. Separate legacy BL_ID references from new authenticated identities; never grant authority because a client claims an old numeric ID. Plan LAN/direct IP first, then server discovery, NAT traversal/relay and identity-service integration as explicit product work.

BLS import/export needs complete records and round-trip tests. New persistence can have its own versioned schema with migrations, but exporting unsupported new features must disclose losses. Add atomic writes/backups and avoid serializing raw physics-engine state as the durable world format.

Add-ons and downloaded worlds are untrusted inputs: bounded decompression and parser allocation, restricted paths, script instruction/schedule budgets, capability-scoped filesystem/network access, and server command validation belong at the boundaries. These requirements follow directly from supporting arbitrary community content.

## Milestones with concrete exit criteria

| Milestone | Deliverable | Exit criterion |
| --- | --- | --- |
| 0. Reference contract | Frozen content manifest, workflows, videos/input traces, target machines and performance budgets | Distinguish v20, launcher modifications and intended changes; record unknowns |
| 1. Content tools | Rust inventory/resolver/readers and minimal wgpu inspection viewer | Corpus failures are categorized; representative BLB/DTS/DSQ/DIF/TER assets display with bounds/collision diagnostics |
| 2. General world import | Bedroom, Kitchen, Slopes, avatar, core movement and build loop | One mission pipeline loads all three; original controls work; complete save records are retained |
| 3. Two-player proof | Headless server, same-world edits, events, movement, save/reload | Two clients agree on ownership and edits; late join works; an evented save still operates after reload |
| 4. Core experience | Menus, HUD, trust, minigames, audio, weapons/vehicle baseline | Recorded v20 workflows complete end-to-end, including player movement and interaction checks |
| 5. Add-on expansion | Source-script host API and growing regression corpus | Teledoor, Gun, Jeep, player and particle packs work through the compatibility API; unsupported calls are visible |
| 6. Polish and scale | Enhanced lighting/shadows, platform parity, large worlds and distribution | Chosen hardware and network budgets pass without changing the accepted feel |

Stages overlap: define server commands and script interfaces during content work, and use a simple two-client room before all map fidelity is complete. Do not build a polished single-player game before discovering networking constraints.

The first vertical-slice acceptance scene should let two players enter Bedroom, move/jump/jet/crouch, use the stock build palette and numpad controls, plant/paint/remove bricks, operate a named-target event, save, restart, and reload with state intact. Kitchen and Slopes must pass the same generic loading path alongside it. A basic Gun and Jeep then test mounted models, particles/audio, dynamic collision and replication.

## Work easily missed

- Exact planting/overlap/raycast/support rules, collision/render/raycast flags, color effects, prints, ownership and event permissions.
- Avatar part hiding/coloring, faces/decals, mounted items, animation transitions and first-person presentation.
- Terrain seams, holes, material layers, repetition, water interaction, ambient sound, sky orientation and spawn semantics.
- Desktop focus/alt-tab, relative mouse capture, wheel/key repeat, UI focus, keyboard layouts, DPI, fullscreen and laptop controls without a numpad.
- Packaging, local content discovery, import progress/errors, cache upgrades, asset provenance and missing dependencies.
- Brick throughput, collision rebuild spikes, shadow draw cost, transparent-brick sorting, script/event storms and late-join bandwidth.
- Administration, trust persistence, bans, private sessions, chat, minigame lifecycle and graceful disconnects.
- Mod load order, package override order, duplicate datablock names and migrations across add-on versions.

For distribution planning, keep original-game assets outside the rewrite's source and release packages by default and support importing a user's installation. That design does not itself decide redistribution rights; any plan to bundle the original assets needs a separate permissions review. Do not inherit the prototype's embedded-content packaging as the release assumption.

## Immediate next implementation

Build the Rust content inspector and compatibility ledger before committing to a broad game framework. It should resolve ZIP/loose assets, report unsupported variants and dependencies, and expose minimal model/map viewing. Pair that with captured v20 controls/workflows and a small script behavior fixture. The largest unknowns are language/host API breadth and generic world fidelity, so those should receive the earliest experiments.

Native desktop first is the working assumption. Required OSes/minimum GPUs, desired brick/player counts, and whether unmodified third-party scripts are a release requirement remain product decisions. Set measurable targets before making renderer or tick-rate promises. No reliable schedule estimate follows from the inventory alone.

## Reproducing the census

From the workspace, run:

```powershell
python tools/inventory_v20.py 'C:\Users\Maxwell\Desktop\Games\B4v21-Launcher-Release\versions\Blockland v20' docs/research/v20-inventory.json
```

The scanner uses bounded reads and regex heuristics for textual declarations. It is not a Torque parser, complete dependency resolver, archive repair tool or compatibility validator. No scanned script/save exceeded its 2 MiB text-read limit in this run. Errors remain visible in the report.

Format background: [Torque shape formats](https://docs.torque3d.org/for-artists/shapes/file-formats), [DSQ specification](https://torquegameengines.github.io/T3D-Documentation/content/documentation/Artist%20Guide/Formats/dsq_format.html). These are reference material; test actual Blockland bytes rather than assuming the newer engine's formats and behavior are identical.
