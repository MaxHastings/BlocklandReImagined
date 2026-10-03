# Bot weapon capabilities — v0.2.1

The native adapter ranks supported inventory slots using their authored attack
mechanisms, current visible opponent, ammo, travel time, draw delay, current
collision and blast clearance. Stable slot ordering and a 0.15 score margin
keep equally useful gear from flipping. It produces ordinary equip/aim/trigger
controls; the existing reaction delay, aim error, turn speed, minigame policy,
image state machine and damage runtime remain authoritative.

## Grounded mechanisms and source coverage

Read-only inspection of `content/weapons-pack-009/weapons.json` confirms these
actual stock definitions have ordinary single-projectile flight without an
alternate state shot, spread, recoil, volley or script descriptor:

| Gear | Speed at scale 1 | Gravity multiplier | Lifetime ticks | Descriptor |
| --- | ---: | ---: | ---: | --- |
| Gun | 90 | 0 | 480 | Direct projectile; ordinary semi-automatic state cadence |
| Rocket Launcher | 65 | 0 | 480 | Direct + splash; damage radius 3, impulse radius 6 |
| Bow | 65 | 0.25 | 480 | Direct ballistic projectile |
| Spear | 50 | 0.5 | 2400 | Direct + splash, release-only charge |
| Sword | 50 | 0 | 12 | Explicit native melee projectile; short effective reach |

No names or IDs determine these capability families. The Sword's native runtime
can correct speed using eye-to-hit / muzzle-to-hit distance. The current Session
hand frame explicitly sets both muzzle origins equal to the eye, so this ratio
is exactly one. The adapter must be revisited when authored animated muzzle
poses replace that temporary host frame. Damage and blast scale use the runtime's
body scale; fixed direct damage stays fixed. Blast safety includes the larger
of damage and impulse radii. Actor-impact arming follows the authored ballistic,
explode-player and explosion fields, rather than suppressing a damage-only arrow
because its effect has a late arming tick.

Akimbo's second firing state is excluded, including its implicit stock left-hand
mount. HE Grenade has a scripted fire/use-up descriptor and a timed/bouncing
explosion, so it needs an explicit lifecycle provider. Recoil, spread, lobs,
state-specific/last-round shots, moving/rested substitutions, volleys, alternate
hitscan explosion/ricochet/flown behavior, cooked in-hand fuses, projectile child
spawns and auras are excluded. A no-damage native tool and a flying projectile
that cannot collide with actors are excluded. An absent projectile is never
invented as a melee attack. Excluded tools retain the existing executor.

## Flight and bounded work

`tactics.rs` solves moving-target interception using the exact native
semi-implicit flight law at 120 Hz: velocity loses gravity before position
advances. Each free-flight segment gives a quadratic in time, including a
fractional final segment. Solutions retain earliest and latest distinct feasible
arcs. Shooter velocity inheritance, zero gravity, finite inputs, positive time,
lifetime, normalized direction and the runtime's 10,000 speed bound are checked.
The pure search resumes under an explicit quadratic budget and allocates no
per-segment storage. Exhausting its budget does not assert unreachability.

The live adapter bounds its supported flight horizon to 256 ticks (2.13 seconds),
checks low arcs first and searches high arcs only within that horizon. Longer
flights close distance using an effective movement reach instead of waiting
forever for a plan. An out-of-horizon result is distinct from temporary global
budget exhaustion. Mounted shots and portal-carried paths retain existing
behavior until an exact typed adapter covers their additional launch transform.

One shared allowance caps 2,048 quadratic solves and 544 native sweep queries per
simulation step. Speculative paths cannot spend the 256-query critical firing
reserve or the 32-query cheap held-fire allowance. A queue rotates expensive
planning across observed fighters, so colliding owner-ID residues cannot starve.
Curved paths use exact runtime tick chords; no coarse chord leaves gravitational
sag unchecked. Straight paths use a conservative full chord; starting inside an
authored ray brick can be refused even where native projectile source grace
would allow it to exit.

Current trajectory checks conservatively include allied bodies and their motion
envelopes. The ordinary query checks ray flags, native map/brick/shape collisions
and passages. Intended-current-body hits receive a separate world probe for a
flying projectile so the body cannot hide an obstacle. An instantaneous ray may
stop its bounded check only after its actual direction hits the intended current
body. A ray miss is refused, rather than silently ignoring the remainder of its
full range.

## Trigger and launch authorization

A release-only charge must have no initial onFire state and no timeout, held,
ammo or loaded edge into onFire. Unknown mixed charge graphs require an explicit
provider. Holding such a validated charge can continue while waiting for the
fair trajectory-validation turn; releasing is authorized separately. Unsafe or
abandoned charge intent cancels through ordinary safe controls rather than
releasing a projectile. Semi-automatic pulls follow actual current image
readiness instead of a global 40-tick pulse. The ranking cadence is a conservative
estimate of the authored native state cycle; the image runtime decides actual
fire timing.

The central integration sets all live weapon frames after player movement and
physics, then revalidates supported intent at trigger consumption. It checks
plan tick, selected slot, mounted image, live ammunition, shooter/target life,
resting state and current hostility. It traces the actual executor direction
after turn limits and aim error, rebasing origin/inherited velocity to the live
launch frame. Re-derived current image/projectile capability must also match the
planned descriptor, including final actor scale; an intervening package change
invalidates the launch. For a flying shot, the endpoint must still intersect the observed
target's constant-motion bounds. Geometry is not cached across frames. Target
motion can change after launch; the check is a grounded prediction, not a promise
about future player decisions or all travel after an unexpected miss.

`WeaponsWorld::ammo_on_equip(actor,slot)` projects canonical next-equipment ammo
without mutation. A never-drawn magazine gets its authored first-draw size and
only an absent shared reserve gets its authored initial amount. Stored depleted
magazines and empty shared reserves stay empty. Existing `ammo()` continues to
report custom mounted images and stowed last grenades. Busy image transitions
can defer equipping; the projection is not permission to switch immediately.

## Scripted tools and explicit next providers

Gravity Gun already uses typed grab/release/reel mechanisms: force, near/far
reach, allowed movable targets, permission and collision, continuous holding,
and release that retains motion. A manipulation provider should describe those
mechanics and a dated observed object/opponent opportunity. It must not assign
imaginary projectile damage or an invented throwing model.

Grapple Rope already uses typed tether/untether around an observed projectile
anchor, line clearance and authored rope limits. A mobility provider should
validate those facts and the body's path. The inspected rule fixes the anchor
at the contact point and requests `swing: 0`; it does not establish winching or
movement-key pumping skill. Its cleanup loop currently enumerates players,
whereas Gravity Gun explicitly includes bots; that difference needs package
verification before claiming NPC mobility support.

Vehicle utility, transported teams and general scripted support/coordination
remain future work. Existing seat claims and promotion remain in place.

## Evidence and remaining verification

The pure module and live-frame tests, focused ammo tests, actual-control chaos
fixtures, optimized combined model/combat measurements and real Bedroom/Beta/ACM
battle runs are recorded in the owning progress entry and performance audit.
This audit describes the admitted mechanism envelope. It does not claim that
unsupported script tools or every native secondary-effect weapon are covered,
or that headless macOS step measurements establish shipped Windows frame rate.
Interactive acceptance remains Maxwell's work.
