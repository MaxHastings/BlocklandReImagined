# 2026-10-03 Effects atlas replacement after Add-On reload

Independent Sol High investigation of Maxwell's Windows `9650a6b1` Duplicator
failure in `/Users/maxhastings/Downloads/client-20261003-225659-874.stderr.log`.
The log reports `Invalid effects instance` twice. It lacks the offending
particle's fields, so it cannot establish which validation clause failed.

## Mechanism and correction

`app/addons.rs::install_packages` replaced the CPU WeaponEffects and ActorEffects
worlds while retaining the GPU EffectsRenderer built from the previous pack.
Texture indices are pack-local. The first host after changing Add-Ons need not
restart the GPU: the map setup rebuilds it only when a previous scene exists.
Consequently, a valid newly appended particle texture can index beyond the old
GPU atlas and produce this exact terminal error. Equal texture counts also do
not establish identical ordering or pixels.

Successful package installation now invalidates the effects renderer. Before
any main, mirror or portal effects preparation, `prepare_render` reconstructs
that atlas from the current weapon effects pack through the same constructor
used at GPU startup. Other GPU pipelines remain available. Failed or cancelled
preparation does not invalidate the active pack or renderer. Invalid particle
data remains an error, now including its index, texture/layer count,
position/color/size/axis/spin for useful future diagnosis.

## Actual content and scope

A bounded read of generated content finds 18 base effects textures. Classic
Duplicator's `duplorcatorparticleb` uses `base/client/ui/brickicons/1x1`;
New Duplicator's wait/spin particles use that icon and its hit particles use
`base/client/ui/brickicons/2x2`. Neither icon belongs to the base effects atlas.
`interface_textures` decodes these UI images and `add_add_on_textures` appends
them to the selected weapon effects pack. All three classic and six new
Duplicator emitters disable emitter color/size overrides, and their particle
keys end at normalized time 1. The hypothesized override extrapolation does
not explain these particles. Placement ghosts produce scene geometry/lines,
not the rejected particle instances.

This is a reproduced atlas-lifecycle defect and a strong match to the new
report, rather than proof of an unavailable Windows particle trace. It does
not establish a repair of the earlier firefight NaN clamp crash. No original
content was changed and no interactive playtest was performed.

## Focused evidence

Root ran the first frozen versions serially:

- `cargo test -p bri-fx-runtime --test gpu_contract --locked`: 2 passed in
  3.32 seconds; `/tmp/bri-v022-effects-atlas-regression.log`.
- `cargo test -p bri-client --lib installed_effects_replace_the_atlas --locked
  -- --nocapture`: 1 passed in 3.13 seconds;
  `/tmp/bri-v022-app-effects-reload-regression.log`.

The GPU test rejects a valid appended texture against a stale atlas, accepts
it after rebuilding, and retains negative/NaN/infinite-size rejection. The
client test uses the actual installation seam and verifies successful
invalidation, reconstruction, equal-count replacement and failed/cancelled
reload preservation. A final test-only strengthening additionally changes the
equal-count texture's pixels and checks stale/current secondary view
preparation. Those changes are frozen pending root's focused retry and strict
Clippy; the full gate and platform publication remain root-owned and pending.
