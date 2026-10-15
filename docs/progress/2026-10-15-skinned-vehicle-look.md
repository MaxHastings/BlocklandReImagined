# Independent skinned vehicle aiming

Root source-rig investigation, 2026-10-15. General engine mechanisms only;
original content, source metadata and conversion remain private.

The private reference attachment had been flattened, including vertices that
blend between joints. Rigid part transforms cannot reproduce those vertices.
Vehicle look animation now discovers an embedded native `look` clip before
external clips. Weighted shapes use the existing avatar mesh posing arithmetic,
with independent geometry per live vehicle. Uploads share immutable GPU texture
and material bindings from the base model. CPU pose slots hold no decoded texture
pixels. Removed vehicles and changed definitions retire their slots. Portal cuts,
vehicle tint, model scale and GPU reset remain part of the normal draw lifecycle.

Optional `weapon.look_animation_pitch: [down, up]` maps asymmetric authored
endpoints around a neutral phase of 0.5. Rendered joints, named sockets and host
muzzle tracks share the mapping. Omitted fields keep the v20 mapping. Added a
protocol-change record and modding documentation. No content-name checks added.

Also corrected vehicle model scale: chassis, wheels and attachments now use
`VehicleInfo.scale`, as seats/physics already did. Authoritative suspension
extension is world-space and is divided by scale before applying the scaled
model, avoiding a second scale multiplication.

Evidence:

- `cargo check -p bri-client -p bri-vehicles-import`: pass.
- Private source conversion checks all 1,046 gun vertices and all 891 triangles,
  including original material assignments/winding, retaining 70 blended vertices.
- Bounded private offscreen captures `artifacts/warthog-restored-rig-inspection`
  in the main checkout: idle/up/down inspected. Upper gun articulates separately
  from the post, source marker parents preserved. All three report no effect
  faults. This is a diagnostic backdrop, not a matched Blood Gulch comparison.
- `python tools/check.py quick --crates bri-client,bri-vehicles,bri-vehicles-import`:
  six steps/64 seconds, pass. Includes independent weighted-gun pose/lifecycle,
  asymmetric phase and world-space suspension regressions.
- `python tools/check.py focused --crates bri-client,bri-vehicles,bri-vehicles-import
  --depth 0`: nine steps/111 seconds, pass. This is the changed crates, not the
  eventual integrated gate or downstream full-suite acceptance.

Failures corrected: `cargo fmt --all` exceeded the Windows command length, so
formatting ran by package; clippy requested Range::contains for down endpoints;
the first no-texture-copy regression found SceneData's default white placeholder
image, so pose slots now explicitly use an empty image list.

Still open: original firing/acceleration animation consumption, rider aim clips,
full source hierarchy for hull/suspension, original lights/dashboard/audio,
flash blend/composition and other fidelity rows in the private coverage ledger.
The inspected flash remains visibly large/bright; these renders do not close it.
No installed executable update, final gate, new-head CI, merge or release here.
