# 2026-10-13 v0.3.0 W6: HUD regions and per-body cameras

Branch `claude/hud-regions-camera-vuyw6r`, into `v0.3.0-experimental`,
stacked on W2 (#109). W6 of the v0.3.0 plan: switching between a Blockhead,
a Mario-like body and a Halo-like body should look and feel like three
games, with the Add-Ons owning the feel and the engine adding no friction
and no per-franchise code.

## What changed

- **HUD regions** (`crates/content/src/hud.rs`). A `hud` file with a
  `"region"` claims one of twelve named boxes on a 480-unit-tall screen
  (corners, edges, `crosshair`, `energy`, `center_print`, `bottom_print`)
  and draws textured elements from its own Add-On's PNGs: images, frame
  strips picked by a value, bars, and text in the game font or a sprite
  font. Values are health, energy, alive, the prints, or a rule's public
  state. `when` limits a claim to a body, a held item or a vehicle seat,
  so the HUD follows the body through a switch. Claiming the last four
  regions hides that stock element; unclaimed regions show the stock HUD.
- **Conflict rule.** One claim per region draws: most specific condition
  (vehicle 8, holding 4, body 2, summed), then `priority`, then package id.
  The loser is named once in Add-On problems (`add_on_health`).
- **Client side** (`crates/client/src/hud_regions.rs`, `ui/screens/play.rs`).
  Layouts come from the catalog the game runs (the server's on a join);
  textures upload once per catalog; a layout reading a rule the server
  does not run is skipped. Pixelated layouts draw with nearest filtering.
- **Client code** gets `hud_region(name, ptr)` (`render.layer`): the box in
  screen space, the unit and whether this Add-On owns it now. The claim is
  still a layout (possibly empty), so the conflict rule covers code too.
- **Camera profiles** (`crates/content/src/camera.rs`, archetype
  `look.camera`, wire note `archetype-camera-profile.md`): `fov` (scaled by
  the player's own FOV), first-person `eye_offset`, view `bob`, held-weapon
  `sway` (trail, settle, bob) and a third-person shoulder pivot. Ownership
  order: aim/zoom, then a rule's `set_fov`, then the body, then stock. A
  vehicle seat, the observer camera and a body switch take over. A body
  with no profile is the stock camera to the bit: the offsets are skipped
  when zero (`view.rs` `moved`) and the sway matrix is only applied when
  present. The `view` record carries the sway so client-drawn view models
  match.
- **Playground**: `playground-runner-hud` (power meter with nine frames,
  cap and sprite-font health counter, gold reticle; art from
  `tools/make_playground_assets.py`), and `sample-trooper` gets a heavy
  shooter's profile (FOV 72, heavier bob, weapon lag, right-shoulder chase
  camera) and joins the Playground mode (`/trooper`, `/blockhead`).

## Decisions

- Eye height stays the W2 `eye` socket; `camera.eye_offset` moves only the
  camera, so aim and shots do not shift with a cosmetic offset.
- Third-person distance stays `camera_distance`; the profile adds only the
  pivot.
- Body FOV is a ratio of the player's own setting rather than an absolute,
  so a player who needs a wide FOV keeps it while the body still feels
  narrower or wider.
- Region boxes are fixed rather than free placement, so two Add-Ons can
  meet on a name and the conflict rule means something.

## Evidence

- `cargo test -p bri-client --test hud_regions_render` (lavapipe): Runner
  HUD claims top_center, top_left and crosshair; meter pixels read hub
  (250, 250, 240), wedge (70, 150, 255), rim (230, 180, 40). `/trooper`:
  no regions, FOV 72/90 of stock, bob 0.050 while walking, weapon trailed,
  shoulder pivot. `/blockhead`: stock FOV, zero offsets, no sway.
- `cargo test -p bri-content -p bri-package-runtime -p bri-client-sandbox
  --lib --tests`, `-p bri-client --lib` (524 passed), `-p bri-ui --lib`,
  `-p bri-sim --test playground`, `-p bri-net --test playground`,
  `-p bri-client --test body_attachments_render`: all pass.
- `cargo clippy` on the changed crates with `-D warnings`: clean.

## Next

- An ammo value for HUDs needs a replicated magazine count (today the
  magazine reaches clients only as the bottom print).
- The vehicle chase camera belongs to W8.
- Max to playtest: Playground README step 7.
