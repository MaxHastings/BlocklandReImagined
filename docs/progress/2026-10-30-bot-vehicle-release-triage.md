# 2026-10-30 Vehicle match release-check triage

Root requested bounded source-only triage of the actual full release failure
`bot_gauntlet::vehicles_are_driven_and_crewed_in_a_team_deathmatch` on candidate
`f62e52cee373a152c95919ea5f4ef6c3eea7154e`. Its enforced fight floor remains 3%;
the actual 2.3% fails before the caller reaches ride printing and crew checks.
Mounted activity already overlaps combat in `gauntlet/shares.rs`, so this is
not the old replacement-of-activity counter error. No band/assertion changes.

Root isolated the same original case against the same regenerated content and
focused build graph: baseline `60fcbe7f4597` passes at fight 7.4%, vehicle 73.2%,
7 kills / 204 shots, crew 149.1 s and gunner 156.5 s; f62 fails again at 2.3%,
matching the full run and its retry. Evidence is coordinator artifacts
`vehicle-case-60fcbe7f4597/` and `vehicle-case-f62e52cee373/`; original full-run
evidence remains `full-gate-f62e52cee373/full-gate.log`.

The recent lower-waypoint landing correction is for a body actually standing
on another body. Its `driving.is_none()` test does not mean on foot:
`bot_vehicle_body` supplies chassis navigation only for controlling wheeled
seats, so passengers/gunners have no driving body. `vehicles::follow_seats`
places every rider with `Player::place`, which explicitly sets grounded true.
Thus a mounted non-driver above the world floor can satisfy `on_body` and be
given a new foot landing waypoint, even though the seat owns its movement.

Root authorized the minimal typed correction after the baseline comparison:
require `!self.seated(bot)` only on that lower-waypoint foot step-off block.
Actual players standing on roofs or allies' heads retain the proven nearby-
floor landing fix. Leave `on_body`, perched/goof policy, the fight floor,
seeds, layout and all existing functionality assertions unchanged. This is a
seat/locomotion ownership guard, not tuning aimed at a percentage.

Formatter/diff checks pass; runtime causality and acceptance are pending root's
broker rerun of the original vehicle case, both deck guards and related
retained contracts. No Cargo/build/main push ran here. A source-only opt-in
pre-finish seat/grounded/waypoint and ride diagnostic patch is preserved at
`artifacts/bot-release-blockers/vehicle-pre-finish-diagnostics.patch` if the
unchanged case still fails; it is not included in this minimal correction.
