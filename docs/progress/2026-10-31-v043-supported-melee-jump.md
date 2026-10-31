# Supported melee jump

The no-jet cube baseline in `2026-10-31-v043-cube-after-objective-routes.md`
left the target alive at 10 health after 120 seconds, despite actual support
contacts. A melee bot on a live allied actor can now make an ordinary vertical
jump when a visible elevated opponent is within its measured melee reach at a
reachable point in that flight. It checks a full-body sweep through fixed
collision before jumping; the motor and ordinary combat own the result. The
action is used only when a route is absent or starts below the support; the
support and rider must be nearly still, and bot stacking opt-outs are respected.
It does not use unseen live target coordinates, inject velocity or increase reach.
Dynamic actors remain subject to the motor's collision checks, not a predicted
moving-support plan. A direct path through a portal is not assumed to be a
vertical jump.

The preliminary action was instrumented: it fired 16 ticks in the 7200-tick
cube run. After adding the fixed-world clearance check and stable-support
conditions, the actual saved encounter killed the player at tick 8479; the
zombie archetype has `can_jet: false`. The 120-second report
`v043-cube-supported-jump-gated.json` records zero bot-step failures. Earlier
instrumented and clearance trials are retained in the private diagnostics.
The tower and staircase saved encounters still killed at ticks 4973 and 3240,
respectively, with zero bot-step failures. These are headless results, not
visual or human-playtest claims. A focused reach-geometry regression,
`navigation_body_tests` (12), `bot_knowledge` (12), and warning-free sim/net
Clippy passed. Moving supports, general coordinated piles and combined jumps
are not established by these tests.

The broader `bot_routes` check initially failed an older test that demanded
giving up on a visible player above an unreachable roof. That contradicts
Maxwell's explicit request to keep pursuing visible elevated players. Replaced
that expectation with a check that the bot executes the reachable partial
route and continues pursuing for at least five seconds during the second ten
seconds. All ten `bot_routes` cases then passed. This is an intentional
contract change, not a claim that the original assertion passed.

Maxwell clarified that a kill on the supplied cube is the practical desired
outcome and asked not to let this feature expand unnecessarily. This passing
saved-world result is useful acceptance evidence for that scenario; continue
the remaining v0.4.3 work without presenting it as a guarantee for arbitrary
geometry or support motion.
