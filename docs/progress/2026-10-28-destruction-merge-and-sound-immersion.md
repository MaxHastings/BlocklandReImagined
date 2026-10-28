# 2026-10-28 Destruction merge and sound-immersion continuation

PR379 merged into main 72632dfec091c5e0d8143958e23350400d152c2b after Maxwell
explicitly authorized merge before his interactive playtest. Its tree matches
validated/packaged engine candidate 61afa2ca58a8036ff8e6a1d58c22d96d383c14ed.
The dirty shared main checkout remains untouched. This existing worktree is now
reused on codex/brick-destruction-audio; preserve its warm playtest target.

## Destruction delivery evidence

- `python tools/check.py focused`: formatting/all-target clippy and 2786 tests
  passed across 239 result targets, 256 ignored.
- Final client correction: `python tools/check.py focused --crates bri-client`
  passed 719 units, 78 ignored, plus all-target clippy/format, 178 seconds.
- `python tools/gate.py` on final head passed 559 test binaries, content startup,
  all-workspace build/clippy and all 23 fixed saves. Three tests passed isolated
  retry and remain reported flaky: ragdoll soft-cone, body-attachment rendering,
  and package-region query. No failure was waived. Final gate total 954 seconds
  after obtaining the lock; previous candidate's full gate also passed.
- Standard 32-bot/3600-tick release performance: Collage mean 5.00ms / 7.00ms
  budget (p99 11.3ms); Afghan 3.44ms / 5.50ms (p99 7.2ms).
- Windows/Linux CI1353 passed on final PR head, run40660801252.
- Warm optimized build of all five paired executables passed; final client
  iteration 92s. Packaged hidden `--check` exited0: 21 maps,1396 definitions,
  35 save pictures,14 modern-light maps. No window/audio device opened.
- Preview.4: E:/Downloads/Destruction-preview-2026-10-28-r4/
  BlocklandReImagined-v0.3.7-destruction-preview.4-windows/Launch.cmd.
  Verified13174 hashes,117 unique package identities/directories and ZIP CRCs.
  SHA256 client:26a0dd8eb38c3e3828fde93f777ba025b8246d7a783c88bed8841cb029667600.
  External DESTRUCTION-PREVIEW-RECEIPT.json records exact source/merge/checks.
- Optional Butterfly Knife missing fire-sound/radius kill-message diagnostics
  match preview.3; inherited source-lighting fallbacks are unchanged.
- Interactive feel/visual acceptance remains Maxwell's. No gameplay input
  automation or visible game launch was performed. Full alpha is still open.

## Next goal: restrained destruction sound immersion

Maxwell asked to continue after destruction, and explicitly asked to set a goal.
The active goal covers a sound playtest using actual recordings or suitable
existing sounds, no generated/AI sounds, no extra user toggle. Keep explosions
believable; use variation and occasional distinctive plausible outcomes, with
bounded local voices/rate so a big collapse is not a sound per brick. This new
increment is separate from mergedPR379; no audio production edits yet.

Read the current audio architecture/source before implementation. Existing
`ClientAudio::contact_profile` queues at most16 local starts/frame, uses seeded
weighted choices, shares decoded assets and ordinary mixer/carry/reverb. Reuse
that seam rather than adding replicated collision/audio events or another mixer.
App drains particle contact audio around frame.rs1709; brick debris steps around
2030. Current legacy BrickKill audio includes Partition and streamed Section
members with an80ms server-tick gap: consider actual audible timing carefully.
Resting contacts/proxy insertion/preparation must not generate endless clacks.
Shared analytic landings need an actual contact location, not an empty bounding
center. Keep policy/asset ownership in native packages and the engine mechanism
bounded. Do not introduce a broad simulation or hardcode guessed foreign policy.

Source discovery (read-only pages, not yet downloaded audio):
- https://freesound.org/people/rioforce/sounds/233654/ (LEGO clicks,CC0)
- https://freesound.org/people/rioforce/sounds/233653/ (LEGO clicks,CC0)
- https://freesound.org/people/rioforce/sounds/233661/ (LEGO pour,CC0)
- https://freesound.org/people/C_Sander/sounds/721040/ (single drops,CC0)
- https://freesound.org/people/C_Sander/sounds/721039/ (multiple drops,CC0)
- https://freesound.org/people/andresix/sounds/347807/ (dice/paper,CC0)
- https://freesound.org/people/Flem0527/sounds/629982/ (dice/table,CC0).
Primary Freesound pages verify these recording descriptions/licenses. HTML
snapshots for233661,233654,721040 are under ignored artifacts/sound-immersion/
source-pages; they expose legitimate public HQ MP3 previews. Original downloads
require login; do not bypass it. Record any preview compression, exact hashes,
trims, levels and author/license in eventual deliverable. SoundSpool's clothespin
listing is secondary and its original has not been verified; do not adopt blindly.
Do not claim listening based only on waveform metrics. Maxwell owns interactive
listening; offline mixer output and technical audio checks are not human acceptance.
