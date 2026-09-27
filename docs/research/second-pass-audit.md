# Second pass: Blockland v20 migration evidence

2026-09-26. This is a feasibility audit, not an implementation commitment. Current user intent is a one-time migration into new editable assets and new game behavior, without a permanent Torque runtime or a promise to execute arbitrary old add-ons unchanged.

## Finding

The one-time migration remains the strongest candidate, with better evidence than the first pass. Core script definitions are recoverable enough to inspect now. Full geometric, visual, collision, animation, and gameplay fidelity remain unproven. These should be tested on a representative conversion slice before the final runtime schemas or renderer architecture are frozen.

No game implementation was started and the original installation was not modified. Research copies, downloaded reference code, decompiler output, and audit notes are local to this workspace. Seven recorded original-file fingerprints still match. Neither the original game nor Blockland2 was run for behavioral comparison.

## 1. The compiled script barrier is smaller than initially assumed

Found [DSO Sharp](https://github.com/Elletra/dso-sharp), which explicitly supports Blockland v20. Its source maps v20 to DSO version 190, matching all 16 local DSO headers. Inspected its command-line and output behavior, copied the original DSO files into `.research/v20-dso`, and ran release 2.1.0 against those copies with `-g blv20 -X`. The tool reported successful decompilation of all 16 and wrote readable `.cs`/`.gui` files. No recovered game scripts were executed.

Evidence: `dso-probe.log`, `dso-probe-tool.json`, `.research/v20-dso/`. The latter JSON records the downloaded executable URL and SHA-256. The available .NET SDK was older than the project's target, so the upstream standalone release was used without installing a new SDK.

The locally recovered main server script contains 431 textual datablock declarations, including:

| Definition family | Declarations |
| --- | ---: |
| Brick types | 136 |
| Particle definitions | 78 |
| Particle emitters | 78 |
| Explosions | 26 |
| Audio profiles | 25 |
| Projectiles | 25 |
| Weapon/tool image definitions | 24 |

It also includes animation mappings, player parameters, material/object definitions, and gameplay functions. The main server and client scripts contain 566 and 878 function declarations respectively. These are source-text counts, not counts of unique active runtime objects or separate features.

Example: `PlayerStandardArmor` supplies forward/back/side speeds of 7/4/6 in legacy units, air control 0.1, jump/run force expressions, crouch speeds, camera parameters, and animation/effect references. These are useful starting inputs, not a guarantee that feeding them into a different physics implementation reproduces movement.

There is also a [published v20 decompilation](https://github.com/Elletra/bl-decompiled). Fourteen of the sixteen locally recovered files match that collection after newline normalization and trimming; the two large gameplay/client scripts differ. Those two have not been semantically reconciled. The local recovered files remain the primary reference for this installation.

**What this proves:** a practical way to inspect the local compiled definitions and control flow exists.

**What it does not prove:** semantic equivalence of every recovered function; recovery of original comments/authoring organization; recovery of native C++ engine implementation; automatic translation into Rust; or permission to redistribute recovered original-game content.

Native calls such as brick `plant()` still appear as calls, not implementations. Collision/planting rules and other native behavior will require source comparison where applicable and measurements against the original executable.

## 2. The UI source is substantially more complete after decompilation

The recovered `allClientGuis-Vanilla.gui` contains 1,610 textual GUI control constructions. It matches the corresponding published decompilation after normalization. The visible loose `allClientGuis.gui` contains only 377 such constructions and is part of the patched installation.

This changes the migration method: extract the original widget hierarchy, layout, labels, image references, profiles, and commands as a specification before redesigning controls. Counts refer to nested controls, not 1,610 screens.

The new UI will still need new widget behavior, focus handling, scaling, scrolling and callbacks. A layout conversion alone does not reproduce its interaction. Use the recovered screens to define a finite workflow list instead of approximating the UI from memory.

## 3. Generic Torque source is a reference, not an exact Blockland engine

Inspected selected files from the MIT-source GarageGames Torque3D v1.1 tag, commit `d0de864ea26293e5e905c6ec1768f985376af3de`. This is not a claim that Blockland v20 uses that exact codebase. The older open-source tree retains useful interior/terrain readers absent or changed in newer workflows.

Concrete comparisons:

- [Interior resource reader](https://github.com/GarageGames/Torque3D/blob/d0de864ea26293e5e905c6ec1768f985376af3de/Engine/source/interior/interiorRes.cpp): outer resource version 44, optional preview, followed by detail records. All 76 readable local DIF entries have header 44, no preview, one detail, and first interior version 0 under that layout.
- [Interior geometry reader](https://github.com/GarageGames/Torque3D/blob/d0de864ea26293e5e905c6ec1768f985376af3de/Engine/source/interior/interiorIO.cpp): explicitly distinguishes TGE versus TGEA version-0 layouts by validating surface reads and retrying. A matching number alone does not identify all internal semantics.
- [Legacy terrain reader](https://github.com/GarageGames/Torque3D/blob/d0de864ea26293e5e905c6ec1768f985376af3de/Engine/source/terrain/terrFile.cpp): documents legacy height samples, material assignments, alpha layers and stored authoring scripts. Its modernization path selects the greatest material layer when constructing a newer layer map; copying that conversion wholesale could discard blending information we want to preserve.
- [Lighting persistence naming](https://github.com/GarageGames/Torque3D/blob/d0de864ea26293e5e905c6ec1768f985376af3de/Engine/source/lighting/common/sceneLighting.cpp) and [persistence reader](https://github.com/GarageGames/Torque3D/blob/d0de864ea26293e5e905c6ec1768f985376af3de/Engine/source/lighting/common/scenePersist.cpp): identify the mission-named `.ml` cache mechanism. The reference reader expects version 17; all 14 inspected local ML entries begin with 16. A direct drop-in reader is not established.

**Consequence:** keep embedded DIF lightmaps, separate mission-lighting caches, and scene lighting parameters until we can compare their contributions. Do not discard ML files merely because they are generated caches or assume DIF data alone captures the displayed lighting.

The two Bedroom ML copies, loose and archived, also have different sizes/hashes. Establish which is active in the original reference rather than arbitrarily choosing one.

## 4. The maps exercise more than three geometry loaders

Scene declaration counts from the actual mission text:

| Map | Observed composition |
| --- | --- |
| Bedroom | One interior, ten StaticShapes, six TSStatics, terrain, grass/foliage, clock/glass/light datablock references |
| Kitchen | One interior, sixteen StaticShapes, nine TSStatics, terrain, distinct lights/clock/glass references |
| Slopes | Terrain, precipitation and water, with no interior |
| Tutorial | Two interiors, 33 triggers, three water blocks, vehicle blockers, a marker and an emitter |
| Paradise day | 120 interior instances and 89 TSStatics, plus water, items, foliage and a physical zone |
| Paradise night | 124 interior instances and 98 TSStatics, plus light and environment changes |

Counts are static text observations, not a live scene census. They nevertheless show why a single-room importer is insufficient.

For stock experience validation, add Tutorial to Bedroom/Kitchen/Slopes: it exercises interaction and triggers, not just appearance. Use one large community map such as Paradise as a stress/coverage fixture if community maps remain in the migration scope. Do not make every installed add-on a release requirement by accident.

Bedroom's named clock pieces also illustrate that an apparently static map can include behavior. A complete scene specification needs prop definitions and callbacks, not merely its DIF and terrain.

## 5. Asset counts are not equivalent to ready-to-use game definitions

The 174 inventoried BLB files include 80 with a BRICK header, 86 SPECIAL, seven SPECIALBRICK, and one unusual file, `base/data/bricks/rounds/1x1x5spike.blb`, beginning with a Tree Cone comment followed by a texture/position section rather than the usual dimensions/type header. Its active use has not been established. Retain it as an exception to investigate rather than count it as a proven complete brick definition.

Dimensions, collision and coverage are only part of a brick. The recovered datablocks supply catalog labels/categories and references; native/game code supplies planting rules and interaction. The earlier phrase '174 brick definitions' should therefore be read as '174 BLB files.'

The two player IFL files list face/decal resources. Recovered client code builds avatar menus from them, regenerates them, and selects IFL frames for preview. In this corpus they are also customization catalogs; they must not simply be converted into automatically playing texture animations.

The `.csx` entries were positively identified by their XML headers as Torque Constructor scene documents, version 4, for MaxwellDM1 and MyChallenge. These are candidate editable source inputs. The associated binary `.clx` files remain unidentified beyond their header; preserve them without asserting runtime relevance. The `.db` entries counted inside the inventoried asset scope are `Thumbs.db`, rather than gameplay databases. This does not classify the separate root-level `cache.db`, which was outside that scope.

## 6. Archive failures are diagnosable

- `Map_BiomeRacing.zip` begins with a RAR signature. 7-Zip identifies it as RAR and lists a MIS and TER member. The first census excluded it because it used a ZIP-only reader. It is not proven corrupt. Listing does not prove map compatibility; these two members are not included in the original inventory counts.
- `Map_Slate_Death_valley.zip` has local-header/central-directory path mismatches rejected by Python's ZIP reader. 7-Zip's integrity test reports all 17 files OK. Its ten rejected member reads are scanner/tool compatibility issues, not proof of missing payloads.

Future migration should detect archive type by signature and normalize paths with bounds/validation. Preserve raw path spellings and record ambiguity. No repair of the original archives was performed.

## 7. Existing converters can accelerate research but need qualification

| Candidate | Useful for | Remaining qualification |
| --- | --- | --- |
| DSO Sharp 2.1.0 | Inspecting local v20 compiled scripts | All 16 produced output; semantic behavior remains untested |
| Blockland2 import/probe code | Prior DTS/DSQ/BLB/DIF/TER discoveries and regression fixtures | Known Bedroom assumptions and partial behavior coverage; no fresh full-corpus execution here |
| [qoh/io_scene_dts](https://github.com/qoh/io_scene_dts) | DTS/DSQ import/export reference and Blender inspection | README targets an old Blender generation; current Blender compatibility and corpus fidelity not tested |
| [RandomityGuy/io_dif](https://github.com/RandomityGuy/io_dif) | DIF/Constructor scene import reference | Project documents Marble Blast-specific collision/export caveats; not a certified Blockland migration path |
| [DemianWright/io_scene_blb](https://github.com/DemianWright/io_scene_blb) | Brick authoring semantics, coverage, directional face groups | Exporter documentation helps establish semantics; not proof of complete import support |

Avoid an OBJ-only conversion as the archival master: it would not retain scene/game metadata, animation and the full original material semantics. A standard geometry format plus explicit sidecar/game definitions remains a candidate, not a finalized schema.

## What this changes in the proposed direction

1. A full TorqueScript interpreter is no longer a prerequisite under the clarified one-time migration scope. Recover definitions and port the chosen behaviors into native systems.
2. Core definitions and original GUI layouts are now available for direct inspection; there is less need to approximate them from screenshots or memory.
3. Lighting preservation deserves its own investigation because mission caches are present and versioned differently from the reference reader.
4. The content scope must distinguish stock core, selected community add-ons, unused/editor resources and incidental cache/authoring data.
5. Existing tools reduce extraction risk, but there is no validated universal converter yet.

## Remaining evidence needed before committing the full plan

| Question | Smallest useful proof | Current status |
| --- | --- | --- |
| Can converted models preserve the avatar? | Inspect all body parts, materials, mounts, animation channels and customization in an independent viewer | Not done |
| Can maps preserve geometry and collision? | Convert Bedroom, Kitchen, Slopes; compare surfaces, transforms, collision and terrain seams | Not done |
| What creates the original lighting? | Compare DIF pages, ML cache data and live v20 screenshots at fixed poses | Source/header evidence only |
| Are recovered scripts semantically reliable? | Check selected decompiled functions against observed behavior and reconcile the two differing published files | Output generated; not behavior-validated |
| How much behavior is native-only? | Trace planting, movement, event dispatch, mounted items and vehicle interaction across script/native boundaries | Some native call boundaries identified |
| What is the exact experience baseline? | Record default controls, building, wrench/events, avatar, host/join and Tutorial workflows in the supplied build | Not done |
| What asset representation should be permanent? | Export/import a representative model/map/brick with sidecar metadata and enumerate every lost field | Not done |

Proceeding with a bounded conversion proof is justified. Declaring all assets recoverable with full fidelity, choosing final schemas, or estimating the entire rewrite schedule is not yet justified.

## Research provenance

- DSO Sharp source: `dd1edbcca56e7d69d4fbcec351d427497cd2f049`; release executable separately fingerprinted in `dso-probe-tool.json`.
- Published decompilation: `b519133d89ff68768abcfe70ce38c958d6f5bf5b`.
- Torque reference: `d0de864ea26293e5e905c6ec1768f985376af3de`; selected files in `.research/torque-reference`, with paths recorded in `tree-selection.json`.
- Local probe results: `second-pass-evidence.json`; static regex counts and shallow format-header reads, with their limitations recorded.
- Original census and fingerprints remain unchanged so that first-pass coverage and exclusions stay explicit.
