# Recorded brick destruction audio

This native package supplies a single cosmetic response policy and eleven short
mono WAV exports. Existing whole-brick break sounds retain their identity.
Sections snap when coherent motion starts; sizeable assemblies get a short
recorded scatter on actual landing. Smaller sections use the single-contact
sound. Later contacts select a quiet plastic click. There is no continuous
rattle loop, server audio event, new gameplay collision or user switch.

The recordings are real LEGO taps, drops into a pile and a bag poured on a table,
by rioforce and C_Sander on Freesound, licensed CC0. The snap role is a short
recording of bricks tapped together, not a claimed recording of studs tearing.
Inputs are publicly playable HQ MP3 previews, not the original WAV masters.
Exact source links, input/output SHA256, trims and modifications are in SOURCES.json.
No generated audio or synthetic replacement sound was used. Processing is mono
conversion, resampling, DC removal, tiny entrance/exit fades and bounded gain.

The mixer still owns spatialization, room/sound-carry effects and volume controls.
The authored ranges are 24 units for snaps,18 for individual contacts and40 for
landings. Contacts need an impulse-equivalent speed of1.2 units/s; normal quiet
resting contact is excluded. The engine bounds reporting to64 per frame, starts
to an8/sec token budget with a burst of3, plus body/cell cooldowns. Large shared
sections use one landing cue; they do not play this full sound per constituent
brick or per collision manifold. Pitch varies only between0.94 and1.06.

Runtime files are committed. Rebuilding requires an installed ffmpeg, but playing
or building the game does not. Run from the repository root:

```sh
python tools/prepare_brick_audio.py --inputs /path/to/cache --ffmpeg /path/to/ffmpeg --fetch
```

The tool only fetches pinned public previews and rejects changed hashes. It does
not authenticate or bypass the original-download login. These are technical
preparations for Maxwell's listening/playtest, not a claim of human acceptance.
