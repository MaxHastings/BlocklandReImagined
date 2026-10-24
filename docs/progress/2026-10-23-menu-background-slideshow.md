# 2026-10-23 Main-menu photo rotation

Maxwell reported that only one background photograph appeared and requested a
random change roughly every five seconds, with a fade, including personal
screenshots. This extends the v0.3.6 candidate; it does not restart the closed
AA2 investigation or change the human interactive-playtest boundary.

## Cause and correction

At candidate `09c1cb633a128c2d652d69772dcfc1098840cad0`, the client collected
stock `screenshots/` image IDs but `NativeScreen::refresh` always selected
`.first()`. No slideshow timer existed. Native captures were written to
`<client-state>/screenshots/` but never admitted to the menu's list. The
v0.3.5 content fix restored ten original photographs; it did not implement
rotation. Original v20 slideshow evidence remains in
[UI behavior research](../research/ui-ux/04-engine-behavior-and-uncertainties.md).
The five-second crossfade cadence here is Maxwell's explicit preference,
not a claim to reproduce v20's exact fade/hold timing.

The ordinary application frame now drives a shuffled photo library: four
seconds held, one second crossfading. Each cycle visits every candidate once,
with no adjacent repeat across shuffle boundaries. JPEG/PNG captures join on
return from gameplay. Discovery and bounded decoding use a worker; two
1920x1080-or-smaller photos occupy the reusable texture slots. Missing,
malformed or oversized photos are skipped with a warning, while a valid
current photo remains visible. A single photo stays visible. Time pauses
outside the menu. Device recreation reuploads the retained two photos.
Save-preview bounded decoding and GPU upload are reused; menu photos preserve
their own aspect instead of the save preview's 4:3 crop. Fade frames update
only the background model and do not rebuild open dialogs.

## Reproducible evidence

Private logs/captures are under this thread's `.research/` and are copied to
`../BlocklandReImagined-private-evidence/zombie-hole-20261023` before cleanup.
No original photographs are committed.

- `menu-slideshow-before.log`: the new application-frame regression rejected
  the old production path: `Image("screenshots/one")` still appeared after
  six seconds. `menu-slideshow-after.log` records the correction passing.
- The permanent application test waits until both photos are prepared before
  advancing time, so delayed initial loading cannot masquerade as rotation.
- The first offscreen implementation used normalized UV endpoints where the
  renderer expects normalized origin/extent. The portrait capture rejected
  that real cropping mistake (`menu-slideshow-portrait-before.log`). The
  correction uses the existing `cover_src` geometry with normalized extents.
- A separate lower-right pixel assertion sampled the original copyright
  artwork. Visual inspection identified this harness mistake; the lower
  sample moved left of that artwork. The upper and lower samples still reject
  a stretched portrait. This was not another production correction.
- The actual converted v20 menu, stock photos and native GPU texture path
  produce before/half/after captures. The halfway image is checked against
  the independent weighted average of endpoint pixels, with a small
  quantization tolerance. One run observed **379,541** visibly changing
  pixels. Device recovery reproduces the same frame. A newly created native
  screenshot is discovered on return and renders with verified portrait crop.
- `menu-slideshow-corrupt-repeat-before.log` rejected a real repeat: skipping
  a bad file exposed the current photo inside an already-started shuffle.
  Selection now excludes the current photo after every skipped candidate,
  not just at the start of a cycle. The retained test pins the failing order.
  Returning from gameplay also discards an unfinished pre-game scan so it
  cannot hide newly captured photos.
- Content-free tests cover empty/single libraries, shuffled coverage and
  boundaries, JPEG/uppercase PNG admission, corrupt files, directory exclusion,
  return refresh, in-game pause and repeated two-slot reuse. A corrupt file
  stays an indexed candidate until visited; the test verifies its eventual
  removal instead of assuming discovery already decoded every file.

Commands from the owned worktree, with `CARGO_BUILD_JOBS=2`,
`CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_TARGET_DIR` unset:

```powershell
cargo test --locked -p bri-client --lib menu_ -- --include-ignored --nocapture
python tools/check.py quick --crates bri-client,bri-ui
python tools/check.py focused --crates bri-client,bri-ui
```

The offscreen case additionally uses `BRI_CONTENT=<worktree>/content` and
`BRI_MENU_OUT=<worktree>/.research/menu-render`. Final exact source pins,
tier results and publication status belong in the v0.3.6 publication receipt.
The obsolete 09c build-only release was canceled and its local gate stopped
when this additional correction changed the candidate. Neither was published.
A Windows link retry was needed after two local check invocations overlapped;
the executable was still running, not a source compilation defect. Final
checks run sequentially against the finished candidate.

Final local acceptance: quick passed in 145 seconds (666 client unit tests,
239 UI unit tests); the final focused tier passed in 120 seconds, including
strict all-target clippy, UI/importer integrations and client units. All four
menu cases then passed together in 10.95 seconds with the assembled public
release content: 160 installed Add-Ons, 78 selected packages including the 19
base packs. That final capture observed 243,679 visibly changing pixels.
All 113 Python tool tests pass. Logs are `menu-quick-verified.log`,
`menu-focused-accepted.log`, `menu-slideshow-accepted-final.log` and
`menu-tool-tests.log`. This does not substitute for the integrated gate or
three-platform archive checks.

Maxwell still owns acceptance of the pacing and appearance on his display.
Offscreen pixels establish rotation/compositing/crop, not subjective preference.
