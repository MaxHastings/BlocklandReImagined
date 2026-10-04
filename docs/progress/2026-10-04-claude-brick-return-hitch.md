# 2026-10-04 Lag while minigame bricks come back

Branch `fix/brick-return-hitch`, from main `53f05cf` (v0.2.3). No wire,
save or content change.

Max, v0.2.3: "after say a few dozen bricks were blown up in a mini game and
10 or 20 seconds later when they re-spawn back in place it lags until it
completes".

## Cause

The lag lasts as long as the fade-in, so the cost repeats every frame of it.

1. The host returns the bricks in one `mutate_many`
   (`crates/sim/src/session/events.rs` `respawn_bricks`). The 2026-10-03
   battle probe measured this at under 0.5 ms (see
   `2026-10-03-v023-battle-performance.md`). It is not the cause.
2. Debris expires 13 s after the blast, before the default 30 s return.
   `BrickDebris::sync_world` (`net_events.rs:230`) forgets the dead bricks
   then, so the `is_dead` settle at `net_events.rs:328` no longer applies.
   `BrickFades::observe` (`net_events.rs:322`) gives every returning brick
   (hidden, then shown) a fade-in on v20's colour curve: 68 frames at
   60 fps, up to `MAX_FADES` = 512 bricks.
3. Each frame of the fade, `FadeModels::upload` (`brick_fade.rs`, called
   from `render.rs:473`) rebuilt every fading brick through `brick_scene`
   and `build_world_scene_materials`. That ran
   `BrickMaterials::surface_materials` for each brick, which copied all six
   brick surface images (TOP, SIDE, BOTTOMEDGE, BOTTOMLOOP, RAMP and white)
   into a scene of its own. On the first frame each brick's scene went
   through `SceneRenderer::upload`, which made its own textures with CPU
   mip chains. Every later frame copied the images again before rewriting
   the vertices. At the end, when the brick turned opaque and its material
   changed, the brick got a second full upload with textures.

For 48 returning bricks the new guard test counts **3,264 per-brick scenes
and 19,584 surface-image copies** over the 68-frame ease on origin/main.
That means 288 textures and their mip chains in the first frame, again in
the last frame, and 288 full-image copies in every frame between.

## Fix

Fading bricks are now built like chunks and debris, against the shared
`BrickPalette`:

- `FadeMeshes` (CPU side, `brick_fade.rs`) builds each fading brick once
  with `world_chunks::build_brick`. A colour change rebuilds its geometry
  in place with the new `world_chunks::rebuild_brick`, which reuses the
  material table and buffers, copies no material list and adds no image.
  Each changed brick reports one of two jobs: `Upload` for a new mesh or a
  changed layout, or `Vertices` otherwise.
- `FadeModels` (GPU side) does `Upload` with
  `SceneRenderer::upload_palette_model`, which writes two small geometry
  buffers and binds the palette's textures. It does `Vertices` with
  `GpuScene::update_vertices`. No texture is ever uploaded for a fade.
- A map reload that replaces the palette now clears the fade models
  (`net_events.rs`) so they cannot keep the old palette's bindings.

The look does not change: the vertices are byte-for-byte what a fresh
build gives (the test checks this every frame), and the curve, the alpha
cut-off, the outlines and the hand-back to the chunks are untouched.

**v20 fidelity:** the fade is kept. `brick_fade.rs` documents from
`blocklandv20.exe` that turning a brick's rendering on eases its alpha
back in on the colour curve (target set in `unpackUpdate` 0x541540, eased
at 0x53cc90). That a minigame respawn reaches this exact path in v20 is
inferred, not separately disassembled. The repo has no evidence that v20
pops returning bricks in.

**No per-frame budget on first builds:** a first build is now one brick's
geometry plus two small buffers, and `MAX_FADES` (512) already bounds the
count. A budget would make a repainted brick blink out for the frames it
waited.

## Other per-revision work on that frame (checked, unchanged)

The fade was the only cost that repeated every frame. These run once per
replica revision and were left as they are:

- `Audio::sync_music`: one pass over all bricks per revision.
- The hidden-brick outline rebuild in `render.rs`: only while a building
  tool is out, and only when the fading set or its faint flags change
  (about twice per fade).
- The ghost re-upload flag.
- `Building::sync_world_changes`: incremental through `WorldLog::between`.
- Chunk rebuilds: incremental (`update_leaving_out` with `known`), on a
  background job. One rebuild happens when the bricks return and one when
  the fades settle. Both were measured on 2026-10-03 at 1.5-2 ms of pooled
  upload for 128 bricks.

## Tests

The guard is `brick_fade::tests::returning_bricks_fade_in_without_textures_or_per_frame_uploads`:
48 bricks (a few dozen, as Max reported) go from hidden to shown and are
stepped through the whole ease at 60 fps.

- origin/main (the same scenario through main's `brick_scene`, which
  `FadeModels::upload` called for every brick every frame): **FAILED**,
  `68 frames: 3264 per-brick scenes built, 19584 images copied`,
  `left: 19584, right: 0`.
- This branch: **ok**, `68 frames, 2 uploading: FadeDiagnostics { uploads: 96,
  vertex_updates: 3168, images_uploaded: 0 }`. It asserts 0 images, at most
  2 uploads per brick, uploads on at most 2 frames, and exactly one job per
  brick per frame. The in-place mesh must equal a fresh build. Once the
  chunks take the bricks back, nothing is left.

Commands (shared target dir, single crate):

```sh
CARGO_TARGET_DIR=/home/claude/bri-target CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
  CARGO_BUILD_JOBS=3 cargo test -p bri-client --lib returning_bricks_fade_in -- --nocapture
CARGO_TARGET_DIR=... cargo test -p bri-client --lib -- brick_fade world_chunks brick_debris   # 33 passed
CARGO_TARGET_DIR=... cargo clippy -p bri-client --all-targets -- -D warnings                 # clean
CARGO_TARGET_DIR=... cargo test -p bri-client --lib   # 439 passed, 4 failed, 63 ignored
```

All 4 failures are content-dependent icon and atlas tests
(`installed_effects_replace_the_atlas...`, `original_hud_icons_upload...`,
`an_add_on_tool_icon...`, `the_gravity_gun_icon...`). They stop on
`No such file or directory`: this Linux container has no generated content
or v20 install. They do not touch the fade path.

## Not verified

- Real GPU frame time on Max's Windows PC. This container has no GPU, so
  `FadeModels::upload` runs only through its CPU half (`FadeMeshes`).
- Whether v20 eases in a respawned brick: inferred from the documented
  rendering-on easing, not checked against a v20 recording.
- Bricks returning while their debris is still alive (return under 13 s)
  pop in without a fade, as before. Debris rules are unchanged.

Max's playtest: start a minigame with Brick Damage on, blow up a few dozen
bricks with rockets and wait for them to return (default 30 s). They should
fade in with no frame drop.
