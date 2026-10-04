# 2026-10-02 Bot world interaction and allied crews

Implemented on `codex/bot-vehicle-coordination`, branched from main `7ca4b632`.
The branch is for integration review; it has not been merged or released.
Max authorized GPT-6.1 Sol parallel audits and new regression files. Root owns
shared engine integration. No interactive game session was run.

## Mechanism and policy

The existing utility brain now considers environmental opportunities alongside
fighting, chasing, searching, carrying and returning home. It observes authored
capabilities rather than Tank/Jeep/Steel Ball identities. Ground seats and
seatless contact hazards share one small lifecycle: discover, score, reserve,
approach, execute through ordinary mechanisms, measure progress, fail or release.
No behaviour tree framework, recursive planner or imaginary physics force was
added. The [design](../architecture/bot-interactions.md) records the boundary,
budgets, extension rules and further combinations this foundation can support.

`interact` defaults to 0 for kinds that do not choose it. The standard Blockhead
package explicitly opts in with weight 1. Zombie/Shark policy stays in their
packages. Default Add-On enablement is unchanged. Original installations,
authored content identities, save schemas and wire messages are unchanged.

Seat decisions use `controls`/`weapon`, live occupancy, body ride capability,
trust/minigame authority and the normal ride hook. Drivers and gunners have
separate control frames; passengers retain world aim through relative seat look.
Human occupancy overrides bot intentions. Driver/gunner indexing was corrected
through simulation, vehicle stepping/attribution/checkpoint restoration and
client view/prediction; it no longer assumes seats 0 and 2.

A driver plans with chassis footprint and wheel-plane hull clearance and checks
its actual oriented hull before applying throttle. It brakes for allies in
forward and reverse, waits a bounded time for a crew, replans when motion makes
no progress, and relinquishes an unproductive seat. An unarmed chassis can pursue
an enemy and cause ordinary physical runover harm. A gunner uses the actual
mounted projectile, speed/scale, muzzle, charge state and independent aim.
Passengers or gunners that need travel without a driver leave after a bounded
wait. Broken turrets remove the usable gun capability. Existing v20 dismount
search and fallback remain the exit mechanism.

All walking actors now transfer a finite, inelastic share of momentum stopped
at an actual motor sweep to an eligible dynamic body. Permission and held-object
checks are shared for humans and bots, and successful physical contact uses the
existing mover credit. Contact point, normal and velocity retain their owning
collider's frame through portal soup clipping/filling, including contact before
the walker crosses. Approach and allied corridors use oriented object geometry.
The Steel Ball keeps its authored mass/friction/impact threshold: rolling is
possible; flat walking is not a guarantee of lethal speed. Gravity, slopes and
existing momentum can change the result through normal physics.

Enemy evidence records identity, position, observation tick and expiry. Allied
warnings preserve its age, never read hidden movement, and never refresh old
information just by relaying it. Death/removal and alliance changes invalidate
hostility. Friendly-fire permission does not define an enemy. Hitscan reach is
read from the actual ray. Invalid hostile intent aborts a held hand or mounted
charge instead of taking its release-to-fire transition. The hand image restarts
through existing mount events, preserving selection, paint and inventory.
Unrelated crew members leaving do not cancel another gunner's charge.

Read-only `Session::bot_thoughts` exposes behaviour, dated evidence, goal, path
and current claim for headless diagnostics; it cannot assign a bot's decisions.
Claims are bounded (16 active, 64 failures), expire after 3 s without progress,
have a 15 s maximum age and a 2 s resource-specific retry delay. Discovery shares
32 visibility queries per tick, visits at most 8 objects per bot, rotates callers
and cursors, and retains existing navigation budgets. Steering looks at no more
than 24 nearby path points.

## Evidence and repairs

New content-free session suites exercise actual autonomous choices, movement,
projectiles and damage:

- `bot_interactions`: 14 cases covering unfamiliar reordered seats, actual
  driving/gunner fire, passenger world aim on a rotated chassis, charged fire
  and rearming, lone-driver bounded wait, human occupancy, permissions, disabled
  kind policy, death/removal/rest, physical runover, allies ahead/behind, and a
  loaded narrow gap that requires going around or leaving the driver seat.
- `bot_knowledge`: 8 cases covering paired worlds with different hidden enemy
  positions, memory age/expiry, death and alliance invalidation, human/brick/rules
  bot alliances with friendly fire enabled, actual long-range hitscan damage,
  immediate reservation release when a rules bot is removed, and an Armed spear
  cancelled on alliance change then usable again when hostility returns.
- `bot_physics_interactions`: 11 cases covering normal human walking contact,
  no-contact and mass controls, authority, obstruction, gravity-assisted impact
  attribution, autonomous unnamed-hazard approach/pushing, held objects, allied
  corridors, and abandonment/cooldown for an immobilized hazard.
- `sweep_contact_frames`: 2 cases verifying source contact coordinates stay
  unchanged and far-side contact coordinates transform before centre crossing.
- Unit/vehicle checks cover claim arbitration/capacity/expiry/progress, finite
  contact impulses, full-width chassis navigation, low spawn-plate clearance,
  reordered/ambiguous roles, mounted charge cancellation and independent crew
  charge state.

Failures drove integrated repairs rather than forced scenario success. Stale
seat snapshots invalidated a gunner's claim immediately after the driver boarded;
live occupancy fixed admission. A nav cell's arrival tolerance left a gunner
4.016 m from a 4 m seat; nearby terminal interaction approaches now continue to
the authored point through the ordinary motor. A spawn plate was mistaken for a
chassis obstruction; wheel support and hull clearance are now distinct. Contact
impulses against a wall were not progress; lease renewal now also requires actual
directed displacement. The blocked-body regression accepts ordinary motor
replan exhaustion as an early failure as well as lease expiry and verifies the
same retry delay. Its fixture physically constrains the body, because abandoning
a deliberate push does not forbid subsequent ordinary walking contacts.

Review also found and repaired orphan claims after bot removal, old timers on
reboarding, stranded out-of-range gunners, broken-gun selection, double-scaled
runtime body dimensions, rotated loose-object approach extents, portal contact
frames, hand charge cancellation and unrelated-seat charge cancellation.
Armed-crew fixtures use low invented projectile/contact damage to keep a target
alive for multi-shot observations; a separate full-damage unarmed regression
requires real runover harm. Test fixture ownership and geometry were corrected
when they did not actually represent the intended permission or obstruction.
The broad run exposed a mistaken new navigation assertion counting more than
four waypoints despite `simplify` deliberately reducing a straight run. The
check now requires an actual complete path reaching within 1 m of the goal,
forbids jump/crawl/portal steps and also verifies a taller obstacle still blocks
the chassis. This changes test evidence, not navigation behaviour.

The original-content pass also exposed a synthetic human boarding regression:
the motor reported an arbitrary corner of a flat contact patch, which gave a
centred walker an artificial lever arm and spun the parked chassis. Contacts now
report the mean of the equally near clipped vertices on that physical feature.
Sweep timing and normal resolution are unchanged. Exact ordinary/portal contact
coordinates and both synthetic/original walking-and-jumping boarding checks
pass after the correction.

Commands run use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`, with
`CARGO_TARGET_DIR` unset, and `--locked`. Final validation is recorded below.

| Command | Result |
|---|---|
| `cargo test --locked -p bri-motor -p bri-vehicles -p bri-weapons -p bri-sim -p bri-chaos` | 861 passed, 0 failed; 202 opt-in cases ignored in this run. All 33 new bot session cases and both portal contact cases passed. |
| `cargo test --locked -p bri-motor -p bri-vehicles -p bri-weapons -- --include-ignored` | 318 passed, 0 failed, 0 ignored, including original mounted charge and independent crew state cases. |
| `cargo test --locked -p bri-sim -- --include-ignored` | 646 passed, 0 failed, 0 ignored against the final contact implementation and original content. |
| `cargo test --locked -p bri-sim --test vehicles -- --include-ignored` | 51 passed, 0 failed, 0 ignored after the contact-patch correction. |
| `cargo test --locked -p bri-client only_a_steering_seat_drives_its_vehicle` | The focused client steering assertion passed; other client cases filtered out. |
| `cargo clippy --locked -p bri-sim -p bri-vehicles -p bri-motor -p bri-weapons -p bri-chaos -p bri-client --all-targets -- -D warnings` | Passed against the final source. |
| `cargo build --locked -p bri-client`, then `target/debug/bri-client --check` | Build and startup validation passed: 14 maps, 963 brick definitions, 35/35 save pictures, no Add-On health problems. No window or audio device opened. |
| `rustfmt --check --edition 2024 --config skip_children=true` on the 27 touched Rust files; `git diff --check` | Passed. |

These are local component checks, not the combined release gate. Test counts in
separate rows overlap. Generated content was held stable for these checks; the
release coordinator must validate its repaired content snapshot after syncing
it. Original source installations were not modified.
All content-consuming commands have finished; syncing the coordinator's repaired
generated snapshot can proceed without changing inputs under these test runs.

## Integration and playtest handoff

The release coordinator is combining other work in an isolated worktree. Preserve
both sides of overlaps in `session.rs`, `combat.rs`, `movables.rs`, `vehicles.rs`
and `motor/player.rs`. Its Workshop mover-credit fallback must use the actual
`Definition::control_seat`, rather than `occupants.first()`. Its independent
teleport-yaw normalization repair should be preserved alongside motor contacts;
this branch did not duplicate that repair.

Aircraft/boat piloting, strategic objectives, cover tactics, construction and
predictive multi-body physics planning remain future providers. Shot safety is
local and straight-segment based, not a full ballistic/portal friendly-fire
planner. Ordinary equipment preference remains the existing policy. Conservative
chassis navigation may reject routes an expert human could negotiate. Windows
packaging/gating and interactive tactical feel are not certified by these local
checks. No alpha acceptance item was checked off.

After integration review and the release gate, Maxwell should test: an ordinary
Tank/Jeep with two allied Blockheads; a human driving with bot passengers; a
parked or destroyed turret; a teammate crossing the driver's forward/reverse
path; a route that narrows or becomes blocked; losing a target or changing teams
while charging; a loose ball on flat ground and a slope; and vehicle removal or
rules rest/resume. Observe natural successes and failures instead of forcing the
examples. No packaged release is claimed by this branch.
