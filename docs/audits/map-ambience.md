# Map ambience dependency and acceptance handoff

Current baseline: origin/main `030b43a03`, inspected 2026-10-29. Ambience is now a v0.4.2 requirement by Maxwell’s instruction to release coordination. This lane is **not ready** for merge/release.

Current delivery checkpoint: Maxwell approved four original-map recordings, then all eight chosen additions, and explicitly directed "pursue with the12 recordings you got". **Palette frozen at12**: original Creek frogs/waterfall/shore and Forest Dark crickets; new room/dry-air/cold-air/leaves/shore/rain beds plus robin/blackbird calls. The broader candidate table below is exploration history, not a requirement to add compressor, night alternatives or more insects. `tools/map_ambience/palette.json` is the fixed21-map shipping policy; source decisions/evidence are in `source-approvals.json`. No further sources/features without a new explicit user instruction or a reproduced blocker requiring correction.

Current approved pair: `artifacts/map-ambience/shipping-candidate-r6/{map_ambience,map_ambience_rules}`. Six beds are bounded Ogg Vorbis from exactly the approved PCM excerpts; two birds retain their WAV bytes. Python recipe/source/palette/policy/complete-file verification and actual native checkers pass. Root actual consumers pass: c4fc client geometry/hammer/handle checks,966 fixed21-map/spacing/late-join cases,20b original-emitter exact-count/full-field replay. Initial failed consumer evidence is retained in progress notes. No duplicate starts; policies initialize on_tick within1s, avoiding join callback plus retained replay duplication.

Warm private preview delivered before merge: `E:/Downloads/Map-Ambience-Preview-20261029/Launch-Ambience.cmd`, client `v0.4.2-ambience-preview1 (32cd91b9d)`, importer-ac436, source/policy/media966. Native4checks, packaged headless startup and13450file hashes pass. Actual native scene bytes are unchanged in original-map audio refresh; original archive identity and separate augmentation receipts are verified. Maxwell's preview process is active; only he performs interactive listening. Feature-branch default entry is prepared after native validation; actual release bundle regeneration, current-main integration, remaining scoped Clippy/CI/guards and Maxwell's integrated mix acceptance remain open. Neither source approval nor technical checks establish mix acceptance.

## Shipped inventory and visual evidence

Inventory read from the installed v0.4.1 `content/packages.json`, disabled package list, base `map-bundle-017/bundle.json`, and every installed map provider; cross-checked against current `packages/default-addons.json` and client `ContentIndex`/`world_maps`. There are 14 stock missions, six enabled community variants, and shipped/off-by-default Arctic Challenge v3: **21**. AvP is explicitly withdrawn; private Halo/AA maps and sample world providers are not public release defaults. Saved brick builds in the world index are builds on environments, not additional map environments.

All 21 packaged preview images were actually inspected in three contact sheets; exact source paths and hashes are in `tools/map_ambience/maps.json`. Additional existing imagery inspected: historical main `artifacts/persistent-scene/kitchen-0.png` and `artifacts/feedback/20261016-gsf-paradise-terrain.png`. These are limited views, not exhaustive or current source-game fidelity proof. Maxwell subsequently supplied an overhead Tutorial gameplay capture; it was inspected and pinned as an additional reference in `maps.json`, correcting the initial room-tone-only suggestion to exposed outdoor air. Open rafters must not count as a sealed roof. No new game window or GPU render was launched. Construct/Destruct previews are blank, deliberately retained as uncertain/quiet.

| Map variant | Visible assessment / proposed treatment | Actual recording candidates |
|---|---|---|
| Bedroom | Sheltered carpeted room, large furniture and daylight through a window. Keep the bed nearly subliminal; exterior treatment needs shelter transitions. | room |
| Bedroom - Dark | Same furnished room under isolated lamp light. Less exterior activity; no added horror drone. | room |
| Construct | The actual packaged preview is blank white. An abstract building space; deliberate quiet pending evidence for any specific environmental source. | Original authored audio / quiet |
| Destruct | The actual packaged preview is blank black. Preserve quiet; no invented machinery or destruction sounds. | Original authored audio / quiet |
| Halloween Slate | Orange clouded horizon and open empty expanse. Sparse low air, with no Halloween novelty cries or music. | dry-air |
| Kitchen | Large tiled kitchen, refrigerator, cabinets, illuminated fluorescent fixture and window openings. Quiet room bed; compressor belongs at the refrigerator, not throughout the map. | room, fridge |
| Kitchen - Dark | Same kitchen under artificial light. Keep appliance identity, softer room bed; light buzz is an optional powered-fixture detail. | room, fridge |
| Skylands | Open bright sky with cloud layers and reflected-looking horizon. Gentle exposed air; the picture alone does not establish an ocean shore. | dry-air |
| Slate | Huge empty blue/clouded horizon. Air only, quiet enough for long building sessions; no invented birds. | dry-air |
| Slate Desert | Flat sandy expanse under yellow hazy daylight. Low dry gusts; no city, insects or sandstorm impacts. | dry-air |
| Slate Sea Revised | Broad calm water to the horizon in pale pink daylight. Gentle water wash and very low air; avoid nearby beach breakers in open water. | shore, dry-air |
| Slate Storm Revised | Dark overcast open water with visible slanted rain. One rain bed; restrained distant thunder already in the candidate, no second weather loop. | rain |
| The Slopes | Bare snow-covered mountain ridges and falling snow. Variable cold air, no foliage rustle or continuous crunching snow. | cold-air |
| Tutorial | Maxwell’s 2026-10-29 overhead capture shows open roof framing, exposed walkways and grassy enclosing slopes. Outdoor breeze baseline, with local partial shelter; preserve instructional cues. | dry-air |
| Arctic Challenge v3 (off by default) | Exposed snowy mountain obstacle course and elevated structures. Cold air; no invented ski-lift machinery from the thin rail-like structures. | cold-air |
| Creek | Visible rock waterfall, sheltered water and wooden dock. Restore original frog/waterfall/shore emitter recordings at their authored positions first; no duplicate water bed. | Original authored audio / quiet |
| Emerald Isles 4 | Palm-lined grassy/rocky islands beside blue water. Air outdoors; gentle water only near an evidenced shoreline. The thumbnail does not establish the entire island network. | dry-air, shore |
| Forest | Dense green canopy and trunks around a foggy grassy path. Soft moving foliage; compare a bird-free segment to avoid repetitive calls. | leaves |
| Forest - Dark | Dense dark trunks/grass beneath a large moon. Restore original soundNight.wav crickets first; external night candidates are alternatives, not additional layers. | Original authored audio / quiet |
| GSF Paradise | Packaged postcard shows colorful low buildings beneath a mountain. Existing 20261016-gsf-paradise-terrain.png additionally shows palm, beach, water and sheltered structures; local shore and sheltered transitions matter. | dry-air, shore |
| GSF Paradise-Night | Packaged night postcard shows colorful buildings under a large moon. Shared day terrain is known, but night coverage is limited: soft outdoor insects are provisional; quiet buildings and localized water. | dry-air, night, shore |

## Pinned recording candidates

All candidate-specific source pages link CC0 1.0, permitting modification and game redistribution without mandatory attribution; retain author/source/changes anyway. `tools/map_ambience/candidates.json` pins actual input hashes and direct public HQ-preview URLs. These are **lossy MP3 previews**, not acquired lossless originals, even where source metadata advertises WAV/AIFF/FLAC. The reported source resolution is not the preview resolution. No account/download restriction was bypassed. Candidate pages and acquired previews are outside Git under ignored lane artifacts.

| Key | Author / direct audition source | Original advertised format (not acquired) |
|---|---|---|
| cold-air | [winter wind 01a.aiff](https://freesound.org/people/klangfabrik/sounds/117504/), klangfabrik | See asset-specific description in pinned catalog |
| dry-air | [Chergui wind in Agadir](https://freesound.org/people/clement.bernardeau/sounds/718973/), clement.bernardeau | See asset-specific description in pinned catalog |
| fridge | [Large Refrigerator compressor hum 1](https://freesound.org/people/FOSSarts/sounds/740110/), FOSSarts | See asset-specific description in pinned catalog |
| leaves | [Ambiance_Wind_Forest_Trees_Loop_01.wav](https://freesound.org/people/Nox_Sound/sounds/530908/), Nox_Sound | See asset-specific description in pinned catalog |
| night | [Ambiance_Nature_Night_Cricket_Calm_Loop_Stereo.wav](https://freesound.org/people/Nox_Sound/sounds/637083/), Nox_Sound | See asset-specific description in pinned catalog |
| night-alt | [Forest at Night Ambience](https://freesound.org/people/timothyd4y/sounds/587907/), timothyd4y | See asset-specific description in pinned catalog |
| palms | [Wind through Palm Trees.wav](https://freesound.org/people/klangfabrik/sounds/423800/), klangfabrik | See asset-specific description in pinned catalog |
| rain | [Rain and Thunder.mp3](https://freesound.org/people/pmstrain/sounds/348805/), pmstrain | See asset-specific description in pinned catalog |
| room | [Room tone Very quiet Small apartment room loopable edlarez vsnr.wav](https://freesound.org/people/visionear/sounds/565535/), visionear | See asset-specific description in pinned catalog |
| shore | [Calm ocean waves](https://freesound.org/people/SamsterBirdies/sounds/578524/), SamsterBirdies | See asset-specific description in pinned catalog |

Audition report: ignored `artifacts/map-ambience/audition/audition.html`. Reproduce with:

```powershell
python tools/prepare_map_ambience_audition.py --cache artifacts/map-ambience/inputs --output artifacts/map-ambience/audition --content E:/Downloads/BlocklandReImagined-v0.4.1-windows/content
```

Add `--download` only to obtain missing pinned public previews. Report includes per-map reference imagery, playable full loops and 25-second excerpts crossing each boundary twice. Processing preserves stereo/dynamics, removes sub-35Hz rumble, linearly overlaps up to 2 seconds, limits beds to 180 seconds before overlap, and sets quiet audition levels. Exact gain, peak/RMS, loop duration, sample rate, channel count and input/output hashes are in generated `provenance.json`. No compressor or fabricated events. These levels require ordinary-gameplay listening; technical continuity statistics do not establish inaudibility. Room and compressor have shorter periods and need repetition assessment. Palms preview has creator-documented artifacts, so is an alternative pending original acquisition; never promote it silently. Cold wind was recorded near a partially open window, so its whistle needs contextual judgment. Night recordings are alternatives to Forest Dark’s original crickets, not extra layers.

Rejected by description: craigsmith `438880` Low Wind (vintage tape hiss/clipping/explosive rumble); Nox interior rain/fire `553887` (contains unrelated interior/fire sources for open storm sea). TRP `715632` light buzz page verified CC0 but request later returned HTTP403, so no file adopted. New sources were chosen for visible environments, not titles alone. Human listening has **not** been performed or claimed. Seek suitable openly accessible original-quality alternatives before asking for login access.

## Existing mechanisms and original audio gap

`ClientAudio::play_ambient`, rule `play_ambient[_all]`, and `AmbientPlay` already provide keyed effects-bus loops, fades and optional world positions. Session retains global beds for late joins and clears them on map change. Long sound profiles may use streaming (`music: true`) while `as_ambient` routes playback to effects. No second playback subsystem is warranted.

Creek importer report explicitly retains AudioEmitter nodes14–16 as Unadapted. Installed map packages carry no audio clips or emitter rules. Pinned source archive `Map_Creek.zip` SHA256 `335751840111ef75446ad5ccc96ca6d033d3100213ad78de8750a370dbd6d098` declares `SFFrogSound`, `SFDamLoopSound`, `SFShoreSound` in `creek_resources.cs`; exact original inputs:

- `sound/swamp_frogs_loop.wav`: `43a3ac2f48a2505ff71aa4a2e03ef16273d8f3671410e81caf0782960b5cadd9`
- `sound/dam_water_loop.wav`: `c4a261782c94e133056bc0e70c408e895251800b838188434b8abaae9d4678fa`
- `sound/shore.wav`: `fbbf167280e76952ad6ada9b3d2f9f025278162248d63fcb548a2f8bd3adc37a`

Forest Dark authors an AudioEmitter with direct `./soundNight.wav`, loop on, volume0.5 and reference/max distances9521/9521.01. Pinned archive `Map_Forest.zip` SHA256 `eb041482d7b007610677f1bac631f543f07eb583af7ec89ff469c2586a409052`; original `soundNight.wav` SHA256 `ef3ced0683bb60f0fe8ac87f3b112bc43ee8c76637c6fdfe4dec3c1335337127`. Original archives were read only from `S:/SteamLibrary/steamapps/common/Blockland/Add-Ons`; no original bytes are committed. Source files are original community content under their existing bundled provenance, **not CC0 merely because this lane adds CC0 candidates**.

Candidate `map_audio` now exports byte-identical original recordings, stable per-emitter sound identities, authored gain/reference/max distances and transformed positions into ordinary native assets and start-once package rules. The importer report exempts only exact instances with emitted consumers; unsupported cones, loop gaps/counts, channels, profile descriptions and unknown fields remain explicit diagnostics. A synthetic native-package/Session test covers profile and direct-file resolution, neutral versus nonneutral cones, initial/late joins, no repeated starts and a different map. Compilation and actual Creek/Forest conversion remain pending warm broker validation. Converter capability is not approval to ship the four restored recordings; their auditions and approvals remain separate gates.

## Arbitrary brick interiors

Maxwell asked how buildings made from arbitrary bricks should affect ambience. Recommended first scope: continuous nearby roof/enclosure/openness sampling from actual current brick/map geometry; smooth quieter/duller exterior beds under shelter, sound-path obstruction for local emitters, restrained room reverb; updates after place/remove/destruction under bounded work budgets. A roof-only shelter must differ from a closed room, and doors/windows/holes must leak exterior sound. No map-name engine branches or pre-authored indoor tags for every player build. This is proposed scope, not implemented acceptance.

Candidate `ce0b6e2c0` extends existing hearing/playback with bounded 17-ray current-geometry roof/enclosure sampling, positional obstruction without a RegionMap, optional environmental sound metadata and per-voice smoothed gain/low-pass blend. Existing handles and loop phases persist. Authored reverb remains; automatic room reverb is deferred. Broker reports environment3, weapons14 and audio33 tests passing; two project-Clamp Clippy corrections are committed in `01850153b`. Client consumer/actual geometry and destruction-opening integration remain unverified. These technical checks do not establish Maxwell's listening acceptance.

## Finite delivery and mix acceptance

This chat owns ambience only; the release coordinator owns all other lanes and global release. Deliver one approved package for the fixed21-map inventory, supported original Creek/Forest recovery, a small chosen set of sparse details, provenance/approval receipts and an actual private shipping preview. Freeze the palette after source decisions. No further source libraries unless an essential choice is rejected or unusable; no material absorption engine, portal solver, automatic reverb project or unrelated SFX audit.

Maxwell explicitly required balancing against the rest of the game on October29. Acceptance is the **combined** bed/detail/local-emitter mix at ordinary game volume against footsteps, tools/building, weapons, NPC speech and voice chat, including the loudest legitimate overlapping layers and shelter transitions. Check speech intelligibility and gameplay-cue audibility, room entry/exit level changes and repeated details over time. Solo audition approval establishes source choice only. No blanket dB target or offline normalization proves an integrated mix; retain headroom and existing volume controls, and obtain Maxwell's actual gameplay listening acceptance.

Finite acoustics cases: outdoors, sealed brick room, roof-only shelter, doorway/window/open rafters, actual brick-edit/destruction opening, positional source without RegionMap, smooth gain/filter and loop/stream continuity. Required lifecycle checks include initial/late join, map change/cleanup and queue/resource pressure. The original four map recordings now have playable controls in the audition report with original-package provenance and pending approval labels. New media stays out of shipping adoption until source-specific approval.

### Private preview listening handoff

Deliver the actual packaged folder and exact source/content identity before asking for game-mix acceptance. The HTML auditions are source-choice evidence, not that preview. Maxwell performs all interactive checks at his ordinary game volume; retain his existing volume settings. Use a small representative listening pass over the selected layers: Tutorial (open-air breeze), Bedroom (room), The Slopes (cold air), Slate Sea Revised (shore), Slate Storm Revised (rain), Forest near its authored trees (foliage and sparse birds), Creek (original local emitters) and Forest Dark (original crickets). Automated map-selection coverage must separately cover all21 variants; this representative pass does not claim every map has been interactively accepted.

During that pass, walk/jump/land, place/remove bricks and use the ordinary tools/weapons available in the build; include NPC dialogue and voice chat where available. Check the combined ambience leaves these cues clear, has headroom in the loudest legitimate overlap and does not call attention to loop boundaries or repetitive calls. Allow at least two minutes near Forest trees to hear the35–85s detail spacing; silence away from those fixed sources is intentional. Re-entering areas and joining later must not duplicate beds.

For shelter, compare exposed ground, open Tutorial rafters, roof-only cover and a closed ordinary brick room. Approach an opening, then remove a wall/roof with an ordinary supported tool. Outside sounds should change smoothly without stopping/restarting the loop or making a doorway act like an abrupt volume switch. A missing cue, masking, abrupt/pumping transition, repetitive recording or inappropriate environment is a specific correction against the fixed palette, not permission for a new source campaign. Record technical checks, source approval and Maxwell's mix acceptance separately; do not check off unavailable NPC/voice-chat combinations by inference.

## Latest listening/editing direction

Maxwell withdrew the YouTube recording: **"actually don't use it"**. No media was acquired; stop permission/source/excerpt work for it. He now wants simple focused beds: Bedroom and Kitchen each Indoor/Outdoor Day/Night, plus all-outdoor Slopes, Slate and Slate Desert (11 editing slots). Keep other maps' current setup. Do not add new random bird/gust events to these focused maps. His follow-ups explicitly retain Kitchen fridge and the proximity behavior, including the waterfall. Earlier approved gusts are now reference-only rather than scheduled additions.

Delivered `E:/Downloads/Ambience-Ableton-Review-20261030`: 11 independently named priority WAVs, all15 palette/reference WAVs from13 recordings, exact original source files, 67 current/planned placement rows including all Forest anchors and original emitter coordinates/ranges, source/approval/hash receipts and a solo listening page. The11 priority files are seeded copies of current approved room/wind beds, **not eleven already-distinct recordings**; this was disclosed. Maxwell is reviewing and may edit in Ableton. Preserve this folder and all edits; exporter refuses any existing output folder. Final source choices, per-slot edits and combined gameplay mix acceptance remain open. Production r7/headbb99 remain unchanged during this review.

Current r8 follow-up wires these11 editing slots to independent package profile IDs using `priority-slots.json`. The four Bedroom/Kitchen variants now each start complementary Interior/Exterior beds; Slopes, Slate and Slate Desert each start one Exterior bed. Fridge remains its separate measured local emitter. All19 resulting profile files are byte-identical to the corresponding approved r7 starting samples; **independent identities do not imply distinct authored recordings or final mix acceptance**. The other14 maps' policy is unchanged. Generated source/recipe/file checks and both actual preserved native Add-On checkers pass; Python5+2+2 pass. Updated21-map Session test is unrun until the coordinator brokers the policy-only follow-up. Review folder remains immutable while Maxwell edits.

Warm combined source36a now passes client sound_carry5, actual PCM A→B→A1 and explicitly executed actual native Bedroom/Kitchen four-variant collision1. Runtime file blobs match current65e exactly. See [verified context receipt](../progress/2026-10-30-ambience-context-runtime-verified.md). Package policy/Clippy/preview and actual listening acceptance remain separate; these scoped client results do not accept the mix or claim all-map interactive fidelity.

Actual r8 policy follow-up on private84a then caught a real32-byte Ambient key rejection (1 test failed,1 passed). r10 shortens only package keys to `ma/<clip>`, preserving full track IDs. Trace also found original Creek/Forest exporter keys exceeded32 while its synthetic fixture IDs were short; generic conversion now hashes only the loop key to30 bytes, keeping full source/track identity and recording the key in provenance. Long-ID actual Session regression plus one streamed positional-return PCM case await warm execution, and actual original packages need fresh audio-only conversion on private copies. See [bounded-key correction](../progress/2026-10-30-ambience-bounded-loop-keys.md). Static checkers passing must not be reported as this dynamic behavior passing.

## Completion evidence still required

Current follow-up: the original12 recording approvals remain, and Maxwell additionally approved the original-WAV fridge, both existing-source desert gust phrases and the gently recessed wind treatment. `followup-selections.json` pins those decisions. Candidate r7 adds the measured Kitchen/day+dark fridge surface as one retained positioned/streamed loop (ref45/max270), marks the existing room bed Interior, and applies the selected wind filtering with no global gain compensation. The DSP helper reproduces the approved comparison's exact PCM hash. All old r6 clips except dry-air remain byte-identical, including Storm rain. r7 generation, recipe/file/source checks, decoded finite/unclipped PCM and both preserved preview1 native Add-On checkers passed; new local policy startup/latejoin native integration tests and d9 client context/PCM tests remain queued with the coordinator. No candidate r7 playable preview or mix acceptance is claimed.

The earlier selected Arizona Ambience video `LoO63RtYqP4`, **29:00–32:00**, is now WITHDRAWN as recorded above and in `requested-desert-source.json`. It is no longer a release dependency. r7 remains an intermediate data candidate: pending final focused per-context choices/edits, context policy and integrated preview acceptance. Do not treat the11 exported editing slots as independently authored or accepted final game mixes.

- Original emitter conversion plus actual rule consumer verified for Creek and Forest Dark; preserve exact input/resource provenance and diagnose unsupported semantics.
- Audition/source-quality judgment and master acquisition (or explicitly accepted preview quality); select final low-noise excerpts and non-repetitive loops.
- Package-owned 21-map palette/layer policy, correct quiet spaces and state transitions.
- Bounded indoor/outdoor behavior matching actual bricks, including roof-only cover, doorway/window approach, destroyed wall/roof, and moving between rooms; no restart/pumping on routine updates.
- Generated Add-On recipe, source/license notices and hashes verified in the private shipping bundle, with no original media in Git.
- Changed-area Python/importer/rule and mixer checks; source/compiler checks for every modified crate, then packaged headless startup. Reused idle checkout has no target; coordinator offered preserved v041 addon-check/run executables and warm shared-gate converter validation.
- Maxwell listening in ordinary gameplay with footsteps/weapons/NPC/voice chat, and explicit subjective acceptance.

No alpha/release acceptance item checked off. No main push, release tag, visible game playtest, shared-target build or whole-workspace gate was performed.
