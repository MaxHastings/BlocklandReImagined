;; Gravity Gun effects, drawn on every player's screen for everyone's gun:
;;
;; - the gun itself: the Printer it is built from, reskinned as an alien
;;   weapon (alien.wgsl): a dark oily shell with glowing veins that pulse
;;   at rest and flare while the beam is on;
;; - holding: a beam (beam.wgsl, a soft glow and a hot core) from the
;;   gun's muzzle, leaving it the way the player aims and bending round
;;   into the spot it grabbed, so it whips when the held thing lags; a
;;   glow where it grips, a faint bubble round the held thing (field.wgsl)
;;   and sparks circling it (spark.wgsl);
;; - the trigger held with nothing caught: a thinner beam to where it
;;   points;
;; - a throw (letting go of something flying): a shockwave ring
;;   (ring.wgsl) and a spray of sparks where it went, and a smaller ring
;;   at the muzzle;
;; - sounds for each, placed where they happen: grab, drop and launch
;;   (made by tools/make_showcase_sounds.py);
;; - a dead player held while the Ragdoll Add-On runs: the limb the beam
;;   met (`physics.local`, its limbs are shared bodies) is pulled to the
;;   beam's end, so the body dangles from it, and flies on when let go.
;;   The server carries the corpse itself; this is only how it hangs.
;;
;; Everything comes from what the game already knows (`world.read`):
;; where players and vehicles are drawn, where each player's gun is drawn
;; and its muzzle (`held`), the gun's own model (`image_mesh`), and each
;; player's `beam` from the Gravity Gun Add-On's public state: [held kind,
;; held id, beam on, throws, last throw's kind, last throw's id, beam
;; length]; kinds 1 vehicle, 2 player, 3 Add-On creature. The server sends
;; that only when it changes; the motion itself comes from the game's own
;; pose updates.
;;
;; This is the source of main.wasm; `cargo test -p bri-client-sandbox
;; --test showcase` checks the two match (BRI_BLESS=1 rewrites it).
;;
;; Memory layout:
;;   0      strings
;;   1024   identity model matrix (16 f32)
;;   1088   draw parameters (16 f32)
;;   1152   the environment record (12 f32)
;;   1280   the held record (20 f32): the gun's model matrix, its muzzle
;;   1408   what rigid_find found (8 f32)
;;   1440   a body as rigid_get gives it (16 f32)
;;   2048   player records, 16 f32 (64 bytes) each, up to 64
;;   8192   vehicle records, 16 f32 each, up to 256
;;   24576  per-player effect state, 128 bytes each, 64 slots:
;;            +0 id  +4 in use  +8 throws  +12 throws known
;;            +24 last throw time  +28 throw centre xyz
;;            +40 throw direction xyz  +52 throw radius  +56 seen this frame
;;            +60 muzzle at the throw xyz  +72 what was held last frame
;;            +76 its id  +80 the grip, in the held thing's own frame xyz
;;            +96 the ragdoll limb gripped (i32, 0 none)  +100 the grip on
;;            it, in its frame xyz  +112 where it was pulled last frame xyz
;;            +124 whether there was a last frame
;;   32768  mesh vertices being built (32 bytes each)
;;   65536  mesh indices being built
;;   98304  creature records, 8 f32 (32 bytes) each, up to 64
(module
  (import "bri" "log" (func $log (param i32 i32)))
  (import "bri" "random" (func $random (result i32)))
  (import "bri" "shader" (func $shader (param i32 i32) (result i32)))
  (import "bri" "mesh_create" (func $mesh_create (param i32 i32 i32 i32) (result i32)))
  (import "bri" "material_create" (func $material_create (param i32) (result i32)))
  (import "bri" "material_blend" (func $material_blend (param i32 i32)))
  (import "bri" "draw_with" (func $draw_with (param i32 i32 i32 i32)))
  (import "bri" "environment" (func $environment (param i32)))
  (import "bri" "players" (func $players (param i32 i32) (result i32)))
  (import "bri" "vehicles" (func $vehicles (param i32 i32) (result i32)))
  (import "bri" "entities" (func $entities (param i32 i32) (result i32)))
  (import "bri" "image_kind" (func $image_kind (param i32 i32) (result i32)))
  (import "bri" "image_mesh" (func $image_mesh (param i32) (result i32)))
  (import "bri" "held" (func $held (param i32 i32 i32) (result i32)))
  (import "bri" "state_num" (func $state_num (param i32 i32 i32 i32 i32 i32) (result f32)))
  (import "bri" "sound_at" (func $sound_at (param i32 i32 f32 f32 f32 f32) (result i32)))
  (import "bri" "rigid_find" (func $rigid_find (param f32 f32 f32 f32 f32 f32 f32 i32) (result i32)))
  (import "bri" "rigid_hold"
    (func $rigid_hold (param i32 f32 f32 f32 f32 f32 f32 f32 f32 f32 f32)))
  (import "bri" "rigid_get" (func $rigid_get (param i32 i32) (result i32)))
  (memory (export "memory") 2)

  (global $tube (mut i32) (i32.const 0))
  (global $sphere (mut i32) (i32.const 0))
  (global $band (mut i32) (i32.const 0))
  (global $sparks (mut i32) (i32.const 0))
  (global $m_beam (mut i32) (i32.const 0))
  (global $m_field (mut i32) (i32.const 0))
  (global $m_spark (mut i32) (i32.const 0))
  (global $m_ring (mut i32) (i32.const 0))
  (global $m_skin (mut i32) (i32.const 0))
  ;; The gun's image, as `players` records name it, and its model once
  ;; someone holds it (-1 before).
  (global $gun (mut i32) (i32.const 0))
  (global $gun_mesh (mut i32) (i32.const -1))
  (global $players_n (mut i32) (i32.const 0))
  (global $vehicles_n (mut i32) (i32.const 0))
  (global $entities_n (mut i32) (i32.const 0))
  ;; What $locate found: centre, radius and rotation.
  (global $ox (mut f32) (f32.const 0))
  (global $oy (mut f32) (f32.const 0))
  (global $oz (mut f32) (f32.const 0))
  (global $or (mut f32) (f32.const 0))
  (global $qx (mut f32) (f32.const 0))
  (global $qy (mut f32) (f32.const 0))
  (global $qz (mut f32) (f32.const 0))
  (global $qw (mut f32) (f32.const 1))
  ;; What $rotate made.
  (global $rx (mut f32) (f32.const 0))
  (global $ry (mut f32) (f32.const 0))
  (global $rz (mut f32) (f32.const 0))

  (data (i32.const 0) "client/beam.wgsl")
  (data (i32.const 32) "client/field.wgsl")
  (data (i32.const 64) "client/spark.wgsl")
  (data (i32.const 96) "client/ring.wgsl")
  (data (i32.const 128) "gravity-gun")
  (data (i32.const 144) "beam")
  (data (i32.const 160) "gravity gun effects ready")
  (data (i32.const 192) "client/sounds/grab.wav")
  (data (i32.const 224) "client/sounds/drop.wav")
  (data (i32.const 288) "client/sounds/launch.wav")
  (data (i32.const 352) "client/alien.wgsl")
  (data (i32.const 384) "gravity-gun-tool:image/gravitygun")

  ;; ---- Meshes ----

  ;; A (cols x rows) grid at uv (0..1, 0..1), two triangles a cell.
  (func $grid (param $cols i32) (param $rows i32) (result i32)
    (local $i i32) (local $j i32) (local $at i32) (local $a i32)
    (local.set $at (i32.const 32768))
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
    (local.set $at (i32.const 65536))
    (local.set $j (i32.const 0))
    (block $done
      (loop $cells
        (br_if $done (i32.ge_s (local.get $j) (local.get $rows)))
        (local.set $i (i32.const 0))
        (block $row_done
          (loop $row
            (br_if $row_done (i32.ge_s (local.get $i) (local.get $cols)))
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
    (call $mesh_create
      (i32.const 32768)
      (i32.mul (i32.add (local.get $cols) (i32.const 1)) (i32.add (local.get $rows) (i32.const 1)))
      (i32.const 65536)
      (i32.mul (i32.mul (local.get $cols) (local.get $rows)) (i32.const 6))))

  ;; A number in [0, 1) from the host's presentation randomness.
  (func $rand (result f32)
    (f32.div
      (f32.convert_i32_u (i32.shr_u (call $random) (i32.const 8)))
      (f32.const 16777216)))

  ;; One vertex: position (x, y, 0), normal (seed), uv (u, v).
  (func $vertex (param $at i32) (param $x f32) (param $y f32)
                (param $s0 f32) (param $s1 f32) (param $s2 f32) (param $u f32) (param $v f32)
    (f32.store (local.get $at) (local.get $x))
    (f32.store offset=4 (local.get $at) (local.get $y))
    (f32.store offset=8 (local.get $at) (f32.const 0))
    (f32.store offset=12 (local.get $at) (local.get $s0))
    (f32.store offset=16 (local.get $at) (local.get $s1))
    (f32.store offset=20 (local.get $at) (local.get $s2))
    (f32.store offset=24 (local.get $at) (local.get $u))
    (f32.store offset=28 (local.get $at) (local.get $v)))

  ;; `n` camera-facing quads for spark.wgsl: each quad's four corners share
  ;; its place in the batch (position x) and its random numbers (normal
  ;; and position y); uv holds the corner.
  (func $particles (param $n i32) (result i32)
    (local $k i32) (local $at i32) (local $ix i32) (local $base i32)
    (local $index f32) (local $s0 f32) (local $s1 f32) (local $s2 f32) (local $s3 f32)
    (local.set $k (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $k) (local.get $n)))
        (local.set $index
          (f32.div (f32.convert_i32_s (local.get $k)) (f32.convert_i32_s (local.get $n))))
        (local.set $s0 (call $rand))
        (local.set $s1 (call $rand))
        (local.set $s2 (call $rand))
        (local.set $s3 (call $rand))
        (local.set $at (i32.add (i32.const 32768) (i32.mul (local.get $k) (i32.const 128))))
        (call $vertex (local.get $at) (local.get $index) (local.get $s3)
          (local.get $s0) (local.get $s1) (local.get $s2) (f32.const -1) (f32.const -1))
        (call $vertex (i32.add (local.get $at) (i32.const 32)) (local.get $index) (local.get $s3)
          (local.get $s0) (local.get $s1) (local.get $s2) (f32.const 1) (f32.const -1))
        (call $vertex (i32.add (local.get $at) (i32.const 64)) (local.get $index) (local.get $s3)
          (local.get $s0) (local.get $s1) (local.get $s2) (f32.const 1) (f32.const 1))
        (call $vertex (i32.add (local.get $at) (i32.const 96)) (local.get $index) (local.get $s3)
          (local.get $s0) (local.get $s1) (local.get $s2) (f32.const -1) (f32.const 1))
        (local.set $ix (i32.add (i32.const 65536) (i32.mul (local.get $k) (i32.const 24))))
        (local.set $base (i32.mul (local.get $k) (i32.const 4)))
        (i32.store (local.get $ix) (local.get $base))
        (i32.store offset=4 (local.get $ix) (i32.add (local.get $base) (i32.const 1)))
        (i32.store offset=8 (local.get $ix) (i32.add (local.get $base) (i32.const 2)))
        (i32.store offset=12 (local.get $ix) (local.get $base))
        (i32.store offset=16 (local.get $ix) (i32.add (local.get $base) (i32.const 2)))
        (i32.store offset=20 (local.get $ix) (i32.add (local.get $base) (i32.const 3)))
        (local.set $k (i32.add (local.get $k) (i32.const 1)))
        (br $each)))
    (call $mesh_create (i32.const 32768) (i32.mul (local.get $n) (i32.const 4))
                       (i32.const 65536) (i32.mul (local.get $n) (i32.const 6))))

  (func $material (param $name i32) (param $len i32) (result i32)
    (local $m i32)
    (local.set $m (call $material_create (call $shader (local.get $name) (local.get $len))))
    ;; Every effect is light: added over the scene, both faces, no depth.
    (call $material_blend (local.get $m) (i32.const 1))
    (local.get $m))

  (func (export "init")
    (global.set $tube (call $grid (i32.const 2) (i32.const 64)))
    (global.set $sphere (call $grid (i32.const 36) (i32.const 18)))
    (global.set $band (call $grid (i32.const 48) (i32.const 1)))
    (global.set $sparks (call $particles (i32.const 96)))
    (global.set $m_beam (call $material (i32.const 0) (i32.const 16)))
    (global.set $m_field (call $material (i32.const 32) (i32.const 17)))
    (global.set $m_spark (call $material (i32.const 64) (i32.const 17)))
    (global.set $m_ring (call $material (i32.const 96) (i32.const 16)))
    ;; The skin is solid: it covers the game's own Printer.
    (global.set $m_skin (call $material_create (call $shader (i32.const 352) (i32.const 17))))
    (global.set $gun (call $image_kind (i32.const 384) (i32.const 33)))
    (f32.store (i32.const 1024) (f32.const 1))
    (f32.store (i32.const 1044) (f32.const 1))
    (f32.store (i32.const 1064) (f32.const 1))
    (f32.store (i32.const 1084) (f32.const 1))
    (call $log (i32.const 160) (i32.const 25)))

  ;; ---- Drawing ----

  ;; Parameter `i` (0 to 3) of the next draw.
  (func $param (param $i i32) (param $x f32) (param $y f32) (param $z f32) (param $w f32)
    (local $at i32)
    (local.set $at (i32.add (i32.const 1088) (i32.mul (local.get $i) (i32.const 16))))
    (f32.store (local.get $at) (local.get $x))
    (f32.store offset=4 (local.get $at) (local.get $y))
    (f32.store offset=8 (local.get $at) (local.get $z))
    (f32.store offset=12 (local.get $at) (local.get $w)))

  (func $draw (param $mesh i32) (param $material i32)
    (call $draw_with (local.get $mesh) (local.get $material) (i32.const 1024) (i32.const 1088)))

  ;; The gun's colour: the cold blue-green of its veins and beam.
  (func $colour (param $alpha f32)
    (call $param (i32.const 3) (f32.const 0.3) (f32.const 0.95) (f32.const 1) (local.get $alpha)))

  ;; A beam from the muzzle (x0) through the bend's pull (b) to the end
  ;; (x1): a wide glow and a thin core, `strength` 0 to 1.
  (func $beam (param $x0 f32) (param $y0 f32) (param $z0 f32)
              (param $bx f32) (param $by f32) (param $bz f32)
              (param $x1 f32) (param $y1 f32) (param $z1 f32)
              (param $seed f32) (param $strength f32)
    (call $param (i32.const 0) (local.get $x0) (local.get $y0) (local.get $z0)
      (f32.mul (f32.const 0.16) (local.get $strength)))
    (call $param (i32.const 1) (local.get $x1) (local.get $y1) (local.get $z1) (f32.const 0))
    (call $param (i32.const 2) (local.get $bx) (local.get $by) (local.get $bz) (local.get $seed))
    (call $colour (f32.mul (f32.const 0.75) (local.get $strength)))
    (call $draw (global.get $tube) (global.get $m_beam))
    (call $param (i32.const 0) (local.get $x0) (local.get $y0) (local.get $z0)
      (f32.mul (f32.const 0.035) (local.get $strength)))
    (call $param (i32.const 1) (local.get $x1) (local.get $y1) (local.get $z1) (f32.const 1))
    (call $param (i32.const 2) (local.get $bx) (local.get $by) (local.get $bz)
      (f32.add (local.get $seed) (f32.const 2.7)))
    (call $colour (local.get $strength))
    (call $draw (global.get $tube) (global.get $m_beam)))

  ;; One glowing orb at x, `size` across.
  (func $orb (param $x f32) (param $y f32) (param $z f32) (param $size f32) (param $alpha f32)
    (call $param (i32.const 0) (local.get $x) (local.get $y) (local.get $z) (f32.const 0))
    (call $param (i32.const 1) (f32.const 3) (f32.const 0) (f32.const 1) (local.get $size))
    (call $colour (local.get $alpha))
    (call $draw (global.get $sparks) (global.get $m_spark)))

  ;; ---- Finding things ----

  ;; Where the object of `kind` (1 vehicle, 2 player, 3 creature) and `id`
  ;; is drawn: sets $ox $oy $oz (its middle), $or (its size) and $q (its
  ;; rotation; none for players and creatures). 0 when not found.
  (func $locate (param $kind f32) (param $id f32) (result i32)
    (local $i i32) (local $at i32)
    (global.set $qx (f32.const 0))
    (global.set $qy (f32.const 0))
    (global.set $qz (f32.const 0))
    (global.set $qw (f32.const 1))
    (if (f32.eq (local.get $kind) (f32.const 1))
      (then
        (local.set $i (i32.const 0))
        (block $done
          (loop $each
            (br_if $done (i32.ge_s (local.get $i) (global.get $vehicles_n)))
            (local.set $at (i32.add (i32.const 8192) (i32.mul (local.get $i) (i32.const 64))))
            (if (f32.eq (f32.load (local.get $at)) (local.get $id))
              (then
                (global.set $ox (f32.load offset=8 (local.get $at)))
                (global.set $oy (f32.load offset=12 (local.get $at)))
                (global.set $oz (f32.load offset=16 (local.get $at)))
                (global.set $qx (f32.load offset=20 (local.get $at)))
                (global.set $qy (f32.load offset=24 (local.get $at)))
                (global.set $qz (f32.load offset=28 (local.get $at)))
                (global.set $qw (f32.load offset=32 (local.get $at)))
                ;; The record's radius spans the box's corners; the
                ;; bubble hugs a little tighter.
                (global.set $or (f32.mul (f32.load offset=48 (local.get $at)) (f32.const 0.72)))
                (return (i32.const 1))))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $each)))))
    (if (f32.eq (local.get $kind) (f32.const 2))
      (then
        (local.set $i (i32.const 0))
        (block $done
          (loop $each
            (br_if $done (i32.ge_s (local.get $i) (global.get $players_n)))
            (local.set $at (i32.add (i32.const 2048) (i32.mul (local.get $i) (i32.const 64))))
            (if (f32.eq (f32.load (local.get $at)) (local.get $id))
              (then
                (global.set $ox (f32.load offset=8 (local.get $at)))
                (global.set $oy (f32.add (f32.load offset=12 (local.get $at)) (f32.const 1.3)))
                (global.set $oz (f32.load offset=16 (local.get $at)))
                (global.set $or (f32.const 1.5))
                (return (i32.const 1))))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $each)))))
    (if (f32.eq (local.get $kind) (f32.const 3))
      (then
        (local.set $i (i32.const 0))
        (block $done
          (loop $each
            (br_if $done (i32.ge_s (local.get $i) (global.get $entities_n)))
            (local.set $at (i32.add (i32.const 98304) (i32.mul (local.get $i) (i32.const 32))))
            (if (f32.eq (f32.load (local.get $at)) (local.get $id))
              (then
                (global.set $ox (f32.load offset=4 (local.get $at)))
                (global.set $oy (f32.add (f32.load offset=8 (local.get $at)) (f32.const 1.3)))
                (global.set $oz (f32.load offset=12 (local.get $at)))
                (global.set $or (f32.const 1.5))
                (return (i32.const 1))))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $each)))))
    (i32.const 0))

  ;; Turn (x, y, z) by the rotation $locate found (`sign` 1), or back
  ;; (`sign` -1), into $rx $ry $rz: v + 2w(q x v) + 2q x (q x v).
  (func $rotate (param $x f32) (param $y f32) (param $z f32) (param $sign f32)
    (local $ax f32) (local $ay f32) (local $az f32) (local $w f32)
    (local $tx f32) (local $ty f32) (local $tz f32)
    (local.set $ax (f32.mul (global.get $qx) (local.get $sign)))
    (local.set $ay (f32.mul (global.get $qy) (local.get $sign)))
    (local.set $az (f32.mul (global.get $qz) (local.get $sign)))
    (local.set $w (global.get $qw))
    ;; t = 2 (q x v)
    (local.set $tx (f32.mul (f32.const 2)
      (f32.sub (f32.mul (local.get $ay) (local.get $z)) (f32.mul (local.get $az) (local.get $y)))))
    (local.set $ty (f32.mul (f32.const 2)
      (f32.sub (f32.mul (local.get $az) (local.get $x)) (f32.mul (local.get $ax) (local.get $z)))))
    (local.set $tz (f32.mul (f32.const 2)
      (f32.sub (f32.mul (local.get $ax) (local.get $y)) (f32.mul (local.get $ay) (local.get $x)))))
    ;; v + w t + q x t
    (global.set $rx (f32.add (f32.add (local.get $x) (f32.mul (local.get $w) (local.get $tx)))
      (f32.sub (f32.mul (local.get $ay) (local.get $tz)) (f32.mul (local.get $az) (local.get $ty)))))
    (global.set $ry (f32.add (f32.add (local.get $y) (f32.mul (local.get $w) (local.get $ty)))
      (f32.sub (f32.mul (local.get $az) (local.get $tx)) (f32.mul (local.get $ax) (local.get $tz)))))
    (global.set $rz (f32.add (f32.add (local.get $z) (f32.mul (local.get $w) (local.get $tz)))
      (f32.sub (f32.mul (local.get $ax) (local.get $ty)) (f32.mul (local.get $ay) (local.get $tx))))))

  ;; The effect state of the player `id`: theirs, else a free slot.
  (func $slot (param $id f32) (result i32)
    (local $i i32) (local $at i32) (local $free i32)
    (local.set $free (i32.const -1))
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 64)))
        (local.set $at (i32.add (i32.const 24576) (i32.mul (local.get $i) (i32.const 128))))
        (if (f32.eq (f32.load offset=4 (local.get $at)) (f32.const 1))
          (then
            (if (f32.eq (f32.load (local.get $at)) (local.get $id))
              (then (return (local.get $at)))))
          (else
            (if (i32.lt_s (local.get $free) (i32.const 0))
              (then (local.set $free (local.get $at))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (if (i32.lt_s (local.get $free) (i32.const 0))
      (then (local.set $free (i32.const 24576))))
    (memory.fill (local.get $free) (i32.const 0) (i32.const 128))
    (f32.store (local.get $free) (local.get $id))
    (f32.store offset=4 (local.get $free) (f32.const 1))
    (f32.store offset=24 (local.get $free) (f32.const -100))
    (local.get $free))

  (func $state (param $player i32) (param $index i32) (result f32)
    (call $state_num (i32.const 128) (i32.const 11) (i32.const 144) (i32.const 4)
      (local.get $player) (local.get $index)))

  ;; Play one of the sounds (by its string's address and length) there.
  (func $sound (param $name i32) (param $len i32) (param $volume f32)
               (param $x f32) (param $y f32) (param $z f32)
    (drop (call $sound_at (local.get $name) (local.get $len) (local.get $volume)
      (local.get $x) (local.get $y) (local.get $z))))

  ;; ---- Each frame ----

  (func (export "frame") (param $t f32) (param $dt f32)
    (local $i i32) (local $at i32) (local $slot i32) (local $player i32) (local $drawn i32)
    (local $id f32) (local $held f32) (local $held_id f32) (local $on f32)
    (local $shots f32) (local $shot_kind f32) (local $shot_id f32) (local $reach f32)
    (local $ex f32) (local $ey f32) (local $ez f32)
    (local $lx f32) (local $ly f32) (local $lz f32)
    (local $rx f32) (local $rz f32) (local $rl f32)
    (local $mx f32) (local $my f32) (local $mz f32)
    (local $gx f32) (local $gy f32) (local $gz f32) (local $span f32)
    (local $age f32) (local $shot i32) (local $body i32)
    (local $tx f32) (local $ty f32) (local $tz f32)
    (local $vx f32) (local $vy f32) (local $vz f32)
    (global.set $players_n (call $players (i32.const 2048) (i32.const 64)))
    (global.set $vehicles_n (call $vehicles (i32.const 8192) (i32.const 256)))
    (global.set $entities_n (call $entities (i32.const 98304) (i32.const 64)))
    (call $environment (i32.const 1152))
    ;; Slots not seen this frame belong to players who left.
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 64)))
        (f32.store offset=56 (i32.add (i32.const 24576) (i32.mul (local.get $i) (i32.const 128)))
          (f32.const 0))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (local.set $i (i32.const 0))
    (block $players_done
      (loop $each_player
        (br_if $players_done (i32.ge_s (local.get $i) (global.get $players_n)))
        (local.set $at (i32.add (i32.const 2048) (i32.mul (local.get $i) (i32.const 64))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (local.set $id (f32.load (local.get $at)))
        (local.set $player (i32.trunc_f32_s (local.get $id)))
        (local.set $held (call $state (local.get $player) (i32.const 0)))
        (local.set $on (call $state (local.get $player) (i32.const 2)))
        (if (f32.ne (local.get $on) (local.get $on))
          (then (local.set $on (f32.const 0))))
        ;; Where their gun is drawn, and its muzzle.
        (local.set $drawn (call $held (local.get $player) (i32.const 0) (i32.const 1280)))

        ;; The gun itself, for anyone holding it: the alien skin, over the
        ;; game's own Printer, its veins flaring while the beam is on.
        (if (i32.and (local.get $drawn)
              (f32.eq (f32.load offset=60 (local.get $at)) (f32.convert_i32_s (global.get $gun))))
          (then
            (if (i32.lt_s (global.get $gun_mesh) (i32.const 0))
              (then (global.set $gun_mesh (call $image_mesh (global.get $gun)))))
            (if (i32.ge_s (global.get $gun_mesh) (i32.const 0))
              (then
                (call $param (i32.const 0) (f32.const 0.3) (f32.const 0.95) (f32.const 1)
                  (f32.min (f32.const 1) (f32.max (f32.const 0) (local.get $on))))
                (call $param (i32.const 1) (f32.load (i32.const 1152)) (f32.load (i32.const 1156))
                  (f32.load (i32.const 1160)) (local.get $id))
                (call $param (i32.const 2) (f32.load (i32.const 1164)) (f32.load (i32.const 1168))
                  (f32.load (i32.const 1172)) (f32.const 0))
                (call $param (i32.const 3) (f32.load (i32.const 1176)) (f32.load (i32.const 1180))
                  (f32.load (i32.const 1184)) (f32.const 0))
                (call $draw_with (global.get $gun_mesh) (global.get $m_skin)
                  (i32.const 1280) (i32.const 1088))))))

        ;; No gun state: not a gravity gun player (or not yet known).
        (br_if $each_player (f32.ne (local.get $held) (local.get $held)))
        (local.set $held_id (call $state (local.get $player) (i32.const 1)))
        (local.set $shots (call $state (local.get $player) (i32.const 3)))
        (local.set $shot_kind (call $state (local.get $player) (i32.const 4)))
        (local.set $shot_id (call $state (local.get $player) (i32.const 5)))
        (local.set $reach (call $state (local.get $player) (i32.const 6)))
        (if (f32.ne (local.get $reach) (local.get $reach))
          (then (local.set $reach (f32.const 10))))
        (local.set $slot (call $slot (local.get $id)))
        (f32.store offset=56 (local.get $slot) (f32.const 1))
        (local.set $ex (f32.load offset=20 (local.get $at)))
        (local.set $ey (f32.load offset=24 (local.get $at)))
        (local.set $ez (f32.load offset=28 (local.get $at)))
        (local.set $lx (f32.load offset=32 (local.get $at)))
        (local.set $ly (f32.load offset=36 (local.get $at)))
        (local.set $lz (f32.load offset=40 (local.get $at)))
        ;; The muzzle, where the game draws it; without a drawn gun, ahead
        ;; of the eye, to the right and a little down, where it would be.
        (if (local.get $drawn)
          (then
            (local.set $mx (f32.load (i32.const 1344)))
            (local.set $my (f32.load (i32.const 1348)))
            (local.set $mz (f32.load (i32.const 1352))))
          (else
            (local.set $rl (f32.sqrt (f32.add (f32.mul (local.get $lx) (local.get $lx))
                                              (f32.mul (local.get $lz) (local.get $lz)))))
            (if (f32.lt (local.get $rl) (f32.const 0.01))
              (then (local.set $rx (f32.const 1)) (local.set $rz (f32.const 0)))
              (else
                (local.set $rx (f32.div (f32.neg (local.get $lz)) (local.get $rl)))
                (local.set $rz (f32.div (local.get $lx) (local.get $rl)))))
            (local.set $mx (f32.add (local.get $ex)
              (f32.add (f32.mul (local.get $lx) (f32.const 0.9)) (f32.mul (local.get $rx) (f32.const 0.35)))))
            (local.set $my (f32.add (local.get $ey)
              (f32.sub (f32.mul (local.get $ly) (f32.const 0.9)) (f32.const 0.3))))
            (local.set $mz (f32.add (local.get $ez)
              (f32.add (f32.mul (local.get $lz) (f32.const 0.9)) (f32.mul (local.get $rz) (f32.const 0.35)))))))

        ;; A new throw: remember where it went and which way. The count
        ;; seen first is only noted, so joining never replays one.
        (if (f32.eq (f32.load offset=12 (local.get $slot)) (f32.const 0))
          (then
            (f32.store offset=8 (local.get $slot) (local.get $shots))
            (f32.store offset=72 (local.get $slot) (local.get $held))
            (f32.store offset=76 (local.get $slot) (local.get $held_id))
            (f32.store offset=12 (local.get $slot) (f32.const 1))))
        (local.set $shot (i32.const 0))
        (if (f32.ne (local.get $shots) (f32.load offset=8 (local.get $slot)))
          (then
            (if (call $locate (local.get $shot_kind) (local.get $shot_id))
              (then
                (f32.store offset=24 (local.get $slot) (local.get $t))
                (f32.store offset=28 (local.get $slot) (global.get $ox))
                (f32.store offset=32 (local.get $slot) (global.get $oy))
                (f32.store offset=36 (local.get $slot) (global.get $oz))
                (f32.store offset=40 (local.get $slot) (local.get $lx))
                (f32.store offset=44 (local.get $slot) (local.get $ly))
                (f32.store offset=48 (local.get $slot) (local.get $lz))
                (f32.store offset=52 (local.get $slot) (global.get $or))
                (f32.store offset=60 (local.get $slot) (local.get $mx))
                (f32.store offset=64 (local.get $slot) (local.get $my))
                (f32.store offset=68 (local.get $slot) (local.get $mz))
                (call $sound (i32.const 288) (i32.const 24) (f32.const 1)
                  (global.get $ox) (global.get $oy) (global.get $oz))))
            (local.set $shot (i32.const 1))
            (f32.store offset=8 (local.get $slot) (local.get $shots))))
        ;; Let go without a throw: the hum falls away.
        (if (i32.and
              (i32.and (f32.gt (f32.load offset=72 (local.get $slot)) (f32.const 0.5))
                       (f32.lt (local.get $held) (f32.const 0.5)))
              (i32.eqz (local.get $shot)))
          (then (call $sound (i32.const 224) (i32.const 22) (f32.const 0.7)
            (local.get $mx) (local.get $my) (local.get $mz))))

        (if (i32.and
              (f32.gt (local.get $held) (f32.const 0.5))
              (call $locate (local.get $held) (local.get $held_id)))
          (then
            ;; Caught: note the spot the beam took, in the thing's own
            ;; frame (where the player aimed, as far off as the server
            ;; holds it), and the beam takes hold with a rising hum.
            (if (i32.or
                  (f32.ne (f32.load offset=72 (local.get $slot)) (local.get $held))
                  (f32.ne (f32.load offset=76 (local.get $slot)) (local.get $held_id)))
              (then
                (call $rotate
                  (f32.sub (f32.add (local.get $ex) (f32.mul (local.get $lx) (local.get $reach))) (global.get $ox))
                  (f32.sub (f32.add (local.get $ey) (f32.mul (local.get $ly) (local.get $reach))) (global.get $oy))
                  (f32.sub (f32.add (local.get $ez) (f32.mul (local.get $lz) (local.get $reach))) (global.get $oz))
                  (f32.const -1))
                ;; Never outside the thing itself.
                (local.set $span (f32.sqrt (f32.add (f32.add
                  (f32.mul (global.get $rx) (global.get $rx)) (f32.mul (global.get $ry) (global.get $ry)))
                  (f32.mul (global.get $rz) (global.get $rz)))))
                (if (f32.gt (local.get $span) (global.get $or))
                  (then
                    (local.set $span (f32.div (global.get $or) (local.get $span)))
                    (global.set $rx (f32.mul (global.get $rx) (local.get $span)))
                    (global.set $ry (f32.mul (global.get $ry) (local.get $span)))
                    (global.set $rz (f32.mul (global.get $rz) (local.get $span)))))
                ;; Players and creatures are gripped by the middle.
                (if (f32.ne (local.get $held) (f32.const 1))
                  (then
                    (global.set $rx (f32.const 0))
                    (global.set $ry (f32.const 0))
                    (global.set $rz (f32.const 0))))
                (f32.store offset=80 (local.get $slot) (global.get $rx))
                (f32.store offset=84 (local.get $slot) (global.get $ry))
                (f32.store offset=88 (local.get $slot) (global.get $rz))
                ;; A dead player: the ragdoll limb the beam meets, if the
                ;; Ragdoll Add-On made one.
                (i32.store offset=96 (local.get $slot) (i32.const 0))
                (f32.store offset=124 (local.get $slot) (f32.const 0))
                (if (f32.eq (local.get $held) (f32.const 2))
                  (then
                    (i32.store offset=96 (local.get $slot)
                      (call $rigid_find (local.get $ex) (local.get $ey) (local.get $ez)
                        (local.get $lx) (local.get $ly) (local.get $lz)
                        (f32.add (local.get $reach) (f32.const 2)) (i32.const 1408)))
                    (f32.store offset=100 (local.get $slot) (f32.load (i32.const 1424)))
                    (f32.store offset=104 (local.get $slot) (f32.load (i32.const 1428)))
                    (f32.store offset=108 (local.get $slot) (f32.load (i32.const 1432)))))
                (call $sound (i32.const 192) (i32.const 22) (f32.const 0.9)
                  (global.get $ox) (global.get $oy) (global.get $oz))))
            ;; The grip as the thing is drawn now.
            (call $rotate (f32.load offset=80 (local.get $slot)) (f32.load offset=84 (local.get $slot))
              (f32.load offset=88 (local.get $slot)) (f32.const 1))
            (local.set $gx (f32.add (global.get $ox) (global.get $rx)))
            (local.set $gy (f32.add (global.get $oy) (global.get $ry)))
            (local.set $gz (f32.add (global.get $oz) (global.get $rz)))
            ;; A ragdoll limb gripped: pull it to the beam's end, moving as
            ;; the aim moves, and grip it where it is drawn.
            (local.set $body (i32.load offset=96 (local.get $slot)))
            (if (i32.and (f32.eq (local.get $held) (f32.const 2)) (i32.gt_s (local.get $body) (i32.const 0)))
              (then
                (local.set $tx (f32.add (local.get $ex) (f32.mul (local.get $lx) (local.get $reach))))
                (local.set $ty (f32.add (local.get $ey) (f32.mul (local.get $ly) (local.get $reach))))
                (local.set $tz (f32.add (local.get $ez) (f32.mul (local.get $lz) (local.get $reach))))
                (local.set $vx (f32.const 0))
                (local.set $vy (f32.const 0))
                (local.set $vz (f32.const 0))
                (if (i32.and (f32.eq (f32.load offset=124 (local.get $slot)) (f32.const 1))
                             (f32.gt (local.get $dt) (f32.const 0)))
                  (then
                    (local.set $vx (f32.div (f32.sub (local.get $tx) (f32.load offset=112 (local.get $slot))) (local.get $dt)))
                    (local.set $vy (f32.div (f32.sub (local.get $ty) (f32.load offset=116 (local.get $slot))) (local.get $dt)))
                    (local.set $vz (f32.div (f32.sub (local.get $tz) (f32.load offset=120 (local.get $slot))) (local.get $dt)))))
                (f32.store offset=112 (local.get $slot) (local.get $tx))
                (f32.store offset=116 (local.get $slot) (local.get $ty))
                (f32.store offset=120 (local.get $slot) (local.get $tz))
                (f32.store offset=124 (local.get $slot) (f32.const 1))
                (call $rigid_hold (local.get $body)
                  (f32.load offset=100 (local.get $slot)) (f32.load offset=104 (local.get $slot))
                  (f32.load offset=108 (local.get $slot))
                  (local.get $tx) (local.get $ty) (local.get $tz)
                  (local.get $vx) (local.get $vy) (local.get $vz) (f32.const 1200))
                (if (call $rigid_get (local.get $body) (i32.const 1440))
                  (then
                    (global.set $qx (f32.load (i32.const 1452)))
                    (global.set $qy (f32.load (i32.const 1456)))
                    (global.set $qz (f32.load (i32.const 1460)))
                    (global.set $qw (f32.load (i32.const 1464)))
                    (call $rotate (f32.load offset=100 (local.get $slot)) (f32.load offset=104 (local.get $slot))
                      (f32.load offset=108 (local.get $slot)) (f32.const 1))
                    (local.set $gx (f32.add (f32.load (i32.const 1440)) (global.get $rx)))
                    (local.set $gy (f32.add (f32.load (i32.const 1444)) (global.get $ry)))
                    (local.set $gz (f32.add (f32.load (i32.const 1448)) (global.get $rz)))))))
            ;; The beam leaves the muzzle along the aim and bends into the
            ;; grip: straight while the thing keeps up, a whip when it lags.
            (local.set $span (f32.mul (f32.const 0.5) (f32.sqrt (f32.add (f32.add
              (f32.mul (f32.sub (local.get $gx) (local.get $mx)) (f32.sub (local.get $gx) (local.get $mx)))
              (f32.mul (f32.sub (local.get $gy) (local.get $my)) (f32.sub (local.get $gy) (local.get $my))))
              (f32.mul (f32.sub (local.get $gz) (local.get $mz)) (f32.sub (local.get $gz) (local.get $mz)))))))
            (call $beam (local.get $mx) (local.get $my) (local.get $mz)
              (f32.add (local.get $mx) (f32.mul (local.get $lx) (local.get $span)))
              (f32.add (local.get $my) (f32.mul (local.get $ly) (local.get $span)))
              (f32.add (local.get $mz) (f32.mul (local.get $lz) (local.get $span)))
              (local.get $gx) (local.get $gy) (local.get $gz)
              (local.get $id) (f32.const 1))
            ;; A glow where it grips and at the muzzle.
            (call $orb (local.get $gx) (local.get $gy) (local.get $gz) (f32.const 0.3) (f32.const 0.9))
            (call $orb (local.get $mx) (local.get $my) (local.get $mz) (f32.const 0.1) (f32.const 0.9))
            ;; A faint bubble round the held thing, brightest at the grip.
            (call $param (i32.const 0) (global.get $ox) (global.get $oy) (global.get $oz) (global.get $or))
            (call $param (i32.const 1) (local.get $gx) (local.get $gy) (local.get $gz) (f32.const 0))
            (call $param (i32.const 2) (local.get $id) (f32.const 0.45) (f32.const 0) (f32.const 0))
            (call $colour (f32.const 1))
            (call $draw (global.get $sphere) (global.get $m_field))
            ;; Sparks circling it.
            (call $param (i32.const 0) (global.get $ox) (global.get $oy) (global.get $oz)
              (f32.mul (global.get $or) (f32.const 1.05)))
            (call $param (i32.const 1) (f32.const 0) (f32.const 0) (f32.const 0.3) (f32.const 0.07))
            (call $param (i32.const 2) (f32.const 0) (f32.const 1) (f32.const 0) (f32.const 0))
            (call $colour (f32.const 0.8))
            (call $draw (global.get $sparks) (global.get $m_spark)))
          (else
            ;; The trigger held with nothing caught: a thinner beam out to
            ;; where it points.
            (if (f32.gt (local.get $on) (f32.const 0.5))
              (then
                (local.set $gx (f32.add (local.get $ex) (f32.mul (local.get $lx) (local.get $reach))))
                (local.set $gy (f32.add (local.get $ey) (f32.mul (local.get $ly) (local.get $reach))))
                (local.set $gz (f32.add (local.get $ez) (f32.mul (local.get $lz) (local.get $reach))))
                (call $beam (local.get $mx) (local.get $my) (local.get $mz)
                  (f32.mul (f32.const 0.5) (f32.add (local.get $mx) (local.get $gx)))
                  (f32.mul (f32.const 0.5) (f32.add (local.get $my) (local.get $gy)))
                  (f32.mul (f32.const 0.5) (f32.add (local.get $mz) (local.get $gz)))
                  (local.get $gx) (local.get $gy) (local.get $gz)
                  (local.get $id) (f32.const 0.55))
                (call $orb (local.get $mx) (local.get $my) (local.get $mz) (f32.const 0.08) (f32.const 0.7))))))

        ;; A throw, for just over half a second after it.
        (local.set $age (f32.sub (local.get $t) (f32.load offset=24 (local.get $slot))))
        (if (i32.and (f32.ge (local.get $age) (f32.const 0)) (f32.lt (local.get $age) (f32.const 0.6)))
          (then
            ;; The shockwave where it went...
            (call $param (i32.const 0)
              (f32.load offset=28 (local.get $slot)) (f32.load offset=32 (local.get $slot))
              (f32.load offset=36 (local.get $slot))
              (f32.add (f32.const 2.5) (f32.mul (f32.load offset=52 (local.get $slot)) (f32.const 1.5))))
            (call $param (i32.const 1)
              (f32.load offset=40 (local.get $slot)) (f32.load offset=44 (local.get $slot))
              (f32.load offset=48 (local.get $slot)) (f32.div (local.get $age) (f32.const 0.6)))
            (call $colour (f32.const 1))
            (call $draw (global.get $band) (global.get $m_ring))
            ;; ...a smaller one at the gun...
            (call $param (i32.const 0)
              (f32.load offset=60 (local.get $slot)) (f32.load offset=64 (local.get $slot))
              (f32.load offset=68 (local.get $slot)) (f32.const 1.1))
            (call $draw (global.get $band) (global.get $m_ring))
            ;; ...and sparks flying on with it.
            (call $param (i32.const 0)
              (f32.load offset=28 (local.get $slot)) (f32.load offset=32 (local.get $slot))
              (f32.load offset=36 (local.get $slot)) (f32.load offset=52 (local.get $slot)))
            (call $param (i32.const 1) (f32.const 2) (local.get $age) (f32.const 1) (f32.const 0.1))
            (call $param (i32.const 2)
              (f32.load offset=40 (local.get $slot)) (f32.load offset=44 (local.get $slot))
              (f32.load offset=48 (local.get $slot)) (f32.const 16))
            (call $colour (f32.const 1))
            (call $draw (global.get $sparks) (global.get $m_spark))))
        (f32.store offset=72 (local.get $slot) (local.get $held))
        (f32.store offset=76 (local.get $slot) (local.get $held_id))
        (br $each_player)))
    ;; Free the slots of players no longer here.
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 64)))
        (local.set $at (i32.add (i32.const 24576) (i32.mul (local.get $i) (i32.const 128))))
        (if (f32.eq (f32.load offset=56 (local.get $at)) (f32.const 0))
          (then (f32.store offset=4 (local.get $at) (f32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each))))
)
