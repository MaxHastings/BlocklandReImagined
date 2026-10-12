# 2026-10-12 Load-path waste: first Host, first frame, Single Player shelf

Round 2 of the wasted-work audits, after PR #95 (every Host and Join
re-hashed all content). This round looked at work redone on events
(launch, Host, Join, Change Map, menus, settings, saves) rather than per
frame or tick. The ranked report with every finding is
`perf/waste-audit-2-load-paths.md` in the project files.

## Evidence (Max's v0.2.10 session logs, 2026-10-12)

- All 7 sessions that hosted have one frozen frame right after IN GAME on
  the run's first Host: `Long frame: 3085-3612 ms ... draw (record passes
  1006-1136, Add-On draw setup 219-300, ...)`, with ~1.6 s of the draw in
  no span. A second Host in the same run: worst frame 162 ms.
- The Add-On health warnings are printed a second time just before the
  first Host's LOADING MAP: the first Host reloads every Add-On.

## Changes

1. **First Host no longer reloads the Add-Ons startup just loaded.**
   `App::load` left `packages_from_tools` false, so the first HostGame or
   StartTutorial (and the first close of the Add-Ons screen) ran a full
   `PreparedPackages::load` of the same list: client content, every item,
   vehicle and avatar asset, catalogs, client code, sounds, health. Startup
   now marks the list it loaded as current when nothing was left out (the
   left-out path already did through `apply_packages`). Test:
   `the_first_host_after_startup_does_not_reload_the_add_ons` (fails
   without the change).
2. **Unified/Dynamic world pipelines compile on the startup worker.** The
   12 soft-shading world pipelines and the 2 AO-mask pipelines were created
   lazily on the first in-game frame; on DX12 their first DXC compile is
   ~1.5 s, after which the process cache makes them cheap (hence only the
   first game of a run froze). `SceneRenderer::prepare_lighting_pipelines`
   builds them in the "scene pipelines" worker, which the loading screen
   already waits for. The lazy paths stay as fallbacks.
3. **Single Player builds no download shelf.** `PackageShelf::new` reads and
   SHA-256s every Add-On file (not covered by #95's cache) for joiners, on
   every Host; Single Player has none.

## Checks

`cargo clippy -p bri-render -p bri-client --all-targets -- -D warnings`
clean; `cargo test -p bri-render -p bri-client --lib` 518 + 60 passed (cloud,
lavapipe); `cargo test -p bri-client --test graphics_rebuild` passed.

## Not measured, next

The first-frame time on Max's PC needs a run: expect the 3 s frame gone and
`Compiled scene pipelines in` at startup to roughly double (behind the menu).
What is left on that frame: Add-On GPU calibration (~0.2 s, blocking poll),
`EnvironmentProbe::new`, the reflections reset on the first light-volume
upload, the first kept-shadow draw. Larger open findings (Change Map GPU
restart, per-Host definitions/materials/weapon identity rebuilds, save list
reading every save in full, the LAN shelf needing #95's cache) are in the
report for Max to pick.
