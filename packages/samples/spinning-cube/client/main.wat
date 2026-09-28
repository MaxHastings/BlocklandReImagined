;; Spinning Cube: the client sandbox's sample Add-On. It builds one cube
;; mesh, gives it a material that uses the Add-On's own animated shader
;; (cube.wgsl), and draws it every frame bobbing up and down. The shader
;; spins it and animates its surface.
;;
;; The cube appears three units in front of wherever the camera was on
;; the first frame, and stays there.
;;
;; This is the source of main.wasm; `cargo test -p bri-client-sandbox`
;; checks the two match. Real Add-Ons are usually written in Rust, C or
;; Zig and compiled to wasm32; hand-written text keeps the sample readable
;; without a toolchain.
;;
;; Memory layout:
;;   0     24 vertices, 32 bytes each: position xyz, normal xyz, uv (f32)
;;   768   36 u32 indices, two triangles per face, counter-clockwise
;;   912   "client/cube.wgsl"
;;   928   "spinning cube ready"
;;   1024  the model matrix passed to draw (16 f32, column-major)
;;   1152  the camera: eye xyz, forward xyz (6 f32)
(module
  (import "bri" "log" (func $log (param i32 i32)))
  (import "bri" "shader" (func $shader (param i32 i32) (result i32)))
  (import "bri" "mesh_create" (func $mesh_create (param i32 i32 i32 i32) (result i32)))
  (import "bri" "material_create" (func $material_create (param i32) (result i32)))
  (import "bri" "material_set" (func $material_set (param i32 i32 f32 f32 f32 f32)))
  (import "bri" "draw" (func $draw (param i32 i32 i32)))
  (import "bri" "camera" (func $camera (param i32)))
  (memory (export "memory") 1)
  (global $mesh (mut i32) (i32.const -1))
  (global $material (mut i32) (i32.const -1))
  (global $placed (mut i32) (i32.const 0))
  (global $base_y (mut f32) (f32.const 0))
  (data (i32.const 0)
    "\00\00\00\3f\00\00\00\bf\00\00\00\3f\00\00\80\3f\00\00\00\00\00\00\00\00\00\00\00\00\00\00\80\3f"
    "\00\00\00\3f\00\00\00\bf\00\00\00\bf\00\00\80\3f\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\80\3f"
    "\00\00\00\3f\00\00\00\3f\00\00\00\bf\00\00\80\3f\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\00\00"
    "\00\00\00\3f\00\00\00\3f\00\00\00\3f\00\00\80\3f\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00"
    "\00\00\00\bf\00\00\00\bf\00\00\00\bf\00\00\80\bf\00\00\00\00\00\00\00\00\00\00\00\00\00\00\80\3f"
    "\00\00\00\bf\00\00\00\bf\00\00\00\3f\00\00\80\bf\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\80\3f"
    "\00\00\00\bf\00\00\00\3f\00\00\00\3f\00\00\80\bf\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\00\00"
    "\00\00\00\bf\00\00\00\3f\00\00\00\bf\00\00\80\bf\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00"
    "\00\00\00\bf\00\00\00\3f\00\00\00\3f\00\00\00\00\00\00\80\3f\00\00\00\00\00\00\00\00\00\00\80\3f"
    "\00\00\00\3f\00\00\00\3f\00\00\00\3f\00\00\00\00\00\00\80\3f\00\00\00\00\00\00\80\3f\00\00\80\3f"
    "\00\00\00\3f\00\00\00\3f\00\00\00\bf\00\00\00\00\00\00\80\3f\00\00\00\00\00\00\80\3f\00\00\00\00"
    "\00\00\00\bf\00\00\00\3f\00\00\00\bf\00\00\00\00\00\00\80\3f\00\00\00\00\00\00\00\00\00\00\00\00"
    "\00\00\00\bf\00\00\00\bf\00\00\00\bf\00\00\00\00\00\00\80\bf\00\00\00\00\00\00\00\00\00\00\80\3f"
    "\00\00\00\3f\00\00\00\bf\00\00\00\bf\00\00\00\00\00\00\80\bf\00\00\00\00\00\00\80\3f\00\00\80\3f"
    "\00\00\00\3f\00\00\00\bf\00\00\00\3f\00\00\00\00\00\00\80\bf\00\00\00\00\00\00\80\3f\00\00\00\00"
    "\00\00\00\bf\00\00\00\bf\00\00\00\3f\00\00\00\00\00\00\80\bf\00\00\00\00\00\00\00\00\00\00\00\00"
    "\00\00\00\bf\00\00\00\bf\00\00\00\3f\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\00\00\00\00\80\3f"
    "\00\00\00\3f\00\00\00\bf\00\00\00\3f\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\80\3f\00\00\80\3f"
    "\00\00\00\3f\00\00\00\3f\00\00\00\3f\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\80\3f\00\00\00\00"
    "\00\00\00\bf\00\00\00\3f\00\00\00\3f\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\00\00\00\00\00\00"
    "\00\00\00\3f\00\00\00\bf\00\00\00\bf\00\00\00\00\00\00\00\00\00\00\80\bf\00\00\00\00\00\00\80\3f"
    "\00\00\00\bf\00\00\00\bf\00\00\00\bf\00\00\00\00\00\00\00\00\00\00\80\bf\00\00\80\3f\00\00\80\3f"
    "\00\00\00\bf\00\00\00\3f\00\00\00\bf\00\00\00\00\00\00\00\00\00\00\80\bf\00\00\80\3f\00\00\00\00"
    "\00\00\00\3f\00\00\00\3f\00\00\00\bf\00\00\00\00\00\00\00\00\00\00\80\bf\00\00\00\00\00\00\00\00"
  )
  (data (i32.const 768)
    "\00\00\00\00\01\00\00\00\02\00\00\00\00\00\00\00\02\00\00\00\03\00\00\00"
    "\04\00\00\00\05\00\00\00\06\00\00\00\04\00\00\00\06\00\00\00\07\00\00\00"
    "\08\00\00\00\09\00\00\00\0a\00\00\00\08\00\00\00\0a\00\00\00\0b\00\00\00"
    "\0c\00\00\00\0d\00\00\00\0e\00\00\00\0c\00\00\00\0e\00\00\00\0f\00\00\00"
    "\10\00\00\00\11\00\00\00\12\00\00\00\10\00\00\00\12\00\00\00\13\00\00\00"
    "\14\00\00\00\15\00\00\00\16\00\00\00\14\00\00\00\16\00\00\00\17\00\00\00"
  )
  (data (i32.const 912) "client/cube.wgsl")
  (data (i32.const 928) "spinning cube ready")

  (func (export "init")
    (global.set $mesh
      (call $mesh_create (i32.const 0) (i32.const 24) (i32.const 768) (i32.const 36)))
    (global.set $material
      (call $material_create (call $shader (i32.const 912) (i32.const 16))))
    ;; params[0]: the cube's colour, a warm orange.
    (call $material_set (global.get $material) (i32.const 0)
      (f32.const 0.95) (f32.const 0.55) (f32.const 0.15) (f32.const 1))
    ;; The identity part of the model matrix; frame fills in the bob.
    (f32.store (i32.const 1024) (f32.const 1))
    (f32.store (i32.const 1044) (f32.const 1))
    (f32.store (i32.const 1064) (f32.const 1))
    (f32.store (i32.const 1084) (f32.const 1))
    (call $log (i32.const 928) (i32.const 19)))

  ;; Place the cube on the first frame, then bob it up and down 0.15 units
  ;; on a two-second triangle wave.
  (func (export "frame") (param $t f32) (param $dt f32)
    (local $x f32)
    (if (i32.eqz (global.get $placed))
      (then
        (call $camera (i32.const 1152))
        ;; Translation (floats 12 to 14) = eye + forward * 3.
        (f32.store (i32.const 1072)
          (f32.add (f32.load (i32.const 1152)) (f32.mul (f32.load (i32.const 1164)) (f32.const 3))))
        (global.set $base_y
          (f32.add (f32.load (i32.const 1156)) (f32.mul (f32.load (i32.const 1168)) (f32.const 3))))
        (f32.store (i32.const 1080)
          (f32.add (f32.load (i32.const 1160)) (f32.mul (f32.load (i32.const 1172)) (f32.const 3))))
        (global.set $placed (i32.const 1))))
    (local.set $x (f32.mul (local.get $t) (f32.const 0.5)))
    (local.set $x (f32.sub (local.get $x) (f32.floor (local.get $x))))
    (f32.store (i32.const 1076)
      (f32.add (global.get $base_y) (f32.mul (f32.const 0.15)
        (f32.sub
          (f32.mul (f32.abs (f32.sub (f32.mul (local.get $x) (f32.const 2)) (f32.const 1)))
                   (f32.const 2))
          (f32.const 1)))))
    (call $draw (global.get $mesh) (global.get $material) (i32.const 1024)))
)
