# 2026-10-30 v0.4.2 volume-hook fixture isolation

The full release attempt at `f62e52cee373a152c95919ea5f4ef6c3eea7154e`
passed its build, content startup and Clippy, then ran 528 binaries. Its vehicle
behaviour-band failure persists on isolated retry; the separate volume-hook
case passes alone after failing in the batch. The full gate is failed and has
not reached explicit corpus/performance checks. Native Linux completed its
package/startup checks; Windows/macOS builds were deliberately cancelled when
the source required another correction, not reported as compiler failures.

The volume-hook failure is a test-fixture ownership race: both tests construct
`bri-volume-hooks-<process id>`, and either fixture's Drop recursively removes
that same directory while the other catalog reads `zone/behaviour.json`.
The actual failure is `package.file.missing`, not an ambient-volume event
contract violation. Give each catalog a unique `tempfile::TempDir`, using the
already declared dependency and preserving its original lifetime. All event
and region assertions stay unchanged. Focused parallel verification and the
new full release run remain required.

The vehicle comparison uses identical generated release content and Cargo
target selection: baseline `60fcbe7f4597` passes with fight share 7.4%, whereas
`f62e52cee373` fails at 2.3% under the unchanged 3% floor. Owner correction
`df722713a5f28e2dafd07b2cda809d18200c43e4` passes the original complete
vehicle case by excluding seated riders from a foot-only landing correction.
The source-backed guard and its limits are recorded in the
[owner receipt](2026-10-30-bot-vehicle-release-triage.md). This is not a new
policy/band/seed/layout adjustment; deck safety still needs its retained
controls in the final combined source.

Preserve the actual failures, retries and intentional cancellations under
`artifacts/v042-release/full-gate-f62e52cee373/attempt-1-vehicle-band-failure/`
and `vehicle-case-<source>/`. An avatar portrait “flaky” string before the
Rust test marker comes from a tooling unit-test mock; it is not a product
failure. Earlier genuine mirror/ragdoll failure evidence remains retained.
No release pass or publication is claimed. Maxwell's approved recordings,
mix and human playtest blessing remain unchanged; full alpha stays incomplete.
