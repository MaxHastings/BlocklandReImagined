# 2026-10-01 Grapples through the client; plain parts take the colour shift

Maxwell, on the b5d99c948 test build: the HookShot "is white, supposed to
be all blue". The HookShot fires but doesn't pull, and the Grapple Rope
doesn't swing. The missing pull and swing come from its packages.json
leaving both `-rules` Add-Ons off. The Bundled originals lane owns that fix
(`crates/package/src/defaults.rs`). This entry covers what that fix doesn't.

## Plain parts are the item's colour

The importer bound a material with no texture (the HookShot's `black50`) to
an opaque white texture. Opaque item materials draw as paint overlays: the
texture over the colour shift, by the texture's alpha (`scene.wgsl`,
`mix(lit, albedo.rgb, albedo.a)`). An opaque white texture hid the colour,
so those parts drew white. They now bind `placeholder:clear` (white, alpha
0), so they show the item's colour shift. That is `0.2 0.2 1` blue for the
HookShot, and white for an item without a colour shift. The placeholder
cube keeps opaque white. I inferred that Torque drew untextured parts in the
colour shift from Maxwell's report and the colour fields; I did not measure
it.

Guard: `import.rs`
`a_material_with_no_texture_draws_plain_and_keeps_its_model` checks the
binding is `placeholder:clear`, with alpha 0, under the item's tint.

## The holder's own client dropped the rope

`Predictor::reconcile` kept the whole predicted state whenever the host's
pose matched within float noise. A rope the host tied to a player standing
still (the Grapple Rope fired from the ground) was dropped by their own
client until something else moved them. The client now takes the host's
state and keeps only its own feet and velocity through noise.

Guards:
- `sim/tests/prediction.rs`
  `a_correction_within_noise_still_takes_the_hosts_rope`.
- The bundled grapple tests in `grapples.rs` now play the player through
  the game's client prediction. Moves and commands reach the host 4 ticks
  late, carrying their aim, and the host's poses come back 4 ticks late.
  The tests check what the player's own client shows. The bundled Grapple
  Rope test fails on the old reconcile: "the holder's own client sees the
  rope".

Both bundled tests pass with the original scripts from
grapple-originals.zip in place of the stand-ins.
