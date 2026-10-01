;; Grappling Hook effects, drawn on every player's screen for everyone's
;; hook:
;;
;; - the launcher itself: the Printer it is built from, rebuilt as a
;;   riveted gunmetal winch gun with brass drum bands wound with cable
;;   (winch.wgsl), its amber gauge glowing while the grapnel is out;
;; - firing: the four-claw steel grapnel (grapnel.wgsl), claws half open,
;;   flies from the muzzle to where it will bite at the rule's speed,
;;   paying out its steel cable (cable.wgsl), with a crack as it fires
;;   and a clang as it bites;
;; - hooked: the claws spring open, the winch whirs, and the cable runs
;;   taut from the launcher (or from the hands, when something else is in
;;   them) to the grapnel, buzzing as it takes the load; its strands run
;;   along as the winch reels. A grapnel in a player or vehicle rides
;;   along with it;
;; - a miss: out as far as it goes and back in;
;; - letting go: the grapnel folds and zips back into the muzzle.
;;
;; Everything comes from what the game already knows (`world.read`): where
;; players and vehicles are drawn, where each player's launcher is drawn
;; and its muzzle (`held`), the launcher's own model (`image_mesh`), and
;; each player's `hook` from the Grappling Hook Add-On's public state:
;; [phase, x, y, z, distance, kind, id]; phases 0 none, 1 flying out to x,
;; 2 hooked at x, 3 missed; kind 0 a brick or the map, 1 player `id`, 2
;; vehicle `id`. The server sends that only when it changes; the pulling
;; itself comes from the game's own pose updates, so it costs no
;; bandwidth.
;;
;; This is the source of main.wasm; `cargo test -p bri-client-sandbox
;; --test showcase` checks the two match (BRI_BLESS=1 rewrites it).
;;
;; Memory layout:
;;   0      strings
;;   1024   identity model matrix (16 f32)
;;   1088   draw parameters (16 f32)
;;   1152   the environment record (12 f32): sun direction, sun colour,
;;          ambient, sky
;;   1280   the held record (20 f32): the launcher's model matrix, muzzle
;;   2048   player records, 16 f32 (64 bytes) each, up to 64
;;   6144   vehicle records, 16 f32 each, up to 32: id, kind, position xyz,
;;          rotation xyzw, velocity xyz, radius, padding
;;   8192   per-player effect state, 96 bytes each, 64 slots:
;;            +0 id  +4 in use  +8 seen this frame  +12 phase last frame
;;            +16 when that phase began  +28 when it bit  +32 how hard it
;;            buzzes  +36 the grapnel's crown last frame xyz  +48 whether
;;            this shot has bitten  +52 when it was let go  +56 how far the
;;            strands have run  +60 the cable's length last frame
;;            +64 where on what it bit xyz (in its own frame)  +76 whether
;;            that is known yet
;;   16384  mesh vertices being built (32 bytes each)
;;   65536  mesh indices being built
(module
  (import "bri" "log" (func $log (param i32 i32)))
  (import "bri" "shader" (func $shader (param i32 i32) (result i32)))
  (import "bri" "mesh_create" (func $mesh_create (param i32 i32 i32 i32) (result i32)))
  (import "bri" "material_create" (func $material_create (param i32) (result i32)))
  (import "bri" "draw_with" (func $draw_with (param i32 i32 i32 i32)))
  (import "bri" "environment" (func $environment (param i32)))
  (import "bri" "players" (func $players (param i32 i32) (result i32)))
  (import "bri" "vehicles" (func $vehicles (param i32 i32) (result i32)))
  (import "bri" "image_kind" (func $image_kind (param i32 i32) (result i32)))
  (import "bri" "image_mesh" (func $image_mesh (param i32) (result i32)))
  (import "bri" "held" (func $held (param i32 i32 i32) (result i32)))
  (import "bri" "state_num" (func $state_num (param i32 i32 i32 i32 i32 i32) (result f32)))
  (import "bri" "sound_at" (func $sound_at (param i32 i32 f32 f32 f32 f32) (result i32)))
  (memory (export "memory") 2)

  (global $tube (mut i32) (i32.const 0))
  (global $claw (mut i32) (i32.const 0))
  (global $m_cable (mut i32) (i32.const 0))
  (global $m_grapnel (mut i32) (i32.const 0))
  (global $m_skin (mut i32) (i32.const 0))
  ;; The launcher's image, as `players` records name it, and its model
  ;; once someone holds it (-1 before).
  (global $gun (mut i32) (i32.const 0))
  (global $gun_mesh (mut i32) (i32.const -1))
  (global $players_n (mut i32) (i32.const 0))
  (global $vehicles_n (mut i32) (i32.const 0))

  (data (i32.const 0) "client/cable.wgsl")
  (data (i32.const 32) "client/grapnel.wgsl")
  (data (i32.const 64) "client/winch.wgsl")
  (data (i32.const 96) "grappling-hook")
  (data (i32.const 112) "hook")
  (data (i32.const 128) "grappling hook effects ready")
  (data (i32.const 160) "client/sounds/fire.wav")
  (data (i32.const 192) "client/sounds/clamp.wav")
  (data (i32.const 224) "client/sounds/winch.wav")
  (data (i32.const 256) "client/sounds/release.wav")
  (data (i32.const 288) "grappling-hook-tool:image/grapplinghook")

  ;; How fast the grapnel flies, units a second: the rule's `speed()`.
  (global $speed f32 (f32.const 200))
  ;; The grapnel's size, and how far behind its crown the cable ties on.
  (global $size f32 (f32.const 1.0))
  (global $tie f32 (f32.const 0.6))
  (global $radius f32 (f32.const 0.028))
  ;; Where the hands are above the feet: the engine's rope grip.
  (global $grip f32 (f32.const 2.25))

  ;; ---- Meshes ----

  ;; A (cols x rows) grid at uv (0..1, 0..1), two triangles a cell; the
  ;; shaders bend it into a tube.
  (func $grid (param $cols i32) (param $rows i32) (result i32)
    (local $i i32) (local $j i32) (local $at i32) (local $a i32)
    (local.set $at (i32.const 16384))
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
      (i32.const 16384)
      (i32.mul (i32.add (local.get $cols) (i32.const 1)) (i32.add (local.get $rows) (i32.const 1)))
      (i32.const 65536)
      (i32.mul (i32.mul (local.get $cols) (local.get $rows)) (i32.const 6))))

  (func (export "init")
    ;; The cable: twelve round (its six strands show on the edge), ninety-
    ;; six along (smooth as it buzzes). The grapnel's parts are short.
    (global.set $tube (call $grid (i32.const 12) (i32.const 96)))
    (global.set $claw (call $grid (i32.const 8) (i32.const 16)))
    ;; Everything is solid: it hides behind the world and the world
    ;; behind it.
    (global.set $m_cable (call $material_create (call $shader (i32.const 0) (i32.const 17))))
    (global.set $m_grapnel (call $material_create (call $shader (i32.const 32) (i32.const 19))))
    (global.set $m_skin (call $material_create (call $shader (i32.const 64) (i32.const 17))))
    (global.set $gun (call $image_kind (i32.const 288) (i32.const 39)))
    (memory.fill (i32.const 8192) (i32.const 0) (i32.const 6144))
    (f32.store (i32.const 1024) (f32.const 1))
    (f32.store (i32.const 1044) (f32.const 1))
    (f32.store (i32.const 1064) (f32.const 1))
    (f32.store (i32.const 1084) (f32.const 1))
    (call $log (i32.const 128) (i32.const 28)))

  ;; ---- Drawing ----

  ;; Parameter `i` (0 to 3) of the next draw.
  (func $param (param $i i32) (param $x f32) (param $y f32) (param $z f32) (param $w f32)
    (local $at i32)
    (local.set $at (i32.add (i32.const 1088) (i32.mul (local.get $i) (i32.const 16))))
    (f32.store (local.get $at) (local.get $x))
    (f32.store offset=4 (local.get $at) (local.get $y))
    (f32.store offset=8 (local.get $at) (local.get $z))
    (f32.store offset=12 (local.get $at) (local.get $w)))

  ;; Parameters 2 and 3 as the cable and grapnel read them: the sunlight's
  ;; direction times its strength, the ambient light, and a number each.
  (func $light (param $w2 f32) (param $w3 f32)
    (local $strength f32)
    (local.set $strength
      (f32.mul (f32.const 0.4)
        (f32.add (f32.load (i32.const 1164))
          (f32.add (f32.load (i32.const 1168)) (f32.load (i32.const 1172))))))
    (call $param (i32.const 2)
      (f32.mul (f32.load (i32.const 1152)) (local.get $strength))
      (f32.mul (f32.load (i32.const 1156)) (local.get $strength))
      (f32.mul (f32.load (i32.const 1160)) (local.get $strength))
      (local.get $w2))
    (call $param (i32.const 3)
      (f32.load (i32.const 1176)) (f32.load (i32.const 1180)) (f32.load (i32.const 1184))
      (local.get $w3)))

  (func $draw (param $mesh i32) (param $material i32)
    (call $draw_with (local.get $mesh) (local.get $material) (i32.const 1024) (i32.const 1088)))

  ;; The cable from its start (x0) to where it ties onto the grapnel (x1).
  (func $cable (param $x0 f32) (param $y0 f32) (param $z0 f32)
               (param $x1 f32) (param $y1 f32) (param $z1 f32)
               (param $sag f32) (param $shiver f32) (param $run f32)
    (call $param (i32.const 0) (local.get $x0) (local.get $y0) (local.get $z0) (global.get $radius))
    (call $param (i32.const 1) (local.get $x1) (local.get $y1) (local.get $z1) (local.get $sag))
    (call $light (local.get $shiver) (local.get $run))
    (call $draw (global.get $tube) (global.get $m_cable)))

  ;; The grapnel: its crown at c, pointing along u (a unit vector), its
  ;; claws turned `spin` round it and `open` (0 folded to 1 open). The
  ;; shank, then four claws.
  (func $grapnel (param $cx f32) (param $cy f32) (param $cz f32)
                 (param $ux f32) (param $uy f32) (param $uz f32)
                 (param $spin f32) (param $open f32)
    (local $part i32)
    (local.set $part (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $part) (i32.const 5)))
        (call $param (i32.const 0) (local.get $cx) (local.get $cy) (local.get $cz) (global.get $size))
        (call $param (i32.const 1) (local.get $ux) (local.get $uy) (local.get $uz)
          (f32.convert_i32_s (local.get $part)))
        (call $light (local.get $spin) (local.get $open))
        (call $draw (global.get $claw) (global.get $m_grapnel))
        (local.set $part (i32.add (local.get $part) (i32.const 1)))
        (br $each))))

  ;; ---- State ----

  ;; The memory of player `id`: its slot, else a fresh one (the first
  ;; when all are taken).
  (func $slot (param $id f32) (result i32)
    (local $i i32) (local $at i32) (local $free i32)
    (local.set $free (i32.const -1))
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 64)))
        (local.set $at (i32.add (i32.const 8192) (i32.mul (local.get $i) (i32.const 96))))
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
      (then (local.set $free (i32.const 8192))))
    (memory.fill (local.get $free) (i32.const 0) (i32.const 96))
    (f32.store (local.get $free) (local.get $id))
    (f32.store offset=4 (local.get $free) (f32.const 1))
    (f32.store offset=28 (local.get $free) (f32.const -100))
    (f32.store offset=52 (local.get $free) (f32.const -100))
    (local.get $free))

  ;; A number of player `player`'s `hook`; NaN when there is none.
  (func $state (param $player i32) (param $index i32) (result f32)
    (call $state_num (i32.const 96) (i32.const 14) (i32.const 112) (i32.const 4)
      (local.get $player) (local.get $index)))

  (func $sound (param $name i32) (param $len i32) (param $volume f32)
               (param $x f32) (param $y f32) (param $z f32)
    (drop (call $sound_at (local.get $name) (local.get $len) (local.get $volume)
      (local.get $x) (local.get $y) (local.get $z))))

  (func $length (param $x f32) (param $y f32) (param $z f32) (result f32)
    (f32.sqrt (f32.add (f32.mul (local.get $x) (local.get $x))
      (f32.add (f32.mul (local.get $y) (local.get $y)) (f32.mul (local.get $z) (local.get $z))))))

  (func $clamp01 (param $x f32) (result f32)
    (f32.min (f32.const 1) (f32.max (f32.const 0) (local.get $x))))

  ;; A triangle wave from -1 to 1, `x` in cycles.
  (func $wave (param $x f32) (result f32)
    (f32.sub
      (f32.mul (f32.const 4)
        (f32.abs (f32.sub (local.get $x) (f32.add (f32.floor (local.get $x)) (f32.const 0.5)))))
      (f32.const 1)))

  ;; The record of player `id` this frame, or 0.
  (func $player_record (param $id f32) (result i32)
    (local $i i32) (local $at i32)
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (global.get $players_n)))
        (local.set $at (i32.add (i32.const 2048) (i32.mul (local.get $i) (i32.const 64))))
        (if (f32.eq (f32.load (local.get $at)) (local.get $id))
          (then (return (local.get $at))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (i32.const 0))

  ;; The record of vehicle `id` this frame, or 0.
  (func $vehicle_record (param $id f32) (result i32)
    (local $i i32) (local $at i32)
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (global.get $vehicles_n)))
        (local.set $at (i32.add (i32.const 6144) (i32.mul (local.get $i) (i32.const 64))))
        (if (f32.eq (f32.load (local.get $at)) (local.get $id))
          (then (return (local.get $at))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (i32.const 0))

  ;; Turn (x, y, z) by the unit quaternion at `q` (x y z w), or by its
  ;; inverse when `back`; the result goes to 1360..1372.
  (func $rotate (param $q i32) (param $back i32) (param $x f32) (param $y f32) (param $z f32)
    (local $qx f32) (local $qy f32) (local $qz f32) (local $qw f32)
    (local $tx f32) (local $ty f32) (local $tz f32)
    (local.set $qx (f32.load (local.get $q)))
    (local.set $qy (f32.load offset=4 (local.get $q)))
    (local.set $qz (f32.load offset=8 (local.get $q)))
    (local.set $qw (f32.load offset=12 (local.get $q)))
    (if (local.get $back)
      (then
        (local.set $qx (f32.neg (local.get $qx)))
        (local.set $qy (f32.neg (local.get $qy)))
        (local.set $qz (f32.neg (local.get $qz)))))
    ;; t = 2 (q x v); v' = v + w t + q x t
    (local.set $tx (f32.mul (f32.const 2)
      (f32.sub (f32.mul (local.get $qy) (local.get $z)) (f32.mul (local.get $qz) (local.get $y)))))
    (local.set $ty (f32.mul (f32.const 2)
      (f32.sub (f32.mul (local.get $qz) (local.get $x)) (f32.mul (local.get $qx) (local.get $z)))))
    (local.set $tz (f32.mul (f32.const 2)
      (f32.sub (f32.mul (local.get $qx) (local.get $y)) (f32.mul (local.get $qy) (local.get $x)))))
    (f32.store (i32.const 1360)
      (f32.add (local.get $x)
        (f32.add (f32.mul (local.get $qw) (local.get $tx))
          (f32.sub (f32.mul (local.get $qy) (local.get $tz)) (f32.mul (local.get $qz) (local.get $ty))))))
    (f32.store (i32.const 1364)
      (f32.add (local.get $y)
        (f32.add (f32.mul (local.get $qw) (local.get $ty))
          (f32.sub (f32.mul (local.get $qz) (local.get $tx)) (f32.mul (local.get $qx) (local.get $tz))))))
    (f32.store (i32.const 1368)
      (f32.add (local.get $z)
        (f32.add (f32.mul (local.get $qw) (local.get $tz))
          (f32.sub (f32.mul (local.get $qx) (local.get $ty)) (f32.mul (local.get $qy) (local.get $tx)))))))

  ;; Where the grapnel of the player in `slot` is held now: the point it
  ;; bit (x), or, in a player or vehicle, the same spot on it wherever it
  ;; has gone. The spot is learned the first time it is seen. The result
  ;; goes to 1360..1372.
  (func $anchor (param $slot i32) (param $kind f32) (param $id f32)
                (param $x f32) (param $y f32) (param $z f32)
    (local $at i32)
    (f32.store (i32.const 1360) (local.get $x))
    (f32.store (i32.const 1364) (local.get $y))
    (f32.store (i32.const 1368) (local.get $z))
    (if (f32.eq (local.get $kind) (f32.const 1))
      (then
        (local.set $at (call $player_record (local.get $id)))
        (if (local.get $at)
          (then
            (if (f32.eq (f32.load offset=76 (local.get $slot)) (f32.const 0))
              (then
                (f32.store offset=64 (local.get $slot) (f32.sub (local.get $x) (f32.load offset=8 (local.get $at))))
                (f32.store offset=68 (local.get $slot) (f32.sub (local.get $y) (f32.load offset=12 (local.get $at))))
                (f32.store offset=72 (local.get $slot) (f32.sub (local.get $z) (f32.load offset=16 (local.get $at))))
                (f32.store offset=76 (local.get $slot) (f32.const 1))))
            (f32.store (i32.const 1360)
              (f32.add (f32.load offset=8 (local.get $at)) (f32.load offset=64 (local.get $slot))))
            (f32.store (i32.const 1364)
              (f32.add (f32.load offset=12 (local.get $at)) (f32.load offset=68 (local.get $slot))))
            (f32.store (i32.const 1368)
              (f32.add (f32.load offset=16 (local.get $at)) (f32.load offset=72 (local.get $slot))))))))
    (if (f32.eq (local.get $kind) (f32.const 2))
      (then
        (local.set $at (call $vehicle_record (local.get $id)))
        (if (local.get $at)
          (then
            (if (f32.eq (f32.load offset=76 (local.get $slot)) (f32.const 0))
              (then
                (call $rotate (i32.add (local.get $at) (i32.const 20)) (i32.const 1)
                  (f32.sub (local.get $x) (f32.load offset=8 (local.get $at)))
                  (f32.sub (local.get $y) (f32.load offset=12 (local.get $at)))
                  (f32.sub (local.get $z) (f32.load offset=16 (local.get $at))))
                (f32.store offset=64 (local.get $slot) (f32.load (i32.const 1360)))
                (f32.store offset=68 (local.get $slot) (f32.load (i32.const 1364)))
                (f32.store offset=72 (local.get $slot) (f32.load (i32.const 1368)))
                (f32.store offset=76 (local.get $slot) (f32.const 1))))
            (call $rotate (i32.add (local.get $at) (i32.const 20)) (i32.const 0)
              (f32.load offset=64 (local.get $slot))
              (f32.load offset=68 (local.get $slot))
              (f32.load offset=72 (local.get $slot)))
            (f32.store (i32.const 1360)
              (f32.add (f32.load (i32.const 1360)) (f32.load offset=8 (local.get $at))))
            (f32.store (i32.const 1364)
              (f32.add (f32.load (i32.const 1364)) (f32.load offset=12 (local.get $at))))
            (f32.store (i32.const 1368)
              (f32.add (f32.load (i32.const 1368)) (f32.load offset=16 (local.get $at)))))))))

  ;; ---- Each frame ----

  (func (export "frame") (param $t f32) (param $dt f32)
    (local $i i32) (local $at i32) (local $slot i32) (local $player i32) (local $drawn i32)
    (local $holding i32)
    (local $id f32) (local $phase f32) (local $last f32) (local $age f32)
    (local $ax f32) (local $ay f32) (local $az f32) (local $far f32)
    (local $kind f32) (local $target f32)
    (local $lx f32) (local $ly f32) (local $lz f32)
    (local $mx f32) (local $my f32) (local $mz f32)
    (local $cx f32) (local $cy f32) (local $cz f32)
    (local $ux f32) (local $uy f32) (local $uz f32)
    (local $dist f32) (local $frac f32) (local $show i32)
    (local $sag f32) (local $shiver f32) (local $open f32)
    (global.set $players_n (call $players (i32.const 2048) (i32.const 64)))
    (global.set $vehicles_n (call $vehicles (i32.const 6144) (i32.const 32)))
    (call $environment (i32.const 1152))
    ;; Slots not seen this frame belong to players who left.
    (local.set $i (i32.const 0))
    (block $cleared
      (loop $clear
        (br_if $cleared (i32.ge_s (local.get $i) (i32.const 64)))
        (f32.store offset=8 (i32.add (i32.const 8192) (i32.mul (local.get $i) (i32.const 96)))
          (f32.const 0))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $clear)))
    (local.set $i (i32.const 0))
    (block $players_done
      (loop $each_player
        (br_if $players_done (i32.ge_s (local.get $i) (global.get $players_n)))
        (local.set $at (i32.add (i32.const 2048) (i32.mul (local.get $i) (i32.const 64))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (local.set $id (f32.load (local.get $at)))
        (local.set $player (i32.trunc_f32_s (local.get $id)))
        (local.set $phase (call $state (local.get $player) (i32.const 0)))
        (if (f32.ne (local.get $phase) (local.get $phase))
          (then (local.set $phase (f32.const 0))))
        ;; Where their held item is drawn, and its muzzle; whether it is
        ;; the launcher.
        (local.set $drawn (call $held (local.get $player) (i32.const 0) (i32.const 1280)))
        (local.set $holding (i32.and (local.get $drawn)
          (f32.eq (f32.load offset=60 (local.get $at)) (f32.convert_i32_s (global.get $gun)))))

        ;; The launcher itself, for anyone holding it: the winch gun over
        ;; the game's own Printer, its gauge glowing while the grapnel is
        ;; out.
        (if (local.get $holding)
          (then
            (if (i32.lt_s (global.get $gun_mesh) (i32.const 0))
              (then (global.set $gun_mesh (call $image_mesh (global.get $gun)))))
            (if (i32.ge_s (global.get $gun_mesh) (i32.const 0))
              (then
                (call $param (i32.const 0)
                  (if (result f32) (i32.and (f32.gt (local.get $phase) (f32.const 0.5))
                                            (f32.lt (local.get $phase) (f32.const 2.5)))
                    (then (f32.const 1)) (else (f32.const 0)))
                  (local.get $t) (f32.const 0) (local.get $id))
                (call $param (i32.const 1) (f32.load (i32.const 1152)) (f32.load (i32.const 1156))
                  (f32.load (i32.const 1160)) (f32.const 0))
                (call $param (i32.const 2) (f32.load (i32.const 1164)) (f32.load (i32.const 1168))
                  (f32.load (i32.const 1172)) (f32.const 0))
                (call $param (i32.const 3) (f32.load (i32.const 1176)) (f32.load (i32.const 1180))
                  (f32.load (i32.const 1184)) (f32.const 0))
                (call $draw_with (global.get $gun_mesh) (global.get $m_skin)
                  (i32.const 1280) (i32.const 1088))))))

        (local.set $slot (call $slot (local.get $id)))
        (f32.store offset=8 (local.get $slot) (f32.const 1))
        (local.set $last (f32.load offset=12 (local.get $slot)))
        (local.set $ax (call $state (local.get $player) (i32.const 1)))
        (local.set $ay (call $state (local.get $player) (i32.const 2)))
        (local.set $az (call $state (local.get $player) (i32.const 3)))
        (local.set $far (call $state (local.get $player) (i32.const 4)))
        (local.set $kind (call $state (local.get $player) (i32.const 5)))
        (local.set $target (call $state (local.get $player) (i32.const 6)))
        (if (i32.or (f32.ne (local.get $far) (local.get $far))
              (i32.or (f32.ne (local.get $ax) (local.get $ax))
                (i32.or (f32.ne (local.get $ay) (local.get $ay)) (f32.ne (local.get $az) (local.get $az)))))
          (then (local.set $phase (f32.const 0))))
        (if (f32.ne (local.get $kind) (local.get $kind))
          (then (local.set $kind (f32.const 0))))
        (local.set $lx (f32.load offset=32 (local.get $at)))
        (local.set $ly (f32.load offset=36 (local.get $at)))
        (local.set $lz (f32.load offset=40 (local.get $at)))
        ;; Where the cable starts: the launcher's muzzle while they hold
        ;; it, else their hands, where the engine's rope holds them.
        (if (local.get $holding)
          (then
            (local.set $mx (f32.load (i32.const 1344)))
            (local.set $my (f32.load (i32.const 1348)))
            (local.set $mz (f32.load (i32.const 1352))))
          (else
            (local.set $mx (f32.load offset=8 (local.get $at)))
            (local.set $my (f32.add (f32.load offset=12 (local.get $at)) (global.get $grip)))
            (local.set $mz (f32.load offset=16 (local.get $at)))))

        ;; A new phase: note when it began, and hear it.
        (if (f32.ne (local.get $phase) (local.get $last))
          (then
            (f32.store offset=16 (local.get $slot) (local.get $t))
            (if (i32.or (f32.eq (local.get $phase) (f32.const 1)) (f32.eq (local.get $phase) (f32.const 3)))
              (then
                ;; Fired: a fresh grapnel, cracking out.
                (f32.store offset=48 (local.get $slot) (f32.const 0))
                (f32.store offset=76 (local.get $slot) (f32.const 0))
                (call $sound (i32.const 160) (i32.const 22) (f32.const 0.8)
                  (local.get $mx) (local.get $my) (local.get $mz))))
            (if (f32.eq (local.get $phase) (f32.const 2))
              (then
                ;; Bitten: the winch takes the load, the cable buzzes.
                (f32.store offset=28 (local.get $slot) (local.get $t))
                (f32.store offset=32 (local.get $slot) (f32.const 0.1))
                (f32.store offset=60 (local.get $slot) (f32.const -1))
                (call $sound (i32.const 224) (i32.const 23) (f32.const 0.7)
                  (local.get $mx) (local.get $my) (local.get $mz))
                ;; Bitten before it was seen to fly (joining mid-shot, a
                ;; shot at point blank): clang now.
                (if (f32.eq (f32.load offset=48 (local.get $slot)) (f32.const 0))
                  (then
                    (f32.store offset=48 (local.get $slot) (f32.const 1))
                    (call $sound (i32.const 192) (i32.const 23) (f32.const 0.9)
                      (local.get $ax) (local.get $ay) (local.get $az))))))
            (if (i32.and (f32.eq (local.get $phase) (f32.const 0))
                  (i32.or (f32.eq (local.get $last) (f32.const 1)) (f32.eq (local.get $last) (f32.const 2))))
              (then
                ;; Let go: the grapnel folds and zips back in.
                (f32.store offset=52 (local.get $slot) (local.get $t))
                (call $sound (i32.const 256) (i32.const 25) (f32.const 0.6)
                  (local.get $mx) (local.get $my) (local.get $mz))))
            (f32.store offset=12 (local.get $slot) (local.get $phase))))
        (local.set $age (f32.sub (local.get $t) (f32.load offset=16 (local.get $slot))))
        (local.set $show (i32.const 0))
        (local.set $sag (f32.const 0))
        (local.set $shiver (f32.const 0))
        (local.set $open (f32.const 0.35))

        (if (f32.eq (local.get $phase) (f32.const 1))
          (then
            ;; Flying out: the grapnel covers the distance at the rule's
            ;; speed, and bites (clang) when it gets there.
            (local.set $dist (call $length (f32.sub (local.get $ax) (local.get $mx))
              (f32.sub (local.get $ay) (local.get $my)) (f32.sub (local.get $az) (local.get $mz))))
            (local.set $frac (call $clamp01
              (f32.div (f32.mul (local.get $age) (global.get $speed)) (f32.max (local.get $dist) (f32.const 0.01)))))
            (if (i32.and (f32.ge (local.get $frac) (f32.const 1))
                  (f32.eq (f32.load offset=48 (local.get $slot)) (f32.const 0)))
              (then
                (f32.store offset=48 (local.get $slot) (f32.const 1))
                (call $sound (i32.const 192) (i32.const 23) (f32.const 0.9)
                  (local.get $ax) (local.get $ay) (local.get $az))))
            (local.set $cx (f32.add (local.get $mx) (f32.mul (f32.sub (local.get $ax) (local.get $mx)) (local.get $frac))))
            (local.set $cy (f32.add (local.get $my) (f32.mul (f32.sub (local.get $ay) (local.get $my)) (local.get $frac))))
            (local.set $cz (f32.add (local.get $mz) (f32.mul (f32.sub (local.get $az) (local.get $mz)) (local.get $frac))))
            ;; The cable pays out behind it, a hair of belly.
            (local.set $sag (f32.mul (f32.const 0.015) (f32.mul (local.get $dist) (local.get $frac))))
            (local.set $show (i32.const 1))))

        (if (f32.eq (local.get $phase) (f32.const 2))
          (then
            ;; Hooked: the grapnel sits where it bit, riding along on a
            ;; player or vehicle; its claws are open.
            (call $anchor (local.get $slot) (local.get $kind) (local.get $target)
              (local.get $ax) (local.get $ay) (local.get $az))
            (local.set $cx (f32.load (i32.const 1360)))
            (local.set $cy (f32.load (i32.const 1364)))
            (local.set $cz (f32.load (i32.const 1368)))
            (local.set $open (call $clamp01 (f32.mul (local.get $age) (f32.const 12))))
            (local.set $dist (call $length (f32.sub (local.get $cx) (local.get $mx))
              (f32.sub (local.get $cy) (local.get $my)) (f32.sub (local.get $cz) (local.get $mz))))
            ;; The strands run along as the winch reels the cable in or out.
            (if (f32.ge (f32.load offset=60 (local.get $slot)) (f32.const 0))
              (then
                (f32.store offset=56 (local.get $slot)
                  (f32.add (f32.load offset=56 (local.get $slot))
                    (f32.sub (local.get $dist) (f32.load offset=60 (local.get $slot)))))))
            (f32.store offset=60 (local.get $slot) (local.get $dist))
            ;; A winch cable is taut: the least belly.
            (local.set $sag (f32.mul (f32.const 0.004) (local.get $dist)))
            ;; The buzz as it takes the load: fourteen a second, dying away
            ;; in half a second.
            (local.set $frac (call $clamp01
              (f32.sub (f32.const 1)
                (f32.mul (f32.sub (local.get $t) (f32.load offset=28 (local.get $slot))) (f32.const 2)))))
            (local.set $shiver
              (f32.mul (f32.mul (f32.load offset=32 (local.get $slot)) (f32.mul (local.get $frac) (local.get $frac)))
                (call $wave (f32.mul (f32.sub (local.get $t) (f32.load offset=28 (local.get $slot))) (f32.const 14)))))
            (local.set $show (i32.const 1))))

        (if (f32.eq (local.get $phase) (f32.const 3))
          (then
            ;; A miss: out as far as it goes, then back in a quarter of a
            ;; second.
            (local.set $dist (f32.div (f32.max (local.get $far) (f32.const 0.01)) (global.get $speed)))
            (local.set $frac
              (if (result f32) (f32.lt (local.get $age) (local.get $dist))
                (then (f32.div (local.get $age) (local.get $dist)))
                (else (call $clamp01
                  (f32.sub (f32.const 1) (f32.mul (f32.sub (local.get $age) (local.get $dist)) (f32.const 4)))))))
            (local.set $cx (f32.add (local.get $mx) (f32.mul (f32.sub (local.get $ax) (local.get $mx)) (local.get $frac))))
            (local.set $cy (f32.add (local.get $my) (f32.mul (f32.sub (local.get $ay) (local.get $my)) (local.get $frac))))
            (local.set $cz (f32.add (local.get $mz) (f32.mul (f32.sub (local.get $az) (local.get $mz)) (local.get $frac))))
            (local.set $sag (f32.mul (f32.const 0.03) (f32.mul (local.get $far) (local.get $frac))))
            (local.set $show (f32.gt (local.get $frac) (f32.const 0.02)))))

        (if (f32.eq (local.get $phase) (f32.const 0))
          (then
            ;; Let go a moment ago: the grapnel folds and zips back to the
            ;; launcher from where it was, in a fifth of a second.
            (local.set $frac (call $clamp01
              (f32.sub (f32.const 1)
                (f32.mul (f32.sub (local.get $t) (f32.load offset=52 (local.get $slot))) (f32.const 5)))))
            (if (f32.gt (local.get $frac) (f32.const 0.02))
              (then
                (local.set $cx (f32.add (local.get $mx)
                  (f32.mul (f32.sub (f32.load offset=36 (local.get $slot)) (local.get $mx)) (local.get $frac))))
                (local.set $cy (f32.add (local.get $my)
                  (f32.mul (f32.sub (f32.load offset=40 (local.get $slot)) (local.get $my)) (local.get $frac))))
                (local.set $cz (f32.add (local.get $mz)
                  (f32.mul (f32.sub (f32.load offset=44 (local.get $slot)) (local.get $mz)) (local.get $frac))))
                (local.set $dist (call $length (f32.sub (local.get $cx) (local.get $mx))
                  (f32.sub (local.get $cy) (local.get $my)) (f32.sub (local.get $cz) (local.get $mz))))
                (local.set $sag (f32.mul (f32.const 0.05) (local.get $dist)))
                (local.set $open (f32.mul (local.get $frac) (f32.const 0.5)))
                (local.set $show (i32.const 1))))))

        (if (local.get $show)
          (then
            ;; Where the grapnel is, for zipping back from when let go.
            (if (f32.ne (local.get $phase) (f32.const 0))
              (then
                (f32.store offset=36 (local.get $slot) (local.get $cx))
                (f32.store offset=40 (local.get $slot) (local.get $cy))
                (f32.store offset=44 (local.get $slot) (local.get $cz))))
            ;; It points from the cable's start to its crown.
            (local.set $ux (f32.sub (local.get $cx) (local.get $mx)))
            (local.set $uy (f32.sub (local.get $cy) (local.get $my)))
            (local.set $uz (f32.sub (local.get $cz) (local.get $mz)))
            (local.set $dist (call $length (local.get $ux) (local.get $uy) (local.get $uz)))
            (if (f32.lt (local.get $dist) (f32.const 0.001))
              (then
                (local.set $ux (local.get $lx))
                (local.set $uy (local.get $ly))
                (local.set $uz (local.get $lz)))
              (else
                (local.set $ux (f32.div (local.get $ux) (local.get $dist)))
                (local.set $uy (f32.div (local.get $uy) (local.get $dist)))
                (local.set $uz (f32.div (local.get $uz) (local.get $dist)))))
            ;; The cable, unless the grapnel is still in the muzzle.
            (if (f32.gt (local.get $dist) (global.get $tie))
              (then
                (call $cable (local.get $mx) (local.get $my) (local.get $mz)
                  (f32.sub (local.get $cx) (f32.mul (local.get $ux) (global.get $tie)))
                  (f32.sub (local.get $cy) (f32.mul (local.get $uy) (global.get $tie)))
                  (f32.sub (local.get $cz) (f32.mul (local.get $uz) (global.get $tie)))
                  (local.get $sag) (local.get $shiver) (f32.load offset=56 (local.get $slot)))))
            (call $grapnel (local.get $cx) (local.get $cy) (local.get $cz)
              (local.get $ux) (local.get $uy) (local.get $uz)
              (f32.mul (local.get $id) (f32.const 1.3)) (local.get $open))))
        (br $each_player)))
    ;; Forget players who left.
    (local.set $i (i32.const 0))
    (block $freed
      (loop $free
        (br_if $freed (i32.ge_s (local.get $i) (i32.const 64)))
        (local.set $at (i32.add (i32.const 8192) (i32.mul (local.get $i) (i32.const 96))))
        (if (f32.eq (f32.load offset=8 (local.get $at)) (f32.const 0))
          (then (f32.store offset=4 (local.get $at) (f32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $free))))
)
