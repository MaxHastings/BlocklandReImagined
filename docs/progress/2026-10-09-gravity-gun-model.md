# 2026-10-09 The Gravity Gun gets its own model, effects and sounds

Max asked for a Gravity Gun model of our own in place of the Printer,
built in Blender, blocky like the game, with the concept art as direction
rather than a spec: a dark chunky shell, cyan glowing seams, a glowing core
and a yellow grip. He turned down the three-prong front for the concept's
C-claw. He approved the preview renders ("looks good to me. lets add it to
the game.") and this branch wires it in. It is not merged to main here: the
"Review and merge to main" thread takes it into the preview build.

## What changed

- `tools/make_gravity_gun_model.py` (new) builds the gun in Blender's
  Python (`bpy`, headless) and writes
  `packages/showcase/gravity-gun-tool/assets/models/gravity-gun.shape.json`
  plus one flat 4x4 PNG per material. It is deterministic (the same
  sha256 every run). It has about 2,900 triangles. At `SCALE` 0.72 it is
  1.78 units long, 1.21 tall with its grip and 0.5 wide. Max asked for it
  not to be as small as the Printer, sized against the stock weapons. This
  container has no v20 content, so third-party packs built to stock scale
  (in the project files) were measured with `bri_convert::shape::read_dts`
  at their largest visible detail:
  - Heavy Pistol 1.12 long
  - SMGs (MP40, Heavy SMG) 1.33 to 1.45 long
  - Combat MG 1.78 long, 1.01 tall
  - Launchers (Strike Launcher, APG) 1.46 to 1.88 long
  - Rifles (M1 Garand, BAR, Combat Rifle) 2.0 to 2.3 long

  The gun sits between an SMG and a rifle, launcher-sized, as a heavy
  two-handed-looking tool should.
  - Chunky bevelled blocks: a receiver, an octagonal barrel with glowing
    windows, collar rings round a glowing core, a C-claw of two arms, and
    a raked grip with yellow finger pads, backstrap and trigger.
  - Every glowing line is a groove cut into the shell (an exact boolean
    whose cut faces take the glow material), so the lines sit in the
    surface at any distance.
  - Nodes: `mountPoint` at the grip, `muzzlePoint` on the core's face,
    and `arm_top` and `arm_bottom` hinges. `open` (0.18 s, with a little
    overshoot) and `close` (0.12 s) swing the claw 16 degrees each way.
  - Each face carries its material's slot in u and how far along the gun
    it is in v. The skin uses these to know what it is drawing.
  - `--render DIR` renders preview sheets and a first-person view at the
    image's `eye_offset` and `eye_rotation`. `--blend FILE` saves the
    scene for editing. `--icon` renders the fallback item icon.
- The new skin `skins/gravity.wgsl` replaces `alien.wgsl` (deleted). It
  draws only the glowing faces:
  - Pulses run along the seams toward the claw. The core breathes. The
    violet vents blink.
  - While the trigger grabs (`energy_states: ["Grab"]`), everything runs
    faster and burns whiter.
  - The shell and grip keep the game's own lighting from the model. The
    claw's seams are a separate slot the skin leaves alone, because a skin
    draws at rest pose and the claw moves.
- `weapons.json`, for both the item and the image:
  - The model is `models/gravity-gun.shape.json`, with its own colours
    (`color_shift` off).
  - `eye_offset` [0.85, -1.0, -1.6] and `eye_rotation` [-2, -8, -6],
    tuned from first-person renders. The muzzle is turned a little toward
    the crosshair so the holder sees the claw, the core and the side
    seams, not just the back plate.
  - A cyan image light (radius 4).
  - The Grab state plays `open` and the Release state plays `close`.
- The icon: `gravity_gun.render.json` now draws the model's own textures
  (`"textured": true`) in the Printer's pose. The shipped
  `icons/gravity_gun.png` (shown until that drawing is ready) is now a
  Cycles render of the model. It replaces `tools/make_showcase_icons.py`'s
  ray-marched stand-in, which is deleted.
- Effects (`gravity-gun-fx`):
  - At rest, the armed gun's core glows softly and motes drift into it.
    The client now tells the Gravity Gun from other held images by its
    `image_kind`, so a hammer gets none of this.
  - On a catch, a ring and a burst of sparks fly out along the beam.
  - While holding, motes rise through the held thing.
  - The beam has two strands twisting down it, and the bubble has two
    turning rings.
  - Sparks are now small turning squares (blocky like bricks). Only the
    orb is round.
- Sounds (`tools/make_showcase_sounds.py`):
  - `grab.wav` gains the claw's two-knock clack as it opens, and
    `drop.wav` gains a snap shut.
  - New `hold.wav` is a seamless one-second throbbing hum, repeated while
    holding.
  - `reach.wav` is byte-identical.

## Evidence

- Preview sheets, a first-person view and the `.blend` were shared with
  Max in the thread (`/mnt/project-files/gravity-gun-model/opus/`).
- `cargo test -p bri-client-sandbox --test showcase -- --include-ignored`:
  9/9 passed on lavapipe. Draw counts were updated for the idle glow,
  ring, burst and motes.
- `cargo test -p bri-client --lib -- items:: item_icon_render`: 28
  passed (9 ignored need v20 content). This includes the synthetic run
  of the icon drawn from the model in the Printer's pose.
- `cargo test -p bri-sim --test showcase`: 33 passed.
  `cargo test -p bri-net --test showcase`: 1 passed.
  `cargo test -p bri-package-runtime`: all passed.
- `cargo clippy -p bri-client -p bri-client-sandbox --tests -- -D warnings`
  and `cargo fmt --all -- --check` are clean.
- `bri-addon-check` reports OK for `gravity-gun-tool` and `gravity-gun-fx`.
- Not run here: the full gate and the v20-content variants. The icon's
  real Printer pose with the `clockwise_quarter_turns` it already had is
  unverified, so check the tool-slot icon in the playtest.

## Open

- Max's playtest: hand fit in third person, the first-person placement,
  the claw opening and closing, the effects and the hum.
- The size was judged from third-party weapons at stock scale, not the
  stock Rocket Launcher, Gun, Bow, Sword or Printer themselves. If it
  reads big or small in the hand, change `SCALE` (and `EYE_OFFSET`) in
  the tool and rerun it.
- The drawn icon was checked only against the synthetic Printer
  stand-in. The real Printer pose is checked by the ignored `content`
  variant of `the_gravity_gun_icon_is_drawn_from_its_model_like_the_printers`
  in the push gate.
