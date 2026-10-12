# 2026-10-11 Perf waste fix: the script tick view is built only when a think is due

Finding 17 (server side) of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files).

## What was wrong

With any Add-On package enabled, every 120 Hz tick built the full script
`Snapshot` (a `PlayerView` with several String clones for every peer and
bot, plus every entity and each package's entity variables) before checking
whether any package entity's think was due. Most ticks have none due, and a
game whose packages spawn no entities never does.

## Fix

`Session` collects the due thinks first and builds the tick view only when
that list is non-empty. Hooks that run outside the tick view still build
their own snapshot as before. `Session::package_snapshots()` counts the
snapshots built, for tests.

Correctness: a think still sees the state at the start of its tick, exactly
as before; nothing else read the view.

## Evidence

- Work count: `cargo test -p bri-sim --test packages
  idle_ticks_build_no_script_snapshot` (240 idle ticks of the Stress Lab
  packages build at most one snapshot).
- `cargo test -p bri-sim --test packages`, `cargo clippy -p bri-sim --tests
  -- -D warnings`: pass.

## Next

The client side of finding 17 (item world and projectile pose keys) is a
separate change.
