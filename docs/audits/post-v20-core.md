# Post-v20 core behavior register

Reviewed Badspot's release announcements from the August 2012 shader update
through the latest release announcement listed on the official Development
board, r2033 (May 2020). Board pages
[1](https://forum.blockland.us/index.php?board=41.0),
[2](https://forum.blockland.us/index.php?board=41.20),
[3](https://forum.blockland.us/index.php?board=41.40) and
[4](https://forum.blockland.us/index.php?board=41.60) establish the announcement
coverage. This is a behavior audit, not a claim to possess unpublished Torque
source changes. Later source files, installed v21 shaders and add-on revisions
are separate evidence pinned in the v0.4.3 plan.

Disposition terms below identify the work owner, not completed acceptance:
**render** feeds the v0.4.3 lighting/render matrix; **bots** feeds the event and
navigation crosswalk; **boundary** identifies a native authority, resource,
conversion or lifecycle contract to check through its existing owner. **legacy**
means the specific Torque implementation/API/platform is absent and its patch
must not be transplanted. Content/game-mode/GUI additions are outside this core
audit unless independently requested. No acceptance item is checked off by
reading a changelog.

| Primary announcement | Relevant core behavior | Disposition |
| --- | --- | --- |
| [Shader release](https://forum.blockland.us/index.php?topic=203515.0) | Per-pixel lighting/fog/specular even without shadows; cascaded directional shadows and replicated environment changes. | Render: baseline behavior, quality-tier costs and state replication. |
| [r1701](https://forum.blockland.us/index.php?topic=204787.0) | Ground lighting, unnecessary minimum-tier shadow work, transparent brick parts, cloud motion and destruction cost. | Render; destruction lifecycle. Torque package deactivation is legacy. |
| [r1707](https://forum.blockland.us/index.php?topic=205280.0) | Transparent avatar depth, water transitions, scaled step height, crouch clearance, flare fading and custom sound/debris crashes. | Render and boundary: body scaling, audio inputs and dedicated lifecycle. |
| [r1711](https://forum.blockland.us/index.php?topic=205830.0) | Cascade transitions/blending, distant caster treatment, excluding noncasters, third-person flare; invalid wrench events and authentication persistence. | Render plus event/authority boundaries. |
| [r1713](https://forum.blockland.us/index.php?topic=205911.0) | Shader compatibility reporting. | Legacy driver telemetry, not a WGSL runtime requirement. |
| [r1715](https://forum.blockland.us/index.php?topic=206296.0) | Low-view-distance shadow blending/bias and old OpenGL fallback. | Render for view-distance interaction; legacy GL extension checks. |
| [r1728](https://forum.blockland.us/index.php?topic=207435.0) | Authored self-illumination, shadows far from spawn, audio filename/truncated-WAV safety and an air-control check. | Render and boundary. The air-control note does not identify the reported accumulating movement delay. |
| [r1730](https://forum.blockland.us/index.php?topic=207738.0) | Lower-tier memory, public brick cleanup, vehicle impact audio and shader compatibility. | Render/resource/lifecycle boundaries; legacy GL-specific work. |
| [r1741](https://forum.blockland.us/index.php?topic=210380.0) | Corrupt OGG loading, invalid file requests, building above print plates/roads and changing-brick collision. | Boundary: decoder/import, construction and collision invalidation. Door content itself is not a new feature mandate. |
| [r1744](https://forum.blockland.us/index.php?topic=210433.0) | JVS door compatibility correction. | Legacy script/package interaction; use existing native door semantics. |
| [r1749](https://forum.blockland.us/index.php?topic=212158.0) | Shader particle cost and legacy-door loading overhead. | Render budget; legacy door add-on policy. |
| [r1753](https://forum.blockland.us/index.php?topic=212634.0) | Projectiles/debris receiving lighting; emitter errors and dedicated-door packet problems. | Render family matrix and dedicated lifecycle boundary. |
| [r1766](https://forum.blockland.us/index.php?topic=213982.0) | Projectile wall collisions, scaled projectile lighting/shadows, mounted-image scaling and dismount orientation. | Render plus ordinary physics/attachment transition contracts. Sports content excluded. |
| [r1775](https://forum.blockland.us/index.php?topic=216871.0) | File security, callback overrides during saves and packet-size failures. | Boundary analogues: sandboxed file access, stable persistence and bounded codec. Torque constants/APIs are legacy. |
| [r1783](https://forum.blockland.us/index.php?topic=218189.0) | Append and deletion during player-touch callbacks. | Native persistence and deferred-operation lifetime boundaries; no Torque VM. |
| [r1808](https://forum.blockland.us/index.php?topic=223980.0) | Admin-only input events, reset cancellation, persistent critical schedules, control deletion, camera collision/orbit stability and smoke removal in water. | Boundary: authority and lifecycle; inspect relevant minigame/control transitions. Racing content and ownership relaxations are not copied wholesale. |
| [r1853](https://forum.blockland.us/index.php?topic=226906.0) | Bot-hole event surface; ModTer visual/collision consistency; minigame damage, spawn markers, reset events and death-camera/respawn rules. | Bots crosswalk plus boundary checks. Existing native rules remain authoritative. |
| [r1855](https://forum.blockland.us/index.php?topic=226927.0) | Shark-bite crash and camera control-object type correction. | Native damage/control lifetime boundaries; script return-type patch is legacy. |
| [r1864](https://forum.blockland.us/index.php?topic=232456.0) | Invalid brick geometry must not crash; diagnostics identify brick files; address/auth fixes. | Importer/catalog validation and diagnostics, including #414. Legacy address conventions excluded. |
| [r1908](https://forum.blockland.us/index.php?topic=246281.0) | FOV setting and Steam/platform transition. | Existing camera setting; legacy distribution/platform policy. |
| [r1918](https://forum.blockland.us/index.php?topic=247576.0) | Input reacquisition after overlay focus and authentication outages. | Native focus/input lifecycle and authentication boundary; old DirectInput/Steam patch is legacy. |
| [r1922](https://forum.blockland.us/index.php?topic=247908.0) | Authentication failure diagnostics. | Existing native connection diagnostics; game-mode/GUI changes excluded. |
| [r1925](https://forum.blockland.us/index.php?topic=249045.0) | Timeout versus explicit auth rejection; disconnect while editor is open. | Native connection/control teardown boundary. No new Steam auth policy. |
| [r1931](https://forum.blockland.us/index.php?topic=251607.0) | Supported-card shader defaults, activation flood protection and server CLI settings. | Native quality defaults/command admission; legacy GUI/content edits excluded. |
| [r1956](https://forum.blockland.us/index.php?topic=273265.0) | Particle frame rate replication, environment packet sizing, loading chat draft continuity, spawn animation, ModTer faces and projectile radius. | Render, loading/chat, animation and conversion boundaries. Proposed ghosting optimization was reverted in this same announcement. |
| [r1959](https://forum.blockland.us/index.php?topic=273444.0) | Ghost resource limits, large-address flag regression and omitted defaults file. | Native bounded replication/resource and actual packaged startup checks; Torque tables and 32-bit flag patch are legacy. |
| [r1966–67](https://forum.blockland.us/index.php?topic=281644.0) | Trust before portal use, burning vehicles, time-step freeze, ModTer coverage and missing animations. | Authority, lifecycle, motion and rendering boundary checks. Eval-vulnerable legacy scripts are not executed by the native importer. |
| [r1972–73](https://forum.blockland.us/index.php?topic=287710.0) | Join/download throughput, Bot_Hole admin checks and slow-motion wall phasing. | Loading, bot-event authority and motor collision contracts; old packet/XP fixes are legacy. |
| [r1981](https://forum.blockland.us/index.php?topic=289421.0) | Download retry/budget control, orphan construction ghosts, tumbling versus respawn and focus-held keys. | Native transfer/construction/control boundary checks. Torque FastPackets is not a new protocol setting. |
| [r1983](https://forum.blockland.us/index.php?topic=289522.0) | Word/field crash and screenshot output directory. | Legacy string routines; native output-path confinement boundary. |
| [r1984](https://forum.blockland.us/index.php?topic=290045.0) | Further word-routine crashes and Windows SDK rollback. | Legacy native-string/SDK implementation, not a behavior port. |
| [r1985–86](https://forum.blockland.us/index.php?topic=302905.0) | Team chat flood protection, client script injection and chunk size. | Native chat admission, sandbox and codec/resource boundaries. |
| [r1988](https://forum.blockland.us/index.php?topic=317792.0) | Original account-key ID ranges. | Legacy account service, outside native release behavior. |
| [r1991–97](https://forum.blockland.us/index.php?topic=321223.0) | Formatting overflow, malformed join/LAN data and oversized authenticated names. | Native bounded codec/identity inputs; no C-string patch transplant. |
| [r2000](https://forum.blockland.us/index.php?topic=323839.0) | Network-event security, speed manipulation and leaking splash objects. | Authority/movement and effect lifetime boundaries. |
| [r2005–12](https://forum.blockland.us/index.php?topic=325765.0) | Unsafe string operations and authentication incident response. | Native input/resource boundaries. Original account migration, compiler mitigation and content tweaks are legacy. |
| [r2023–31](https://forum.blockland.us/index.php?topic=325891.0) | Steam identity migration, join validation, passworded server/list fixes and multi-client authentication. | Native identity/connection semantics; preserve current product policy rather than copy the original service migration. |
| [r2033](https://forum.blockland.us/index.php?topic=325961.0) | Duplicate-client policy and partial network-event packet overflow during loading. | Native identity/admission and bounded loading-message contracts. |

## Release follow-through

Rendering acceptance must cover opaque/alpha-tested/alpha-blended world surfaces,
avatar faces/decals, projectiles, debris and authored emissive effects; light,
shadow, scale, distance and environment transitions cannot be tested only on a
single brick. Distinguish intentional full-bright/glow assets from the faulty
unlit path. The requested subtle material treatment remains separate from
lighting correctness and from Robbin's custom luminance-based shader sketch.

The existing release plan owns these corrections and their evidence. For
boundary rows, inspect existing owners/tests and record either a demonstrated
mismatch and correction or the native mechanism that makes the original defect
inapplicable. Do not expand this into a second implementation platform, relax
authority to imitate old fixes, or claim all listed behaviors verified yet.
