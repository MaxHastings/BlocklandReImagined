# Recovery probes must preserve the body's actual headroom

Underground recovery warnings led to a source-free counterexample: a synthetic
four-unit body with 0.01 clearance above its top and a small floor contact. Recovery raised the
entire query hull by ROOM_LIFT (0.05) to clear the floor. That creates a false
ceiling overlap and can repeatedly relocate an otherwise fitting character.
The independent small placement fixture fails before correction.

The candidate queries the real hull at its real pose and inspects all contact
manifolds. It tolerates only a supporting-floor contact within the existing
0.05 floor allowance and at the feet. Ceiling, wall and deeper floor contacts
still block. All manifolds matter because floor and ceiling can belong to one
map collider. Unsupported/empty contacts remain conservative. Zero-depth edge
contacts from composite-cylinder queries also remain conservative; otherwise a
retained real-ceiling negative control finds a false free recovery location.

The fixture covers boxes, cylinders and spheres against separate solids and a
combined triangle mesh. It rejects real floor/ceiling penetration, then walks
normally through the tight room without recovery relocations. All motor tests
pass in the intermediate run; the final retained walking fixture passes all
three spawn-recovery checks in 0.02s. The first sphere fixture required equal
stand/crouch heights to satisfy its actual schema. Baseline, debug and passing
logs remain under ignored .research/pipeline-integration/tight-headroom-*.
Recovery warnings now include owner identity so future reports can distinguish
a human pawn from bots. The same floor-contact rule also covers room checks for body resizing,
crouch release and rising from prone. The retained prone-to-crouch fixture
fails before that extension and passes after it, while a real low ceiling still
prevents standing. All 36 motor checks pass after the complete correction.

Not integrated or shipped. Quick passes; focused passes in 1,019s (nine dependent
crates, client unit checks and clippy). The five private assembled Interdiction
checks pass in 244.24s: posture, ordinary mission inputs/case recovery, independent
cabinet shots/observer state, the facility floor route and cellar descent.
Whole-map ordinary traversal, performance and the integrated gate remain required. The currently handed-off
playtest package is unchanged. Lighting and other collision-conversion defects
are separate unresolved work; this fixture does not prove the whole report fixed.

The initial private map fixture assumed that an overlap log's feet were on a
floor. Actual source columns disprove that: the real floor is substantially
lower, and the fixture falls. Preserve the failed fixture as an experiment;
retry from the actual support surface. The synthetic defect remains valid,
but its explanatory scope for the reported invisible blockers is unproved.

The corrected private room fixture walks from X=-11.42 to -20.06 at Y=-36.47
in 70.01s (including map load), with no recovery warnings after its first route
sample. This is an integration guard at the actual source floor, not reproduction
of every reported invisible blocker; the original logged height was airborne.
