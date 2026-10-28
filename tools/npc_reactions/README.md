# Bundled NPC reaction pack

This is the reproducible source recipe for the default NPC reaction Add-On:
118 actual online recordings, 25 observed gameplay contexts. Audio bytes are
kept out of Git. The catalog pins every original recording's SHA-256 and source
page/download URL; the generated provenance also hashes each processed WAV.
Uploader attribution and redistribution rights remain unverified. Original v20
sounds and source installations are untouched.

From the repository root:

```powershell
python -m pip install numpy scipy soundfile
python tools/build_npc_reactions.py --download --output content/addons/npc_popculture
```

`--download` is only necessary for missing cache files. The default private cache
is `%LOCALAPPDATA%/BlocklandReImagined/audio-cache/npc-reactions-v1`. It survives
worktree cleanup. A changed source fails its pinned hash rather than silently
replacing a reference. The builder refuses to overwrite directories it did not
create. It creates the shared recording package and a server-only
`npc_popculture_rules` companion. Enable **NPC Pop-Culture Reactions** in Add-Ons;
the companion follows automatically. No save edit or new game mode is needed.
For an existing generated content directory use its absolute path as `--output`.

`audition.html` in the generated recording package plays every processed clip.
It is a listening handoff, not evidence that every clip has been auditioned or
cleared. The processing is mono PCM16/22.05 kHz, explicit catalog excerpts,
silence trimming with margins, DC removal, bounded RMS/peak normalization and
6 ms fades. The target RMS is 0.16 (about -15.9 dBFS), bounded by a 0.88
sample peak and an 8x normalization limit; high-crest recordings can remain
below that target. Profile and reaction gains are unity, and speech uses the
ordinary 10/60-unit positional range. This avoids the earlier double gain
reduction and shorter range that made voices hard to hear during combat.
No synthesized voice, claimed impersonation or generated substitute
is used. Long recordings retain their full originals in the private cache.

The 18 requested headline references have source recordings in the catalog.
Their proposed placements are adapted to what the game actually reports:
“Boom headshot” is a rare overconfident multikill boast, **not proof of a
headshot**. “That's a lot of damage” follows accepted heavy player damage;
there is no invented structural-collapse event. Launch cues require observed
upward velocity, not the jet button on a NoJet player.

The policy uses existing perception/death/round hooks and health/motion views.
New generic bot reports expose confirmed provider carriage, authoritative
objective completion and repeated failed headway. It never guesses objective
success from arriving nearby, or calls a stationary guard stuck. Idle chatter
is restricted to outside minigames. Roster sampling uses `bots()` because
`players()` contains connected humans only. Health deltas are sampled after
acceptance; the pre-filter `on_damage` hook cannot establish an injury.

Mixing is positional (`speak_at`, with explicit actor attribution), with a 60-unit radius. There are at most two
active reactions globally, with no overlapping reaction sources within 24 units,
at least three seconds between starts, a 10–16 second bot cooldown, and a
120-second global cooldown for iconic references (45 seconds for other clips).
The pending queue holds at most 16 short-lived candidates. Higher priority
reactions supersede a bot's pending chatter; equal priorities rotate fairly.
Confirmed objective outcomes can bypass the bot's spacing, displace combat
chatter and wait briefly, but cannot overlap that bot's voice, even after fast
vehicle travel. Pitch is consistent per bot, with milder treatment for micro sounds.

65/30/5 is a selection preference when a context has micro/quote/rare choices,
not a promise of that measured final mix. The policy stays in the correct
situation when a tier has no candidates. Match reports retain actual cue
profiles, positions, pitches and ticks plus eligible/played totals per event,
clip and tier. They make tuning measurable without changing bot decisions.

Content-free Rust tests exercise the actual policy with synthetic clip metadata:
positioned actor-attributed speech operations, cross-bot iconic suppression, queue bounds, quiet
stationary game bots, accepted bot-health sampling and human silence. The opt-in
saved-match probe also refuses package errors, a missing requested companion or
no reaction presentation during a run of two minutes or longer.

Maxwell authorized default enablement and release bundling on 2026-10-28.
Bootstrap builds the pinned recordings; release CI fetches their verified native
bundle. Both recording and rules packages ship and start enabled, and players
may turn the Add-On off. CI rejects stale recipes, missing companions and changed
recordings. Recording source notices and attribution remain in the generated
receipt; this default does not assert that source rights have been verified.
Network access, copyrighted assets and source-game installations are unnecessary
for the ordinary test suite. Check native imported clips with `bri-addon-check`
and use the ten-minute probe documented in the gameplay progress entry with
`BRI_DRIVE_ADDONS=blockhead_bot,gamemode_slayer,gamemode_slayer_ctf,weapon_hegrenade,npc_popculture`.
