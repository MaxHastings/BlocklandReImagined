# Blockhead footsteps

First-party ReImagined audio, included in the base game. Maxwell confirmed these
recordings are cleared for public distribution on 2026-10-27. They are derived
from the supplied Foley recordings, not from original Blockland game sounds.
No broader third-party license or CC0 status is asserted.

The four mono 48 kHz PCM16 WAVs preserve Maxwell's selected F mix. `recipe.json`
records the source hashes and processing; `sounds.json` is the base Blockhead
archetype's contact policy. Its +2.5 dB contact gain combines with the native
profiles' later -1.5 dB trim: +1 dB overall relative to the original F playtest.
The underlying unprocessed recordings are not needed to build or play the game.

The existing native resource catalog ships these files; its normal sound loader,
spatial placement and hash validation are reused. Bootstrap tracks this folder
as an importer input. Horse and separately authored character sound policies
retain their own configuration. No separate Add-On is required.

`land_01.wav` is Maxwell's selected landing B: a dedicated landing impact plus
a quieter filtered medium-impact body, with a softened attack. His subsequent
+3 dB and +1.5 dB adjustments are baked into the clip. `make_landing.py --source
<supplied-foley-directory>` rebuilds it with NumPy, SciPy and SoundFile;
`landing-recipe.json` records processing and hashes. Source recordings and
Python libraries are not needed to build or play the game. Maxwell requested
including this selected sound in the merged game; no broader source license is
asserted.
It plays once on touchdown through the existing `sounds.land` mechanism, at a
fixed level with the normal positional profile. This is new ReImagined feedback,
not recovered vanilla behavior. No surface variants or hard-fall scaling yet.
