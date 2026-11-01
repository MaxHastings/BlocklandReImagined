# v0.4.3: final execution handoff

Current integration branch at preparation: `2769553a2`, based on
`origin/main` `9390c39cf` (96 commits ahead, none behind). Maxwell has finished
playtesting and does not want further playtests. This handoff orders remaining
work by player impact and records an explicit stopping rule; it does not waive
the release checks or claim that unverified behavior is fixed. See the broader
[requirements](2026-10-31-v043-remaining-release-requirements.md) and linked
progress records for evidence.

## 1. Finish the recent gameplay corrections

1. **Afghan CTF bot startup and navigation.** The real Afghan save reproduced
   13/15 bots waiting on long searches. Useful partial routes now let all 15
   have routes in the observed run by tick 486. Retain the saved-world probe,
   focused bot-route and two tower-flag tests. Check for a real regression in
   saved Afghan-with-zombies and the combined release build; do not invent
   perfect AI or rerun all saved cases after unrelated changes. The 32-bot
   playtest-profile mean of 5.741 ms is acceptable to Maxwell; run the actual
   release-profile performance check once and report materially different
   outcomes, rather than optimize the small difference for its own sake.
2. **Stairs, balcony and cube.** Support-margin routing improved the observed
   first stair turn and the cube and tower saved encounters still reached their
   targets. The balcony is mixed: its first kill was slower than baseline,
   despite more bots progressing. Run one bounded inspection for a concrete
   route/control defect and a supported fix if one is found. Otherwise document
   the limitation; do not build an open-ended stacking or cooperation system.
   Keep the no-jet cube, stair and tower controls after relevant changes.
3. **Leave/rejoin.** The creator-membership bug and default Slayer reset policy
   are fixed. Both actual Afghan saves passed headless leave, two seconds of
   bot movement while absent, and rejoin with freshly imported Slayer rules.
   Ensure those fresh rules are in the final package and check combined
   multiplayer/lifecycle behavior once. Explicitly enabled Reset When Empty
   still pauses the game.
4. **Bot events.** Power, respawn, search/wander, naming, melee and opt-in idle
   seeking have focused tests. A bot spawn can now use the existing Player
   `setTeam` event for a minigame team; its owner checks pass. Confirm the
   event works on initial and replacement lives in a saved build and that the
   idle-seeking option behaves in the final content. Distinguish that team
   control from Bot_Hole's relationship modes; add only useful, grounded
   missing controls with execution and lifecycle evidence, not no-op aliases.

## 2. Communications and minigame integration

5. **Loading and global voice/chat.** The loading UI, horizontal speaker row,
   accepted loading chat/voice, Global voice and bounded modder processing
   seams are implemented and have focused UI, real-QUIC and audio tests.
   Run the remaining combined lifecycle checks: actual sample continuity
   across accepted loading/entry, map change, cancel/reconnect, mute and
   revocation, plus global alive/dead membership and game boundaries. Fix a
   reproduced leak or break; do not expand the radio seam into a preset/DSP
   feature. Verify Page Up/Down, F2 and Ctrl+O stay in loading UI and the
   known closed-channel exit remains fixed.
6. **Persistent minigame authority.** Existing tests cover creator departure,
   rejoin, combat isolation and resource ownership; the actual Afghan saves
   cover the zero-human bot case for two simulated seconds. Finish the bounded
   cross-game, reconnect, reset/end and save/restart checks with actual package
   rules where relevant. Verify a former participant cannot fight in the old
   game or transfer ownership by joining another game.

## 3. Rendering, destruction and audio

7. **Destruction.** Moving light/emitter/audio carrier identity, partition
   replication and item/bot/vehicle source suspension have focused native,
   network and lifecycle checks. Run one integrated changed-area check for
   actual effects/restoration and observer/late-join behavior; fix a concrete
   mismatch. Existing independent items/actors must not be deleted just
   because their source brick is destroyed.
8. **Lighting and material detail.** Dark ordinary overlays and translucent
   surfaces have offscreen controls; a subtle stock brick gloss and optional
   normal-map seam exist, including ordinary local-light gloss. Check a close
   representative render under dark, sun and local light, and relevant
   transparent/intentional emissive controls. Original brick PNGs are shading
   masks, not authored height maps; do not promise per-texture normal maps or
   infer physical relief blindly. Apply supported material improvements only,
   record missing maps/other material paths and do not claim every texture is
   complete without evidence.
9. **BadDream #413 and v21 lighting #415.** BadDream has source-backed animated
   light records, but mask decoding, timing and actor illumination are not yet
   established. Make one bounded source-backed implementation attempt if it
   can be validated; otherwise leave #413 incomplete with the precise limit.
   Preserve the v21 core audit and ship justified fixes, not speculative TV
   lamps or claims of complete v21 fidelity.
10. **Sound and #412.** Swing gains, indoor ambience, the selected outdoor
    OGG and per-play music volume are implemented and have focused offline
    checks. Validate the final package's actual asset bindings and gains,
    looping/fade and music-slider behavior, retaining headroom. Maxwell waived
    additional listening tests; report that subjective combined balance was
    not newly tested rather than making further ungrounded gain changes.

## 4. Bounded unresolved reports and issue triage

11. **Movement delay, hammer and restarts.** Guest input admission has a real
    fix; the separate slow-onset local movement feeling still has no proven
    cause. The unswung hammer on real ModTer and the friend's server restart
    also remain unproven. Use existing logs and one supported targeted check if
    a concrete new lead exists; otherwise list each as unresolved with the
    available diagnostics. Do not reset players periodically, guess, or ask
    Maxwell for another playtest.
12. **Collision and #400.** Host/prediction death collision has focused tests;
    the ragdoll settling candidate has repeated fixture evidence. Keep the
    relevant death, respawn and support-loss checks; report if the exact
    reported symptom or intermittent failure remains unproven.
13. **#414, #416, #417 and other issues.** Bot_Town's eight source-referenced
    bricks have import, plant, save/reload, collision and late-join checks;
    retain diagnostics and packaged integration evidence. Window resolutions
    have a correction and need platform checks. Adaptive draw distance was
    assessed and is not added without demonstrated need. Recheck all open
    issues immediately before release and record what is fixed, incomplete or
    intentionally not adopted; do not close an issue merely to make the list
    look clean.

## 5. Integration and release

14. **Content and source provenance.** Reconcile with current `origin/main`,
    preserving unrelated work and protocol history. Preview.2 reused verified
    previous original Add-On conversions; its rebuilt Slayer is available
    separately. The exact pinned Tier 1 source has not been found: an archive
    candidate is a different hash and edition. Finish the final-bundle path
    without silently substituting source, falsifying current-generation
    receipts, or dropping Add-Ons. If reused conversions are proposed for the
    release, first establish and record their compatibility and provenance and
    make the packaging policy explicitly reflect that decision; private
    preview verification alone is insufficient.
15. **Final gates and publication.** Run changed-area checks, the deliberately
    extended physics/protocol/content release checks, current platform CI and
    history/protocol guards. Check actual Windows, Linux and macOS archives,
    generated content and packaged startup through available local/CI paths.
    Write release notes with verified features and unresolved reports; publish
    immutable GitHub tag/release `v0.4.3`, then independently verify assets and
    digests. Do not claim completion before the checks and artifacts exist.

Work may proceed in a different order when dependencies demand it. A report
that remains vague after its bounded investigation is carried as incomplete,
per Maxwell's instruction; do not mislabel it as fixed or let speculation
indefinitely delay the rest of the release. The broader alpha contract stays
open. No further human playtest is required.
