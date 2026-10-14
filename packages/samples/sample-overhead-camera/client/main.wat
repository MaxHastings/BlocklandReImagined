;; Overhead Camera: the free-look key (control 10) is this Add-On's, and
;; pressing it switches between the game's camera and one 14 units above
;; and 6 behind the player, looking down at them. Only the view moves:
;; the player still aims and walks as before.
;;
;; Memory: 0..32 the shot for camera_set, 32 on, 36 key held last frame,
;; 256.. the players() records (16 f32 each).
(module
  (import "bri" "input_claim" (func $claim (param i32) (result i32)))
  (import "bri" "action_down" (func $down (param i32) (result i32)))
  (import "bri" "local_player" (func $me (result i32)))
  (import "bri" "players" (func $players (param i32 i32) (result i32)))
  (import "bri" "camera_set" (func $set (param i32) (result i32)))
  (memory (export "memory") 1)

  ;; Every frame: hold the free-look key and flip the view as it goes down.
  (func (export "frame") (param f32 f32)
    (local $held i32)
    (drop (call $claim (i32.const 1024)))
    (local.set $held (call $down (i32.const 10)))
    (if (i32.and (local.get $held) (i32.eqz (i32.load (i32.const 36))))
      (then (i32.store (i32.const 32) (i32.xor (i32.load (i32.const 32)) (i32.const 1)))))
    (i32.store (i32.const 36) (local.get $held)))

  ;; Before the game draws: while on, put the camera over the player.
  (func (export "camera_frame") (param f32 f32)
    (local $n i32) (local $i i32) (local $at i32) (local $me f32)
    (if (i32.eqz (i32.load (i32.const 32))) (then (return)))
    (local.set $me (f32.convert_i32_s (call $me)))
    (local.set $n (call $players (i32.const 256) (i32.const 16)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_s (local.get $i) (local.get $n)))
        (local.set $at (i32.add (i32.const 256) (i32.mul (local.get $i) (i32.const 64))))
        (if (f32.eq (f32.load (local.get $at)) (local.get $me))
          (then
            ;; Eye: feet + (0, 14, 6); look: (0, -14, -6); no roll; the
            ;; player's own field of view.
            (f32.store (i32.const 0) (f32.load offset=8 (local.get $at)))
            (f32.store (i32.const 4) (f32.add (f32.load offset=12 (local.get $at)) (f32.const 14)))
            (f32.store (i32.const 8) (f32.add (f32.load offset=16 (local.get $at)) (f32.const 6)))
            (f32.store (i32.const 12) (f32.const 0))
            (f32.store (i32.const 16) (f32.const -14))
            (f32.store (i32.const 20) (f32.const -6))
            (f32.store (i32.const 24) (f32.const 0))
            (f32.store (i32.const 28) (f32.const 0))
            (drop (call $set (i32.const 0)))
            (br $done)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))))
