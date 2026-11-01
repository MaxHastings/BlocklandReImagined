# Preserve stair traversal with level-route edge safety

The full gate on `655a9cc1e` failed the four-full-floor flag test: both bots
reached the staircase but the level-route drop guard held them back while
ascending short treads, so neither returned a flag. With the guard disabled,
the identical focused scenario completed three captures per side. The first
attempt to look ahead only for descending waypoints did not fix it. A trace
showed valid nearby *ascending* waypoints immediately before the veto.

The guard now recognizes nearby planned height changes in either direction,
within three body widths and the next six waypoints. It still applies the
stricter unplanned-drop check to level routes, and does not disable the
existing safety checks on off-route footwork. The four-floor flag test again
completed three captures per side; all six `bot_hardening` cases including
the four deck layouts, all six bot-action safety unit cases, all ten
`bot_routes` cases, the opened-door case and the vehicle TDM case passed.
Formatting and changed-target Clippy passed.

The real Beta16 tower CTF cases again captured without jets (center 3624/6998,
near-wall 3640/6872 pickup/capture ticks). The supplied cube, staircase and
balcony replays loaded every brick and killed the target at sampled ticks
10921, 2401 and 3486. Against the same-setup pre-edge-guard controls those
were unchanged, unchanged and about one second slower respectively. These
finite tests do not guarantee every staircase or fall is safe. Release
performance, the full extended gate and exact-commit platform checks are still
required; no interactive playtest occurred.
