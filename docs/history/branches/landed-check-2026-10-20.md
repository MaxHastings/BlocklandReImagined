# Branch landed-check, 2026-10-20

Historical note. Written 2026-10-20 against main `a2c2430` (engine). For every branch head found in the cloud project's git bundles (and every branch still on GitHub) that is not an ancestor of main, this lists how much of the branch's added code is already on main.

**How to read it.** Many branches were squash-merged, so they are not ancestors of main even though their work landed. `on main` is the share of the branch's added lines (counted against its merge base) that appear, trimmed, in the same file on main today. About 85% or more means it landed (the rest is later edits). Under about 50% means check by hand. This is a screen, not a proof.

Branches that *are* ancestors of main (or whose trial merge leaves main's tree unchanged) are left out. Restore any branch with `git fetch <bundle> refs/remotes/origin/<branch>:refs/heads/<branch>`.

| Bundle | Branch | Tip | Last commit | Added lines | On main | Subject |
|---|---|---|---|---|---|---|
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/ue2-natives` | `71a4fb8bc` | 2026-10-19 | 6316 | 0% | ue2-natives: research test that dumps decoded script function bodies (BRI_DUMP) |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `claude/project-thread-bz6q94` | `c794989d1` | 2026-10-19 | 13100 | 0% | native lists: WeaponAttachment and Pickup transcribed from the second Engine.dll |
| (still on GitHub) | `claude/docs-seams-rows` | `a05031d64` | 2026-10-20 | 91 | 0% | Docs: seams.md main-status for the 80 rows that named no code item |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/aa-generated-map-engine` | `9360c5249` | 2026-10-19 | 27074 | 9% | Test: a script's one-shot sound is heard in range only |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `claude/ue2-natives-world` | `4415877be` | 2026-10-19 | 68033 | 12% | UE2 natives: FinishInterpolation poll, GETPING, FilterAlways, GetLanguageFilter |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/aa-cutover` | `ce44d4fd3` | 2026-10-19 | 74534 | 12% | Cargo.lock: aa-mode depends on ue2-net |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/aa-sound-natives` | `2b7f3f229` | 2026-10-19 | 70272 | 13% | Merge remote-tracking branch 'origin/codex/aa-cutover' into codex/aa-sound-nativ |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `claude/aa-net-on-cutover-346` | `85957ef29` | 2026-10-19 | 73184 | 13% | Protocol note: mode menu input (mode_ui, @ui) |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `claude/project-thread-io6rzk-hud` | `999520d01` | 2026-10-19 | 70033 | 13% | Merge codex/aa-cutover (0165f411f) into claude/weapon-live |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/ue2-lighting` | `b22b0ed55` | 2026-10-19 | 70151 | 13% | An actor takes the Sunlights of its zone, as the original selects them |
| (still on GitHub) | `claude/wall-step` | `01f2ab9d3` | 2026-10-20 | 112 | 14% | Revert "Motor: unstick widens its search to the body's height when nothing withi |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/ue2-weapon-draw` | `b32a6a032` | 2026-10-19 | 48981 | 15% | Leave other lanes' client files as they were (formatting only) |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/ue2-weapon-draw-2` | `bb6b050eb` | 2026-10-19 | 50384 | 16% | Compile each surface family in its own pipeline; add vertex-light material |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/ue2-effects-rebased` | `4e38d93c6` | 2026-10-19 | 41747 | 17% | Fix clippy in the vertex light import test |
| (still on GitHub) | `claude/project-thread-fx51mn` | `51807529c` | 2026-10-20 | 34 | 17% | Fix v0.3.3 log errors: throwing rules unset fields, animation shape paths, Fizze |
| (still on GitHub) | `claude/bot-office` | `c62e3c265` | 2026-10-20 | 37 | 18% | Test: measure how wide a bot goes round a wall every quarter second, not every s |
| (still on GitHub) | `claude/hit-bones` | `91cc0ed5d` | 2026-10-20 | 384 | 21% | Merge remote-tracking branch 'origin/main' into claude/hit-bones |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `claude/aa-net-on-cutover` | `dbdff90a3` | 2026-10-19 | 44954 | 31% | netsync/ue2-net: link budget from the engine's own saturation (UNetConnection::T |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/ue2-effects-on-main` | `530bf9317` | 2026-10-19 | 9994 | 34% | Add the three-axis texture rotator to UV motion |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `claude/project-thread-io6rzk` | `07eae0eb3` | 2026-10-19 | 60661 | 39% | Cargo.lock for the HUD crates |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `claude/ue2-census` | `58f28d80c` | 2026-10-19 | 48575 | 43% | Add whole-game UE2 native census tools and ranked report |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/ue2-ground-pick` | `4049912a9` | 2026-10-19 | 27022 | 51% | Bring the prone unit tests to the binary's MaybeUnProne (falling pawns only) and |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/ue2-blend-modes` | `30f961310` | 2026-10-19 | 22888 | 71% | Add the redraw-the-receiving-triangles projection pass and the receiver builders |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/aa-terrain-layers` | `09d562925` | 2026-10-19 | 14394 | 98% | Document bri_layer_blend in the map guide |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/aa-map-sounds` | `841b4f30c` | 2026-10-19 | 18971 | 98% | Placed map sounds: looping sound sources in a map scene, played by the client |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `codex/aa-collision-surfaces` | `481bfeee1` | 2026-10-19 | 18997 | 99% | Per-collider query masks: a col_ node lists the queries it blocks |
| branch-cleanup/engine-unmerged-2026-10-20.bundle | `claude/project-thread-t3kkxz` | `cfcedabf2` | 2026-10-20 | 466 | 99% | Progress: AA solo match rebased on main |
| (still on GitHub) | `claude/fifteen-minute-minigame-ki5ju2` | `7b4877487` | 2026-10-20 | 24 | 100% | Make a Game: plain entry name and an Invite a friend button in the workshop pick |
| (still on GitHub) | `claude/minigame-bots-findable-rkyhh1` | `2545a0710` | 2026-10-20 | 13 | 100% | Show Add-On bot count on the Game page via a new bots setting place |
| (still on GitHub) | `claude/zombies-no-guns-ujpp0y` | `bdba21de7` | 2026-10-20 | 5 | 100% | Key hole bots' weapons on hShoot, as Bot_Hole does, not hMelee |
| branch-cleanup/v0210-backup-2026-10-12.bundle | `claude/v0210-integration-tw92or` | `2d6d22664` | 2026-10-12 | 53 | 0% | Release notes for v0.2.10 |
| branch-cleanup/backup-12-2026-09-29.bundle | `revive/docs-drift` | `544065ca3` | 2026-09-27 | 1002 | 0% | Guard docs/architecture and follow the new default packs |
| branch-cleanup/backup-12-2026-09-29.bundle | `modplatform-package-wip` | `768b2ffdb` | 2026-09-27 | 1950 | 0% | Park the mod platform lane's package crate for the cloud campaign |
| branch-cleanup/backup-12-2026-09-29.bundle | `revive/creature-notes` | `777266ec9` | 2026-09-27 | 50 | 0% | Write the seams a creature package hits on today's surfaces |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/tutorial-horse` | `d8298cf88` | 2026-09-28 | 33 | 0% | Let a Tutorial rider get off the horse |
| unmerged-worktrees-archive.bundle | `archive/tuthammer` | `dcbef7763` | 2026-10-05 | 439 | 0% | Archive: uncommitted edits in BlocklandReImagined-tuthammer on tutorial-hammer ( |
| branch-archive.bundle | `revive/visual-compare` | `1855c46a7` | 2026-09-27 | 1174 | 1% | WIP: side-by-side v20 vs native screenshot comparison |
| unmerged-worktrees-archive.bundle | `archive/creature` | `6b5a061a2` | 2026-10-05 | 827 | 1% | Archive: uncommitted edits in BlocklandReImagined-creature on creature-spike (20 |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/status-doc-3r57b9` | `a65a2b13c` | 2026-09-28 | 100 | 18% | STATUS.md: a16 at protocol 40, brick damage, defaults picked, a17 plans |
| tech-debt/registry-wip.bundle | `registry` | `e3279163f` | 2026-10-01 | 2569 | 19% | Importer ports: one folder each, found instead of listed |
| unmerged-worktrees-archive.bundle | `archive/tutorial-targets` | `7ec537fc6` | 2026-09-28 | 19 | 21% | Stand still on the spot before building in the Tutorial walkthrough |
| branch-cleanup/backup-12-2026-09-29.bundle | `door-closers-join-address` | `859c012cf` | 2026-09-27 | 381 | 38% | Keep your bricks on rejoin, reconnect after drops, and join by address simply |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/night-qa` | `7b935d617` | 2026-09-28 | 549 | 42% | Fetch the host's client-only Add-Ons on join; keep tool-chosen Add-Ons |
| branch-cleanup/deleted-30-2026-09-29.bundle | `revive/v20-parity` | `f928d22f5` | 2026-09-27 | 199 | 45% | Turn at v20's keyboard turn speed |
| branch-cleanup/deleted-30-2026-09-29.bundle | `revive/v20-parity-on-a13` | `5b8c2f387` | 2026-09-28 | 227 | 47% | Aim the hammer range test from v20's Eye node |
| branch-cleanup/deleted-30-2026-09-29.bundle | `revive/v20-parity-on-main` | `b7b87768e` | 2026-09-28 | 227 | 47% | Aim the hammer range test from v20's Eye node |
| branch-cleanup/backup-12-2026-09-29.bundle | `items-player-types` | `814f2d562` | 2026-09-27 | 772 | 50% | WIP: horse rig for HorseArmor players and scaled avatars (not wired into App yet |
| branch-cleanup/deleted-30-2026-09-29.bundle | `revive/v20-parity-on-84f9ac9` | `fe239ff81` | 2026-09-28 | 242 | 50% | Fire from the Eye node's lead too, and expect v20's keyboard turn rate |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/pushable-bricks-rhtfom` | `631325f33` | 2026-09-28 | 363 | 53% | Keep knocked-out brick physics client-only, pushed by what each client draws |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/orthogonality-audit-67nx3z` | `8ade02e1e` | 2026-09-28 | 405 | 53% | Ask before trusting a new server identity or large Add-On downloads |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/mod-manager-import` | `e34f05559` | 2026-09-28 | 344 | 65% | Add-Ons: import old Blockland add-ons dropped into content/Add-Ons |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/modding-guide` | `0e3ebdef4` | 2026-09-28 | 691 | 66% | Guide: say how an Add-On's side is picked, and to split mixed ones |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/first-impressions` | `35fca1e42` | 2026-09-28 | 124 | 66% | Drop the feature-freeze wording from the first-impressions status |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/fix-gate-speed` | `746b47e41` | 2026-10-07 | 205 | 66% | Gate: a crashed test process fails the gate; timings survive target wipes |
| mod-manager/modding-guide.bundle | `modding-guide` | `ac60264ae` | 2026-09-28 | 688 | 66% | Add the Add-On author guide |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/project-thread-u0pw8d` | `af7388488` | 2026-09-28 | 78 | 67% | Keep a rejoining player's owner number, and rejoin after a drop |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/duplicator` | `c7a5b840d` | 2026-09-28 | 1573 | 67% | Add the Duplicator as an Add-On on new copy-and-place seams |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/fix-minigame-set-favs` | `97b5fc13d` | 2026-10-07 | 47 | 68% | Create Mini-Game Set Favs keeps its own state |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/tester-guide-659gp6` | `52b0bf99d` | 2026-09-28 | 287 | 69% | Ship a tester guide and feature list in every release folder |
| branch-cleanup/deleted-30-2026-09-29.bundle | `revive/chat-emotes` | `d8da15c6e` | 2026-09-27 | 358 | 69% | Match v20 chat colours, admin/join lines and emotes (/hug /zombie /bsd /wtf) |
| branch-archive.bundle | `claude/project-thread-3pq3wl` | `4cabd5195` | 2026-09-29 | 628 | 71% | Merge origin/main (protocol 55) into the cleanup pass |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/chat-emotes` | `bfcac6ddc` | 2026-09-28 | 355 | 71% | Say chat protocol 38 and let the join test skip the welcome line |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/final-touches` | `f2be40aa1` | 2026-09-28 | 408 | 72% | Write up the v20 sweep; openBSD says so when building is disabled |
| branch-cleanup/deleted-30-2026-09-29.bundle | `items` | `638ddc7cf` | 2026-09-27 | 1020 | 73% | Play v20's ItemPickup sound when a tool is picked up or dropped |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/remote-anim` | `bf745c63d` | 2026-09-28 | 536 | 73% | Show other players' swings, rider look and ghost bricks as v20 did |
| tech-debt/cleanup-fold.bundle | `cleanup-fold` | `cb5e2ee16` | 2026-10-01 | 405 | 73% | Delete dead code, probes and old-format compatibility |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/docs-cleanup-6xoamb` | `9d673aafa` | 2026-09-28 | 488 | 74% | Tidy the docs for release: new README and index, drop stale handoffs |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/skis-v20` | `1945ca18e` | 2026-09-28 | 319 | 76% | Spray tire emitters from wheels on the ground, like v20 (protocol 39) |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/modern-hosting-gd6utj` | `f4e2d582d` | 2026-09-28 | 2018 | 77% | Merge main into modern hosting |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/first-impressions-audit-3t2lqq` | `d9e0ead35` | 2026-09-28 | 1475 | 78% | Merge main into first-impressions audit |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/fi-first-run-2` | `59f9d3892` | 2026-09-28 | 89 | 79% | Offer the Tutorial and a name on first run |
| unmerged-worktrees-archive.bundle | `archive/stresslab-hud` | `6e44a66ca` | 2026-10-05 | 275 | 81% | Archive: uncommitted edits in BlocklandReImagined-worktrees/stresslab-hud on cla |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/reported-bugs-audit-yb218k` | `9d3985e21` | 2026-09-28 | 379 | 81% | Keep the chain kill out of package brick removal |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/red-team-o8nvo2` | `92fcb8feb` | 2026-09-28 | 742 | 82% | Red-team audit of Add-On client code and hosting |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/fi-captions-2` | `056244cae` | 2026-09-28 | 165 | 83% | Add captions for game sounds |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/fi-first-run` | `2838de5a8` | 2026-09-28 | 79 | 83% | Offer the Tutorial and a name on first run |
| branch-cleanup/deleted-30-2026-09-29.bundle | `fix/avatar-topology-reupload` | `3aa8e171c` | 2026-09-27 | 157 | 83% | Merge remote-tracking branch 'origin/main' into pr3 |
| branch-cleanup/deleted-30-2026-09-29.bundle | `breakables` | `ab6a16aa4` | 2026-09-27 | 688 | 83% | Smash v20 glass map shapes with fast player impacts |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/pr-tidy-uc564s` | `c90a96896` | 2026-09-28 | 202 | 83% | Ask joining players before a server's Add-On code runs |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/fi-plant-warning` | `ea3360cb0` | 2026-09-28 | 222 | 83% | Turn the ghost brick red before a plant that would fail |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/project-thread-dfojm5` | `14e0e9bda` | 2026-09-27 | 1423 | 84% | Show imported bricks in the brick menu |
| branch-cleanup/deleted-30-2026-09-29.bundle | `tool-rules` | `a4c775e83` | 2026-09-27 | 354 | 84% | Undo the print before the plant in the app_flow undo check |
| branch-cleanup/deleted-30-2026-09-29.bundle | `revive/admin-orb` | `b80ec18cf` | 2026-09-27 | 731 | 84% | Match v20's admin camera: orb, teleport effects, vehicle carry on F7 |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/game-modes` | `3d420ea2f` | 2026-09-28 | 664 | 85% | Game modes: pick what a hosted game runs from Start Game |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/crash-hunt-4hjmam` | `6be939834` | 2026-09-28 | 3045 | 85% | Hunt crashes with chaos soaks and fuzzers; fix what they found |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/crash-hunt-fix` | `d92605d9d` | 2026-09-28 | 3073 | 85% | Keep the collision refresh from simulating dynamic bodies |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/pong-colour` | `57e3e178a` | 2026-09-28 | 877 | 86% | Fade bricks to a new paint colour like v20 |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/v20-fidelity` | `9d21ce5e9` | 2026-09-28 | 443 | 86% | Close the fidelity audit's open items |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/remote-poses-flake` | `d1db4615f` | 2026-09-28 | 29 | 86% | Make the remote swing test sample densely under load |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/admin-orb` | `ef421bbff` | 2026-09-28 | 761 | 86% | Match v20's admin camera: orb, teleport effects, vehicle carry on F7 |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/brick-damage` | `69117cd04` | 2026-09-28 | 319 | 88% | Merge remote-tracking branch 'origin/main' into claude/brick-damage |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/map-fixes` | `239c2920f` | 2026-09-28 | 189 | 89% | Plant into sloped terrain and keep the Tutorial single player |
| branch-cleanup/deleted-30-2026-09-29.bundle | `revive/water-player` | `6cc7bf9c4` | 2026-09-27 | 566 | 89% | Tint the screen under water; add froth, bubbles and coloured splashes |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/net-graph` | `fcb3263e2` | 2026-09-28 | 1713 | 89% | Bring back v20's net graph and add a performance overlay |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/fi-colourblind` | `01f05a22a` | 2026-09-28 | 75 | 90% | Add a colour-vision option to the 3D view |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `revive/water-player-on-a13` | `5721b9608` | 2026-09-28 | 569 | 90% | Size water coverage from the player archetype, and read the world brick map |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/pong-events` | `9034a7016` | 2026-09-28 | 565 | 90% | Make v20's Demo Pong playable and audit its events |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `slides` | `afedd7339` | 2026-09-27 | 1000 | 90% | Climb 1x brick stairs again under the v20 sweep; glass hits from the sweep |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/fi-draw-distance` | `d215609ee` | 2026-09-28 | 152 | 90% | Bring back v20's Max Draw Distance slider |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/mod-manager-n90clz` | `ce6454157` | 2026-09-28 | 2247 | 91% | Add-Ons screen: game modes are server-side content |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/package-wip-fold` | `db08aa246` | 2026-09-28 | 430 | 91% | Fold the modplatform package draft into main: capability words, strict manifests |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/load-overlap` | `dbc39009b` | 2026-09-28 | 186 | 91% | Read BLB grid slices in file order and check loads within a save |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/crosshair-v20` | `5f7edcac4` | 2026-09-28 | 54 | 92% | Hide the crosshair outside first person, as v20 does |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/guide-walkthrough-s8sxrn` | `97099bbf3` | 2026-09-28 | 11807 | 92% | Rewrite the Add-On guide from a cold walkthrough |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/addons-tab` | `dd8fa708e` | 2026-09-28 | 14 | 92% | Start Game's Add-Ons tab opens the Add-Ons screen |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/v0.2.6-fighting-pushes-ki8k6d` | `ec7db637e` | 2026-10-07 | 1464 | 92% | Progress: v0.2.7 note for a tighter pusher bound in the spot chooser |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/floor-flush` | `feed88898` | 2026-09-28 | 82 | 92% | Rest map-floor placements on the nearest plate plane, like v20 |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/stress-campaign-73g1eu` | `c0b625888` | 2026-09-28 | 11344 | 93% | Halve the per-tick share of engine-run package work to 200k operations |
| branch-cleanup/backup-12-2026-09-29.bundle | `fx` | `daa5bc5c7` | 2026-09-27 | 100 | 93% | WIP: axis-angle image rotations, PlayerSplash ring emitter |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/settings-b7jwno` | `def9c3849` | 2026-09-28 | 523 | 93% | Options: quality presets, frame cap, music volume, background mute, slider value |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/smoothing` | `fa1669167` | 2026-09-28 | 1038 | 93% | Let slewed clocks catch up faster after a hitch |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/v0.2.6-fighting-rooftop-ki8k6d` | `1400e814e` | 2026-10-07 | 2397 | 94% | Merge remote-tracking branch 'origin/main' into claude/v0.2.6-fighting-rooftop-k |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/first-impressions-2` | `2369bfe7d` | 2026-09-28 | 152 | 94% | Apply the host's Server Settings: brick limit, plant rate, chat length |
| branch-cleanup/deleted-30-2026-09-29.bundle | `claude/client-sandbox-nz17bc` | `696d3d342` | 2026-09-28 | 119 | 94% | Keep base packages quiet and one broken Add-On from disabling the rest |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/gate-speed-notes` | `79076812e` | 2026-10-07 | 2611 | 94% | Progress: gate speed note, check every bundled Add-On enabled in v0.2.7 |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/fi-server-settings` | `89f0cbdbd` | 2026-09-28 | 174 | 94% | Give the rate-heavy tests generous Server Settings |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/fi-accessibility` | `e84ac9e60` | 2026-09-28 | 72 | 94% | Add a UI Size setting |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/fog-sky-floor-jjyotf` | `4571b9c6f` | 2026-10-08 | 4843 | 95% | Clear sky and floor again; far things fade into the sky behind them |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/fi-controls` | `b94c8363f` | 2026-09-28 | 103 | 95% | Add a Toggle Crouch option and let Mouse 4 and 5 be bound |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/project-thread-uoh3ap-acceptance` | `ed9882d7e` | 2026-10-07 | 2100 | 95% | Acceptance run: grenade harm read from real damage, named numbers, first seconds |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/bot-navigation` | `03a195272` | 2026-10-07 | 6072 | 96% | A bot near a weapon with a step still to climb is not at work |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/fi-brick-search` | `17dc54703` | 2026-09-28 | 141 | 96% | Add a search box to the brick selector |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/v0.2.6-measured-moves-x42j1c` | `2fd8cede5` | 2026-10-07 | 971 | 96% | shove_landing: copying colliders spends the allowance; hardest impact |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/project-thread-uoh3ap-acceptance-print` | `5f28fe112` | 2026-10-07 | 6386 | 96% | Acceptance run: every run on its own line, grenade harm by seed |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/project-thread-uoh3ap` | `5fd6cfaf0` | 2026-10-07 | 2594 | 96% | Building tools are read by their data: the wrench is no weapon, the wands are |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/fix-crash-snapshot-wait` | `633c2a03b` | 2026-10-07 | 4697 | 96% | Crash capture test: say only what the wait shows |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/project-thread-uoh3ap-v026-fixes-rooftop` | `8c0c7b34f` | 2026-10-07 | 3825 | 96% | Merge commit '1400e814' into v026-fixes-rooftop |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/bundle-match` | `f9196bb73` | 2026-09-28 | 3874 | 96% | Bump every base pack a clean bootstrap no longer reproduces |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/v0.2.6-fighting-allyharm-ki8k6d` | `b85d0c66f` | 2026-10-07 | 6713 | 97% | Fire gate: presses no plan made are judged too; push accounting by the gate's ve |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/v026-stack-plain` | `e05a13834` | 2026-10-07 | 9075 | 97% | Merge the v0.2.6 plan and design docs into the v0.2.6 stack |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/project-thread-uoh3ap-acceptance-final` | `ed0680c05` | 2026-10-07 | 2065 | 97% | Acceptance run: grenade harm summed per variant, no damage record unread |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/project-thread-uoh3ap-v026-fixes` | `1418df5b6` | 2026-10-07 | 3159 | 98% | Merge remote-tracking branch 'origin/main' into v026-fixes-on-main |
| branch-cleanup/v026-backup-2026-10-08.bundle | `claude/project-thread-uoh3ap-addon-versions` | `ded6ee82a` | 2026-10-07 | 3376 | 98% | Updated Add-Ons say so, not that they start off |
| branch-cleanup/deleted-branches-2026-09-29.bundle | `claude/fi-gamepad` | `c428ffe8c` | 2026-09-28 | 234 | 99% | Play with a gamepad |
| branch-cleanup/v0210-backup-2026-10-12.bundle | `claude/release-permissions` | `44b3fe746` | 2026-10-11 | 9 | 100% | Allow release tagging, workflow runs and GitHub release commands |
| branch-cleanup/v0210-backup-2026-10-12.bundle | `claude/allow-delete-claude-branches` | `ef408c071` | 2026-10-11 | 1 | 100% | Merge remote-tracking branch 'origin/main' into claude/allow-delete-claude-branc |
