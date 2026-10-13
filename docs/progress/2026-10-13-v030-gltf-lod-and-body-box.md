# 2026-10-13 v0.3.0 W5: distant detail and a body's own size

Branch `claude/gltf-importer-boc77k`, into `v0.3.0-experimental`. Max's
question for practical multiplayer: will eight friends in Halo-sized models
stay smooth on a weak PC, and do different body types collide sensibly?
Follows `2026-10-12-v030-gltf-import.md`.

## What changed

- **Distant detail, opt-in** (`crates/addon-import/src/gltf/lod.rs`).
  `--lod TRIANGLES` gives a body a second detail level with that many
  triangles (`--lod-pixels N` where it switches; 100 pixels tall by
  default). Off by default: Max judged Combat Evolved's 2003-era models
  light enough not to need it now, after it was built and measured. Quadric
  error edge collapse (Garland and Heckbert), half-edge: a vertex moves onto
  a neighbour and takes its normal, UV and bone weights, so the result keeps
  its skinning with no new attributes. A seam's copies of a vertex move
  together, each along its own side, so seams stay closed; open borders stay
  put; no collapse joins parts that follow different bones; a collapse that
  folds a face is refused. Objects named `<name>_LOD1` replace the generated
  detail (rigid ones are moved into the original's frame); an object with
  no twin then draws as it is. In the shape: `detail100` (mesh offset 0)
  and `detail0` (offset 1), each visible object with two meshes.
- **The client picks a body's detail by its height on screen**
  (`crates/client/src/avatar.rs` `AvatarAssets::detail_at`,
  `AvatarMesh::screen_height`, set each frame in `app/frame.rs` from the
  last rendered camera and `ViewState::pixels_per_unit`). Bodies of one
  detail (the Blockhead, the horse, every v20 body) are unchanged. The
  posed-mesh layout key now includes each object's mesh, so switching
  detail rebuilds the layout instead of writing into the old one.
- **A body's collision box is its own size.** The archetype's `movement`
  was empty, so every imported body had the Blockhead's 1.25 by 2.65 box:
  Mario (1.6 tall) was hit and blocked a metre above his head. It now gets
  `stand_height` from the model, `width` at the Blockhead's ratio, crouch
  height scaled the same way, and stand, crouch and sit eyes from the eye
  socket in the idle pose. `--box-width` and `--box-height` set it (Mario's
  SM64 hitbox: `--box-width 0.74 --box-height 1.6`).
- Vehicle report also lists `pivot` and `turret` empties (mounted guns);
  the guide covers mounted guns, weapon sounds and effect textures, team
  colour armour, the collision box and hand-made `_LOD1` meshes.

## Evidence

- `cargo test -p bri-addon-import --lib gltf`: 18 pass, among them
  `a_heavy_body_gets_a_distant_detail_that_keeps_its_skin_and_sockets`
  (8,652 to at most a quarter; every vertex fully weighted; sockets kept;
  posed, every distant vertex is a full-detail vertex and the extents
  match within 0.02), `hand_made_lod1_objects_stand_in_at_a_distance`,
  `a_bodys_box_can_be_set`, and in `lod.rs` a rounded box simplified to
  budget that stays closed (every edge joins two faces).
- `cargo test -p bri-client --test gltf_body`:
  `eight_heavy_bodies_draw_their_distant_detail_when_small`, eight bodies of
  15,372 triangles each running, debug build on the container's software
  GPU (lavapipe), 20 frames each way:

  | | triangles | posing and skinning | drawing |
  |---|---|---|---|
  | near (full detail) | 122,976 | 65.6 ms | 43.5 ms |
  | far (distant detail) | 30,816 | 14.5 ms | 10.4 ms |

  Absolute numbers are a debug build on a CPU rasterizer; the ratio (about
  4.5 times less posing, 4 times less drawing) is the evidence. Switching
  back near, and starting with no frame drawn, both draw the full detail.
- Khronos samples (not committed): BrainStem 61,666 to 15,415 triangles in
  3.5 s (debug build), CesiumMan 4,672 to 1,168.
- Mario (Max's own file, not committed): 752 triangles; `--lod 300`
  gives 332. Box 0.755 by 1.6, eye 1.19.

## Next

- Projected size uses the archetype's standing height; a crouched body
  switches slightly sooner. Fine for a distance swap.
- Vehicles and held items keep one detail; add the same switch for
  vehicles if a Warthog turns out heavy.
