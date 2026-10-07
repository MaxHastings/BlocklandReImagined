# 2026-10-07 Smooth first-person stand-up from /sit

Max reported in v0.2.5: sitting in first person and getting up, the camera
jumps straight to standing height instead of easing up.

## Cause
While the local player sits on foot, the first-person eye is the drawn
body's animated `Eye` node (`App::posed_eye`). The body blends into and out
of `sit` over one action transition (`TRANSITION_TIME`, 0.25 s), so sitting
down already eased. But the moment the host cleared `sitting`, `posed_eye`
returned `None` and the camera fell back to the predicted eye at standing
height in one frame, while the body was still drawn half seated.

## Fix
- `AvatarMesh` remembers when its body last left the `sit` pose and
  reports `getting_up()`: the share of the sit left, 1 falling to 0 over one
  action transition. It keeps its own clock, so running or jumping off part
  way up (a new transition) does not restart or cut it.
- `App::posed_eye` takes the smoothed predicted eye and, while getting up on
  foot, eases from the drawn head toward it by that share. Both ends match
  what was shown before and after, so the view has no jump whatever the
  model's upright `Eye` node is.
- Sitting down is unchanged: it already follows the body's blend.
- No new constants: the duration is the existing `TRANSITION_TIME`.

## Evidence
- `cargo test -p bri-client --lib getting_up`: new
  `getting_up_eases_out_of_the_sit_over_one_transition` (synthetic) passes;
  content variant needs generated v20 content.
- `cargo test -p bri-client --lib`: 476 passed; the 6 failures all need a
  wgpu adapter, which the cloud container lacks.
- `cargo clippy -p bri-client --all-targets -- -D warnings`: clean.

## Next
Not yet checked visually in the installed game; Max's playtest confirms.
