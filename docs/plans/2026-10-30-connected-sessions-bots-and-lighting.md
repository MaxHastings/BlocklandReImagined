# Connected sessions bots and lighting release plan

Approved by Maxwell October 30, 2026, for autonomous implementation and GitHub
publication as v0.4.3 after the agreed acceptance and release conditions. This covers the requested voice,
minigame, NPC, rendering, audio and open-issue work as one release scope, delivered
through several private previews. Implementation is active on
`codex/v043-integration`; the broader alpha contract remains open.

Acceptance update: after trying preview.1, Maxwell reported it "seems fine so
far", requested continued autonomous execution and explicitly said no further
playtests are needed. Future manual-preview/combined-playtest requirements below
are superseded by that instruction. Complete the remaining integrated automated,
headless, source-comparison and offscreen checks, and report their limits without
claiming human acceptance of unplayed features. The intermittent local movement
symptom remains under investigation; this short preview is not a long-run proof.

Second review completed October 30 with additional source, installed-content,
saved-brick, audio-level and recorded-input inspection. Maxwell confirmed:

- Global minigame voice includes living and dead members together.
- Loading voice/chat follows the authorized destination game when known, otherwise
  Server Lobby. Loading alone confers no team/private-channel rights.
- Material detail should be subtle and preserve the original toy-like appearance.
- Hammer flicker occurs while walking uphill on 45-degree ModTer slopes holding
  an unswung hammer. Ragdolls were enabled, but no corpses were present.
- Add accumulating movement delay to this release's priority investigations. Mouse
  response may be unaffected, but that is uncertain; a networking cause is a guess.
- Bot_Town investigation is active again after Maxwell supplied
  `E:/Downloads/Bot_Town.zip`; the temporary hold is superseded.
- Add moving lights/emitters on destroyed brick debris and suspension of destroyed
  bricks' item spawners. Cosmetic following and authoritative spawning are distinct.
- Include issue 415 and Robbin's shader references, evaluating their proposed
  implementation against existing native rendering and customization mechanisms.

The second-review evidence and remaining technical uncertainties are recorded in
[the review receipt](../progress/2026-10-30-release-plan-second-review.md).
The compact [execution checklist](2026-10-30-next-release-execution.md) records
readiness, dependencies and the step-by-step delivery rules.

The recommended direction is to preserve Blockland's content and recognizable
behavior while correcting engine defects and selectively adopting later core
improvements. Reports establish problems to investigate; their suggested causes
and implementation sketches are hypotheses until checked. This release does not
complete the broader alpha contract.

## Audit baseline

The primary checkout is historical source `591ff3c72` with unrelated dirty work.
The audit therefore inspected Git objects from current `origin/main`, initially
`74c7e62fa` and then fetched `9390c39cf63874ac22b8470541666b1f208ef891`.
The intervening changes concern macOS microphone packaging and tooling, rather
than the gameplay paths discussed below. No branch checkout was switched, runtime
source changed or original installation modified. The reference public build is v0.4.2,
`93c1439dacea2c6bce19ca735195bb0f6b944daa`.

Read alongside current main's `docs/STATUS.md`, `docs/alpha-contract.md`,
`docs/architecture/platform-principles.md`, `docs/architecture/seams.md`,
`docs/testing-tiers.md`, and the
[v0.4.2 publication receipt](../progress/2026-10-30-v042-public-release.md).
Those newer documents must be read from current source when starting work; the
historical checkout's release status is stale.

### Findings that change the implementation plan

| Request | Finding | Implication |
| --- | --- | --- |
| Global voice inside a minigame | Already declared in `packages/voice_chat/behaviour.json`: Proximity, Global and Team. Current `voice.rhai` blocks dead-to-living speech in every mode. | Keep the existing mode mechanism, verify its workflow, and change Global to include living/dead members together as Maxwell confirmed. |
| Voice while loading | `client/src/app/frame.rs` requires `entered` and a gameplay view. Server `VoiceRouting::refresh` and voice admission use gameplay `peers`, excluding `lobbies`. | Requires an accepted-connection lifecycle change on both ends, not just removing a client condition. |
| Chat while loading | `poll_arrival` already displays lobby chat. Server broadcasts the global chat stream to lobbies, while `Session::chat_game` sends game messages to members privately. | Resolve audience eligibility and delivery, rather than assuming all loading chat is broken. |
| Creator leaves while battle continues | `minigames::remove_member` ends the game for its owner. `spawn_brick_game` derives bot membership from the builder's current membership. | Separate management, participation and encounter ownership. Changing only Leave would strand bots outside the game. |
| Spawn rotation | `reconcile_bot_brick` and brick respawn create a brain from position without using the brick's rotation. Supplied v21 `holes.cs` sets the bot transform with the brick rotation. | Correct the authoritative spawn frame across every lifecycle path. |
| Search through buildings | Existing dated evidence and exploration are useful. `search_memory.rs` searches the last observed position plus four points at radius 3.5, with bounded deadlines. | Whole-building pursuit needs connected routes and a broader evidence-driven search, not only longer memory. |
| Bots apparently Ctrl-K when stuck | `bots.rs` computes trapped respawn utility and `Behaviour::Respawn` submits `Command::Suicide`. | A plausible mechanism exists. Trace it in the reported save before assigning cause; ordinary navigation failures should recover without needless death. |
| Zombie stacking | Supplied v21 Zombie sets `hSuperStacker=1`; Bot_Hole has collision-triggered stack/jump behavior. | Source-backed behavior to adapt through generic support/contact and ordinary movement. |
| Bot wrench controls | Native power, respawn, sight, wander, return-home, auto-respawn and melee-damage controls already exist. Original Bot-target search/team/name/behavior vocabulary is broader. | Extend the existing typed controls and importer; preserve distinct target classes and parameter meanings. |
| Full-bright decals | Brick/stock-avatar overlays already use a lit material family; imported shape flags can select other behavior. | The reported visual defect is not yet assigned to one universal shader branch. Trace conversion, flags, binding and final pixels. |
| Bump/specular materials | Native shapes already support normal, metal/roughness and emissive maps. Brick overlays have a separate pigment/paint contract. | Reuse the material system while extending brick and other missing surface paths coherently. |
| Corpse blocks living players | A prior correction puts corpses in a group excluded by living-player motor queries. Canonical death still calls `set_corpse`. | Treat the new report as an uncovered path or regression; do not replace it with a blanket collision-off workaround. |
| Accumulating movement delay | An October 14 report described the same reset-sensitive symptom and corrected skipped-tick movement credit. That correction remains. Current recordings include sustained movement-rate refusals for a guest, while other recordings do not. | Investigate both the recurring shared input failure and the reported local feel; do not claim the guest evidence proves the host's cause or merely repeat the old fix. |
| Destroyed brick attachments | `WorldEffects::attach` constructs light/emitter sources from the brick's static position. Debris exposes moving transforms separately. Item reconciliation has no explicit destroyed-state guard. | Connect cosmetic attachment transforms to debris identity and trace authoritative item suspension; distinguish destruction from merely disabling rendering. |

These findings combine source inspection and recorded-call analysis. No new
headless gameplay reproduction or interactive session was run during planning.

## Release scope and sequence

Every requested item remains in scope. Discovery of a larger dependency changes
the implementation estimate or requires an explicit scope decision; it does not
silently remove the feature. There is no credible completion date before the
saved-world navigation and lighting investigations establish their actual causes.
The greatest uncertainties are multi-level navigation, stable physical stacking,
animated DIF lighting and complete material-family coverage.

| Phase | Deliverable | Exit condition |
| --- | --- | --- |
| 1 | Reproduction records and source crosswalk | Prioritize the movement-delay trace and saved-world navigation. Each report has exact inputs, a failing control or explicitly unresolved cause, and an owner in the existing seam map. Complete the core post-v20 changelog register. |
| 2 | Session and creator continuity preview | Loading voice/chat, discoverable global minigame voice, and a creator leaving without stopping the encounter work through real hosted connections. |
| 3 | Navigation and bot controls preview | Rotation, indoor routes, sustained evidence-driven search, builder events and stuck recovery pass the five saved-world cases relevant to each behavior. |
| 4 | Physical zombie stacking preview | Cube stacking and support loss work without jets; indoor routes remain passing independently. |
| 5 | Lighting materials and audio preview | Ordinary lit overlays, intentional emission, BadDream lighting, moving debris attachments, item-spawner lifecycle, material coverage and requested gain changes have technical and visual/listening evidence. |
| 6 | Integrated release candidate | All five currently identified GitHub issues and movement/death/hammer/destruction reports have the agreed outcomes and evidence; Maxwell accepts the combined playtest; final release checks and all platform packages pass. |

Small independent fixes, including music gain and importer diagnostics, can land
earlier within these phases. Root retains integration and shared engine ownership.
No new agents are required for this planning work. Any later explicitly delegated
work follows AGENTS.md's model, reasoning and path ownership requirements.

## Voice and chat through loading

### Behavior and audience

Voice becomes available after the server accepts the authenticated connection,
while Add-Ons download, map resources prepare, gameplay begins and maps change.
It cannot depend on a body, a loaded world camera or world rendering. Existing
PTT/VAD/disabled choices, personal/admin mutes, focus behavior and microphone
permissions continue to govern capture. Loading never opens a disabled microphone.

Use one connection-level conversation identity across the loading-to-gameplay
handoff. Maxwell confirmed the default audience: a valid retained game on reconnect, then
an eligible default minigame, otherwise Server Lobby. Loading UI names the current
conversation. Reuse existing game selection/invitation authority; a new channel
picker is not a prerequisite. Invite-only games and
team/dead channels require their actual eligibility; loading is not a spectator
backdoor. A pending conversation association does not spawn a body, earn points,
take a team slot or enable gameplay commands.

Where a loading participant has no valid spatial origin, use explicit
nonpositional loading playback for the authorized conversation. Do not fabricate
a position at the origin or borrow another player's camera. Package policy owns
these defaults and any permitted loading audience; the engine owns accepted
identity, transport, bounded work and lifecycle state.

Deliver minigame-scoped chat to eligible loading participants through the same
audience decision. Preserve message order/cursors and partially typed text across
entry; do not replay private chat from before authorization or broadcast every
minigame's messages to every loading player. Distinguish ordinary public chat,
game notices, team messages and kill feeds when reproducing the report.

### Implementation and acceptance

Extend the arrival protocol and read-only policy view with the minimal necessary
connection/conversation state. Reuse VoiceLink, Opus, route revisions and existing
audio queues; maintain one capture/playout lifecycle across connection replacement.
Do not run scripts per audio packet. Keep capture/playout responsive during actual
resource loading, including client work that can stall the frame thread.

Device-free tests use actual accepted QUIC arrival and gameplay clients, deliberately
held in download/map-loading states. Require bidirectional authorized voice and
activity, eligible chat, isolated other games, live mute/revocation, late/reordered
packets, cancel/reconnect/map changes and a clean handoff without duplicate playback.
Record drop/queue/latency behavior under bounded loading pressure. Human device,
microphone permission and conversation usability remain Maxwell's acceptance.

Expose the existing minigame Voice Mode clearly in Add-On Settings, with Proximity,
Global and Team and a meaningful disabled state when Voice Chat is not enabled.
Verify changes take effect, save/reload and respect host/editor permissions.
Global includes living and dead members together, with no distance attenuation.
It remains isolated from other minigames. Preserve existing Proximity/Team defaults
for death eligibility; independently configurable death policy can use the existing
package settings mechanism if needed, rather than forcing extra settings into this
increment. Test live changes between modes so buffered speech cannot restore an
obsolete route or carry Global eligibility into a restricted conversation.

## A creator can leave a running minigame

Make ownership a durable management relationship separate from active membership.
A game retains its settings, resources, teams, bots and round lifecycle when its
creator stops participating. Keep existing minigame IDs; do not create a fake player
or move ownership to an arbitrary bot.

Recommended UX in the creator's minigame window:

- **Leave minigame** removes the creator from play and keeps the encounter running,
  restoring the ordinary outside sandbox state. Explain that consequence beside
  the action. Keep **Watch** separate and reuse the existing spectator/camera
  mechanism where applicable; a new camera framework is not part of this request.
  Both retain management access through **My minigame**.
- **Rejoin** uses normal admission, team/loadout and spawn rules. **Manage**, **Reset**
  and **End minigame** act on the owned game even while its owner is outside.
- Ending is explicit. Other players' ordinary Leave remains ordinary membership
  departure. Observation confers no combat body or participant powers.

Bind encounter resources to their game independently of the builder's current
membership: hole bots, vehicle spawns, relevant bricks, event contexts and reset
ownership. Preserve their bindings when the owner leaves, joins another game or
disconnects; resolve new placements explicitly instead of moving existing encounters
with the builder. Check `use_all_players_bricks`, damage on both sides of membership,
team bot filling, default/exclusive games and Slayer's empty-game/reset policies.

Recommended lifetime: the game continues within the running server until explicitly
ended, its applicable mode policy ends it, the map changes or the server shuts down.
Owner disconnect retains authorized host administration and principal-based reclaim
on reconnect. Persistence across a server restart follows explicit saved-game rules,
not an implied always-running service. Closing a listen server still stops its game.
Retain game/member/resource limits and prevent creating unlimited abandoned games.

Acceptance requires a human-created bot battle that advances and deals real damage
after its creator leaves, with remaining clients agreeing. Also test owner rejoin,
disconnect/reconnect, management denial for unrelated players, reset/end, save/load,
zero human participants, and an owner participating in another game. Confirm the
outside/observer player cannot be killed or act as an immune combat participant.

## Navigation pursuit and physical stacking

### Diagnose the full chain

Trace target eligibility and observation, remembered evidence, destination choice,
route requests, navigation topology, body clearance, local steering, actual motor
contacts, path invalidation and the subsequent melee attempt. Record rejection
reasons, route frontier/cost, destination changes, stalled progress and any self-kill
decision. A pathfinder returning nodes does not establish that the zombie walked
the route or damaged its target.

Correct destination projection for a visible elevated enemy: choose a reachable
approach, entrance or route to their supporting floor rather than requiring a
route directly to airborne feet. Permit initial movement away from the target to
reach stairs. Distinguish a vertical route, an unreachable target, deferred search
work and a stale/blocked route. An unreachable target is still a threat; the bot
can approach underneath and retain pursuit without repeatedly recomputing failure.

Audit stairs, ramps, narrow doors, landings, multi-floor surfaces at the same X/Z,
headroom, body scale and collision masks. Prefer safe clearance through usable
treads and landings; avoid edge hugging caused by point-based routing or shortcut
smoothing. Do not hardcode stair centers: narrow viable routes and passing crowds
still need usable navigation. Cache shared geometry/topology by actual body
capabilities and relevant world generations; moving targets do not invalidate
unchanged world geometry. Build changes invalidate affected routes correctly.

The existing grid already keys nodes by floor height and off-center standing
positions and measures reach from the motor. It also has bounded partial searches
and shared budgets. First establish whether these mechanisms lose the route through
sampling, cost/heuristic, exhaustion, smoothing or following. A replacement navmesh,
hand-authored building graph or blanket budget increase is not a presumed solution.

### Search from evidence

Retain a target through brief jumps/occlusion with commitment and reacquisition
hysteresis. Separate that short continuity window from longer search intent.
Extend the existing search to connected entrances, corridors, stairs and unchecked
reachable areas around the last sighting and observed travel direction. Search
through a building after witnessing entry; learn from subsequent sight/sound and
actual searched space. Bound work fairly across bots and frames and expose why
search finishes. Navigation queue time must not consume the entire pursuit window.

Normal bots do not read unseen live player transforms. They can eventually search
an entire reachable building under continuing search policy, but cannot guarantee
finding someone who left unseen. Visible targets, remembered targets, a player
spawning above without ever being seen, and deliberate exact-location tracking
have separate acceptance scenarios.

Ordinary blocked movement should try valid alternative routes, local escape and
replanning. Do not use suicide to conceal a pathfinding failure or as routine
zombie encounter recovery. Keep any intentional self-respawn behavior explicit in
kind/mode policy and permission-checked. Verify the reported inverted body separately
from genuine collider orientation, life transitions and cosmetic ragdolls.

### Stacking

Adapt the source behavior through authored bot capabilities, generic living-body
support contacts, bounded cooperation and ordinary jump/move controls. Zombie
content enables it; unrelated actors do not gain a name-based exception. Before
adding a seam, extend the existing motor/contact and bot-coordination owners.

Support queries must distinguish the actual top of a living body from a cosmetic
head mesh. Standing on a support is physically accepted by server movement and
agrees with prediction/replication. Coordinate approach and jumping enough to form
a useful pile without freezing every participant. Prefer an ordinary usable route
when appropriate, while allowing stacking as a real alternative.

Test one zombie as a negative control, sufficient allies for the cube height,
different body scales, low ceilings, pushing, supporting bots moving/dying,
reset/despawn and late observers. Death immediately revokes living support and
still leaves corpses nonblocking to living players. No jets, teleportation,
invisible ramps, artificial lifts or increased melee reach. A finite pile need not
reach an arbitrarily high target. Record actual supported positions and a legitimate
attack, with bounded CPU and stable state under crowds.

## Bot spawn frames and wrench events

Use one authoritative spawn-frame calculation for initial spawn, automatic respawn,
event respawn, type replacement, minigame reset and save/reload. Apply the brick's
authored heading to body, brain and first replicated pose before spawn events/AI.
Preserve the established source-backed placement; do not introduce guessed offsets.
Test all four brick rotations, a non-default body and two-client late join.

Complete a source-to-native event crosswalk for the supplied Bot_Hole revision.
Ship useful behavior groups through the existing event/operation pipeline:

| Group | Required behavior |
| --- | --- |
| Lifecycle | Brick power/respawn/type; Bot Off/On/Reset power meanings; existing spawn/death hooks. |
| Detection and search | Source search-radius/mode choices, native sight controls, search persistence, and the explicit tracking option below. |
| Movement | Wander distance/mode, return home, run speed, look at player/brick and go to named brick. |
| Identity and relationships | Bot name, hostility/team choices, and actual game-team assignment where applicable. Bot_Hole's Enemy/Friendly/Neutral/Owner/Custom relationships must not be confused with a Slayer team index. |
| Combat and presentation | Melee damage, weapon selection/removal, item drop, idle behavior, gestures and supported authored appearance choices. Preserve source meaning and existing native capabilities. |

Preserve Bot versus Player versus Self target contracts. For example, original
Bot-target `SetBotPowered` has Off/On/Reset choices; brick-target `setBotPowered`
is boolean. Native world-unit distance controls are not aliases for every original
list-plus-radius event. Imported rows need explicit semantic conversion or a precise
unsupported reason. No inert menu entries and no general TorqueScript VM.

Provide an explicit builder tracking control on the hole/spawner for **normal
perception**, **track a selected eligible player**, and **continually select eligible
enemies**. The final event name follows the established catalog. Live tracking is
declared information supplied by encounter policy; it can pursue through occlusion
but cannot shoot through walls or bypass damage, game/team and life eligibility.
Turning it off removes that information immediately. Revalidate target death,
disconnect, replacement life and game changes. Do not transmit hidden target
positions to clients as a side effect.

Acceptance follows wrench selection, typed admission, authorized event execution,
observable bot behavior, save/reload, resets and multiplayer agreement. Delayed rows
must not affect a replacement body. Use existing lifecycle regressions and add
missing contracts, rather than duplicate every control's implementation in tests.

## Lighting and materials

### Later Blockland core fixes

Use later releases as evidence for desirable core outcomes, while keeping v20
interiors, terrain and original content. The old official
[Change Log](https://blockland.us/Change-Log.html) stops at v20; it cannot establish
complete post-v20 coverage. The official
[Development board](https://forum.blockland.us/index.php?board=41.0) spans later
announcements through r2033. This audit read selected original posts across that
range; completing the release-by-release register is Phase 1 work.

| Primary evidence | Candidate contract to inspect in BRI |
| --- | --- |
| [r1715](https://forum.blockland.us/index.php?topic=206296.0) | Shadows remain coherent at short view distances; inspect bias and blending rather than copying old GL workarounds. |
| [r1728](https://forum.blockland.us/index.php?topic=207435.0) | Intentional self-illumination under shaders, distance-related shadow stability, robust malformed WAV input. This does not identify BRI's reported decal cause. |
| [Developer r1931–r1983 compilation](https://store.steampowered.com/oldnews/?appgroupname=Blockland+4-Pack&appids=250340&feed=steam_community_announcements&headlines=0&l=german) | Stuck animation on spawn, loading handoff, slow-player wall collision, ModTer surface coverage, missing-animation robustness and Bot_Hole command authority. |
| [r1985–r1986](https://forum.blockland.us/index.php?topic=302905.0) | Team-chat budgets and safe handling of script/text inputs. |
| [r1991–r1997](https://forum.blockland.us/index.php?topic=321223.0), [r2000](https://forum.blockland.us/index.php?topic=323839.0), [r2005–r2012](https://forum.blockland.us/index.php?topic=325765.0) | Bounded malformed network/text inputs and effect cleanup; assess equivalent Rust boundaries, not obsolete Torque patches. |
| [r2033](https://forum.blockland.us/index.php?topic=325961.0) | Loading must not freeze or disconnect from partially delivered oversized events. |

The register will classify each relevant core fix as already covered, reproduced
gap to correct, inapplicable old-engine mechanism or unresolved. Keep new modes,
unrelated GUI work and obsolete online services outside this core sweep. Do not
claim all later fixes were ported merely because the list was read.

### Issue 415 and the supplied shader writeup

[Issue 415](https://github.com/MaxHastings/BlocklandReImagined/issues/415) overlaps
the dark-texture report and requests later lighting controls plus a GLSL file-pair
loader. Treat its behavior descriptions and proposed implementation separately.
BRI already has typed direct light, ambient light, shadow color, fog and vignette
settings (`content::atmosphere`), a native renderer, and WGSL sandbox validation.
Audit their actual UI-to-world-to-final-pixel behavior before declaring them missing.

The attachment named `default_frag` uses `shadowSplitCount` and depth cutoffs to
select/blend shadow-map cascades. Four cascades are not a limit of four sun-cast
shadows. The attachment named `custom_frag` derives pseudo-bump contrast from texture
luminance; its custom treatment is an aesthetic reference, not verified stock behavior
or a sound universal height-map rule for faces, prints and arbitrary artwork.
Both files in the attached default pair are byte-identical to `renderCsm_frag.glsl`
and `renderCsm_vert.glsl` in the local Steam Blockland installation at
`S:/SteamLibrary/steamapps/common/Blockland` (Steam app 250340, installed build
5096106). That installation is supplementary read-only v21 evidence; the designated
v20 content reference is unchanged. Recovered client scripts corroborate the
shader presets and environment GUI variables, not the unavailable engine internals.

Recommended disposition: deliver lighting correctness and coherent native control
behavior, preserve original glow/particle distinctions, and document which existing
Add-On shader extension surfaces can express the desired customization. Correct
any actual missing capability through the established renderer/sandbox owners.
Do not add a second runtime shader language or recreate Torque's filename/global
variable loader merely to match the sketch. Any remaining incompatibility with
raw v21 GLSL is explicitly reported in the issue disposition and plan approval;
native behavior parity must not be described as drop-in GLSL compatibility.

Use original/reference shader evidence separately from Robbin's custom shader.
Compare representative dark-room textures, direct/ambient/shadow tint, fog,
vignette, shadow-cascade boundaries and intended emissive effects across supported
lighting modes. Source-game environment experimentation belongs to Maxwell; no
agent-driven original-game play session is authorized. Do not commit recovered
scripts or supplied original shader files.

### Lit transparency and intentional emission

Audit brick overlays/prints, avatar faces/shirts, transparent bricks, shape cutouts,
blended materials and imported map surfaces through both scene shader routes and
all supported lighting modes. Check authored flags, pigment alpha versus opacity,
sRGB/linear handling, ambient/exposure, local and sun light, shadow reception and
AO ordering. Ordinary pigment follows its underlying surface's lighting. Alpha
blending alone must not make it emissive. Authored glow/unlit/effect materials keep
their distinct behavior.

Use a dark room with zero and nonzero ambient, a switched local light, moving actor,
opaque reference, printed/translucent brick and intentional emitter. Compare final
pixels and transitions, not just material enum values. Verify shadows/decals through
MSAA, secondary views and relevant quality paths. Capture source-reference evidence
when needed without automating Maxwell's original game.

### BadDream and issue 413

[Issue 413](https://github.com/MaxHastings/BlocklandReImagined/issues/413) concerns
both a bright TV and its influence on surrounding objects. General renderer emission
already exists; imported DIF material binding and animated lighting need investigation.
`convert/src/interior.rs` explicitly skips animated-light and light-state arrays and
reports adaptation pending. The supplied converted `bd2.dif` has five `gsfstatic`
surfaces with flags 16 and lightmaps 0/1. A bounded prefix inspection of the original
DIF, following the converter's validated non-extended layout, finds one animated
light, four light states, 116 state-data entries and a 3,312-byte state buffer.
This strengthens the animated-light lead, but decoding its surface associations,
timing and actor-lighting semantics remains necessary.

Trace original DIF material flags, embedded animation/state/lightmap records,
mission light objects and shape effects. Determine separately what lights the TV
surface and what lights characters nearby. A bloom halo or emissive texture alone
does not supply scene illumination. Preserve original flicker/state timing if
present. Adapt verified source metadata into typed native materials/lights; authored
manual overrides remain separate and explain their evidence. Do not special-case
the texture's name in the renderer or add an invented always-on lamp.

Acceptance compares TV on/off timing and surrounding actor/static illumination in
Classic, Unified and Dynamic, records any deliberate mode difference, and checks an
unrelated lit map and intentional emissive surface as controls.

### Normal mapping and specular response throughout the world

Extend the existing native normal/material mechanisms to every world-surface family:
stock and Add-On bricks, slopes, prints/decals on their base material, avatars/bots,
items/weapons, vehicles, interior/static geometry and terrain. Water, sky, particles
and deliberately unlit surfaces keep appropriate specialized shading. UI textures
do not acquire world-space gloss. This is complete material applicability coverage,
not one arbitrary bump/specular setting applied to every image.

Use authored normal maps and roughness/specular parameters where supplied. For
original textures without them, provide reviewed material-family defaults and
optional derived detail overlays; preserve diffuse source images byte-for-byte.
Do not infer surface height from lettering, faces or arbitrary brightness. Bricks
retain their molded-plastic identity; metal, fabric and terrain remain distinguishable.
Maxwell selected subtle detail preserving the toy-like look for the first preview.
Normal maps modify lighting, not collision or silhouette. Actual cast shadows come
from geometry; additional relief/self-shadowing needs explicit reviewed support.

Validate tangent handedness, mirrored/repeated UVs, scale, slope orientation, normal
strength, color space, energy/highlight behavior, alpha and material Fx. A coverage
report must account for every shipped world material by family, selected profile,
detail source and explicit neutral/specialized handling. All families use the shared
system; missing authored maps have a defined fallback. Compare a small coherent
gallery before rolling settings across the corpus. Measure frame cost, memory and
pack size, including low-detail settings. Maxwell accepts the resulting appearance.

## Accumulating movement delay

Maxwell reports subtle walking/jetting delay after many minutes or hours of ordinary
play. It remains until Ctrl-K/respawn or F8/F7 restores normal feel, then can recur.
Mouse/camera delay and host-versus-guest occurrence are not established. A network
cause remains a hypothesis. Do not substitute periodic resets for a correction.

The [October 14 investigation](../progress/2026-10-14-host-stall-move-refusals.md)
found a matching symptom and persistent movement refusal after host stalls. Its
skipped-tick credit correction (`207c819ef`) and dropped-input acknowledgment
correction (`d7721dd72`) remain in current source. Read their actual evidence and
challenge the limits of their tests before proposing another fix.

Second-pass read-only analysis of recorded calls found:

| Recording | Final simulation tick divided by 120 | Movement-rate refusals |
| --- | ---: | --- |
| match-131 | 13.61 minutes | 0 |
| match-132 | 65.51 minutes | 257,398 for guest owner 28, recurring from tick 263937 to 454333 |
| match-133 | 46.47 minutes | 0 |
| match-134 | 91.03 minutes | 229 across four owners in a short burst near tick 417100 |
| match-135 | 12.90 minutes | 0 |

These are recorded admission outcomes, not a fresh replay or input-to-screen
measurement. Final ticks describe simulation time, not exact wall-clock durations.
The sustained guest failure is concrete evidence in the shared input path, but is
not yet a reproduction of Maxwell's host character feeling sluggish. Current text
logs mainly record rendering/frame timing and do not expose the required movement
state. Good FPS or zero admission failures cannot establish responsive movement.

Inspect the sustained failure's onset and recovery, unique accepted sequences versus
redundant attempts, clock skips, per-player allowance/refill, server input queue,
drain pacing and consumed acknowledgments. Correlate body/control transitions and
map/reconnect changes. Compare exactly what respawn and F8/F7 reset on both ends:
prediction history, sequence baselines, accumulator, clock estimate, correction
offset, effective movement tuning, motor contacts and control claims. Retain normal
anti-speed/flood limits and deterministic recording semantics.

If existing recordings cannot reveal the client side, add bounded lightweight
diagnostics to the private preview: a short ring of movement samples and an explicit
capture marker before the workaround, plus a paired sample after recovery. Include
queue/ack age, sequence gaps, allowance/refusals, prediction corrections, client and
server pacing, frame/input timing, body/control identity and relevant state changes.
Keep it off expensive per-frame logging and avoid collecting unrelated chat or secrets.

Acceptance needs the captured bad state or a supported failing reproduction, a
correction that addresses its actual cause, bounded deterministic controls and an
extended session exercising relevant stalls/jitter/control/life transitions. Keep
local and remote clients as controls. An hours-long server-only soak cannot establish
local input feel; Maxwell must confirm the same accumulated symptom no longer recurs
in normal play. Record any retained inability to reproduce rather than claiming a
fix from isolated tests. This report remains a release requirement unless Maxwell
explicitly changes that scope.

## Destroyed brick lights emitters and spawners

Maxwell reports that lights and particle emitters stay behind when their bricks
are blown up. Current cosmetic attachment construction uses the original brick's
center; moving debris transforms live in a separate system. The item-spawner
reconciliation path also needs checking for destruction eligibility.

Associate each brick's cosmetic attachments with its current visual debris frame,
including source offsets, emitter direction and light/flare orientation. Reuse
existing stable brick/debris identities and lifecycle owners. One attachment follows
one appropriate parent; do not multiply lights or emitters across all fragments.
Particles already emitted remain in their authored world/attached frame; moving the
emitter must not teleport an existing smoke trail. Local cosmetic debris motion
does not move server gameplay state or add per-frame physics replication.

On debris expiry, capacity culling or disabled debris presentation, remove/suppress
the detached sources coherently instead of leaving a lamp or emitter floating at
the original location. Restore exactly one source at the authoritative brick frame
on respawn; permanent removal, Undo, map changes and late joining must not leave
or duplicate effects. Verify ordinary explosions, fake-kill events and the existing
moving-section path without introducing a new fracture system.

Recommended item policy: while a brick is destroyed, suspend its static pickup and
future spawn production; resume only when that brick actually respawns. Preserve
its authored settings and existing pickup due-time. A due-time can elapse while
suspended, but cannot produce a pickup before the source brick returns; a remaining
delay still applies after restoration. Do not reset every timer merely by reconciling
the hidden brick. Check recovered original lifecycle evidence before finalizing any
deliberate behavior difference.
Items already independently dropped remain governed by their normal lifetime.
Trace vehicle/bot spawners and brick sound attachments as adjacent consumers, fixing
or explicitly accounting for any same-lifecycle gap. Merely nonrendering bricks
remain distinct from destroyed bricks: do not disable legitimate invisible
spawners/effects by testing `visible` alone.

Acceptance follows a light/emitter brick through movement, rotation, rest, fade and
respawn with an observer; checks direction and a fixed reference light; verifies no
old-position emitter, fragment multiplication or duplicate restoration; and exercises
item pickup/respawn during destruction plus permanent removal and late join.
The existing static/moving effects and item checks should supply reusable controls.

## Death collision hammer animation and audio

### Death collision and issue 400

Reproduce the lingering blocker with authoritative collider/contact diagnostics
across weapon death, suicide, bots/humans, mounted/scaled bodies, body changes,
ragdoll on/off, late join, timeout and respawn. Compare host movement with client
prediction and distinguish the old gameplay collider from cosmetic ragdoll bodies.
Living-player movement must stop colliding at canonical death and resume ordinary
collision only for the new live body. Preserve intended weapon/vehicle/grab behavior
instead of making every corpse interaction a sensor. Stacking support must revoke
on death in the same transition.

[Issue 400](https://github.com/MaxHastings/BlocklandReImagined/issues/400) is a
separate intermittent soft-cone settling test. Reproduce with its exact fixture
in isolated and bounded concurrent runs, log initialization/contact/solver ordering
and establish why results differ. Preserve finite state, actual falling and joint
attachment requirements. No threshold widening, skip or retry-only green result.
Do not close the gameplay collision report by fixing this test or vice versa.

### Hammer flicker on ramps

Use actual 45-degree ModTer slanted-brick motor motion with an equipped but unfired
hammer. Maxwell specifies ragdolls enabled and no corpses present; reproduce that
combination before treating ragdoll interaction as a cause. Trace
grounded state, locomotion/action transitions, weapon ready/action layers, image
states, item socket transforms and replicated pose timing. Compare idle, uphill,
downhill, stair edges, crouch, other tools and a genuine swing in first/third person.
Fix the earliest incorrect state or composition rule. Do not smooth the held item
to mask an authoritative oscillation. Acceptance requires stable unswung presentation
and retained genuine action timing locally and for an observer, followed by Maxwell's
same-ramp playtest.

### Requested gain changes

Raise the approved hammer/wrench swing swishes by **+6 dB relative to v0.4.2**
(amplitude factor about 1.995), retaining all three accepted variations. Extend to
sword swinging through its actual swing/image action if supported, without replacing
the sword's contact cues or duplicating swing playback. `tools/tool_swishes/` and
`prepare_tool_swishes.py` already record source, processing and policy; extend these
existing recipes. Check the sword binding and sound overlap before selecting the
final mix. Keep original hit sounds and timing.

Raise the indoor/outdoor ambience beds by **+4 dB RMS relative to v0.4.2**
(linear gain about 1.585). Apply consistently where those beds run, explicitly
checking Bedroom and Slate Desert and indoor/outdoor transitions. Preserve the
accepted loop edits and relative balance. Audit Storm/weather and localized fridge/
waterfall/shore layers so they are not accidentally boosted twice or swept into an
unrelated audio overhaul.

The installed swishes all peak at -12.00 dBFS; +6 dB would put their sample peaks
near -6.00 dBFS. The installed indoor/outdoor bases measure about -30.87/-31.58 dBFS
RMS and -15.84/-10.71 dBFS sample peak; +4 dB keeps sample peaks below full scale.
These are offline measurements, not true-peak or combined-mix acceptance.
Add-On sound-profile volume is restricted to 0–1 and the current profiles use 1.
Prefer raising the generated derivative waveforms through the existing recipes,
updating receipts while retaining originals and approved loop edits, over inventing
an unrelated engine gain feature. The two ambience masters currently record +12 dB
relative to the approved unity loops; this request makes +16 dB total, not +4 total.
The sword's Fire state is empty while Activate has its draw sound, so the existing
augmentation mechanism has a plausible swing-only slot without replacing draw audio.

Verify
finite samples, peaks/headroom, measured RMS change, loop seams, fade targets and
the actual limiter/mixer result under repeated swings plus ambience, music and
voice. The intended gain change should remain audible without clipping or broad
ducking. Produce before/after listening samples and the playable preview; Maxwell
and friends judge audibility on their normal sliders.

Maxwell subsequently clarified that the goal is meeting ordinary player
expectations and balancing the whole soundscape. Treat +6/+4 dB as the requested
starting points, with evidence-backed final mix adjustments permitted. Speech,
important impacts and combat cues must stay readable against the ambience; exact
isolated gain arithmetic does not establish acceptance of the combined experience.

During execution Maxwell confirmed the original ambience WAV exports are no
longer available and explicitly authorized using the v0.4.2 OGGs. The preserved
release clips are now pinned immutable generation inputs. Keep their derived
identity separate from the historical original-recording approval lineage, retain
the existing loop seams, and regenerate gain edits from that fixed baseline.

## Other GitHub issues

The repository-wide open-issue query initially returned 400, 412 and 413. Maxwell
subsequently added the newly opened 414 and 415. All five have been read, including
their comment threads at inspection. Re-query before the release candidate; new issues
receive explicit triage rather than silently expanding work forever.

### Music gain issue 412

[Issue 412](https://github.com/MaxHastings/BlocklandReImagined/issues/412) is confirmed:
`MusicPlay` lacks volume, script options reject it, and the client uses asset gain
without a per-play multiplier. Add finite per-play gain defaulting to 1 through
script operations, validation, authoritative notices, transport and audio playback.
The final level is asset gain × requested gain × the player's Music slider × master.
Keep music on the Music bus. Apply the same gain to initial playback and fade target;
test silence, fades, replacement, looping and non-looping tracks without an initial
audible burst. Define out-of-range handling consistently with ambient and reject
nonfinite inputs. Inspect any retained/replayed music state as a consumer.

Add the protocol change record/version increment when notice representation changes.
Alpha has no mixed-version compatibility requirement; a serde default does not make
old clients accept a new strict field. Existing scripts omitting volume retain their
behavior. Do not route music through ambient as a workaround.

### Cross-package brick import issue 414

[Issue 414](https://github.com/MaxHastings/BlocklandReImagined/issues/414) has a
confirmed structural gap: brick geometry lookup checks local source/reference;
`loadable_bricks` can continue without an unresolved entry or return with zero
bricks. The checker's full false-success chain still needs a fresh reproduction.

Maxwell supplied `E:/Downloads/Bot_Town.zip` after the temporary source hold.
Its eight brick definitions explicitly reference `Add-ons/Bot_Hole/4xSpawn.blb`,
and it contains all eight icons. `server.cs` names Bot_Hole through
`LoadRequiredAddOn`; the supplied Bot_Hole archive contains that exact shared BLB.
Thus the eight-brick expectation is supported by the actual source, rather than
only the reporter's manually repaired package. That repaired package is not needed
to establish the original references and is not a prerequisite to a fresh import.

Resolve geometry from declared dependency inputs with canonical virtual paths,
provenance and hashes. Reuse `borrow_brick` and the existing local native-copy
pattern where possible. Avoid scanning arbitrary installed packages by basename
or selecting whichever one happens to appear first. Missing, ambiguous, cyclic or
untrusted paths receive precise diagnostics. Make import results reproducible on a
second machine and when the upstream geometry changes.

Generate catalog, mesh/audit/collision bindings and icons together with clear
per-brick outcomes. A valid icon can survive partial conversion, but must not make
an unusable brick appear playable. Align `bri-addon-check` with the runtime definition
resolver; report **0 of 8 bricks loadable** prominently and fail readiness for a
package claiming bricks with none loadable. Partial packs explicitly name usable
and unusable entries; raw `bricks.json` cannot substitute for a loadable catalog.

Reimport unmodified Bot_Town with its actual Bot_Hole dependency, compare all eight
bricks, plant/collide, save/reload and late join using a clean generated package.
Keep the reporter's repaired package as comparison evidence, not the importer input
or a golden file silently trusted as source. Add a public invented dependency
fixture for CI. Exercise absent dependency, renamed/case-varied path, conflicting
providers, changed hashes and icons without mesh. Protect all source installations.

## Saved world bot acceptance set

All five saves were located and their binary headers and packed brick data read
without loading the game.
They use native saved-build schema 2, include minigames and name
`v20/add-ons/map_bedroom/bedroom.mis`. Source files remain untouched and private.
The screenshots describe symptoms; they do not prove route clearance or bot sight.
The cube contains three zombie holes; the staircase contains three holes and 75
2x8 stair bricks with predominantly 0.6-unit rises. Navigation already models
body-specific stepping and multiple elevations. Capture the actual collision and
route trace before concluding the stair geometry is impossible or needs more jump.

Directory: `%LOCALAPPDATA%/BlocklandReImagined/saves/`
`map-4d73e668253c06fd87db449cb2dbd43648630b0a81c19e36033389f799aaa1e0/`.

| Save filename | Bricks | Required outcome |
| --- | ---: | --- |
| `MattressHouseZombiesCan'tGetMe.world.json` | 17,518 | Find a valid interior route to the balcony; independently assess physical stacking as an alternative. |
| `ZombiesCantGetMeBeta16CityTower.world.json` | 8,528 | Enter the tower and climb its valid stairs to the player; stacking cannot substitute for passing the stair route. |
| `ZombieCan'tGetOutOfHouse.world.json` | 158 | Exit through a usable opening, recover from the blocked/small opening and crowding, and explain any self-kill or inverted pose. |
| `ZombiesHaveTroubleGettingMeStairCase.world.json` | 82 | Find and climb the switchback from an elevated-target start and a witnessed chase; prefer usable treads/landings over edge hugging. |
| `ZombieCan'tGetMeOnTopOfCube.world.json` | 5 | Physically stack living nonjet zombies to reach the cube top and perform a valid attack. |

Exact SHA256, in the table's order:

```text
cf5c923fa056a8c866d78d76e0a5cc8d41094764da05b795b41c29a8fb8ef76e
468c390587b333fbd93acc506139fa95265457d0a679dc2404615afc4728b37b
e3a06814be1084f7e3e855601a27a5cbb8124498f4ee8ca203e8fc01406d1386
eab0e30791f4f77c786f17b4b21cdde09ae0166d1a6bc3cb123b63560430b510
c4bdf81af809bcc02781bcdf490a7a36f1df5a0f96ff3a69b648991e685806b6
```

Maxwell's reported player placements, native Y-up:

| Case | Tick | Feet | Camera | Yaw and pitch radians |
| --- | ---: | --- | --- | --- |
| Mattress house | 9948 | [139.2500, 313.2000, 166.7500] | [139.3010, 315.3565, 166.6184] | 0.370010, -0.690797 |
| City tower | 28584 | [120.9194, 315.6000, 74.8856] | [120.9164, 317.7565, 75.0267] | -3.119982, -0.955797 |
| House exit | 46404 | [103.4420, 286.8100, 95.8493] | [103.3009, 288.9665, 95.8440] | -1.533160, -0.140797 |
| Staircase | 71412 | [98.8623, 332.2019, 81.6137] | [98.7939, 334.3584, 81.7371] | -2.635524, -0.950690 |
| Cube | 87228 | [90.3241, 294.6100, 98.5718] | [90.4652, 296.7665, 98.5722] | 1.572950, -0.751822 |

All reports identify owner 1, grounded true and crouched false on v0.4.2. Reported
ticks are observation points, not reproducible simulation seeds; saves are not
complete live-match replays. Capture actual Add-On hashes, settings, bot kinds,
spawn transforms and initial target evidence in each reproduction. Use an isolated
state directory and copies of saves for headless investigation. Preserve the supplied
screenshots with the private evidence, outside committed original content.
Saved minigame configuration is not the full enabled Add-On environment. The current
selection has several movement/interaction Add-Ons and may differ from the original
report. Retain an exact native-Zombie baseline and the observed combined configuration
as separate controls; current installed choices alone do not prove past activation.

For each case record time to first useful movement, route entry, floor/landing
progress, first legitimate attack, path work, stalls, deaths and cause. Establish
bounded deadlines from measured route length and actual movement capability before
running corrected acceptance; do not keep increasing timeouts until it passes.
Controls include target relocation, temporary sight loss, blocked versus open route,
one versus several bots and a truly unreachable height. Ordinary controls and
authoritative motor/damage determine success. Maxwell's visual/feel acceptance is
recorded separately.

## Evidence inputs and remaining investigation

The supplied `E:/Downloads/Bot Add-Ons.zip` contains the same Bot_Hole and Zombie
copies documented in [the prior source audit](../progress/2026-10-23-supplied-hole-bots.md).
Read them in memory; never execute or commit their original scripts.

| Input | SHA256 |
| --- | --- |
| Bot_Hole BL v21 r2024 nested ZIP | `500b917ca1f62f71e5a4c43f8c1097478e71d974e9adddb9cd04d88a3aa80220` |
| Bot_Zombie BL v21 r1865 nested ZIP | `002a9e6da08e9841a29c203908b89d23b0ee11aee6102b009079c995e9782bcf` |
| Bot_Town supplied ZIP, 66,470 bytes | `c4e9a0123a62486c01a9c1f15c0dfd0ab65bb3ea399fa77379cde7628285ce2d` |
| Issue 413 original attachment ZIP | `03423a061cd195fdb243fe1fe18647c80ed5849650fe70dd7c153a5f68e5d9c9` |
| Its nested Map_BadDream.zip | `14a69475f8b3ded887793eebabf26393b761984890ff0f7b11fb472adc2e5480` |
| Issue 413 converted attachment ZIP | `3a88c3ed2bf7e0abcf24440f84ba88847adcd8232f963bd2c77f731a7563e713` |

Useful original bot locations: `holes.cs` 1868–1870 applies spawn rotation;
`packages.cs` around 162–194 conditions stacking on contacts and an elevated
followed target; `bot_zombie.cs` enables the capability. `events.cs` contains actual
search/wander choice lists and the power/team/name/weapon/appearance contracts.
Use complete functions when implementing, not isolated matching lines.

Still unresolved: exact failure stages in each saved world, the local movement-delay
cause and its relationship to the recorded guest refusal, the particular lingering
collider, hammer flicker's originating state, original TV state/illumination semantics,
the root of issue 400, complete issue 415 control/render-path behavior, and the
complete post-v20 core-fix crosswalk. These are
bounded first implementation investigations with the acceptance outcomes above;
none is an assumed waiver or an established diagnosis.

## Validation playtest and publication

Use an existing warm clean checkout based on current main, preserving other threads'
work. Keep CARGO_TARGET_DIR unset and use `--profile playtest` for private builds.
Before changing shared mechanisms, confirm the current seam owner and update the
existing seam declaration or add an owned row. Keep package policy out of engine
mechanisms, conversion outside runtime dependencies, and generated assets separate
from manual overrides. Add schema/protocol versions and change records when their
representations change; no alpha migrations or backward-compatibility project.

Each increment runs the smallest meaningful changed-crate formatting, warning-free
Clippy, unit and relevant direct-consumer/integration checks. Inspect the actual
focused plan with `python tools/check.py plan focused`; use explicit integration
targets for network voice, event execution, navigation, motor support and rendering.
Affected content gets a freshly verified generation receipt and `bri-client --check`
against the actual preview. Record failures and flaky retries honestly.

Ship each private playable folder after changed-area checks and package startup.
Do not delay it for ZIPs, LTO or merge-only checks. Maxwell performs all gameplay,
microphone, feel and listening acceptance. No local GPU suites, soaks or benchmarks
while he is playtesting. Each preview includes exact source/content identity, short
scenario instructions and a small list of questions it resolves.

The final candidate warrants the extended release gate because the combined scope
changes connection authority, wire/save state and shared movement/support behavior.
Run it once the integrated candidate is ready, not on every tuning change. Require
passing exact-source platform CI, history/protocol checks, fixed-save coverage and
the relevant unchanged performance budgets. Measure the five new saves as saved-world
consumers, plus representative bot crowds; record simulation timing separately from
render FPS. Keep the five-second first map/server load and three-second repeated
load targets visible with actual phase timings and correct content invalidation.

Publication checklist:

- [ ] Every requested item has acceptance evidence; any proposed deferral is explicitly accepted by Maxwell.
- [ ] Both ordinary route pursuit and physical stacking pass their separate saved-world cases; builder controls remain useful with the native AI.
- [ ] Loading/gameplay conversation privacy and creator-departure resource ownership pass actual network lifecycle tests.
- [ ] Dark lit surfaces and intentional emitters both work; material coverage and BadDream results are reviewed.
- [ ] Death collision and unswung-hammer flicker on 45-degree ModTer slopes have reproduction and corrected integrated evidence.
- [ ] Accumulating movement delay has a supported cause/correction and long-session acceptance; the recorded guest refusal is separately explained.
- [ ] Destroyed-brick lights/emitters follow their debris without duplicate or orphan sources; authoritative spawners suspend and restore correctly.
- [ ] Requested swish/ambience gains and music gain preserve sliders, fades, headroom and accepted sound identity.
- [ ] Issues 400, 412, 413, 414 and 415 have the agreed outcomes on the shipped source, with GLSL compatibility disposition explicit and new issues triaged.
- [x] Maxwell tried preview.1 and reported it seems fine so far; he waived further playtests and requested continued autonomous execution. This is limited preview feedback, not human acceptance of future features.
- [ ] The combined final candidate passes the remaining integrated automated and offscreen acceptance checks; validation limits are explicit.
- [ ] Required CI, extended release gate and guarded history/protocol checks pass; pre-push hook remains enabled.
- [ ] Windows x86-64, Apple-silicon macOS and Linux x86-64 packages are built and verified at the same source, including current macOS microphone permission packaging.
- [ ] Actual extracted Windows package starts with isolated state and `--check`; all archives have correct source/version stamps, manifests and SHA256SUMS.
- [ ] Release notes distinguish fixes, modernization, source comparison, technical checks and human acceptance; existing published tags/assets stay immutable.
- [ ] Published assets and their digests are independently verified; finished worktree build output is cleaned under the existing guarded policy.

Recommended approval contract: approval of this plan authorizes implementation,
private previews and publication once these conditions pass, without repeated
routine permission requests. Material scope changes or unresolved acceptance
failures return to Maxwell; they are not silently deferred to make a release.
