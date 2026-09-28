;; Gravity Gun effects, drawn on every player's screen for everyone's gun:
;;
;; - holding: a crackling beam (beam.wgsl, a soft glow and a hot core)
;;   from the gun to the held object, a force bubble round it
;;   (field.wgsl) and sparks orbiting it (spark.wgsl);
;; - charging a throw: a swelling orb at the muzzle with sparks drawn in;
;; - a throw or punt: a shockwave ring (ring.wgsl) and a spray of sparks
;;   where it struck, and a smaller ring at the muzzle;
;; - sounds for each, placed where they happen: grab, drop, charge, launch
;;   and punt (made by tools/make_showcase_sounds.py).
;;
;; Everything comes from what the game already knows (`world.read`):
;; where players and vehicles are drawn, and each player's `beam` from the
;; Gravity Gun Add-On's public state: [held kind, held id, charging, shots
;; fired, last shot's kind, last shot's id]; kinds 1 vehicle, 2 player.
;; The server sends that only when it changes; the motion itself comes
;; from the game's own pose updates.
;;
;; This is the source of main.wasm; `cargo test -p bri-client-sandbox
;; --test showcase` checks the two match (BRI_BLESS=1 rewrites it).
;;
;; Memory layout:
;;   0      strings
;;   1024   identity model matrix (16 f32)
;;   1088   draw parameters (16 f32)
;;   2048   player records, 16 f32 (64 bytes) each, up to 64
;;   8192   vehicle records, 16 f32 each, up to 256
;;   24576  per-player effect state, 96 bytes each, 64 slots:
;;            +0 id  +4 in use  +8 shots  +12 shots known  +16 charging
;;            +20 charge start  +24 last shot time  +28 shot centre xyz
;;            +40 shot direction xyz  +52 shot radius  +56 seen this frame
;;            +60 muzzle at the shot xyz  +72 what was held last frame
;;   32768  mesh vertices being built (32 bytes each)
;;   65536  mesh indices being built
(module
  (import "bri" "log" (func $log (param i32 i32)))
  (import "bri" "random" (func $random (result i32)))
  (import "bri" "shader" (func $shader (param i32 i32) (result i32)))
  (import "bri" "mesh_create" (func $mesh_create (param i32 i32 i32 i32) (result i32)))
  (import "bri" "material_create" (func $material_create (param i32) (result i32)))
  (import "bri" "material_blend" (func $material_blend (param i32 i32)))
  (import "bri" "draw_with" (func $draw_with (param i32 i32 i32 i32)))
  (import "bri" "players" (func $players (param i32 i32) (result i32)))
  (import "bri" "vehicles" (func $vehicles (param i32 i32) (result i32)))
  (import "bri" "state_num" (func $state_num (param i32 i32 i32 i32 i32 i32) (result f32)))
  (import "bri" "sound_at" (func $sound_at (param i32 i32 f32 f32 f32 f32) (result i32)))
  (memory (export "memory") 2)

  (global $tube (mut i32) (i32.const 0))
  (global $sphere (mut i32) (i32.const 0))
  (global $band (mut i32) (i32.const 0))
  (global $sparks (mut i32) (i32.const 0))
  (global $m_beam (mut i32) (i32.const 0))
  (global $m_field (mut i32) (i32.const 0))
  (global $m_spark (mut i32) (i32.const 0))
  (global $m_ring (mut i32) (i32.const 0))
  (global $players_n (mut i32) (i32.const 0))
  (global $vehicles_n (mut i32) (i32.const 0))
  ;; What $locate found: centre and radius.
  (global $ox (mut f32) (f32.const 0))
  (global $oy (mut f32) (f32.const 0))
  (global $oz (mut f32) (f32.const 0))
  (global $or (mut f32) (f32.const 0))

  (data (i32.const 0) "client/beam.wgsl")
  (data (i32.const 32) "client/field.wgsl")
  (data (i32.const 64) "client/spark.wgsl")
  (data (i32.const 96) "client/ring.wgsl")
  (data (i32.const 128) "gravity-gun")
  (data (i32.const 144) "beam")
  (data (i32.const 160) "gravity gun effects ready")
  (data (i32.const 192) "client/sounds/grab.wav")
  (data (i32.const 224) "client/sounds/drop.wav")
  (data (i32.const 256) "client/sounds/charge.wav")
  (data (i32.const 288) "client/sounds/launch.wav")
  (data (i32.const 320) "client/sounds/punt.wav")

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
    (global.set $tube (call $grid (i32.const 2) (i32.const 48)))
    (global.set $sphere (call $grid (i32.const 36) (i32.const 18)))
    (global.set $band (call $grid (i32.const 48) (i32.const 1)))
    (global.set $sparks (call $particles (i32.const 96)))
    (global.set $m_beam (call $material (i32.const 0) (i32.const 16)))
    (global.set $m_field (call $material (i32.const 32) (i32.const 17)))
    (global.set $m_spark (call $material (i32.const 64) (i32.const 17)))
    (global.set $m_ring (call $material (i32.const 96) (i32.const 16)))
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

  ;; The gravity gun's orange.
  (func $orange (param $alpha f32)
    (call $param (i32.const 3) (f32.const 1) (f32.const 0.5) (f32.const 0.1) (local.get $alpha)))

  ;; ---- Finding things ----

  ;; Where the object of `kind` (1 vehicle, 2 player) and `id` is drawn:
  ;; sets $ox $oy $oz (its middle) and $or (its size). 0 when not found.
  (func $locate (param $kind f32) (param $id f32) (result i32)
    (local $i i32) (local $at i32)
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
    (i32.const 0))

  ;; The effect state of the player `id`: theirs, else a free slot.
  (func $slot (param $id f32) (result i32)
    (local $i i32) (local $at i32) (local $free i32)
    (local.set $free (i32.const -1))
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 64)))
        (local.set $at (i32.add (i32.const 24576) (i32.mul (local.get $i) (i32.const 96))))
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
    (memory.fill (local.get $free) (i32.const 0) (i32.const 96))
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

  (func $clamp01 (param $x f32) (result f32)
    (f32.min (f32.const 1) (f32.max (f32.const 0) (local.get $x))))

  ;; ---- Each frame ----

  (func (export "frame") (param $t f32) (param $dt f32)
    (local $i i32) (local $at i32) (local $slot i32) (local $player i32)
    (local $id f32) (local $held f32) (local $held_id f32) (local $charging f32)
    (local $shots f32) (local $shot_kind f32) (local $shot_id f32)
    (local $ex f32) (local $ey f32) (local $ez f32)
    (local $lx f32) (local $ly f32) (local $lz f32)
    (local $rx f32) (local $rz f32) (local $rl f32)
    (local $mx f32) (local $my f32) (local $mz f32)
    (local $charge f32) (local $age f32) (local $shot i32)
    (global.set $players_n (call $players (i32.const 2048) (i32.const 64)))
    (global.set $vehicles_n (call $vehicles (i32.const 8192) (i32.const 256)))
    ;; Slots not seen this frame belong to players who left.
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 64)))
        (f32.store offset=56 (i32.add (i32.const 24576) (i32.mul (local.get $i) (i32.const 96)))
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
        ;; No gun state: not a gravity gun player (or not yet known).
        (br_if $each_player (f32.ne (local.get $held) (local.get $held)))
        (local.set $held_id (call $state (local.get $player) (i32.const 1)))
        (local.set $charging (call $state (local.get $player) (i32.const 2)))
        (local.set $shots (call $state (local.get $player) (i32.const 3)))
        (local.set $shot_kind (call $state (local.get $player) (i32.const 4)))
        (local.set $shot_id (call $state (local.get $player) (i32.const 5)))
        (local.set $slot (call $slot (local.get $id)))
        (f32.store offset=56 (local.get $slot) (f32.const 1))
        ;; The muzzle: ahead of the eye, to the right and a little down,
        ;; where the gun sits in the player's hand.
        (local.set $ex (f32.load offset=20 (local.get $at)))
        (local.set $ey (f32.load offset=24 (local.get $at)))
        (local.set $ez (f32.load offset=28 (local.get $at)))
        (local.set $lx (f32.load offset=32 (local.get $at)))
        (local.set $ly (f32.load offset=36 (local.get $at)))
        (local.set $lz (f32.load offset=40 (local.get $at)))
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
          (f32.add (f32.mul (local.get $lz) (f32.const 0.9)) (f32.mul (local.get $rz) (f32.const 0.35)))))

        ;; Charging: remember when it began, and whine as it builds.
        (if (f32.gt (local.get $charging) (f32.const 0.5))
          (then
            (if (f32.eq (f32.load offset=16 (local.get $slot)) (f32.const 0))
              (then
                (f32.store offset=20 (local.get $slot) (local.get $t))
                (call $sound (i32.const 256) (i32.const 24) (f32.const 0.8)
                  (local.get $mx) (local.get $my) (local.get $mz))))
            (f32.store offset=16 (local.get $slot) (f32.const 1)))
          (else (f32.store offset=16 (local.get $slot) (f32.const 0))))

        ;; A new shot: remember where it struck and which way it went. The
        ;; count seen first is only noted, so joining never replays one.
        (if (f32.eq (f32.load offset=12 (local.get $slot)) (f32.const 0))
          (then
            (f32.store offset=8 (local.get $slot) (local.get $shots))
            (f32.store offset=72 (local.get $slot) (local.get $held))
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
                ;; A throw of what was held booms; a punt knocks.
                (if (f32.gt (f32.load offset=72 (local.get $slot)) (f32.const 0.5))
                  (then (call $sound (i32.const 288) (i32.const 24) (f32.const 1)
                    (global.get $ox) (global.get $oy) (global.get $oz)))
                  (else (call $sound (i32.const 320) (i32.const 22) (f32.const 0.9)
                    (global.get $ox) (global.get $oy) (global.get $oz))))))
            (local.set $shot (i32.const 1))
            (f32.store offset=8 (local.get $slot) (local.get $shots))))
        ;; Let go without a throw: the hum falls away.
        (if (i32.and
              (i32.and (f32.gt (f32.load offset=72 (local.get $slot)) (f32.const 0.5))
                       (f32.lt (local.get $held) (f32.const 0.5)))
              (i32.eqz (local.get $shot)))
          (then (call $sound (i32.const 224) (i32.const 22) (f32.const 0.7)
            (local.get $mx) (local.get $my) (local.get $mz))))

        (local.set $charge (f32.const 0))
        (if (f32.gt (local.get $charging) (f32.const 0.5))
          (then
            (local.set $charge (call $clamp01
              (f32.div (f32.sub (local.get $t) (f32.load offset=20 (local.get $slot)))
                       (f32.const 0.75))))))

        ;; Holding something: beam, bubble and orbiting sparks.
        (if (i32.and
              (f32.gt (local.get $held) (f32.const 0.5))
              (call $locate (local.get $held) (local.get $held_id)))
          (then
            ;; Caught: the beam takes hold with a rising hum.
            (if (f32.lt (f32.load offset=72 (local.get $slot)) (f32.const 0.5))
              (then (call $sound (i32.const 192) (i32.const 22) (f32.const 0.9)
                (global.get $ox) (global.get $oy) (global.get $oz))))
            ;; The soft outer glow...
            (call $param (i32.const 0) (local.get $mx) (local.get $my) (local.get $mz) (f32.const 0.45))
            (call $param (i32.const 1) (global.get $ox) (global.get $oy) (global.get $oz) (local.get $charge))
            (call $param (i32.const 2) (local.get $id) (f32.const 0.7) (f32.const 0) (f32.const 0))
            (call $orange (f32.const 1))
            (call $draw (global.get $tube) (global.get $m_beam))
            ;; ...and the white-hot core.
            (call $param (i32.const 0) (local.get $mx) (local.get $my) (local.get $mz) (f32.const 0.085))
            (call $param (i32.const 2) (f32.add (local.get $id) (f32.const 2.7)) (f32.const 1)
              (f32.const 0) (f32.const 1))
            (call $draw (global.get $tube) (global.get $m_beam))
            ;; The bubble, gripped where the beam arrives.
            (call $param (i32.const 0) (global.get $ox) (global.get $oy) (global.get $oz) (global.get $or))
            (call $param (i32.const 1) (local.get $mx) (local.get $my) (local.get $mz) (local.get $charge))
            (call $param (i32.const 2) (local.get $id) (f32.const 0.9) (f32.const 0) (f32.const 0))
            (call $orange (f32.const 1))
            (call $draw (global.get $sphere) (global.get $m_field))
            ;; Sparks circling it.
            (call $param (i32.const 0) (global.get $ox) (global.get $oy) (global.get $oz)
              (f32.mul (global.get $or) (f32.const 1.05)))
            (call $param (i32.const 1) (f32.const 0) (f32.const 0) (f32.const 0.4) (f32.const 0.09))
            (call $param (i32.const 2) (f32.const 0) (f32.const 1) (f32.const 0) (f32.const 0))
            (call $orange (f32.const 0.9))
            (call $draw (global.get $sparks) (global.get $m_spark))))

        ;; Charging a throw: an orb swelling at the muzzle, sparks drawn in.
        (if (f32.gt (local.get $charging) (f32.const 0.5))
          (then
            (call $param (i32.const 0) (local.get $mx) (local.get $my) (local.get $mz) (f32.const 0))
            (call $param (i32.const 1) (f32.const 3) (f32.const 0) (f32.const 1)
              (f32.add (f32.const 0.1) (f32.mul (local.get $charge) (f32.const 0.28))))
            (call $orange (f32.add (f32.const 0.5) (f32.mul (local.get $charge) (f32.const 0.5))))
            (call $draw (global.get $sparks) (global.get $m_spark))
            (call $param (i32.const 0) (local.get $mx) (local.get $my) (local.get $mz) (f32.const 1.3))
            (call $param (i32.const 1) (f32.const 1) (local.get $charge) (f32.const 0.5) (f32.const 0.05))
            (call $orange (f32.const 1))
            (call $draw (global.get $sparks) (global.get $m_spark))))

        ;; A throw or punt, for just over half a second after it.
        (local.set $age (f32.sub (local.get $t) (f32.load offset=24 (local.get $slot))))
        (if (i32.and (f32.ge (local.get $age) (f32.const 0)) (f32.lt (local.get $age) (f32.const 0.6)))
          (then
            ;; The shockwave where it struck...
            (call $param (i32.const 0)
              (f32.load offset=28 (local.get $slot)) (f32.load offset=32 (local.get $slot))
              (f32.load offset=36 (local.get $slot))
              (f32.add (f32.const 2.5) (f32.mul (f32.load offset=52 (local.get $slot)) (f32.const 1.5))))
            (call $param (i32.const 1)
              (f32.load offset=40 (local.get $slot)) (f32.load offset=44 (local.get $slot))
              (f32.load offset=48 (local.get $slot)) (f32.div (local.get $age) (f32.const 0.6)))
            (call $orange (f32.const 1))
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
            (call $orange (f32.const 1))
            (call $draw (global.get $sparks) (global.get $m_spark))))
        (f32.store offset=72 (local.get $slot) (local.get $held))
        (br $each_player)))
    ;; Free the slots of players no longer here.
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 64)))
        (local.set $at (i32.add (i32.const 24576) (i32.mul (local.get $i) (i32.const 96))))
        (if (f32.eq (f32.load offset=56 (local.get $at)) (f32.const 0))
          (then (f32.store offset=4 (local.get $at) (f32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each))))
)
