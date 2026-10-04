# 2026-10-03 Creature source review and interrupted Shark restart

Sol High reviewed the pinned Steam Bot_Shark and Bot_Zombie archives read-only
for v0.2.2. This lane owns only the Shark companion, metadata, focused creature
tests and this note. Root retains shared simulation and importer integration.
No original scripts, content or assets were copied into Git; no game window,
desktop controls or interactive playtest was used.

## Source-backed safe completion

Bot_Shark SHA-256 remains
`02f9ec67645e6b3736be651fe4298a8796fe8d1f2edd7515028212d96b70ac04`.
The original `SharkHoleBot::onBotDamage` releases a captured rider on the
one-in-four decision or a hit of at least 50, runs away and schedules its
ordinary bot loop to restart after exactly 2000ms. The existing native port
already retains the 200ms harm grace, threshold/random release and canonical
mount/damage path, but immediately resumed its brain after release.

The package now retains its existing rest adaptation until 240 simulation
ticks have elapsed after harm release. It suppresses recapture while paused,
then wakes through the already authorized `rest_bot` operation. It prunes
pending restarts on holder/victim death, spawn, leave, MiniGame membership or
team changes, resets/end/configuration, changed holder body/kind, missing
victim and lost damage permission. No delayed rest survives a later life or
game. A body change can wake an owned kind; a foreign kind receives no rest
operation. Ordinary capture expiry and other release reasons remain immediate.

The focused test additions cover the exact boundary and 15 invalidation cases.
The new Session scenario obtains a capture by ordinary actor approach, submits
canonical attributed damage through a fixture command, checks actual rider
release and the paused brain, then checks reacquisition from observation after
the deadline. It does not assign bot targets, mounts or authored package state.

## Actual checks

`rustfmt --edition 2024 crates/chaos/tests/shark_policy.rs` and bounded
`git diff --check` passed. Root's first serialized
`cargo test -p bri-chaos --test shark_policy --locked -- --nocapture`
run passed 13, failed the new harm scenario and ignored the original-content
scenario (`/tmp/bri-v022-shark-restart.log`). All policy and 15 lifecycle
invalidation cases passed. The real harm scenario initially submitted 50
raw indirect package damage. Canonical crouch scaling reduced that to 37.5
before the package hook, below its source release threshold. The fixture now
submits 100 raw (75 observed on the crouched swimmer) and asserts observed
nonlethal harm of at least 50 before its unchanged actual release, exact
restart and ordinary reacquisition assertions. Rerun is pending root's
serialized build interval. The ignored original-content capture test requires the
generated Shark companion to be refreshed before proving this latest policy.
Prior 13-test original-content evidence is recorded in the Shark hardening
entry; it does not by itself verify this new restart state.

Read-only source review also found that smaller-Shark capture uses holder
thread 1 for `biteReady` and victim thread 0 for `biteFix`, then restores
holder 1/victim 0 to `root` on release. The prior companion used thread 2
for both actors. The supported body-thread operations now match the source,
with exact capture/release assertions in the smaller-body test. This proves
the animation cue selection; rendered pose quality remains Maxwell's.

Root's next combined run (`/tmp/bri-v022-linked-creator-shark.log`) again
passed 13/failed 1/ignored 1. Actual canonical harm and unmount plus the full
240-tick resting interval now passed. The remaining failure was the fixture's
single-step reacquisition assertion: `Session::step` runs controller(t),
package hooks(t), then advances physics/public tick to t+1. When the loop
first reaches public tick deadline, the latest hook saw deadline-1. The next
step runs a resting controller at deadline, then the hook wakes it. The next
controller at deadline+1 is the first active turn. The fixture now explicitly
checks the target-free deadline controller phase, then requires genuine
observation/recapture on the immediately following control turn. This is a
two-step phase bound, not an extended source delay or arbitrary observation
grace. The package's exact 240-tick deadline remains unchanged and directly
tested. The latest rerun is still pending root's serialized interval.

## Honest remaining gaps

Shark still rests rather than executing the source flee movement, including
the restart interval. Forced vehicle ejection, infected-Shark victim conversion,
full white-Shark aggression/no-strafe policy, random Cool Shark renaming and
the original SharkHoleBite kill icon remain unsupported. Named Cool Sharks
already receive the authored accessories. Chosen water/land speeds remain an
explicit adaptation of the source's zero native speeds. Existing engine bot
operations expose neither flee controls, per-actor aggression/strafe policy
nor renaming; this lane added no creature-specific engine exception.

Bot_Zombie SHA-256 is
`002a9e6da08e9841a29c203908b89d23b0ee11aee6102b009079c995e9782bcf`.
Its source infection prefixes the name with Zombie once, marks infection,
clears the weapon, copies selected attacker policy, preserves stronger melee
damage and resets both bot loops. Human-shaped victims take the Zombie body;
Sharks retain their own body with head/chest infection color and other bodies
retain their model with whole-body color. The source preserves the special
face `memePBear`. Already-Zombie victims of another side can instead receive
a persistent random mark that prevents later infection. The current generic
`converts_below` mechanism converts a wounded opposing brick bot after its
swipe by replacing its whole kind/body and healing it. It cannot represent
the retained-body infection, name prefix, persistent mark or selective policy
copying. Those are source-backed open differences, not inferred parity. No
Zombie framework or speculative mechanics were introduced for this release.

The lane is done when focused Shark/import tests and the refreshed ignored
original-content capture proof pass without diagnostics, with the above gaps
kept open. Visual and interactive acceptance remains Maxwell's.
