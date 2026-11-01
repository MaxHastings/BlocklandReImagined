# Vehicle TDM arena bounds

The extended gate's vehicle TDM case put its host at (-92, -92) on the
synthetic fixture's 200-by-200-unit floor. Its bots can now explore and pursue
beyond that floor: the failure log recorded one at y=-6405, while fight
activity was 0.1% and the existing 3% floor failed. This invalidates the
scenario as a contained 90-second vehicle battle; it does not establish why a
real map might allow a player or bot into a void.

The TDM fixture now extends the synthetic floor using four adjacent collision
slabs, retaining the original central floor and walls. It asserts that no
participant falls below y=-5, and retains the existing combat-activity,
vehicle-use, crew, gunner and ordinary-seat-exit assertions. With the same
scenario otherwise, fight activity was 34%, and the focused test passed.
`cargo fmt -p bri-chaos -- --check` and changed-target Clippy passed in the
Windows playtest profile. This is a fixture correction, not a general world
boundary or bot recovery fix. The full release gate remains required.
