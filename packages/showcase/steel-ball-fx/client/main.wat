;; Steel Ball sounds: the ball clanks (or, for a softer knock, thuds)
;; where it strikes something, louder the harder it hit. The ball's look is
;; the Steel Kit's own model: bare metal the game draws with its
;; environment probe, so it needs no code here.
;;
;; Each frame it asks the game where the vehicles are (`vehicles`, the
;; `world.read` capability). When a Steel Ball's velocity changes sharply
;; between frames it struck something.
;;
;; This is the source of main.wasm; `cargo test -p bri-client-sandbox
;; --test showcase` checks the two match (BRI_BLESS=1 rewrites it).
;;
;; Memory layout:
;;   0      strings
;;   2048   vehicle records, 16 f32 (64 bytes) each, up to 256
;;   24576  what each ball did last frame, 32 bytes each, 256 slots:
;;            +0 id  +4 in use  +8 velocity xyz  +20 last sound time
;;            +24 seen this frame
(module
  (import "bri" "vehicle_kind" (func $vehicle_kind (param i32 i32) (result i32)))
  (import "bri" "vehicles" (func $vehicles (param i32 i32) (result i32)))
  (import "bri" "sound_at" (func $sound_at (param i32 i32 f32 f32 f32 f32) (result i32)))
  (memory (export "memory") 1)
  (global $ball (mut i32) (i32.const -1))
  (data (i32.const 64) "steel-ball-kit:vehicle/steelball")
  (data (i32.const 160) "client/sounds/clank.wav")
  (data (i32.const 192) "client/sounds/thud.wav")

  ;; The memory of the ball `id`: its slot, else a fresh one (0 when full).
  (func $slot (param $id f32) (result i32)
    (local $i i32) (local $at i32) (local $free i32)
    (local.set $free (i32.const -1))
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 256)))
        (local.set $at (i32.add (i32.const 24576) (i32.mul (local.get $i) (i32.const 32))))
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
      (then (return (i32.const 0))))
    (memory.fill (local.get $free) (i32.const 0) (i32.const 32))
    (f32.store (local.get $free) (local.get $id))
    (f32.store offset=20 (local.get $free) (f32.const -100))
    (local.get $free))

  ;; Listen to one ball: a sharp change of speed since last frame is a hit.
  (func $listen (param $at i32) (param $t f32)
    (local $slot i32) (local $dx f32) (local $dy f32) (local $dz f32) (local $hit f32)
    (local.set $slot (call $slot (f32.load (local.get $at))))
    (if (i32.eqz (local.get $slot)) (then (return)))
    (f32.store offset=24 (local.get $slot) (f32.const 1))
    (if (f32.eq (f32.load offset=4 (local.get $slot)) (f32.const 1))
      (then
        (local.set $dx (f32.sub (f32.load offset=36 (local.get $at)) (f32.load offset=8 (local.get $slot))))
        (local.set $dy (f32.sub (f32.load offset=40 (local.get $at)) (f32.load offset=12 (local.get $slot))))
        (local.set $dz (f32.sub (f32.load offset=44 (local.get $at)) (f32.load offset=16 (local.get $slot))))
        (local.set $hit (f32.sqrt (f32.add (f32.mul (local.get $dx) (local.get $dx))
          (f32.add (f32.mul (local.get $dy) (local.get $dy)) (f32.mul (local.get $dz) (local.get $dz))))))
        (if (i32.and (f32.gt (local.get $hit) (f32.const 4.5))
                     (f32.gt (f32.sub (local.get $t) (f32.load offset=20 (local.get $slot))) (f32.const 0.15)))
          (then
            (f32.store offset=20 (local.get $slot) (local.get $t))
            (if (f32.gt (local.get $hit) (f32.const 9))
              (then
                (drop (call $sound_at (i32.const 160) (i32.const 23)
                  (f32.min (f32.const 1) (f32.div (local.get $hit) (f32.const 22)))
                  (f32.load offset=8 (local.get $at)) (f32.load offset=12 (local.get $at))
                  (f32.load offset=16 (local.get $at)))))
              (else
                (drop (call $sound_at (i32.const 192) (i32.const 22)
                  (f32.min (f32.const 1) (f32.div (local.get $hit) (f32.const 10)))
                  (f32.load offset=8 (local.get $at)) (f32.load offset=12 (local.get $at))
                  (f32.load offset=16 (local.get $at))))))))))
    (f32.store offset=4 (local.get $slot) (f32.const 1))
    (f32.store offset=8 (local.get $slot) (f32.load offset=36 (local.get $at)))
    (f32.store offset=12 (local.get $slot) (f32.load offset=40 (local.get $at)))
    (f32.store offset=16 (local.get $slot) (f32.load offset=44 (local.get $at))))

  (func (export "init")
    (global.set $ball (call $vehicle_kind (i32.const 64) (i32.const 32))))

  (func (export "frame") (param $t f32) (param $dt f32)
    (local $count i32) (local $i i32) (local $at i32)
    (local.set $count (call $vehicles (i32.const 2048) (i32.const 256)))
    ;; Balls not seen this frame are gone.
    (local.set $i (i32.const 0))
    (block $cleared
      (loop $clear
        (br_if $cleared (i32.ge_s (local.get $i) (i32.const 256)))
        (f32.store offset=24 (i32.add (i32.const 24576) (i32.mul (local.get $i) (i32.const 32)))
          (f32.const 0))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $clear)))
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (local.get $count)))
        (local.set $at (i32.add (i32.const 2048) (i32.mul (local.get $i) (i32.const 64))))
        ;; A Steel Ball: record float 1 is its kind.
        (if (f32.eq (f32.load offset=4 (local.get $at)) (f32.convert_i32_s (global.get $ball)))
          (then (call $listen (local.get $at) (local.get $t))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (local.set $i (i32.const 0))
    (block $freed
      (loop $free
        (br_if $freed (i32.ge_s (local.get $i) (i32.const 256)))
        (local.set $at (i32.add (i32.const 24576) (i32.mul (local.get $i) (i32.const 32))))
        (if (f32.eq (f32.load offset=24 (local.get $at)) (f32.const 0))
          (then (f32.store offset=4 (local.get $at) (f32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $free))))
)
