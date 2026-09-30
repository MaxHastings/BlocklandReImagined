;; Grapple Rope effects, drawn on every player's screen for everyone's rope:
;;
;; - the launcher itself: the Printer it is built from, carved from jungle
;;   hardwood (jungle.wgsl), bound with bamboo, wound with a vine, mossy
;;   on top and pinned with brass that glints while the hook is out;
;; - throwing: the three-pronged brass hook (hook.wgsl) flies from the
;;   muzzle to where it will bite at the rule's speed, trailing its rope
;;   (rope.wgsl), with a whoosh as it leaves and a clank as it bites;
;; - hooked: the rope runs from the muzzle to the hook, hanging in a curve
;;   when it is slack and straight when it is taut; when it snaps tight it
;;   twangs and shivers, the harder the more slack it took up; its strands
;;   run along as it is reeled in or out;
;; - a miss: the hook flies out as far as it goes and falls back in;
;; - letting go: the hook and rope zip back into the muzzle.
;;
;; Everything comes from what the game already knows (`world.read`): where
;; players are drawn, where each player's launcher is drawn and its muzzle
;; (`held`), the launcher's own model (`image_mesh`), and each player's
;; `rope` from the Grapple Rope Add-On's public state: [phase, x, y, z,
;; length]; phases 0 none, 1 flying out to x, 2 hooked at x, 3 missed. The
;; server sends that only when it changes; the swinging itself comes from
;; the game's own pose updates, so the rope costs no bandwidth.
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
;;   8192   per-player effect state, 64 bytes each, 64 slots:
;;            +0 id  +4 in use  +8 seen this frame  +12 phase last frame
;;            +16 when that phase began  +20 the rope's length as drawn
;;            +24 its slack last frame  +28 when it last snapped tight
;;            +32 how hard  +36 the hook's crown last frame xyz
;;            +48 whether this throw has bitten  +52 when it was let go
;;            +56 how far the strands have run
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
  (import "bri" "image_kind" (func $image_kind (param i32 i32) (result i32)))
  (import "bri" "image_mesh" (func $image_mesh (param i32) (result i32)))
  (import "bri" "held" (func $held (param i32 i32 i32) (result i32)))
  (import "bri" "state_num" (func $state_num (param i32 i32 i32 i32 i32 i32) (result f32)))
  (import "bri" "sound_at" (func $sound_at (param i32 i32 f32 f32 f32 f32) (result i32)))
  (memory (export "memory") 2)

  (global $tube (mut i32) (i32.const 0))
  (global $prong (mut i32) (i32.const 0))
  (global $m_rope (mut i32) (i32.const 0))
  (global $m_hook (mut i32) (i32.const 0))
  (global $m_skin (mut i32) (i32.const 0))
  ;; The launcher's image, as `players` records name it, and its model
  ;; once someone holds it (-1 before).
  (global $gun (mut i32) (i32.const 0))
  (global $gun_mesh (mut i32) (i32.const -1))
  (global $players_n (mut i32) (i32.const 0))

  (data (i32.const 0) "client/rope.wgsl")
  (data (i32.const 32) "client/hook.wgsl")
  (data (i32.const 64) "client/jungle.wgsl")
  (data (i32.const 96) "grapple-rope")
  (data (i32.const 112) "rope")
  (data (i32.const 128) "grapple rope effects ready")
  (data (i32.const 160) "client/sounds/throw.wav")
  (data (i32.const 192) "client/sounds/bite.wav")
  (data (i32.const 224) "client/sounds/twang.wav")
  (data (i32.const 256) "client/sounds/zip.wav")
  (data (i32.const 288) "grapple-rope-tool:image/grapplerope")

  ;; How fast the hook flies, units a second: the rule's `speed()`.
  (global $speed f32 (f32.const 160))
  ;; The hook's size, and how far behind its crown the rope ties on.
  (global $size f32 (f32.const 1.1))
  (global $tie f32 (f32.const 0.605))
  (global $radius f32 (f32.const 0.05))
  ;; How fast the rope reels (the engine's default), units a second.
  (global $reel f32 (f32.const 24))

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
    ;; The rope: twelve round (its three strands stand out on the edge),
    ;; ninety-six along (smooth when it sags). The hook's parts are short.
    (global.set $tube (call $grid (i32.const 12) (i32.const 96)))
    (global.set $prong (call $grid (i32.const 8) (i32.const 16)))
    ;; Everything is solid: it hides behind the world and the world
    ;; behind it.
    (global.set $m_rope (call $material_create (call $shader (i32.const 0) (i32.const 16))))
    (global.set $m_hook (call $material_create (call $shader (i32.const 32) (i32.const 16))))
    (global.set $m_skin (call $material_create (call $shader (i32.const 64) (i32.const 18))))
    (global.set $gun (call $image_kind (i32.const 288) (i32.const 35)))
    (memory.fill (i32.const 8192) (i32.const 0) (i32.const 4096))
    (f32.store (i32.const 1024) (f32.const 1))
    (f32.store (i32.const 1044) (f32.const 1))
    (f32.store (i32.const 1064) (f32.const 1))
    (f32.store (i32.const 1084) (f32.const 1))
    (call $log (i32.const 128) (i32.const 26)))

  ;; ---- Drawing ----

  ;; Parameter `i` (0 to 3) of the next draw.
  (func $param (param $i i32) (param $x f32) (param $y f32) (param $z f32) (param $w f32)
    (local $at i32)
    (local.set $at (i32.add (i32.const 1088) (i32.mul (local.get $i) (i32.const 16))))
    (f32.store (local.get $at) (local.get $x))
    (f32.store offset=4 (local.get $at) (local.get $y))
    (f32.store offset=8 (local.get $at) (local.get $z))
    (f32.store offset=12 (local.get $at) (local.get $w)))

  ;; Parameters 2 and 3 as the rope and hook read them: the sunlight's
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

  ;; The rope from the muzzle (x0) to where it ties onto the hook (x1).
  (func $rope (param $x0 f32) (param $y0 f32) (param $z0 f32)
              (param $x1 f32) (param $y1 f32) (param $z1 f32)
              (param $sag f32) (param $shiver f32) (param $run f32)
    (call $param (i32.const 0) (local.get $x0) (local.get $y0) (local.get $z0) (global.get $radius))
    (call $param (i32.const 1) (local.get $x1) (local.get $y1) (local.get $z1) (local.get $sag))
    (call $light (local.get $shiver) (local.get $run))
    (call $draw (global.get $tube) (global.get $m_rope)))

  ;; The hook: its crown at c, pointing along u (a unit vector), its
  ;; prongs turned `spin` round it. The shank, then three prongs.
  (func $hook (param $cx f32) (param $cy f32) (param $cz f32)
              (param $ux f32) (param $uy f32) (param $uz f32) (param $spin f32)
    (local $part i32)
    (local.set $part (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $part) (i32.const 4)))
        (call $param (i32.const 0) (local.get $cx) (local.get $cy) (local.get $cz) (global.get $size))
        (call $param (i32.const 1) (local.get $ux) (local.get $uy) (local.get $uz)
          (f32.convert_i32_s (local.get $part)))
        (call $light (local.get $spin) (f32.const 0))
        (call $draw (global.get $prong) (global.get $m_hook))
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
        (local.set $at (i32.add (i32.const 8192) (i32.mul (local.get $i) (i32.const 64))))
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
    (memory.fill (local.get $free) (i32.const 0) (i32.const 64))
    (f32.store (local.get $free) (local.get $id))
    (f32.store offset=4 (local.get $free) (f32.const 1))
    (f32.store offset=28 (local.get $free) (f32.const -100))
    (f32.store offset=52 (local.get $free) (f32.const -100))
    (local.get $free))

  ;; A number of player `player`'s `rope`; NaN when there is none.
  (func $state (param $player i32) (param $index i32) (result f32)
    (call $state_num (i32.const 96) (i32.const 12) (i32.const 112) (i32.const 4)
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

  ;; ---- Each frame ----

  (func (export "frame") (param $t f32) (param $dt f32)
    (local $i i32) (local $at i32) (local $slot i32) (local $player i32) (local $drawn i32)
    (local $id f32) (local $phase f32) (local $last f32) (local $age f32)
    (local $ax f32) (local $ay f32) (local $az f32) (local $far f32)
    (local $ex f32) (local $ey f32) (local $ez f32)
    (local $lx f32) (local $ly f32) (local $lz f32)
    (local $rx f32) (local $rz f32) (local $rl f32)
    (local $mx f32) (local $my f32) (local $mz f32)
    (local $cx f32) (local $cy f32) (local $cz f32)
    (local $ux f32) (local $uy f32) (local $uz f32)
    (local $dist f32) (local $frac f32) (local $show i32)
    (local $sag f32) (local $shiver f32) (local $length f32) (local $wanted f32)
    (local $slack f32) (local $step f32)
    (global.set $players_n (call $players (i32.const 2048) (i32.const 64)))
    (call $environment (i32.const 1152))
    ;; Slots not seen this frame belong to players who left.
    (local.set $i (i32.const 0))
    (block $cleared
      (loop $clear
        (br_if $cleared (i32.ge_s (local.get $i) (i32.const 64)))
        (f32.store offset=8 (i32.add (i32.const 8192) (i32.mul (local.get $i) (i32.const 64)))
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
        ;; Where their launcher is drawn, and its muzzle.
        (local.set $drawn (call $held (local.get $player) (i32.const 0) (i32.const 1280)))

        ;; The launcher itself, for anyone holding it: carved wood over the
        ;; game's own Printer, its brass glinting while the hook is out.
        (if (i32.and (local.get $drawn)
              (f32.eq (f32.load offset=60 (local.get $at)) (f32.convert_i32_s (global.get $gun))))
          (then
            (if (i32.lt_s (global.get $gun_mesh) (i32.const 0))
              (then (global.set $gun_mesh (call $image_mesh (global.get $gun)))))
            (if (i32.ge_s (global.get $gun_mesh) (i32.const 0))
              (then
                (call $param (i32.const 0)
                  (if (result f32) (i32.and (f32.gt (local.get $phase) (f32.const 0.5))
                                            (f32.lt (local.get $phase) (f32.const 2.5)))
                    (then (f32.const 1)) (else (f32.const 0)))
                  (f32.const 0) (f32.const 0) (local.get $id))
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
        (if (i32.or (f32.ne (local.get $far) (local.get $far))
              (i32.or (f32.ne (local.get $ax) (local.get $ax))
                (i32.or (f32.ne (local.get $ay) (local.get $ay)) (f32.ne (local.get $az) (local.get $az)))))
          (then (local.set $phase (f32.const 0))))
        (local.set $ex (f32.load offset=20 (local.get $at)))
        (local.set $ey (f32.load offset=24 (local.get $at)))
        (local.set $ez (f32.load offset=28 (local.get $at)))
        (local.set $lx (f32.load offset=32 (local.get $at)))
        (local.set $ly (f32.load offset=36 (local.get $at)))
        (local.set $lz (f32.load offset=40 (local.get $at)))
        ;; The muzzle, where the game draws it; without a drawn launcher,
        ;; ahead of the eye, to the right and a little down.
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

        ;; A new phase: note when it began, and hear it.
        (if (f32.ne (local.get $phase) (local.get $last))
          (then
            (f32.store offset=16 (local.get $slot) (local.get $t))
            (if (i32.or (f32.eq (local.get $phase) (f32.const 1)) (f32.eq (local.get $phase) (f32.const 3)))
              (then
                ;; Thrown: a fresh hook, whooshing out.
                (f32.store offset=48 (local.get $slot) (f32.const 0))
                (call $sound (i32.const 160) (i32.const 23) (f32.const 0.8)
                  (local.get $mx) (local.get $my) (local.get $mz))))
            (if (f32.eq (local.get $phase) (f32.const 2))
              (then
                (f32.store offset=20 (local.get $slot) (local.get $far))
                (f32.store offset=24 (local.get $slot) (f32.const 0))
                ;; Bitten before it was seen to fly (joining mid-throw, a
                ;; throw at point blank): clank now.
                (if (f32.eq (f32.load offset=48 (local.get $slot)) (f32.const 0))
                  (then
                    (f32.store offset=48 (local.get $slot) (f32.const 1))
                    (call $sound (i32.const 192) (i32.const 22) (f32.const 0.9)
                      (local.get $ax) (local.get $ay) (local.get $az))))))
            (if (i32.and (f32.eq (local.get $phase) (f32.const 0))
                  (i32.or (f32.eq (local.get $last) (f32.const 1)) (f32.eq (local.get $last) (f32.const 2))))
              (then
                ;; Let go: the hook zips back in.
                (f32.store offset=52 (local.get $slot) (local.get $t))
                (call $sound (i32.const 256) (i32.const 21) (f32.const 0.6)
                  (local.get $mx) (local.get $my) (local.get $mz))))
            (f32.store offset=12 (local.get $slot) (local.get $phase))))
        (local.set $age (f32.sub (local.get $t) (f32.load offset=16 (local.get $slot))))
        (local.set $show (i32.const 0))
        (local.set $sag (f32.const 0))
        (local.set $shiver (f32.const 0))

        (if (f32.eq (local.get $phase) (f32.const 1))
          (then
            ;; Flying out: the hook covers the distance at the rule's
            ;; speed, and bites (clank) when it gets there.
            (local.set $dist (call $length (f32.sub (local.get $ax) (local.get $mx))
              (f32.sub (local.get $ay) (local.get $my)) (f32.sub (local.get $az) (local.get $mz))))
            (local.set $frac (call $clamp01
              (f32.div (f32.mul (local.get $age) (global.get $speed)) (f32.max (local.get $dist) (f32.const 0.01)))))
            (if (i32.and (f32.ge (local.get $frac) (f32.const 1))
                  (f32.eq (f32.load offset=48 (local.get $slot)) (f32.const 0)))
              (then
                (f32.store offset=48 (local.get $slot) (f32.const 1))
                (call $sound (i32.const 192) (i32.const 22) (f32.const 0.9)
                  (local.get $ax) (local.get $ay) (local.get $az))))
            (local.set $cx (f32.add (local.get $mx) (f32.mul (f32.sub (local.get $ax) (local.get $mx)) (local.get $frac))))
            (local.set $cy (f32.add (local.get $my) (f32.mul (f32.sub (local.get $ay) (local.get $my)) (local.get $frac))))
            (local.set $cz (f32.add (local.get $mz) (f32.mul (f32.sub (local.get $az) (local.get $mz)) (local.get $frac))))
            ;; The rope pays out behind it with a little belly.
            (local.set $sag (f32.mul (f32.const 0.04) (f32.mul (local.get $dist) (local.get $frac))))
            (local.set $show (i32.const 1))))

        (if (f32.eq (local.get $phase) (f32.const 2))
          (then
            (local.set $cx (local.get $ax))
            (local.set $cy (local.get $ay))
            (local.set $cz (local.get $az))
            ;; The rope as drawn reels toward the rule's length at the
            ;; engine's rate; its strands run along with it.
            (local.set $length (f32.load offset=20 (local.get $slot)))
            (local.set $wanted (local.get $far))
            (local.set $step (f32.mul (global.get $reel) (local.get $dt)))
            (local.set $step (f32.min (local.get $step)
              (f32.max (f32.neg (local.get $step)) (f32.sub (local.get $wanted) (local.get $length)))))
            (local.set $length (f32.add (local.get $length) (local.get $step)))
            (f32.store offset=20 (local.get $slot) (local.get $length))
            (f32.store offset=56 (local.get $slot)
              (f32.add (f32.load offset=56 (local.get $slot)) (local.get $step)))
            ;; Slack: rope the body is inside of. The rope holds the hands
            ;; and is drawn from the muzzle a little off them, so a hair of
            ;; it does not count.
            (local.set $dist (call $length (f32.sub (local.get $ax) (local.get $mx))
              (f32.sub (local.get $ay) (local.get $my)) (f32.sub (local.get $az) (local.get $mz))))
            (local.set $slack (f32.max (f32.const 0)
              (f32.sub (f32.sub (local.get $length) (local.get $dist)) (f32.const 0.4))))
            ;; A slack rope hangs in a curve about as deep as this (a
            ;; parabola as long as the rope), never deeper than half of it.
            (local.set $sag (f32.min (f32.mul (local.get $length) (f32.const 0.5))
              (f32.sqrt (f32.mul (f32.const 0.375) (f32.mul (local.get $dist) (local.get $slack))))))
            ;; Snapped tight after hanging slack: it twangs and shivers,
            ;; harder the more slack it took up.
            (if (i32.and (f32.gt (f32.load offset=24 (local.get $slot)) (f32.const 1.2))
                         (f32.lt (local.get $slack) (f32.const 0.15)))
              (then
                (f32.store offset=28 (local.get $slot) (local.get $t))
                (f32.store offset=32 (local.get $slot)
                  (f32.min (f32.const 0.6)
                    (f32.add (f32.const 0.1) (f32.mul (f32.load offset=24 (local.get $slot)) (f32.const 0.05)))))
                (call $sound (i32.const 224) (i32.const 23)
                  (f32.min (f32.const 1)
                    (f32.add (f32.const 0.3) (f32.div (f32.load offset=24 (local.get $slot)) (f32.const 8))))
                  (local.get $mx) (local.get $my) (local.get $mz))))
            (f32.store offset=24 (local.get $slot) (local.get $slack))
            ;; The shiver: seven swings a second, dying away in half a
            ;; second.
            (local.set $frac (call $clamp01
              (f32.sub (f32.const 1)
                (f32.mul (f32.sub (local.get $t) (f32.load offset=28 (local.get $slot))) (f32.const 2)))))
            (local.set $shiver
              (f32.mul (f32.mul (f32.load offset=32 (local.get $slot)) (f32.mul (local.get $frac) (local.get $frac)))
                (call $wave (f32.mul (f32.sub (local.get $t) (f32.load offset=28 (local.get $slot))) (f32.const 7)))))
            (local.set $show (i32.const 1))))

        (if (f32.eq (local.get $phase) (f32.const 3))
          (then
            ;; A miss: out as far as it goes, then back in a third of a
            ;; second, the rope bellying as it falls back.
            (local.set $dist (f32.div (f32.max (local.get $far) (f32.const 0.01)) (global.get $speed)))
            (local.set $frac
              (if (result f32) (f32.lt (local.get $age) (local.get $dist))
                (then (f32.div (local.get $age) (local.get $dist)))
                (else (call $clamp01
                  (f32.sub (f32.const 1) (f32.mul (f32.sub (local.get $age) (local.get $dist)) (f32.const 3)))))))
            (local.set $cx (f32.add (local.get $mx) (f32.mul (f32.sub (local.get $ax) (local.get $mx)) (local.get $frac))))
            (local.set $cy (f32.add (local.get $my) (f32.mul (f32.sub (local.get $ay) (local.get $my)) (local.get $frac))))
            (local.set $cz (f32.add (local.get $mz) (f32.mul (f32.sub (local.get $az) (local.get $mz)) (local.get $frac))))
            (local.set $sag (f32.mul (f32.const 0.08) (f32.mul (local.get $far) (local.get $frac))))
            (local.set $show (f32.gt (local.get $frac) (f32.const 0.02)))))

        (if (f32.eq (local.get $phase) (f32.const 0))
          (then
            ;; Let go a moment ago: the hook zips back into the muzzle
            ;; from where it was, in a fifth of a second.
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
                (local.set $sag (f32.mul (f32.const 0.12) (local.get $dist)))
                (local.set $show (i32.const 1))))))

        (if (local.get $show)
          (then
            ;; Where the hook is, for zipping back from when let go.
            (if (f32.ne (local.get $phase) (f32.const 0))
              (then
                (f32.store offset=36 (local.get $slot) (local.get $cx))
                (f32.store offset=40 (local.get $slot) (local.get $cy))
                (f32.store offset=44 (local.get $slot) (local.get $cz))))
            ;; The hook points from the muzzle to its crown.
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
            ;; The rope, unless the hook is still in the muzzle.
            (if (f32.gt (local.get $dist) (global.get $tie))
              (then
                (call $rope (local.get $mx) (local.get $my) (local.get $mz)
                  (f32.sub (local.get $cx) (f32.mul (local.get $ux) (global.get $tie)))
                  (f32.sub (local.get $cy) (f32.mul (local.get $uy) (global.get $tie)))
                  (f32.sub (local.get $cz) (f32.mul (local.get $uz) (global.get $tie)))
                  (local.get $sag) (local.get $shiver) (f32.load offset=56 (local.get $slot)))))
            (call $hook (local.get $cx) (local.get $cy) (local.get $cz)
              (local.get $ux) (local.get $uy) (local.get $uz)
              (f32.mul (local.get $id) (f32.const 1.7)))))
        (br $each_player)))
    ;; Forget players who left.
    (local.set $i (i32.const 0))
    (block $freed
      (loop $free
        (br_if $freed (i32.ge_s (local.get $i) (i32.const 64)))
        (local.set $at (i32.add (i32.const 8192) (i32.mul (local.get $i) (i32.const 64))))
        (if (f32.eq (f32.load offset=8 (local.get $at)) (f32.const 0))
          (then (f32.store offset=4 (local.get $at) (f32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $free))))
)
