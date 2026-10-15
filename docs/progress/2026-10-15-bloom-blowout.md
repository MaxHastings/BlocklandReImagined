# Bloom no longer blows out fire and sunlit terrain

Max's first playtest of the v0.3.1 build on Blood Gulch showed a brick fire
emitter smeared into a large white cloud and sunlit terrain patches glowing
white-hot. Cause: bloom thresholded on the brightest channel at 0.8, so red
sand in full sun and an orange fire (red channel near full) counted as
bright, and the halo was added back at 2.5x.

Change (`crates/render/src/bloom.rs`): the threshold is now on Rec. 709
luminance at 0.9, and intensity is 1.5. White lamps and flashes still glow;
saturated reds and oranges do not. `crates/render/tests/bloom.rs` adds a full
red-orange spot that must not glow, beside the white spot that must.

Evidence: `cargo test -p bri-render --test bloom` and `--lib bloom` pass;
clippy -D warnings is clean for bri-render. Not rendered on Blood Gulch here
(no content in the cloud container); Max confirms in the next playtest.
