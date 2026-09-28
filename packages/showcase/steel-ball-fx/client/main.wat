;; Steel Ball effects: draws every Steel Ball as a polished mirror sphere
;; (steel.wgsl) exactly over the ball the game simulates, reflecting the
;; map's own sky and sun. Players who do not run this code still see the
;; ball's plain brushed-steel model.
;;
;; Each frame it asks the game where the vehicles are (`vehicles`, the
;; `world.read` capability) and draws one sphere per Steel Ball, passing
;; the ball's position, size and rotation and the scene's lighting to the
;; shader as that draw's parameters (`draw_with`).
;;
;; This is the source of main.wasm; `cargo test -p bri-client-sandbox
;; --test showcase` checks the two match (BRI_BLESS=1 rewrites it).
;;
;; Memory layout:
;;   0      strings
;;   1024   identity model matrix (16 f32)
;;   1088   draw parameters (16 f32)
;;   1152   the scene's lighting (12 f32)
;;   2048   vehicle records, 16 f32 (64 bytes) each, up to 256
;;   20480  grid mesh vertices, 32 bytes each
;;   61440  grid mesh indices
(module
  (import "bri" "log" (func $log (param i32 i32)))
  (import "bri" "shader" (func $shader (param i32 i32) (result i32)))
  (import "bri" "mesh_create" (func $mesh_create (param i32 i32 i32 i32) (result i32)))
  (import "bri" "material_create" (func $material_create (param i32) (result i32)))
  (import "bri" "draw_with" (func $draw_with (param i32 i32 i32 i32)))
  (import "bri" "environment" (func $environment (param i32)))
  (import "bri" "vehicle_kind" (func $vehicle_kind (param i32 i32) (result i32)))
  (import "bri" "vehicles" (func $vehicles (param i32 i32) (result i32)))
  (memory (export "memory") 2)
  (global $mesh (mut i32) (i32.const -1))
  (global $material (mut i32) (i32.const -1))
  (global $ball (mut i32) (i32.const -1))
  (data (i32.const 0) "client/steel.wgsl")
  (data (i32.const 64) "steel-ball-kit:vehicle/steelball")
  (data (i32.const 128) "steel ball ready")

  ;; A (cols x rows) grid of vertices at uv (0..1, 0..1), normal +z,
  ;; two counter-clockwise triangles a cell. The shader shapes it.
  (func $grid (param $cols i32) (param $rows i32) (result i32)
    (local $i i32) (local $j i32) (local $at i32) (local $n i32) (local $a i32)
    (local.set $at (i32.const 20480))
    (local.set $j (i32.const 0))
    (block $rows_done
      (loop $rows_loop
        (br_if $rows_done (i32.gt_s (local.get $j) (local.get $rows)))
        (local.set $i (i32.const 0))
        (block $cols_done
          (loop $cols_loop
            (br_if $cols_done (i32.gt_s (local.get $i) (local.get $cols)))
            (f32.store (local.get $at)
              (f32.div (f32.convert_i32_s (local.get $i)) (f32.convert_i32_s (local.get $cols))))
            (f32.store offset=4 (local.get $at)
              (f32.div (f32.convert_i32_s (local.get $j)) (f32.convert_i32_s (local.get $rows))))
            (f32.store offset=8 (local.get $at) (f32.const 0))
            (f32.store offset=12 (local.get $at) (f32.const 0))
            (f32.store offset=16 (local.get $at) (f32.const 0))
            (f32.store offset=20 (local.get $at) (f32.const 1))
            (f32.store offset=24 (local.get $at) (f32.load (local.get $at)))
            (f32.store offset=28 (local.get $at) (f32.load offset=4 (local.get $at)))
            (local.set $at (i32.add (local.get $at) (i32.const 32)))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $cols_loop)))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (br $rows_loop)))
    (local.set $at (i32.const 61440))
    (local.set $j (i32.const 0))
    (block $done
      (loop $cells
        (br_if $done (i32.ge_s (local.get $j) (local.get $rows)))
        (local.set $i (i32.const 0))
        (block $row_done
          (loop $row
            (br_if $row_done (i32.ge_s (local.get $i) (local.get $cols)))
            ;; a = j * (cols + 1) + i; b = a + 1; c = a + cols + 1; d = c + 1
            (local.set $a (i32.add (i32.mul (local.get $j) (i32.add (local.get $cols) (i32.const 1)))
                                   (local.get $i)))
            (i32.store (local.get $at) (local.get $a))
            (i32.store offset=4 (local.get $at) (i32.add (local.get $a) (i32.const 1)))
            (i32.store offset=8 (local.get $at)
              (i32.add (local.get $a) (i32.add (local.get $cols) (i32.const 1))))
            (i32.store offset=12 (local.get $at) (i32.add (local.get $a) (i32.const 1)))
            (i32.store offset=16 (local.get $at)
              (i32.add (local.get $a) (i32.add (local.get $cols) (i32.const 2))))
            (i32.store offset=20 (local.get $at)
              (i32.add (local.get $a) (i32.add (local.get $cols) (i32.const 1))))
            (local.set $at (i32.add (local.get $at) (i32.const 24)))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $row)))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (br $cells)))
    (local.set $n (i32.mul (i32.mul (local.get $cols) (local.get $rows)) (i32.const 6)))
    (call $mesh_create
      (i32.const 20480)
      (i32.mul (i32.add (local.get $cols) (i32.const 1)) (i32.add (local.get $rows) (i32.const 1)))
      (i32.const 61440)
      (local.get $n)))

  (func (export "init")
    (global.set $mesh (call $grid (i32.const 48) (i32.const 24)))
    (global.set $material (call $material_create (call $shader (i32.const 0) (i32.const 17))))
    (global.set $ball (call $vehicle_kind (i32.const 64) (i32.const 32)))
    (f32.store (i32.const 1024) (f32.const 1))
    (f32.store (i32.const 1044) (f32.const 1))
    (f32.store (i32.const 1064) (f32.const 1))
    (f32.store (i32.const 1084) (f32.const 1))
    (call $log (i32.const 128) (i32.const 16)))

  (func (export "frame") (param $t f32) (param $dt f32)
    (local $count i32) (local $i i32) (local $at i32)
    (call $environment (i32.const 1152))
    ;; The lighting is the same for every ball this frame.
    (f32.store (i32.const 1120) (f32.load (i32.const 1152)))
    (f32.store (i32.const 1124) (f32.load (i32.const 1156)))
    (f32.store (i32.const 1128) (f32.load (i32.const 1160)))
    ;; Sun strength: how bright the sun's colour is.
    (f32.store (i32.const 1132)
      (f32.mul (f32.const 0.4)
        (f32.add (f32.load (i32.const 1164))
          (f32.add (f32.load (i32.const 1168)) (f32.load (i32.const 1172))))))
    (f32.store (i32.const 1136) (f32.load (i32.const 1188)))
    (f32.store (i32.const 1140) (f32.load (i32.const 1192)))
    (f32.store (i32.const 1144) (f32.load (i32.const 1196)))
    (f32.store (i32.const 1148) (f32.const 0))
    (local.set $count (call $vehicles (i32.const 2048) (i32.const 256)))
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (local.get $count)))
        (local.set $at (i32.add (i32.const 2048) (i32.mul (local.get $i) (i32.const 64))))
        ;; A Steel Ball: record float 1 is its kind.
        (if (f32.eq (f32.load offset=4 (local.get $at)) (f32.convert_i32_s (global.get $ball)))
          (then
            ;; Centre (floats 2..4), and the ball's radius a hair larger
            ;; than the plain model underneath. Float 12 is the radius of
            ;; a sphere round the ball's box: its radius times the square
            ;; root of 3.
            (f32.store (i32.const 1088) (f32.load offset=8 (local.get $at)))
            (f32.store (i32.const 1092) (f32.load offset=12 (local.get $at)))
            (f32.store (i32.const 1096) (f32.load offset=16 (local.get $at)))
            (f32.store (i32.const 1100)
              (f32.mul (f32.load offset=48 (local.get $at)) (f32.const 0.57966)))
            ;; Rotation (floats 5..8).
            (f32.store (i32.const 1104) (f32.load offset=20 (local.get $at)))
            (f32.store (i32.const 1108) (f32.load offset=24 (local.get $at)))
            (f32.store (i32.const 1112) (f32.load offset=28 (local.get $at)))
            (f32.store (i32.const 1116) (f32.load offset=32 (local.get $at)))
            (call $draw_with (global.get $mesh) (global.get $material)
              (i32.const 1024) (i32.const 1088))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each))))
)
