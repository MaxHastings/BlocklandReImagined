# CI test shards red on main: the software renderer, not a regression

## Evidence
Main at b650033 (run 429) and PR #40 (run 435) fail in the Windows `test`
shards. Failing targets: `bri-client/app_flow`, `multiplayer`, `map_shapes`,
`view_jitter`, `bri-render/mirrors`, `lighting_environment`. Before Soft
Shading and Ambient Occlusion landed (run 391) only `persistent_scene`
failed, with the same "Wait timed out before the submission was completed".

Three causes, none a missing GPU (the runner draws on WARP as designed):

1. `app_flow` weather (`app_flow.rs:263`, 78,311 changed pixels where 0 are
   expected on a dry map). The skylight field is redrawn one direction a
   frame and swapped in on the ninth, so the wet and dry captures differed by
   a field swap, not by precipitation. Reproduced on main with lavapipe
   (79,496 pixels); passes after the fix. The test now renders until the
   field has settled before the first capture.
2. Raw 30 s GPU waits in 17 client tests and `persistent_scene`. WARP spends
   longer than that on a frame with the new passes. They now use
   `support::gpu::wait` (a hang is still caught after 300 s) or
   `wait_indefinitely`, as `app_flow` already did.
3. The gate's 10-minute limit per test binary stopped `mirrors`,
   `lighting_environment` and `view_jitter` while their tests were passing
   (all pass on lavapipe). `gate.py --binary-timeout MINUTES` raises it, and
   CI passes 30. The PC gate keeps ten.

Not covered: `large_build_perf`, `tutorial_walkthrough` and `video_rehearsal`
still use raw waits; they have no `support` module and did not fail.
