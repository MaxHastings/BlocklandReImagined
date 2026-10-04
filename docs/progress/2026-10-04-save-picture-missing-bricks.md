# 2026-10-04 Save pictures missing bricks and squashed

Max (v0.2.3) saved next to the Steel Ball on a soccer field; the save's
picture showed the far buildings but no field bricks or ground, and the
ball looked tall and thin.

## Cause
- Missing bricks: `take_save_picture` drew the picture's scene into the
  frame's own command encoder, then the frame drew again. Every buffer write
  (`queue.write_buffer`) of both draws lands at the one submit, so the
  frame's writes replaced the picture's. The scene renderer's indirect draw
  arguments restart at offset 0 on each `update_camera`; the picture's world
  chunk draws then read the frame's arguments at the same offsets. When the
  two draws record different passes before the main view (the metal
  environment probe's faces change between them), those are other passes'
  arguments and chunks go missing.
- Squashed: Load Bricks' preview box (v20's 294x220) stretches the picture to
  its own shape; Max plays on a wide screen.

## Fix
- The picture records into an encoder of its own and submits it before the
  frame draws (`crates/client/src/app/saves.rs`).
- Pictures are cut to the preview box's shape about their middle when read
  (`save_picture::read`), so wide-screen pictures (old ones too) keep their
  proportions.

## Evidence
- Guard `app::tests::a_save_picture_is_drawn_in_a_submission_of_its_own`
  fails on the old code ("the picture waited on the frame's own submit") and
  passes now (cloud, lavapipe).
- `save_picture::tests::pictures_of_another_shape_show_their_middle_unstretched`.
- `cargo clippy -p bri-client --all-targets -- -D warnings`, `cargo fmt`,
  `cargo test -p bri-client --lib -- save screenshot` (26 passed).

## Next
Max: save a build next to the Steel Ball on v0.2.4 and check the Load
Bricks preview shows the field and a round ball.
