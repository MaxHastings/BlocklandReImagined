# Current private preview

Delivered `E:/Downloads/BlocklandReImagined-v0.4.3-preview.2-windows`,
launchable via `Launch.cmd`. The five shipping executables are the current
warm playtest build, reporting `v0.4.3-preview.2 (614c1e675)`; subsequent
commits before delivery changed only documentation. The current `voice_chat`
and generated `map_ambience` packages are included. The packaged outdoor OGG
matches the selected recording's f4d0f075...f909f29 hash.

To avoid holding a private playtest on unavailable original archives, preserved
the previously converted original Add-Ons from preview.1, verifying every
previously manifested file except `content/packages.json` against its old
manifest. That file was changed by the user's Add-On selection and was carried
forward. No old converted Add-On was relabeled with a new importer receipt.
This is the same preview strategy documented in `2026-10-23-supplied-hole-bots.md`,
not a current-importer bundle or a final release package. Copied files were
inventoried in a new manifest; an independent check found zero mismatches among
13,460 entries. The old preview and original content installations were not
modified.

Packaged `bri-client --check .\content` exited 0: 20 maps, 1,396 brick
definitions, 14 modern light parameter sets. It reports 46 Add-On health
problems and 151 fallback entries; those are not a clean content acceptance.
No game window or audio device was opened. The check wrote normal fallback
diagnostics to the user's standard local game logs. This handoff does not
replace final bundle regeneration, platform CI, release gates, or the remaining
gameplay work.
