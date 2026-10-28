# 2026-10-28 Afghan bots, route breaching and destruction hitches

Maxwell requested an integrated bot increment, an isolated ten-minute headless
match using his saved Afghan scene, a warm Windows playtest and merge after
scoped checks/CI. He then added tank traps, flag-carrier distractions, stairs,
crawl spaces and preview10 lag logs. His latest direction is to fix common,
obvious core problems, without chasing exact screenshot reproductions or
perfection. No interactive game input/window was used. Source installs, live
preview10 and his saved build remain unchanged.

## Decisions and implementation

- Zombie holes expose generic read-only `bot_brick`/`bot_spawn_team` context to
  rules. Slayer owns the binding: explicit wrench Team wins, otherwise a unique
  matching team control/paint color, otherwise ordinary auto-sort. Joining,
  spawning and team-definition changes use the same policy. Nine yellow holes
  use team 3's empty kit, including actual death/respawn; no Zombie-name or
  Team-3 condition is in the engine.
- Installed Zombie data lacked `weapons:false`, despite source hShoot=0 and
  maxWeapons/maxTools=0. Regenerate the real native pack and bundled copy as
  well as Slayer's host policy; updating runtime source alone cannot fix it.
- Short hop/dodge jets use actual available fuel, independently of whether a
  complete navigation flight is possible. Store the admitted burst duration;
  recharge cannot extend an already checked landing. Route flights retain their
  measured reach/fuel checks. No-Jet stays excluded; native thrust/drain remain.
- Breaching is a local pedestrian route action through the existing act arbiter,
  normal equipment/trigger and post-movement friendly/self-harm admission.
  Straight, immediate explosive projectiles with understood capabilities are
  supported; arcs, fuses, charge/release and unknown scripted weapons are not
  guessed. There are two shared local probes/tick, a 24-unit search reach,
  eight-second attempt deadline, at most three presses paced by the image's
  authored cycle, and a twelve-second retry rest. Claims expire with attempts.
  Permission/protected/indestructible checks and damage execution remain shared.
  A clear eye ray is insufficient: require the ordinary body's walkable corridor
  before declaring an opening, then observe actual approach to the exit.
  The normal navigator still owns the route. No hypothetical destruction graph,
  direct brick deletion, map-name exception or new network state was added.
- Navigation failure now has a short three-second action retry, separate from
  the sixty-second rule/action failure blacklist. The old shared blacklist made
  flag carriers lose their valid delivery priority after a failed route and
  spend long stretches chasing enemies. Planning budgets and real completion
  checks remain intact. Crouched waypoints request crouch on approach, retaining
  the existing stand-to-jump exception.
- Alpha fades reuse a verified vertex paint mask inside one blend class, instead
  of rebuilding ordinary brick geometry every frame. Rebuild at the opaque/blend
  boundary and retain the fallback for appearance that cannot safely be reused.
  Debris still runs at 120 Hz; body limits and visible material were not reduced.

## Exact inputs and source evidence

Read-only private save: %LOCALAPPDATA%/BlocklandReImagined/saves/
map-f769ab03668dc20cac8decfc839a9c0a972bc911abbabf126a4c9048a777eaa4/
TDM Afghan with Zombies.world.json (BRI-BUILD v1).
Nine intrinsic Zombie holes, IDs 14108..14116, color 3, no wrench Team override.
The saved mode is Slayer_CTF despite the TDM name. Teams 1/2 prefer eight players
and carry spears/swords/HE/rockets; team 3's unsynced kit is empty. Saved player
bodies are No-Jet, so limited jets require separate actual-tuning coverage.

Supplied Bot Add-Ons archive contains Bot_Zombie BL v21 r1865, pinned source hash
002a9e6da08e9841a29c203908b89d23b0ee11aee6102b009079c995e9782bcf.
The listed Zombie port applies. Slayer's Steam copy matches pinned e1996270...
and its port applies at native version 4.1.5. The vanilla recovered core scripts
and secondary read-only Steam Bot_Hole dependency resolve the mod's geometry.
No original source/content enters Git. Regenerated content and the private bundle
are under ignored artifacts/zombie-team-jets and content/.

## Evidence and limits

The first valid 600-second native trace used the saved CTF rules and a passive
owner: 24 bots (7 + 8 team-fill bots and 9 hole Zombies). Zero bot-step failures;
1,634 projectile observations, 385 deaths, four enemy-flag pickups, one capture
and two own-flag recoveries. The early breach implementation recorded 279
attempts, 154 presses, 84 clear-eye/exit observations and 17 approaches through
opened walls. Final corridor/traversal criteria are stricter. Of 368 carrier
sample-seconds, only 192 were Objective; 61 Fight, 42 Chase and 30 Search samples
reported unsupported semantics after route failures. This exposed the blacklist
problem above; it was not solved by merely increasing the combat priority.

All 5,391 post-join Zombie samples use team 3 and empty inventory. First join
snapshots precede kit delivery. Their observed projectiles are spray-goof
particles and attributed tank death effects, not guns. Do not mistake an effect's
source ID for evidence that its source equipped a weapon.

The trace's server step timings were median 6.325 ms, p95 13.396 ms, p99 25.154 ms,
mean 7.340 ms and worst 461.406 ms. Compilation and Maxwell's game shared the PC;
these are diagnostic measurements, not an isolated server-performance guarantee.
Tank traces invoke recovery and reach the existing 4.7-6.0 second idle/give-up
limit. The exact screenshot trap is unconfirmed; Maxwell explicitly waived
reproducing it. Preserve the real footprint/posed-hull checks.

Preview10 client logs provide direct hitch attribution. In session 190526 a
51.1 ms frame includes 21.7 ms brick-debris work (19.5 ms physics over four
steps); another 54.2 ms frame includes 13.9 ms fade uploads. Session 183122 also
contains 49.9/43.9 ms fade work and separate 37.6-55.6 ms network/update hitches.
Those records do not identify authoritative bot CPU. Startup calibration/map
uploads and unfocused 20 fps pacing are distinct. Fade work is reduced; neither
all remaining client hitches nor a 5/3-second loading target is claimed solved.

## Checks and handoff

- Nine-hole Slayer team/empty kit/actual respawn regression passes.
- Fuel/Leap/Jump actual motor checks: thirty jet-button/thrust ticks with native
  energy use; No-Jet zero. Route fuel/reach regression passes.
- Ordinary rocket patrol makes a real hole and traverses it; protected and
  damage-disabled walls refuse attacks. Crawl fixture traverses a genuinely
  low passage and emerges. It initially hopped over a shallow test roof; the
  corrected tall roof makes crouching necessary.
- Ten fade checks pass. Sixty-four bricks over twenty alpha frames use 64 initial
  uploads and 1,216 geometry-free recolors, matching fresh vertices/indices/batches.
- Scoped focused tier passes (changed crates and direct consumers, plus the
  carryable-objective integration target); final sim units: 471 pass, 5 ignored.
  Updated-source clippy/regression reruns cover the final small admission changes.
- The final unchanged-save 600-second rerun, warm five-executable package,
  file/hash/native-startup verification and exact-head platform CI must finish
  before merge. The PR body/private delivery receipt carries that final evidence.
  Maxwell's interactive acceptance remains separate.

Commands: python tools/check.py quick; python tools/check.py focused --tests
bri-chaos:bot_carryable_objectives; scoped cargo clippy --all-targets -D warnings;
cargo test -p bri-sim -p bri-chaos --lib --test bot_vehicle_routes --no-run;
explicit saved_match_vehicle_routes --ignored with BRI_DRIVE_SECONDS=600 and
BRI_CONTENT/BRI_DRIVE_WORLD pointing at the exact private inputs. Local reports,
logs and failed attempts stay in ignored artifacts/zombie-team-jets/.

Import mistakes caught before handoff: an unlisted archive Slayer copy could not
apply its port; a regeneration without --version was rejected as 1.0.0 versus
4.1.5. Correct source/version/dependencies were regenerated, not silently patched
around package validation. Full workspace/release gates were not substituted for
the scoped checks, and no --no-verify push is used.
