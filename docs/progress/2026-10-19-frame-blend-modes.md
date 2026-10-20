# Unreal Engine 2 frame-buffer blend modes (2026-10-19)

Before: the renderer had opaque, alpha, additive and multiply; a map shape could author the first three, and a UE2 FinalBlend that
modulates, darkens, screens or premultiplies had no blend mode.
After: `AlphaMode::Frame(FrameBlend)` adds Modulate, Darken, Screen, Premultiplied and Invisible, and a static material's `blend` accepts
`multiply`, `modulate`, `darken`, `screen`, `premultiplied` and `invisible`. Factors from D3DDrv.dll's switch (docs/modding/materials.md).
Test: `crates/render/tests/frame_blend.rs` draws each mode over a known surface at three alphas and checks the pixels against the
shader arithmetic. Not established: Modulate's grey 127 constant, AlphaModulate fog behaviour, whether the original lights these surfaces.
