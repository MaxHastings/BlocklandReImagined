# 2026-10-28 Contextual NPC recordings after the gameplay pass

Branch: `codex/afghan-tank-navigation`. This follows the measured gameplay
corrections in [the gameplay handoff](2026-10-28-bot-gameplay-pass.md), rather
than substituting jokes for navigation fixes. The remaining bounded search
delay is still an open observation. Maxwell's interactive acceptance and the
full alpha contract remain open.

## Playable increment

The optional local pack has 118 distinct, hash-pinned online recordings across
25 supported contexts. This is the requested 100–120 clip starting slice, not
an inflated count of pitch variants. All 18 headline references have recordings,
including the five initially absent from MemeBtn: Ackbar, the Skyrim wind line,
Gob's mistake, Dora's celebration and Hudson's game-over line. The committed
[catalog](../../tools/npc_reactions/catalog.json) records exact source pages,
download URLs, original hashes, tiers, placement and explicit excerpts.
MemeBtn, Myinstants and InstantsButton supply these recordings; TF2/Portal
soundboards were inspected but no synthesized substitutes were used.

Downloaded originals live in
`%LOCALAPPDATA%/BlocklandReImagined/audio-cache/npc-reactions-v1`; generated WAVs
and their provenance live in the main generated content directory's
`addons/npc_popculture`. Neither recordings, private traces nor research HTML
are committed. Source installations and the Afghan save remain unchanged.
Rights/uploader attribution remain unverified: this is a local audition pack,
not redistributable release content or a claim of cleared source fidelity.

The [builder and usage handoff](../../tools/npc_reactions/README.md) regenerates
the shared audio package and its server-only `npc_popculture_rules` companion.
The existing Add-On dependency/companion mechanism enables both together.
No bootstrap downloads, migrations, wire changes, new game mode or AI policy
are needed. Each generated directory has an ownership marker; unknown output
directories are refused. Missing cache files require explicit `--download`,
and changed sources fail the committed hash. Full originals remain available
for listening when an explicit short excerpt was selected.

## Causal event and mixing corrections

NPC playback must use world `sound_at`, not `play_sound(bot, ...)`: the latter
targets that NPC's private ears. Native `SoundShaped` presentation now carries
the actual profile, position, pitch and radius through the normal shared cue
path. The package has only the `effects` capability and cannot change controls,
damage, inventory or scoring.

Roster sampling uses `bots()`; `players()` deliberately excludes NPCs. Accepted
health deltas establish pain/healing, rather than pre-filter `on_damage`
attempts that another rule may reject. Spawn resets motion/health history.
Observed falling, landing, launch and seat transitions have separate contexts;
a pressed jet button on a NoJet player is not a jetpack event. Round reactions
use canonical winner lists. Individual objective completion never means a round
win, and a defeated/end-of-round objective is not a completed requested win.

The existing bounded bot report queue gained three content-independent outcome
hooks: provider-confirmed carriage, requested authoritative objective completion,
and repeated failed headway while movement was commanded. No content name
defines an affordance. Pickup reports confirm a planner pickup step's observed
carriage, not a universal pickup-event ledger. A home flag carried for recovery
is also acquisition even though Slayer's enemy-pickup scoreboard excludes it.
Stationary guards, deliberate goofs and planned pauses are not diagnosed from
their location as movement bugs. Idle chatter is outside minigames only.

“Boom headshot” is explicitly a rare overconfident multikill boast; no headshot
was invented from an impact-height guess. Flex Tape follows accepted heavy
damage, not an unsupported structural-collapse event. Four stable per-bot pitch
treatments provide vocal identity, with gentler processing of micro reactions.

There are at most two active reactions globally, no reaction sources overlapping
within 24 horizontal units, a three-second global start gap, 10–16 second bot
spacing, and a two-minute global iconic-reference cooldown (45 seconds for other
clips). Duration/pitch governs active occupancy. The 16-entry pending queue has
short expirations and random tie selection. Confirmed objective outcomes can
displace pending combat chatter and wait up to 3.5 seconds, while respecting
active voices and the global start gap. Other candidates expire after 1.25
seconds; old reactions do not play out of context.

## Evidence and retained limits

All 118 processed WAVs decode: 118 unique original hashes, 183.57 seconds total,
maximum individual duration 4.941 seconds and maximum decoded PCM peak 0.880005.
Processing removes DC, trims silence with margins, resamples to mono PCM16 at
22.05 kHz, bounds RMS/peak gain, and fades excerpt edges. The native Add-On
checker accepts the generated pack and companion. This is technical evidence;
the generated `audition.html` is the complete human listening handoff, not a
claim that every recording has been listened to or cleared.

The first audio-enabled 600-second Afghan trace recorded **140 actual reaction
cues, 37 unique recordings, 14 starts/minute** and no overlap/start-gap/global
repeat violations. Its measured tier mix was 102 micro / 33 quote / 5 rare
(72.9% / 23.6% / 3.6%). The 65/30/5 weights are a preference when that situation
offers all three tiers, not a claimed measured player preference or an enforced
quota that substitutes an unrelated gag. The run exposed combat traffic
crowding out the rare objective completion; priority/admission was corrected
and receives a content-free regression test before the final trace.

The reviewed audio-only candidate then recorded **134 cues / 47 distinct clips**
in 600 seconds, 13.4 starts/minute, with 72 micro / 57 quote / 5 rare. Heavy
accepted injuries had been crowded out entirely in the earlier run; their
priority/chance was raised, producing 18 contextual heavy-injury reactions while
retaining the same global mixing/cooldown limits. Seven carriage reactions and
one confirmed objective-completion reaction reached presentation. The auditor
found zero violations. The 65/30/5 suggestion is deliberately not enforced at
the cost of suppressing a relevant injury or playing an unrelated sound.

The same broader movement review exposed another actual gameplay gap: one bot
pressed against an approach for 22 seconds (218–240). The trace's route leg is
ordinary `walk`; an initial suspicion of a special airborne-leg timeout was
ruled out. It replanned the objective, then rapidly changed remembered search
destinations. `Brain::set_goal` erased the physical no-headway window on every
retarget, so recovery stopped firing while its feet remained unchanged. The
correction retains that window across destination updates of the same intent;
actual pauses, a different intent, intentional stops and body changes reset it.
The regression challenges
both a stationary retargeting bot and one making real progress. A further
600-second native run verifies the correction before integration. Audio policy
is frozen while this gameplay failure receives priority.

The retarget candidate reached 169.04 / 131.88 units from the two tank spawns,
with 44 / 43 life-specific midfield crossings, six enemy flag pickups and one
recovery. It also exposed a distinct 25-second empty-route freeze: collision
contact held a body at an ungrounded pose after a hop, so the earlier landing
invalidation could never fire. This is not deliberate goofy behavior. The
existing route-stop pose now retains its tick, and a stationary ungrounded bot
with no route/search can use the existing bounded safe-floor step-off query
after one second. Its ordinary walking/air-steering waypoint persists until
it clears the contact; the original objective remains. No teleport, jet grant
or map-specific position is used. Nearby floor search is capped at the body's
safe drop. Regression cases reject early recovery, actual falling velocity,
active jets, grounded guards and an intentional stop. This run's 141 audio cues
used 51 recordings with zero mix/cooldown violations; the further integrated
trace is the acceptance candidate for the contact correction.

The retarget change initially preserved evidence across different intent kinds
as well. The existing guard-profile integration test caught an unwanted recovery
hop carrying a guard beyond its post boundary. Preservation is now limited to
destination changes within the same intent kind; returning to a post or stopping
starts afresh. All seven original guard/patrol/follow/flee/ability/body-profile
tests pass without weakening their assertions. Six route/body regressions and
seven actual-policy audio tests also pass, including a draw producing no loss cue,
no celebration of a credited teammate kill, and an urgent outcome waiting for
its own voice after fast vehicle travel.

The frozen feature candidate's `afghan-integrated-final.json` records all 15
bots for 600 seconds with **no long stationary travel candidates**, zero control
failures and zero package runtime errors. The two tank cohorts reach 114.28 /
140.90 units from spawn. Actual projectiles include 586 spears, 98 tank shells,
33 HE grenades and seven sword projectiles; six paint projectiles accompany
explicit programmed goofs. This run has four enemy-flag pickups and one recovery,
with no capture; earlier completed candidates recorded captures, including two
in the preceding contact-recovery run. A vehicle replacement is never counted
as a capture or round win. Its 130 reaction cues use 50 distinct recordings
(72 micro / 54 quote / 4 rare), with no mixing or repeat violations. The unchanged
save hash was rechecked. The interactive timeline and equal-two-minute tank
comparison were regenerated from this report; the plot was visually inspected.

An early smoke run continued without an invalid combined shared/server package.
The recipe was corrected to the existing two-package model. The saved-match
probe now refuses package runtime errors, a missing requested companion, and
zero presentation for a requested reaction pack in runs of at least two minutes.
Its package-store readout and actual cues make an omitted pack observable. An
initial integration assertion incorrectly treated `package_store()` as a value
rather than an `Option`; the reviewed compile/check corrects this.

Content-free tests run the actual reaction script with synthetic profile
metadata: positioned playback, global cross-bot iconic suppression, queue
bounds, quiet stationary game bots, accepted bot-health sampling, human silence,
objective priority in a full combat queue and canonical round winners. No
downloaded assets or network access are needed in CI. Reusable
`tools/analyze_bot_audio.py` audits actual native cues against the generated
durations and policy counters. The opt-in match probe retains real weapon,
damage, movement, objective and presentation observations together.

Commands/evidence (private `artifacts/`):

```powershell
python tools/build_npc_reactions.py --download --output C:/Users/Maxwell/Desktop/Games/BlocklandReImagined/content/addons/npc_popculture
cargo test -p bri-package-runtime --test npc_reactions
python tools/check.py quick
python tools/check.py focused
# Same read-only Afghan save and rules as the gameplay handoff, plus:
$env:BRI_DRIVE_ADDONS='blockhead_bot,gamemode_slayer,gamemode_slayer_ctf,weapon_hegrenade,npc_popculture'
$env:BRI_DRIVE_SECONDS='600'
cargo test --profile playtest -p bri-chaos --test bot_vehicle_routes -- --ignored --nocapture
python tools/analyze_bot_audio.py artifacts/afghan-audio-acceptance.json C:/Users/Maxwell/Desktop/Games/BlocklandReImagined/content/addons/npc_popculture/npc-reactions-generated.json
```

The final quick tier passes (464 sim units, five intentionally ignored long
cases); the focused tier passes six dependent non-GPU integration crates,
warning-free clippy and client units (719 passed, 78 intentionally ignored).
The integrated gate and Windows PR CI are required before merging. Main's
subsequent brick-destruction audio/collision work is retained during integration;
the post-rebase native trace and PR validation record that final combined tree.
No interactive game session was launched or
controlled. Maxwell should enable the optional reaction Add-On and use the
Afghan build to judge volume, pitch, reference identity, delivery timing and
humor alongside real combat cues. Original sounds retain their identity; these
reactions are an optional layer. Subjective listening, human playtest acceptance
and any future redistribution clearance remain separate outstanding work.
