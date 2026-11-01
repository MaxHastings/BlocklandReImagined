# Mirror-render color check in dim lighting

The extended gate's synthetic Bedroom and Slopes mirror checks measured only
80–81 bright-red pixels in their straight views, below the fixed 200-pixel
requirement, with zero in the off control. Inspected the captured Bedroom
straight-on frame: the red pillar is visibly present in the reflected view,
but much of it is dark. Counting red-dominant pixels above 32 rather than 80
recognizes the same hue while excluding near-black noise. The test still
requires more than 200 additional red pixels relative to Mirrors Off, a
20,000-pixel overall difference, and an actual reflection pass; no game
rendering or material behavior changed.

Both synthetic offscreen cases passed with the revised classifier; Bedroom
straight measured 1,922 red pixels with reflections and zero without. Both
generated-content offscreen cases also passed with the same classifier and
minimum-pixel requirement. `cargo fmt -p bri-client -- --check` and `cargo clippy --locked
-p bri-client --test mirror_render -- -D warnings` passed. The generated
content run logged voice input underrun/overrun messages from the local audio
backend; that run is not voice acceptance. A previous gate run's synthetic
free-camera timeout and a generated-content mirror case that passed only on
retry remain reported as flaky until a complete gate succeeds. No interactive
playtest or v20 reference comparison was performed.
