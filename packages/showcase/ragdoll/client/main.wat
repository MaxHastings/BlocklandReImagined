;; Ragdoll: when a player dies, their Blockhead goes floppy instead of
;; playing the death animation, on every player's screen that runs it.
;;
;; Built only from what any Add-On gets:
;; - `world.read`: who is alive, where their corpse is and how it moves;
;; - `avatar.pose`: each body's parts as drawn (the player's own outfit,
;;   from the game's model at run time), and drawing the body posed;
;; - `physics.local`: one box per body part, joined at the part's pivot,
;;   simulated on this PC only. Bricks, terrain and the map stop them;
;;   players, vehicles and shots knock them about; nothing goes over the
;;   network and nothing in gameplay feels them.
;;
;; A ragdoll starts from the pose the player died in, moving as they
;; moved, with a little pop. It follows its corpse: a blast that throws
;; the corpse throws every limb, and if the corpse is carried off the
;; ragdoll is drawn along after it. Its limbs are shared bodies, so other
;; Add-Ons (the Gravity Gun) can pick them up and throw them. It goes when
;; the corpse does (v20's five seconds) or when the player respawns.
;;
;; Which parts become bodies comes from the model: the node each outfit
;; part (`chest`, `rarm`, ...) moves with, a box round what is drawn on
;; it, and a joint where it meets the nearest part above it.
;;
;; This is the source of main.wasm; `cargo test -p bri-client-sandbox
;; --test ragdoll` checks the two match (BRI_BLESS=1 rewrites it).
;;
;; Memory layout:
;;   256    part names, 16 bytes each
;;   512    part name lengths (i32)
;;   576    joint limits per part: swing, twist, friction (3 f32)
;;   1024   player records, 16 f32 (64 bytes) each, up to 64
;;   8192   skeleton records, 16 f32 each, up to 128 nodes
;;   16384  body record (28 f32)
;;   16512  joint record (12 f32)
;;   16576  body state (16 f32)
;;   17408  pose records, 8 f32 (32 bytes) each, up to 16
;;   20480  ragdolls, 256 bytes each, 24 slots:
;;            +0 player id (f32)  +4 state (0 free, 1 ragdoll, 2 given up)
;;            +8 parts  +12 corpse speed last frame
;;            +16 corpse velocity last frame xyz  +28 root part
;;            +32 parts, 8 bytes each (node, body), up to 9
(module
  (import "bri" "random" (func $random (result i32)))
  (import "bri" "players" (func $players (param i32 i32) (result i32)))
  (import "bri" "skeleton" (func $skeleton (param i32 i32 i32) (result i32)))
  (import "bri" "skeleton_part" (func $skeleton_part (param i32 i32 i32) (result i32)))
  (import "bri" "pose" (func $pose (param i32 i32 i32) (result i32)))
  (import "bri" "rigid_create" (func $rigid_create (param i32) (result i32)))
  (import "bri" "rigid_joint" (func $rigid_joint (param i32 i32 i32) (result i32)))
  (import "bri" "rigid_remove" (func $rigid_remove (param i32)))
  (import "bri" "rigid_push" (func $rigid_push (param i32 f32 f32 f32)))
  (import "bri" "rigid_get" (func $rigid_get (param i32 i32) (result i32)))
  (memory (export "memory") 1)

  (global $players_n (mut i32) (i32.const 0))
  ;; $qrot's result.
  (global $rx (mut f32) (f32.const 0))
  (global $ry (mut f32) (f32.const 0))
  (global $rz (mut f32) (f32.const 0))

  ;; Parts, in the order they are built: the torso first, so it is the
  ;; root the others hang from.
  (data (i32.const 256) "chest")
  (data (i32.const 272) "pants")
  (data (i32.const 288) "headskin")
  (data (i32.const 304) "rarm")
  (data (i32.const 320) "larm")
  (data (i32.const 336) "rhand")
  (data (i32.const 352) "lhand")
  (data (i32.const 368) "rshoe")
  (data (i32.const 384) "lshoe")
  (data (i32.const 512) "\05\00\00\00\05\00\00\00\08\00\00\00\04\00\00\00\04\00\00\00\05\00\00\00\05\00\00\00\05\00\00\00\05\00\00\00")
  ;; swing, twist, friction (radians; f32 little-endian):
  ;;   chest 0.5 0.35 0.5, pants 0.5 0.35 0.5, head 0.7 0.6 0.3,
  ;;   arms 1.9 0.9 0.15, hands 0.6 0.4 0.1, legs 1.3 0.35 0.3
  (data (i32.const 576)
    "\00\00\00\3f\33\33\b3\3e\00\00\00\3f"
    "\00\00\00\3f\33\33\b3\3e\00\00\00\3f"
    "\33\33\33\3f\9a\99\19\3f\9a\99\99\3e"
    "\33\33\f3\3f\66\66\66\3f\9a\99\19\3e"
    "\33\33\f3\3f\66\66\66\3f\9a\99\19\3e"
    "\9a\99\19\3f\cd\cc\cc\3e\cd\cc\cc\3d"
    "\9a\99\19\3f\cd\cc\cc\3e\cd\cc\cc\3d"
    "\66\66\a6\3f\33\33\b3\3e\9a\99\99\3e"
    "\66\66\a6\3f\33\33\b3\3e\9a\99\99\3e")

  ;; A number in [0, 1).
  (func $rand (result f32)
    (f32.div
      (f32.convert_i32_u (i32.shr_u (call $random) (i32.const 8)))
      (f32.const 16777216)))

  ;; Rotate (x, y, z) by the unit quaternion at `q` (x y z w): $rx $ry $rz.
  (func $qrot (param $q i32) (param $x f32) (param $y f32) (param $z f32)
    (local $qx f32) (local $qy f32) (local $qz f32) (local $qw f32)
    (local $tx f32) (local $ty f32) (local $tz f32)
    (local.set $qx (f32.load (local.get $q)))
    (local.set $qy (f32.load offset=4 (local.get $q)))
    (local.set $qz (f32.load offset=8 (local.get $q)))
    (local.set $qw (f32.load offset=12 (local.get $q)))
    ;; t = 2 (q.xyz x v)
    (local.set $tx (f32.mul (f32.const 2)
      (f32.sub (f32.mul (local.get $qy) (local.get $z)) (f32.mul (local.get $qz) (local.get $y)))))
    (local.set $ty (f32.mul (f32.const 2)
      (f32.sub (f32.mul (local.get $qz) (local.get $x)) (f32.mul (local.get $qx) (local.get $z)))))
    (local.set $tz (f32.mul (f32.const 2)
      (f32.sub (f32.mul (local.get $qx) (local.get $y)) (f32.mul (local.get $qy) (local.get $x)))))
    ;; v + w t + q.xyz x t
    (global.set $rx (f32.add (f32.add (local.get $x) (f32.mul (local.get $qw) (local.get $tx)))
      (f32.sub (f32.mul (local.get $qy) (local.get $tz)) (f32.mul (local.get $qz) (local.get $ty)))))
    (global.set $ry (f32.add (f32.add (local.get $y) (f32.mul (local.get $qw) (local.get $ty)))
      (f32.sub (f32.mul (local.get $qz) (local.get $tx)) (f32.mul (local.get $qx) (local.get $tz)))))
    (global.set $rz (f32.add (f32.add (local.get $z) (f32.mul (local.get $qw) (local.get $tz)))
      (f32.sub (f32.mul (local.get $qx) (local.get $ty)) (f32.mul (local.get $qy) (local.get $tx))))))

  ;; The record of the player `id` this frame, 0 when not listed.
  (func $find_player (param $id f32) (result i32)
    (local $i i32) (local $at i32)
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (global.get $players_n)))
        (local.set $at (i32.add (i32.const 1024) (i32.shl (local.get $i) (i32.const 6))))
        (if (f32.eq (f32.load (local.get $at)) (local.get $id))
          (then (return (local.get $at))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (i32.const 0))

  (func $slot_at (param $i i32) (result i32)
    (i32.add (i32.const 20480) (i32.shl (local.get $i) (i32.const 8))))

  ;; The ragdoll slot holding player `id`, else -1.
  (func $slot_of (param $id f32) (result i32)
    (local $i i32) (local $at i32)
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 24)))
        (local.set $at (call $slot_at (local.get $i)))
        (if (i32.and
              (i32.ne (i32.load offset=4 (local.get $at)) (i32.const 0))
              (f32.eq (f32.load (local.get $at)) (local.get $id)))
          (then (return (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (i32.const -1))

  ;; Which of the slot's parts moves with `node`, else -1.
  (func $part_of (param $slot i32) (param $node i32) (result i32)
    (local $i i32) (local $n i32)
    (local.set $n (i32.load offset=8 (local.get $slot)))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (local.get $n)))
        (if (i32.eq
              (i32.load offset=32 (i32.add (local.get $slot) (i32.shl (local.get $i) (i32.const 3))))
              (local.get $node))
          (then (return (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (i32.const -1))

  ;; Take every body out of the world.
  (func $drop_bodies (param $slot i32)
    (local $i i32)
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.load offset=8 (local.get $slot))))
        (call $rigid_remove
          (i32.load offset=36 (i32.add (local.get $slot) (i32.shl (local.get $i) (i32.const 3)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (i32.store offset=8 (local.get $slot) (i32.const 0)))

  ;; Add (x, y, z) to every limb's velocity.
  (func $push_all (param $slot i32) (param $x f32) (param $y f32) (param $z f32)
    (local $i i32)
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.load offset=8 (local.get $slot))))
        (call $rigid_push
          (i32.load offset=36 (i32.add (local.get $slot) (i32.shl (local.get $i) (i32.const 3))))
          (local.get $x) (local.get $y) (local.get $z))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each))))

  ;; Build the ragdoll of the player whose record is `rec` in slot `index`,
  ;; from their body as drawn (skeleton records at 8192, `nodes` of them).
  (func $build (param $index i32) (param $rec i32) (param $nodes i32)
    (local $slot i32) (local $player i32) (local $p i32) (local $node i32)
    (local $s i32) (local $body i32) (local $part i32) (local $n i32) (local $guard i32)
    (local $parent i32) (local $root i32) (local $limits i32)
    (local $hx f32) (local $hy f32) (local $hz f32)
    (local $ox f32) (local $oy f32) (local $oz f32)
    (local $vx f32) (local $vy f32) (local $vz f32)
    (local.set $slot (call $slot_at (local.get $index)))
    (local.set $player (i32.trunc_f32_u (f32.load (local.get $rec))))
    (f32.store (local.get $slot) (f32.load (local.get $rec)))
    (i32.store offset=4 (local.get $slot) (i32.const 1))
    (i32.store offset=8 (local.get $slot) (i32.const 0))
    (i32.store offset=28 (local.get $slot) (i32.const -1))
    ;; The corpse's velocity, remembered to tell a blast from a fall.
    (f32.store offset=16 (local.get $slot) (f32.load offset=44 (local.get $rec)))
    (f32.store offset=20 (local.get $slot) (f32.load offset=48 (local.get $rec)))
    (f32.store offset=24 (local.get $slot) (f32.load offset=52 (local.get $rec)))
    (f32.store offset=12 (local.get $slot) (f32.sqrt (f32.add (f32.add
      (f32.mul (f32.load offset=44 (local.get $rec)) (f32.load offset=44 (local.get $rec)))
      (f32.mul (f32.load offset=48 (local.get $rec)) (f32.load offset=48 (local.get $rec))))
      (f32.mul (f32.load offset=52 (local.get $rec)) (f32.load offset=52 (local.get $rec))))))
    ;; Every limb starts with the corpse's motion and the same little pop:
    ;; up, and a random way sideways.
    (local.set $vx (f32.add (f32.load offset=44 (local.get $rec))
      (f32.mul (f32.sub (call $rand) (f32.const 0.5)) (f32.const 4))))
    (local.set $vy (f32.add (f32.load offset=48 (local.get $rec)) (f32.const 3.5)))
    (local.set $vz (f32.add (f32.load offset=52 (local.get $rec))
      (f32.mul (f32.sub (call $rand) (f32.const 0.5)) (f32.const 4))))
    (local.set $p (i32.const 0))
    (block $parts_done
      (loop $parts
        (br_if $parts_done (i32.ge_s (local.get $p) (i32.const 9)))
        (local.set $node (call $skeleton_part (local.get $player)
          (i32.add (i32.const 256) (i32.shl (local.get $p) (i32.const 4)))
          (i32.load (i32.add (i32.const 512) (i32.shl (local.get $p) (i32.const 2))))))
        (block $skip
          (br_if $skip (i32.lt_s (local.get $node) (i32.const 0)))
          (br_if $skip (i32.ge_s (local.get $node) (local.get $nodes)))
          ;; Two parts on one node are one body.
          (br_if $skip (i32.ge_s (call $part_of (local.get $slot) (local.get $node)) (i32.const 0)))
          (local.set $s (i32.add (i32.const 8192) (i32.shl (local.get $node) (i32.const 6))))
          ;; A box round what is drawn on the node, a touch smaller so
          ;; neighbours start apart; a small one where nothing is drawn.
          (if (f32.ne (f32.load offset=4 (local.get $s)) (f32.const 0))
            (then
              (local.set $hx (f32.mul (f32.const 0.46)
                (f32.sub (f32.load offset=48 (local.get $s)) (f32.load offset=36 (local.get $s)))))
              (local.set $hy (f32.mul (f32.const 0.46)
                (f32.sub (f32.load offset=52 (local.get $s)) (f32.load offset=40 (local.get $s)))))
              (local.set $hz (f32.mul (f32.const 0.46)
                (f32.sub (f32.load offset=56 (local.get $s)) (f32.load offset=44 (local.get $s)))))
              (local.set $ox (f32.mul (f32.const 0.5)
                (f32.add (f32.load offset=48 (local.get $s)) (f32.load offset=36 (local.get $s)))))
              (local.set $oy (f32.mul (f32.const 0.5)
                (f32.add (f32.load offset=52 (local.get $s)) (f32.load offset=40 (local.get $s)))))
              (local.set $oz (f32.mul (f32.const 0.5)
                (f32.add (f32.load offset=56 (local.get $s)) (f32.load offset=44 (local.get $s))))))
            (else
              (local.set $hx (f32.const 0.12)) (local.set $hy (f32.const 0.12))
              (local.set $hz (f32.const 0.12))
              (local.set $ox (f32.const 0)) (local.set $oy (f32.const 0))
              (local.set $oz (f32.const 0))))
          (local.set $hx (f32.min (f32.max (local.get $hx) (f32.const 0.04)) (f32.const 8)))
          (local.set $hy (f32.min (f32.max (local.get $hy) (f32.const 0.04)) (f32.const 8)))
          (local.set $hz (f32.min (f32.max (local.get $hz) (f32.const 0.04)) (f32.const 8)))
          ;; The body's frame is the node's, so its pose is the node's.
          (f32.store (i32.const 16384) (f32.const 0))
          (f32.store offset=4 (i32.const 16384) (local.get $hx))
          (f32.store offset=8 (i32.const 16384) (local.get $hy))
          (f32.store offset=12 (i32.const 16384) (local.get $hz))
          (f32.store offset=16 (i32.const 16384) (local.get $ox))
          (f32.store offset=20 (i32.const 16384) (local.get $oy))
          (f32.store offset=24 (i32.const 16384) (local.get $oz))
          (memory.copy (i32.const 16412) (i32.add (local.get $s) (i32.const 8)) (i32.const 28))
          (f32.store offset=56 (i32.const 16384) (local.get $vx))
          (f32.store offset=60 (i32.const 16384) (local.get $vy))
          (f32.store offset=64 (i32.const 16384) (local.get $vz))
          ;; A tumble, strongest in the torso.
          (f32.store offset=68 (i32.const 16384)
            (f32.mul (f32.sub (call $rand) (f32.const 0.5)) (f32.const 6)))
          (f32.store offset=72 (i32.const 16384)
            (f32.mul (f32.sub (call $rand) (f32.const 0.5)) (f32.const 3)))
          (f32.store offset=76 (i32.const 16384)
            (f32.mul (f32.sub (call $rand) (f32.const 0.5)) (f32.const 6)))
          (f32.store offset=80 (i32.const 16384) (f32.const 1))     ;; density
          (f32.store offset=84 (i32.const 16384) (f32.const 0.8))   ;; friction
          (f32.store offset=88 (i32.const 16384) (f32.const 0.15))  ;; bounce
          (f32.store offset=92 (i32.const 16384)                    ;; group
            (f32.convert_i32_s (i32.add (local.get $index) (i32.const 1))))
          (f32.store offset=96 (i32.const 16384) (f32.const 0.15))  ;; damping
          (f32.store offset=100 (i32.const 16384) (f32.const 0.8))
          (f32.store offset=104 (i32.const 16384) (f32.const 1))    ;; shared
          (f32.store offset=108 (i32.const 16384) (f32.const 0))
          (local.set $body (call $rigid_create (i32.const 16384)))
          (local.set $part (i32.load offset=8 (local.get $slot)))
          (i32.store offset=32 (i32.add (local.get $slot) (i32.shl (local.get $part) (i32.const 3)))
            (local.get $node))
          (i32.store offset=36 (i32.add (local.get $slot) (i32.shl (local.get $part) (i32.const 3)))
            (local.get $body))
          (i32.store offset=8 (local.get $slot) (i32.add (local.get $part) (i32.const 1)))
          ;; Joined to the nearest part above it; one with none above is
          ;; the root, or hangs from it when there already is one.
          (local.set $parent (i32.const -1))
          (local.set $n (i32.trunc_f32_s (f32.load (local.get $s))))
          (local.set $guard (i32.const 0))
          (block $found
            (loop $up
              (br_if $found (i32.lt_s (local.get $n) (i32.const 0)))
              (br_if $found (i32.ge_s (local.get $n) (local.get $nodes)))
              (br_if $found (i32.gt_s (local.get $guard) (i32.const 128)))
              (local.set $parent (call $part_of (local.get $slot) (local.get $n)))
              (br_if $found (i32.ge_s (local.get $parent) (i32.const 0)))
              (local.set $n (i32.trunc_f32_s
                (f32.load (i32.add (i32.const 8192) (i32.shl (local.get $n) (i32.const 6))))))
              (local.set $guard (i32.add (local.get $guard) (i32.const 1)))
              (br $up)))
          (local.set $root (i32.load offset=28 (local.get $slot)))
          (if (i32.lt_s (local.get $parent) (i32.const 0))
            (then
              (if (i32.lt_s (local.get $root) (i32.const 0))
                (then
                  (i32.store offset=28 (local.get $slot) (local.get $part))
                  (br $skip)))
              (local.set $parent (local.get $root))))
          ;; The joint: at this node's pivot, twisting along the limb.
          (memory.copy (i32.const 16512) (i32.add (local.get $s) (i32.const 8)) (i32.const 12))
          (call $qrot (i32.add (local.get $s) (i32.const 20))
            (local.get $ox) (local.get $oy) (local.get $oz))
          (f32.store offset=12 (i32.const 16512) (global.get $rx))
          (f32.store offset=16 (i32.const 16512) (global.get $ry))
          (f32.store offset=20 (i32.const 16512) (global.get $rz))
          (local.set $limits (i32.add (i32.const 576) (i32.mul (local.get $p) (i32.const 12))))
          (memory.copy (i32.const 16536) (local.get $limits) (i32.const 12))
          (drop (call $rigid_joint
            (i32.load offset=36 (i32.add (local.get $slot) (i32.shl (local.get $parent) (i32.const 3))))
            (local.get $body)
            (i32.const 16512))))
        (local.set $p (i32.add (local.get $p) (i32.const 1)))
        (br $parts)))
    ;; A model with none of the parts stays as the game animates it.
    (if (i32.eqz (i32.load offset=8 (local.get $slot)))
      (then (i32.store offset=4 (local.get $slot) (i32.const 2)))))

  ;; Draw the ragdoll in slot `slot` as its bodies lie, and keep it with
  ;; its corpse (the player record `rec`).
  (func $update (param $slot i32) (param $rec i32) (param $dt f32)
    (local $i i32) (local $k i32) (local $at i32) (local $root i32)
    (local $rx f32) (local $ry f32) (local $rz f32) (local $have i32)
    (local $vx f32) (local $vy f32) (local $vz f32) (local $speed f32)
    (local $dx f32) (local $dy f32) (local $dz f32) (local $d f32) (local $pull f32)
    (local.set $root (i32.load offset=28 (local.get $slot)))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.load offset=8 (local.get $slot))))
        (local.set $at (i32.add (local.get $slot) (i32.shl (local.get $i) (i32.const 3))))
        (if (i32.and
              (call $rigid_get (i32.load offset=36 (local.get $at)) (i32.const 16576))
              (i32.lt_s (local.get $k) (i32.const 16)))
          (then
            (f32.store (i32.add (i32.const 17408) (i32.shl (local.get $k) (i32.const 5)))
              (f32.convert_i32_s (i32.load offset=32 (local.get $at))))
            (memory.copy (i32.add (i32.const 17412) (i32.shl (local.get $k) (i32.const 5)))
              (i32.const 16576) (i32.const 28))
            (if (i32.eq (local.get $i) (local.get $root))
              (then
                (local.set $have (i32.const 1))
                (local.set $rx (f32.load (i32.const 16576)))
                (local.set $ry (f32.load offset=4 (i32.const 16576)))
                (local.set $rz (f32.load offset=8 (i32.const 16576)))))
            (local.set $k (i32.add (local.get $k) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (if (i32.gt_s (local.get $k) (i32.const 0))
      (then (drop (call $pose
        (i32.trunc_f32_u (f32.load (local.get $slot))) (i32.const 17408) (local.get $k)))))
    ;; A sudden gain in the corpse's speed is a blast or a throw: every
    ;; limb gets it. Slowing down (landing) is the ragdoll's own business.
    (local.set $vx (f32.load offset=44 (local.get $rec)))
    (local.set $vy (f32.load offset=48 (local.get $rec)))
    (local.set $vz (f32.load offset=52 (local.get $rec)))
    (local.set $speed (f32.sqrt (f32.add (f32.add
      (f32.mul (local.get $vx) (local.get $vx)) (f32.mul (local.get $vy) (local.get $vy)))
      (f32.mul (local.get $vz) (local.get $vz)))))
    (if (f32.gt (local.get $speed) (f32.add (f32.load offset=12 (local.get $slot)) (f32.const 4)))
      (then (call $push_all (local.get $slot)
        (f32.sub (local.get $vx) (f32.load offset=16 (local.get $slot)))
        (f32.sub (local.get $vy) (f32.load offset=20 (local.get $slot)))
        (f32.sub (local.get $vz) (f32.load offset=24 (local.get $slot))))))
    (f32.store offset=12 (local.get $slot) (local.get $speed))
    (f32.store offset=16 (local.get $slot) (local.get $vx))
    (f32.store offset=20 (local.get $slot) (local.get $vy))
    (f32.store offset=24 (local.get $slot) (local.get $vz))
    (if (i32.eqz (local.get $have)) (then (return)))
    ;; Kept near its corpse: further than 2.5 units from it, the ragdoll
    ;; is drawn back, harder the further it strayed.
    (local.set $dx (f32.sub (f32.load offset=8 (local.get $rec)) (local.get $rx)))
    (local.set $dy (f32.sub (f32.add (f32.load offset=12 (local.get $rec)) (f32.const 1))
      (local.get $ry)))
    (local.set $dz (f32.sub (f32.load offset=16 (local.get $rec)) (local.get $rz)))
    ;; Fallen out of the world: it gives up and the corpse shows as the
    ;; game animates it.
    (if (f32.gt (local.get $dy) (f32.const 60))
      (then
        (call $drop_bodies (local.get $slot))
        (i32.store offset=4 (local.get $slot) (i32.const 2))
        (return)))
    (local.set $d (f32.sqrt (f32.add (f32.add
      (f32.mul (local.get $dx) (local.get $dx)) (f32.mul (local.get $dy) (local.get $dy)))
      (f32.mul (local.get $dz) (local.get $dz)))))
    (if (f32.gt (local.get $d) (f32.const 2.5))
      (then
        (local.set $pull (f32.div
          (f32.mul (f32.mul (f32.sub (local.get $d) (f32.const 2.5)) (f32.const 6)) (local.get $dt))
          (local.get $d)))
        (call $push_all (local.get $slot)
          (f32.mul (local.get $dx) (local.get $pull))
          (f32.mul (local.get $dy) (local.get $pull))
          (f32.mul (local.get $dz) (local.get $pull))))))

  (func (export "frame") (param $time f32) (param $dt f32)
    (local $i i32) (local $rec i32) (local $slot i32) (local $free i32) (local $nodes i32)
    (local $id f32)
    (global.set $players_n (call $players (i32.const 1024) (i32.const 64)))
    ;; Ragdolls whose player respawned, left, or whose corpse is gone.
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 24)))
        (local.set $slot (call $slot_at (local.get $i)))
        (if (i32.ne (i32.load offset=4 (local.get $slot)) (i32.const 0))
          (then
            (local.set $id (f32.load (local.get $slot)))
            (local.set $rec (call $find_player (local.get $id)))
            (if (i32.or
                  (i32.or (i32.eqz (local.get $rec))
                    (i32.ne (i32.and (i32.trunc_f32_u (f32.load offset=4 (local.get $rec)))
                      (i32.const 2)) (i32.const 0)))
                  (i32.lt_s (call $skeleton (i32.trunc_f32_u (local.get $id)) (i32.const 8192)
                    (i32.const 0)) (i32.const 0)))
              (then
                (call $drop_bodies (local.get $slot))
                (i32.store offset=4 (local.get $slot) (i32.const 0)))
              (else
                (if (i32.eq (i32.load offset=4 (local.get $slot)) (i32.const 1))
                  (then (call $update (local.get $slot) (local.get $rec) (local.get $dt))))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    ;; New deaths.
    (local.set $i (i32.const 0))
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (global.get $players_n)))
        (local.set $rec (i32.add (i32.const 1024) (i32.shl (local.get $i) (i32.const 6))))
        (block $next
          (br_if $next (i32.ne (i32.and (i32.trunc_f32_u (f32.load offset=4 (local.get $rec)))
            (i32.const 2)) (i32.const 0)))
          (br_if $next (i32.ge_s (call $slot_of (f32.load (local.get $rec))) (i32.const 0)))
          (local.set $free (call $slot_of_free))
          ;; Every slot busy: this one dies the v20 way.
          (br_if $next (i32.lt_s (local.get $free) (i32.const 0)))
          (local.set $nodes (call $skeleton (i32.trunc_f32_u (f32.load (local.get $rec)))
            (i32.const 8192) (i32.const 128)))
          (br_if $next (i32.le_s (local.get $nodes) (i32.const 0)))
          (call $build (local.get $free) (local.get $rec)
            (select (local.get $nodes) (i32.const 128)
              (i32.lt_s (local.get $nodes) (i32.const 128)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each))))

  ;; A free ragdoll slot, else -1.
  (func $slot_of_free (result i32)
    (local $i i32)
    (block $done
      (loop $each
        (br_if $done (i32.ge_s (local.get $i) (i32.const 24)))
        (if (i32.eqz (i32.load offset=4 (call $slot_at (local.get $i))))
          (then (return (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $each)))
    (i32.const -1))
)
