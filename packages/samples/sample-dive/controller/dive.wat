;; Dive: a sample movement controller with no world queries. A Blockhead
;; in the air who presses crouch (a new press) while the movement keys
;; push throws itself that way: 14 along the push, and at least 5 up. One
;; dive per time in the air; landing allows the next.
;;
;; This is the source of dive.wasm; `cargo test -p bri-controller` checks
;; the two match (BRI_BLESS=1 rewrites the binary).
;;
;; The record (`crates/controller/src/lib.rs`) is at 0. Memory at 76:
;;   76  crouch was held last tick (1) or not (0)
;;   77  dived since leaving the ground (1) or not (0)
;; The animation intent at 72 is 2 from a dive until landing.
(module
  (memory (export "memory") 1)

  (func (export "io") (result i32) (i32.const 0))

  (func (export "step") (result i32)
    (local $flags i32)
    (local $wx f32)
    (local $wz f32)
    (local $len f32)
    (local.set $flags (i32.load (i32.const 4)))
    ;; Grounded: the dive is over.
    (if (i32.and (local.get $flags) (i32.const 1))
      (then
        (i32.store (i32.const 72) (i32.const 0))
        (i32.store8 (i32.const 77) (i32.const 0))))
    (local.set $wx (f32.load (i32.const 52)))
    (local.set $wz (f32.load (i32.const 60)))
    (local.set $len
      (f32.sqrt (f32.add (f32.mul (local.get $wx) (local.get $wx))
                         (f32.mul (local.get $wz) (local.get $wz)))))
    ;; A new press of crouch, in the air, pushing somewhere, not yet dived.
    (if (i32.and
          (i32.and
            (i32.ne (i32.and (local.get $flags) (i32.const 8)) (i32.const 0))
            (i32.eqz (i32.load8_u (i32.const 76))))
          (i32.and
            (i32.and
              (i32.eqz (i32.and (local.get $flags) (i32.const 1)))
              (i32.eqz (i32.load8_u (i32.const 77))))
            (f32.gt (local.get $len) (f32.const 0.1))))
      (then
        (f32.store (i32.const 28)
          (f32.mul (f32.div (local.get $wx) (local.get $len)) (f32.const 14)))
        (f32.store (i32.const 32)
          (f32.max (f32.load (i32.const 32)) (f32.const 5)))
        (f32.store (i32.const 36)
          (f32.mul (f32.div (local.get $wz) (local.get $len)) (f32.const 14)))
        (i32.store (i32.const 72) (i32.const 2))
        (i32.store8 (i32.const 77) (i32.const 1))))
    (i32.store8 (i32.const 76)
      (i32.ne (i32.and (local.get $flags) (i32.const 8)) (i32.const 0)))
    (i32.const 0))
)
