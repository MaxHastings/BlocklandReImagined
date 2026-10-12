;; Wall Kick: the sample movement controller. A Blockhead in the air who
;; presses jump (a new press, not a held one) while a wall is within 0.6
;; units, toward where they push or else where they face, springs up and
;; away from it: 11 up, 8 out along the wall's normal. A kick takes the
;; jump, so the motor does not also jump; after one, another must wait 8
;; ticks (a quarter second) or until the body lands. Everything else is
;; the stock motor: walking, falling, sliding along the wall.
;;
;; This is the source of wall_kick.wasm; `cargo test -p bri-controller`
;; checks the two match (BRI_BLESS=1 rewrites the binary). Real Add-Ons
;; are usually written in Rust, C or Zig and compiled to wasm32;
;; hand-written text keeps the sample readable without a toolchain.
;;
;; The record (`crates/controller/src/lib.rs` documents it) is at 0. Its
;; 16 bytes of memory at 76 hold this controller's state between ticks:
;;   76  jump was held last tick (1) or not (0)
;;   77  ticks before another kick is allowed
;; The animation intent at 72 is 1 from a kick until landing.
;; Sweep results land at 128: fraction, normal xyz.
(module
  (import "bri" "sweep" (func $sweep (param f32 f32 f32 i32) (result i32)))
  (memory (export "memory") 1)

  (func (export "io") (result i32) (i32.const 0))

  ;; Sweep 0.6 units along (dx, dz); kick off a wall found there.
  (func $kick_off (param $dx f32) (param $dz f32) (result i32)
    (local $len f32)
    (local.set $len
      (f32.sqrt (f32.add (f32.mul (local.get $dx) (local.get $dx))
                         (f32.mul (local.get $dz) (local.get $dz)))))
    (if (f32.lt (local.get $len) (f32.const 0.01))
      (then (return (i32.const 0))))
    (if (i32.eqz
          (call $sweep
            (f32.mul (f32.div (local.get $dx) (local.get $len)) (f32.const 0.6))
            (f32.const 0)
            (f32.mul (f32.div (local.get $dz) (local.get $len)) (f32.const 0.6))
            (i32.const 128)))
      (then (return (i32.const 0))))
    ;; Only walls: a normal that is mostly level.
    (if (f32.ge (f32.abs (f32.load (i32.const 136))) (f32.const 0.3))
      (then (return (i32.const 0))))
    (f32.store (i32.const 28) (f32.mul (f32.load (i32.const 132)) (f32.const 8)))
    (f32.store (i32.const 32) (f32.const 11))
    (f32.store (i32.const 36) (f32.mul (f32.load (i32.const 140)) (f32.const 8)))
    (i32.const 1))

  (func (export "step") (result i32)
    (local $flags i32)
    (local $cool i32)
    (local $kicked i32)
    (local.set $flags (i32.load (i32.const 4)))
    (local.set $cool (i32.load8_u (i32.const 77)))
    (if (local.get $cool)
      (then (local.set $cool (i32.sub (local.get $cool) (i32.const 1)))))
    ;; Grounded: the kick is over.
    (if (i32.and (local.get $flags) (i32.const 1))
      (then
        (i32.store (i32.const 72) (i32.const 0))
        (local.set $cool (i32.const 0))))
    ;; A new press of jump, in the air, with no kick cooling down.
    (if (i32.and
          (i32.and
            (i32.ne (i32.and (local.get $flags) (i32.const 4)) (i32.const 0))
            (i32.eqz (i32.load8_u (i32.const 76))))
          (i32.and
            (i32.eqz (i32.and (local.get $flags) (i32.const 1)))
            (i32.eqz (local.get $cool))))
      (then
        ;; Toward where the keys push first; if no wall is there, where
        ;; the body faces.
        (local.set $kicked
          (call $kick_off (f32.load (i32.const 52)) (f32.load (i32.const 60))))
        (if (i32.eqz (local.get $kicked))
          (then
            (local.set $kicked
              (call $kick_off (f32.load (i32.const 40)) (f32.load (i32.const 48))))))
        (if (local.get $kicked)
          (then
            (i32.store (i32.const 68) (i32.const 1))
            (i32.store (i32.const 72) (i32.const 1))
            (local.set $cool (i32.const 8))))))
    (i32.store8 (i32.const 76)
      (i32.ne (i32.and (local.get $flags) (i32.const 4)) (i32.const 0)))
    (i32.store8 (i32.const 77) (local.get $cool))
    (i32.const 0))
)
