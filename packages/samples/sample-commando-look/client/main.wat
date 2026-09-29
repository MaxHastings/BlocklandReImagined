;; Commando Look's client code: the rifle in first person, and the scope
;; while aiming. It shows two seams a total conversion needs:
;;
;; - material_space: the rifle's boxes are drawn in view space (1), so they
;;   stay put in front of the eye, keep their size while the world zooms,
;;   and never sink into walls; the scope is one quad in screen space (2),
;;   drawn over everything.
;; - view and players: `view` says whether the player is in first person,
;;   aiming and alive; `players` with `image_kind` says whether the local
;;   player holds the Commando Rifle, so nothing is drawn for other items.
;;
;; This is the source of main.wasm; `cargo test -p bri-client-sandbox`
;; checks the two match. Real Add-Ons are usually written in Rust, C or
;; Zig and compiled to wasm32; hand-written text keeps the sample readable
;; without a toolchain.
;;
;; Memory layout:
;;   0     the unit cube: 24 vertices, 32 bytes each (position, normal, uv)
;;   768   its 36 u32 indices
;;   912   the screen quad, -1 to 1: 4 vertices
;;   1040  its 6 indices
;;   1100  "client/sights.wgsl"
;;   1120  "sample-commando-rifle:image/rifle"
;;   1160  "commando sights ready"
;;   1200  a draw's model matrix (16 f32, column-major)
;;   1280  a draw's params (16 f32)
;;   1344  the view record (12 f32)
;;   1408  player records, 64 of 16 f32
(module
  (import "bri" "log" (func $log (param i32 i32)))
  (import "bri" "shader" (func $shader (param i32 i32) (result i32)))
  (import "bri" "mesh_create" (func $mesh_create (param i32 i32 i32 i32) (result i32)))
  (import "bri" "material_create" (func $material_create (param i32) (result i32)))
  (import "bri" "material_space" (func $material_space (param i32 i32)))
  (import "bri" "draw_with" (func $draw_with (param i32 i32 i32 i32)))
  (import "bri" "view" (func $view (param i32)))
  (import "bri" "players" (func $players (param i32 i32) (result i32)))
  (import "bri" "image_kind" (func $image_kind (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (global $cube (mut i32) (i32.const -1))
  (global $quad (mut i32) (i32.const -1))
  (global $gun (mut i32) (i32.const -1))
  (global $scope (mut i32) (i32.const -1))
  (global $rifle (mut i32) (i32.const -1))
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
  (data (i32.const 912)
    "\00\00\80\bf\00\00\80\bf\00\00\00\00\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\00\00\00\00\00\00"
    "\00\00\80\3f\00\00\80\bf\00\00\00\00\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\80\3f\00\00\00\00"
    "\00\00\80\3f\00\00\80\3f\00\00\00\00\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\80\3f\00\00\80\3f"
    "\00\00\80\bf\00\00\80\3f\00\00\00\00\00\00\00\00\00\00\00\00\00\00\80\3f\00\00\00\00\00\00\80\3f"
  )
  (data (i32.const 1040)
    "\00\00\00\00\01\00\00\00\02\00\00\00\00\00\00\00\02\00\00\00\03\00\00\00"
  )
  (data (i32.const 1100) "client/sights.wgsl")
  (data (i32.const 1120) "sample-commando-rifle:image/rifle")
  (data (i32.const 1160) "commando sights ready")

  (func (export "init")
    (local $shader i32)
    (global.set $cube
      (call $mesh_create (i32.const 0) (i32.const 24) (i32.const 768) (i32.const 36)))
    (global.set $quad
      (call $mesh_create (i32.const 912) (i32.const 4) (i32.const 1040) (i32.const 6)))
    (local.set $shader (call $shader (i32.const 1100) (i32.const 18)))
    (global.set $gun (call $material_create (local.get $shader)))
    (call $material_space (global.get $gun) (i32.const 1))
    (global.set $scope (call $material_create (local.get $shader)))
    (call $material_space (global.get $scope) (i32.const 2))
    (global.set $rifle (call $image_kind (i32.const 1120) (i32.const 33)))
    (call $log (i32.const 1160) (i32.const 21)))

  ;; Set the draw's model matrix to a box of size (sx, sy, sz) at
  ;; (x, y, z), and its colour.
  (func $place (param $x f32) (param $y f32) (param $z f32)
                (param $sx f32) (param $sy f32) (param $sz f32)
                (param $r f32) (param $g f32) (param $b f32)
    (memory.fill (i32.const 1200) (i32.const 0) (i32.const 144))
    (f32.store (i32.const 1200) (local.get $sx))
    (f32.store (i32.const 1220) (local.get $sy))
    (f32.store (i32.const 1240) (local.get $sz))
    (f32.store (i32.const 1248) (local.get $x))
    (f32.store (i32.const 1252) (local.get $y))
    (f32.store (i32.const 1256) (local.get $z))
    (f32.store (i32.const 1260) (f32.const 1))
    (f32.store (i32.const 1280) (local.get $r))
    (f32.store (i32.const 1284) (local.get $g))
    (f32.store (i32.const 1288) (local.get $b))
    (f32.store (i32.const 1292) (f32.const 1)))

  ;; One box of the held rifle.
  (func $part (param $x f32) (param $y f32) (param $z f32)
               (param $sx f32) (param $sy f32) (param $sz f32)
               (param $r f32) (param $g f32) (param $b f32)
    (call $place
      (local.get $x) (local.get $y) (local.get $z)
      (local.get $sx) (local.get $sy) (local.get $sz)
      (local.get $r) (local.get $g) (local.get $b))
    (call $draw_with (global.get $cube) (global.get $gun) (i32.const 1200) (i32.const 1280)))

  ;; Whether the local player (flag 1) holds the rifle (record float 15).
  (func $holding (result i32)
    (local $n i32) (local $i i32) (local $at i32)
    (local.set $n (call $players (i32.const 1408) (i32.const 64)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_s (local.get $i) (local.get $n)))
        (local.set $at (i32.add (i32.const 1408) (i32.shl (local.get $i) (i32.const 6))))
        (if (i32.and (i32.trunc_f32_s (f32.load offset=4 (local.get $at))) (i32.const 1))
          (then
            (return (f32.eq (f32.load offset=60 (local.get $at))
                            (f32.convert_i32_s (global.get $rifle))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const 0))

  (func (export "frame") (param $t f32) (param $dt f32)
    (local $flags i32)
    (call $view (i32.const 1344))
    (local.set $flags (i32.trunc_f32_s (f32.load (i32.const 1364))))
    ;; Nothing unless alive (flag 4) and holding the rifle.
    (if (i32.eqz (i32.and (local.get $flags) (i32.const 4))) (then (return)))
    (if (i32.eqz (call $holding)) (then (return)))
    ;; Aiming (flag 2): the scope over the whole screen, and no rifle.
    (if (i32.and (local.get $flags) (i32.const 2))
      (then
;; The quad stretched across the screen's width (x runs to the
        ;; aspect); the shader keeps the lens round.
        (call $place (f32.const 0) (f32.const 0) (f32.const 0)
                     (f32.load (i32.const 1352)) (f32.const 1) (f32.const 1)
                     (f32.const 0) (f32.const 0) (f32.const 0))
        (f32.store (i32.const 1296) (f32.const 1))
        (f32.store (i32.const 1300) (f32.load (i32.const 1352)))
        (call $draw_with (global.get $quad) (global.get $scope) (i32.const 1200) (i32.const 1280))
        (return)))
    ;; In first person (flag 1): the rifle, low on the right.
    (if (i32.eqz (i32.and (local.get $flags) (i32.const 1))) (then (return)))
    ;; Receiver, barrel, scope, magazine, grip and the hand holding it.
    (call $part (f32.const 0.26) (f32.const -0.17) (f32.const -0.62)
                (f32.const 0.09) (f32.const 0.11) (f32.const 0.62)
                (f32.const 0.22) (f32.const 0.24) (f32.const 0.2))
    (call $part (f32.const 0.26) (f32.const -0.15) (f32.const -1.1)
                (f32.const 0.035) (f32.const 0.035) (f32.const 0.4)
                (f32.const 0.1) (f32.const 0.1) (f32.const 0.1))
    (call $part (f32.const 0.26) (f32.const -0.07) (f32.const -0.62)
                (f32.const 0.06) (f32.const 0.06) (f32.const 0.3)
                (f32.const 0.05) (f32.const 0.05) (f32.const 0.06))
    (call $part (f32.const 0.26) (f32.const -0.28) (f32.const -0.72)
                (f32.const 0.06) (f32.const 0.14) (f32.const 0.1)
                (f32.const 0.3) (f32.const 0.33) (f32.const 0.22))
    (call $part (f32.const 0.26) (f32.const -0.28) (f32.const -0.46)
                (f32.const 0.07) (f32.const 0.13) (f32.const 0.08)
                (f32.const 0.16) (f32.const 0.14) (f32.const 0.12))
    (call $part (f32.const 0.27) (f32.const -0.3) (f32.const -0.44)
                (f32.const 0.11) (f32.const 0.09) (f32.const 0.13)
                (f32.const 0.93) (f32.const 0.78) (f32.const 0.6)))
)
