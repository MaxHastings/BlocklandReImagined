;; Air steer: a sample movement controller with no world queries, for a
;; heavy shooter's body. The archetype makes the jump high and floaty
;; (lower gravity) and the run slower; this adds the air control such
;; games give: in the air the movement keys turn the body's speed across
;; the ground toward 6 units/s their way, changing it by up to 24
;; units/s². A body already faster than 6 (a knockback) keeps its
;; momentum, and steering never takes it past 6, so there is no strafe
;; jumping. There is no double jump and no wall kick; the motor does the
;; rest, crouching included.
;;
;; This is the source of air_steer.wasm; `cargo test -p bri-controller`
;; checks the two match (BRI_BLESS=1 rewrites the binary).
;;
;; The record (`crates/controller/src/lib.rs`) is at 0. It keeps no
;; memory. The animation intent at 72 is 3 while it steers, else 0.
(module
  (memory (export "memory") 1)

  (func (export "io") (result i32) (i32.const 0))

  (func (export "step") (result i32)
    (local $wx f32)
    (local $wz f32)
    (local $len f32)
    (local $vx f32)
    (local $vz f32)
    (local $dx f32)
    (local $dz f32)
    (local $gap f32)
    (local $most f32)
    (i32.store (i32.const 72) (i32.const 0))
    ;; Grounded or swimming: the motor's own steering.
    (if (i32.and (i32.load (i32.const 4)) (i32.const 33))
      (then (return (i32.const 0))))
    (local.set $wx (f32.load (i32.const 52)))
    (local.set $wz (f32.load (i32.const 60)))
    (local.set $len
      (f32.sqrt (f32.add (f32.mul (local.get $wx) (local.get $wx))
                         (f32.mul (local.get $wz) (local.get $wz)))))
    (if (f32.lt (local.get $len) (f32.const 0.1))
      (then (return (i32.const 0))))
    (local.set $vx (f32.load (i32.const 28)))
    (local.set $vz (f32.load (i32.const 36)))
    ;; Already faster than a run: keep the momentum.
    (if (f32.gt
          (f32.add (f32.mul (local.get $vx) (local.get $vx))
                   (f32.mul (local.get $vz) (local.get $vz)))
          (f32.const 36))
      (then (return (i32.const 0))))
    ;; From the velocity to 6 along the keys, at most 24 a second of it.
    (local.set $dx
      (f32.sub (f32.mul (f32.div (local.get $wx) (local.get $len)) (f32.const 6))
               (local.get $vx)))
    (local.set $dz
      (f32.sub (f32.mul (f32.div (local.get $wz) (local.get $len)) (f32.const 6))
               (local.get $vz)))
    (local.set $gap
      (f32.sqrt (f32.add (f32.mul (local.get $dx) (local.get $dx))
                         (f32.mul (local.get $dz) (local.get $dz)))))
    (local.set $most (f32.mul (f32.const 24) (f32.load (i32.const 8))))
    (if (f32.gt (local.get $gap) (local.get $most))
      (then
        (local.set $dx (f32.mul (local.get $dx) (f32.div (local.get $most) (local.get $gap))))
        (local.set $dz (f32.mul (local.get $dz) (f32.div (local.get $most) (local.get $gap))))))
    (f32.store (i32.const 28) (f32.add (local.get $vx) (local.get $dx)))
    (f32.store (i32.const 36) (f32.add (local.get $vz) (local.get $dz)))
    (i32.store (i32.const 72) (i32.const 3))
    (i32.const 0))
)
