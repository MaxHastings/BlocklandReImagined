# 2026-10-13 v0.3.0 W5: a clip's "hold still" keys are kept

Branch `claude/gltf-importer-boc77k`, into `v0.3.0-experimental`. Found by
the Mario and feel threads, passed on by integration: Mario's weapon hold
broke while running.

## Cause

`clip()` in `crates/addon-import/src/gltf/mod.rs` dropped an absolute
clip's rotation or translation track when it never left the rest pose
(`still_r`, `still_t`). Layering (`bri_content::animation`) lets only a
clip's tracks override the layer below, so an aim clip that keyed the off
hand at rest lost that key and the run's swing showed through.

## What changed

- An absolute clip keeps every channel the glTF file has, rest or not;
  scale tracks too. Bones without a channel still follow the layer below.
  Overlays (`look`, `head_side`, `head_up`) still leave out bones that do
  not move, since an identity offset changes nothing.
- The guides (`docs/modding/gltf-models.md`, project files) say a clip
  takes over exactly the bones it keys, and how Blender's Optimize
  Animation Size can drop a "hold still" key.

## Evidence

- `cargo test -p bri-addon-import --lib gltf`:
  `a_channel_held_at_rest_still_overrides_the_layer_below`. The test
  figure's run swings its left hand; its aim clip keys that hand at rest.
  Layered run then aim: the hand is at rest, the hips still follow the run.
  Without the change the aim clip has only `arm.R` and the test fails.

## Next

- Bodies imported before this change need reimporting (Mario's included).
