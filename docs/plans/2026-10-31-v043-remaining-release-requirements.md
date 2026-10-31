# v0.4.3: remaining outcomes and release evidence

This is the practical end-goal checklist for the approved v0.4.3 work. The
[detailed plan](2026-10-30-connected-sessions-bots-and-lighting.md) retains
the technical crosswalk and source evidence; this checklist makes the remaining
player outcomes and the evidence needed for release explicit. It does not
assert that unchecked work is unstarted. Maxwell tried preview.1 and waived
further interactive playtests; use automated, headless and offscreen evidence
and report what has not had a human playtest.

Maxwell's investigation boundary: when a report remains vague and the initial
trace reveals no concrete defect or poor design, record it as incomplete and
move on. Make a well-supported correction or worthwhile general cleanup when
the evidence warrants it; do not keep guessing or claim an unverified fix.
Carry any unresolved release outcome into the final handoff explicitly rather
than silently marking it complete.

## Bots, objectives and saves

- [ ] **Pursuit and navigation:** Bots keep pursuing visible elevated players,
  use viable entrances, doorways, ramps, stairs and landings, and can move away
  from a target when the route requires it. Brief lost sight or a jump does not
  discard pursuit. They search reachable connected space using remembered
  observations, without secretly knowing unseen players' current positions.
  Search, replanning and caching stay bounded as the build or bot count changes.
- [ ] **Evidence:** The supplied mattress house, Beta16 tower, house exit and
  staircase saves load without set-aside bricks and produce actual target
  damage/death with ordinary controls; verify the original house-exit position
  and farther control. Keep tests for floor selection, flag pickup/return and
  the edge and center positions in the Beta16 tower. Check meaningful route
  quality, collision and recovery, not merely a path's existence. Unseen-player,
  lost-sight, impossible-route and world-edit cases remain separate controls.
- [ ] **Cube:** The practical requested outcome is that the saved cube's zombies
  can kill the player on top without jets. The current candidate does so in a
  headless replay using real support contacts and ordinary melee/jump controls;
  finish regression and safety checks before counting it done. Preserve ordinary
  routes, correct support loss on death and explicit bot policy. This does not
  promise arbitrary-height piles, synchronized boosts or perfect behavior on
  every moving support. Do not build an open-ended cooperation system merely to
  satisfy an implementation idea if the player outcome is met safely.
- [ ] **Bot controls and events:** Audit useful modern Bot_Hole/Bot_Zombie
  events against the existing NPC system, including lifecycle/power, search and
  wander ranges, team, name and the explicitly authored exact-player-location
  behavior. The exact tracking event obeys game/team/life permissions and can be
  turned off; ordinary bots remain evidence-driven. Verify execution,
  save/reload, replacement lives and multiplayer where applicable. Bot spawn
  rotation is implemented; retain its initial/respawn/lifecycle tests.

## Voice, chat and minigames

- [ ] **Loading communications:** During accepted Add-On download and map load,
  voice and eligible minigame chat remain usable. The audience is the authorized
  destination game when known, otherwise the lobby. Preserve privacy, mute,
  push-to-talk and disabled-microphone rules. Check real accepted connections,
  bidirectional audio, loading-to-game transition, map change, cancellation,
  reconnect, revocation, and continuity of actual audio samples, without
  doubled playback or audio leaking to another game.
- [ ] **Loading controls and display:** Page Up/Down scrolls chat, F2 shows the
  live roster, Ctrl+O opens options, and none sends gameplay controls while
  loading. Speaker names appear in a horizontal, bounded row. The reported
  connection/loading exit must not recur in the reproduced closed-channel case;
  preserve diagnostics for any separate unexplained crash.
- [ ] **Global voice:** A minigame can select Global, and every eligible member,
  alive or dead, can talk to the others regardless of distance. Proximity and
  Team remain available with their own eligibility; changing mode, mute, team
  or game takes effect without stale speech crossing a boundary. Verify saved
  settings and permissions.
- [ ] **Modder voice seams:** Provide the already-scoped bounded, authorized
  routing and processing controls needed for add-ons to build effects such as
  a radio. Verify permissions, resource limits and lifecycle behavior. Modders
  create the effects; a library of presets or a new audio framework is not a
  release requirement.
- [ ] **Creator leaves:** The creator can leave their own minigame while it and
  its bots/resources continue, can manage or rejoin it, and cannot participate
  in that game's combat while outside. Other players cannot take over without
  permission. Check zero-player continuation, another minigame, reconnect,
  reset/end and saved/restarted state. Core departure and management already
  have headless, UI and network coverage; remaining lifecycle combinations
  and persistence still need evidence.

## Movement, death and animation

- [ ] **Accumulating movement delay:** Retain the verified input-admission fix
  and investigate the separately reported slow-onset local movement feel that
  clears after respawn or F8/F7. Use logs, prediction/body diagnostics and a
  failing or otherwise persuasive before/after reproduction. Do not call the
  local symptom fixed solely because the guest admission bug was fixed, and do
  not mask it with periodic resets. If its cause remains unknown, explicitly
  report the gap instead of claiming proof.
- [ ] **Death collision:** Living players stop colliding with the obsolete live
  player body immediately on death, including with ragdolls enabled, and normal
  collision returns for the new life. Test host and prediction, varied death
  paths, respawn and support loss. Keep intended corpse interactions intact.
- [ ] **Issue #400:** Investigate the separately reported intermittent soft-cone
  ragdoll settling failure using its real fixture and bounded repeated runs.
  Establish the cause and stable behavior without wider tolerances, arbitrary
  skips or a retry-only pass.
- [ ] **Hammer on ModTer:** With ragdolls enabled and no corpse present, holding
  an unswung hammer while walking up 45-degree ModTer ramps should not flicker.
  Check the actual animation/state chain and first-/third-person and observer
  views, including real swings, so a fix does not suppress legitimate actions.

## Destruction, lighting and materials

- [ ] **Destroyed-brick attachments:** A light, emitter and applicable audio
  attached to a destroyed brick follow the correct moving debris and orientation,
  stop or fade at the correct time, and restore once when the brick respawns.
  Item/actor spawners do not create new items or actors while their source is
  destroyed; independent existing drops keep their own lifetime. Verify fake
  kills, fragments, visibility-only changes, permanent removal, late join,
  network observers and effect output. Carrier identity and source lifecycle
  have focused coverage; final integrated visual/effect checks remain.
- [ ] **Lighting and transparency:** Ordinary decals, alpha-blended surfaces
  and characters respond correctly to dark ambient and direct lighting rather
  than looking unintentionally full-bright. Preserve intentional emissive
  materials, alpha blending and recognizable original art. Use source-backed
  v20/v21 comparisons and offscreen dark/bright controls across surface families.
- [ ] **Issues #413 and #415:** Investigate BadDream's animated light/TV case
  and meaningful later-version core lighting/rendering differences. Implement
  justified underlying capabilities and controls with validated examples;
  do not blindly copy a proposed shader or claim complete v21 fidelity without
  evidence. Keep rendering cost, compatibility and known limits explicit.
- [ ] **Normal/specular detail:** Provide coherent, subtle normal/bump and
  specular response for applicable brick and other world material paths,
  preserving the original toy-like texture identity. Materials without authored
  maps need sensible defaults. Validate representative textures, dark/direct
  lighting, transparency, visual consistency and performance; do not claim every
  asset has unique authored maps when it does not.

## Audio, imports and GitHub issues

- [ ] **Sound balance:** Retain the approximately +6 dB hammer/wrench/sword
  swing target and +4 dB ambience starting point, balancing against music,
  speech and other effects with headroom and without duplicate cues. Indoor
  ambience stays as chosen; the outdoor base uses Maxwell's supplied car-park
  recording converted to OGG. Validate actual bindings, loops, gains and
  combined mix; don't alter the original source files.
- [ ] **Issue #412:** Keep per-play music volume working through scripting,
  transport, fade/loop and the Music slider, with default behavior preserved.
  The implementation has focused checks; retain protocol/history and packaged
  integration evidence.
- [ ] **Issue #414:** Import the supplied unmodified Bot_Town with its declared
  Bot_Hole dependency; all eight referenced bricks must resolve, appear, plant,
  collide and survive save/reload and late join. Missing/ambiguous dependencies
  get actionable diagnostics and never produce a false-ready package.
- [ ] **Issues #416 and #417:** Audit the newly opened window/resolution and
  automatic draw-distance reports against current behavior, source fidelity,
  actual monitor capabilities, performance and usability. Fix demonstrated
  gaps with a maintainable design and test relevant platforms. An aspect-ratio
  dropdown or indoor-distance heuristic is a suggestion to evaluate, not an
  automatic requirement; document any deliberately rejected approach.
- [ ] **Other open issues and reported logs:** Recheck GitHub before release,
  triage newly opened issues using product judgment, and account for all agreed
  release work. Preserve the friend's restart and connection-exit evidence;
  fix confirmed defects and distinguish an unknown underlying platform failure
  from an established cause.

## Final release

- [ ] Integrate the above without losing current main or unrelated work; pass
  changed-area tests and warning-free checks, and run the deliberately extended
  release validation for shared physics, protocol, content and packaging.
- [ ] Pass current platform CI and history/protocol checks. Verify generated
  content, startup and actual Windows, Linux and macOS release archives on the
  available platform/CI paths. Report any failures or limitations honestly.
- [ ] Produce release notes matching the verified behavior; publish immutable
  GitHub tag/release **v0.4.3**, then independently verify the published assets
  and digests. Do not mark the goal complete until these checks and all agreed
  outcomes are satisfied. The broader alpha contract remains open.
