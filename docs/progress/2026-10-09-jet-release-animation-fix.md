# 2026-10-09 Jet release keeps the airborne standing pose

Branch `codex/jet-release-animation`, based on main `3c8ed1f7`.
Maxwell reports the Blockhead's feet/torso bending backward after jetting
from the floor and releasing the jets in third person.

## Cause and change

The fresh-state locomotion picker uses `jump` whenever a non-jetting,
airborne player rises faster than 0.5 units/s. Releasing jets while still
rising therefore started the original jump clip from zero, despite there
being no new jump. The native clip bends the hip and kicks the legs backward;
its additive motion also bypasses the current absolute-channel transition.

`AvatarMesh` now remembers that jets took over the current flight, using
the same latest simulated tick as action selection. During its remaining
ascent, an otherwise selected jump becomes root (crouch for a crouched
body). Falling, water and scripted actions keep their existing selection.
Landing re-arms ordinary jumps; respawn clears the flight history, and an
outfit rebuild carries it with the running animation.

This fixes the unwanted clip selection without changing the motor, content
or protocol. The existing replicated jet/ground flags suffice for remote
players, whose compact poses omit the owner's jump timers. General additive
transition blending and replacing the ordinary jump-selection heuristic with
explicit jump events remain separate work; this patch avoids widening the
reported fix into jump replication and sampler changes.

Original reference evidence was read without launching or modifying v20:
`Player::pickActionAnimation` at `0x5a2fe0` in the designated E: executable
chooses root without contact and does not choose jump from upward speed.
The local TGE source starts jump in `updateMove` when applying an actual
jump impulse. Native `avatar-pack-002` has the same additive jump clip
observed in the initial investigation.

## Verification

Commands used `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_INCREMENTAL=0`, and left
`CARGO_TARGET_DIR` unset in the reused, clean `land-v027-bot-play` worktree.
For native checks, `BRI_CONTENT` named the main checkout's generated content.

- `cargo test -p bri-client --lib avatar::tests`: 27 passed, 20 content/GPU
  checks ignored. Includes ordinary movement, crouch, held arms, scripted
  actions, sit transitions, outfit rebuilds and respawn.
- `cargo test -p bri-client --lib avatar::tests::jet -- --include-ignored
  --nocapture`: all four new synthetic/native checks passed. The real motor
  starts on a floor, jets for 80 Torque ticks, releases while still rising,
  falls, lands and jumps normally afterward. At release the hip, torso and
  both legs match the standing pose; both local and compact remote poses
  are checked, with empty hands, a right-hand tool and both-hand tools.
  Rebuilding the outfit mid-flight preserves the fix. The second check
  interrupts an ordinary jump with jets, exercises mismatched rendered and
  simulated flags, and checks respawn reset.
- Disabled only the new flight history for a regression sensitivity check:
  all four tests failed. The motor case failed on the first jet-release
  tick (80), selecting `jump`; the interrupted jump replayed `jump` too.
  Restored the fix and all four passed again.
- An initially overbroad `--lib jet -- --include-ignored` filter selected
  an unrelated native vehicle-ejection test, which failed with `Run importer`
  because it looks for content inside this worktree rather than honoring
  `BRI_CONTENT`. The seven other tests passed, including all four new checks.
  Narrowing to `avatar::tests::jet` runs the intended tests without that
  unavailable fixture. No test or known-failure entry was weakened.
- File-scoped rustfmt and `git diff --check` passed.
- `cargo clippy -p bri-client --lib --tests -- -D warnings`: passed, including
  the new test code and client integration-test targets.

No interactive game window or gameplay input was automated. Maxwell's
confirmation is still needed: in third person, jet from the floor and release
while rising, then try an ordinary jump and a jump interrupted by jets.
The complete alpha contract remains open.
