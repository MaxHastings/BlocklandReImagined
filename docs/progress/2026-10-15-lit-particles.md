# 2026-10-15 Authored diffuse particle lighting

Particles may opt into `lit: true`; omission preserves existing self-lit effects.
Snapshots retain this choice and the GPU consumes host ambient, travelling sun
direction/color and up to 256 current point lights. Sprite faces use a viewer-facing
normal, native vertex-lit quadratic point attenuation and the existing native color
convention. Unlit flashes/flares, alpha, fog, depth and blend choice are unchanged.
The client supplies its scene camera and current live effect/vehicle point lights.

This first diffuse path does not receive geometry shadows, sky-band ambient or
baked/map irradiance. It is not claimed as a complete external-game lighting
reconstruction. Standalone adapters have neutral lighting until supplied by their
host. GPU scene-light input is validated before changing the renderer's state.
No game-specific runtime branch or original asset was added publicly.

Private conversion preserves original self-lit flags: 38 rifle and 33 vehicle
particle definitions receive lighting; the remaining 125/123 remain self-lit.
All generated flags agree with source metadata. The private diagnostic helper now
supplies camera lighting consistently. Actual rifle renders load with no item/effect
faults; a third-person casing at 160ms changes only its small projected region
(47 pixels), while the self-lit flash is unchanged. The casing has left the
first-person view at that age, which does not verify its ejection presentation.
Original trajectory/contact audio, exact source lighting and human acceptance stay open.

Validation:

- Client/converter cargo check passes, 17.68s.
- Offscreen GPU contract: 3 tests pass, 2.66s. New cases check ambient color, sun
  direction, point radius/falloff, unlit immunity, alpha retention and invalid input;
  existing blend/depth/fog/orientation checks still pass.
- Initial new pixel expectations assumed linear-light encoding. The documented
  native rendering contract lights in display space; expectations were corrected
  to that convention without changing the shader or weakening existing assertions.
- `python tools/check.py quick --crates bri-content,bri-fx-runtime,bri-convert,bri-client,bri-net`:
  10 steps/264s pass, including 567 client units.
- `python tools/check.py focused --crates bri-content,bri-fx-runtime,bri-convert,bri-client,bri-net --depth 0`:
  15 steps/514s pass, including GPU integrations, crossover and sized join.
- Actual private capture program rebuilt and rendered first/third-person rifle
  views on DX12/DXC without opening a visible game window.

Shared coordination: incoming imported full-body crouch clips are still overwritten
by the client's dedicated crouch layer. Authored hand preservation does not solve
that separate composition defect. This chat owns the subsequent generic fix; the
AA consumer owns private revalidation on its eventual committed SHA. Other checkouts
and their builds are untouched. No merge, installed playtest update or publication.
