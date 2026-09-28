# Native client integration

This is a development client, not the complete vanilla alpha handoff. The
previous UI milestone is now connected to native content, an authoritative
loopback/LAN host, QUIC client state and a persistent world renderer.

## Application and transport

`bri-client` owns native settings, content configuration, controls, asynchronous
session loading and request dispatch. Its platform adapter owns winit, the
wgpu device/surface and UI compositing. No game window is created by tests or by
running the executable without `--run`.

Hosting from the typed UI action loads any of the 14 reference map architectures on a
background worker and starts the existing 120 Hz authoritative server. Solo
binds loopback with one player; LAN binds port 28000 and enforces the selected
player limit. Password fields are rejected explicitly until authentication is
implemented. Host/admin privileges, discovery, durable identity and reconnect
are still required. Current local hosting does not silently grant administrator
status to a network peer.

Movement intentions travel at 60 Hz while reliable requests and replies remain
independently dispatchable. The bridge bounds request queues, write deadlines,
connection preparation and concurrent content jobs. Pose updates retain the
shared world allocation when no bricks changed. Cancellation is tied to UI
session request IDs; late jobs cannot attach to another session. Disconnect also
cancels queued world builds and releases CPU/GPU world resources.

Walking, held jumping/crouching/jetting, held zoom, free look and camera rotation
are connected. Free look does not rotate the player's movement frame. Chat
round-trips through the real server; UI markup/control markers in names and chat
are treated as plain text. Original Blockheads now render from authoritative
poses, with native outfit/material selection and initial movement/look layers.
Client prediction, remote interpolation and animation transitions/tool/emote
selection remain work. The camera uses authoritative pose updates;
third person sweeps a sphere up to eight units behind the eye against native map
geometry and replicated brick collision shapes. Collision respects the brick's
collision flag independently of visibility and targeting. Dynamic actors,
vehicle obstruction, original offsets and camera smoothing remain work.
Spawn-sphere distribution/orientation and exact FOV feel
also require fidelity work; the initial Bedroom center faces a nearby wall.

LAN hosts advertise a listing and their public QUIC certificate over UDP
discovery (port 28050). Opening Join Server lists those and probes every saved
server (`servers.json`: favourites, then the last ten joins) over its game
port for name, map, players and ping; the Favorite button (v20's unused Query
Internet) stars the selected server. Connect to IP accepts an IP, a host name
with an optional port, or a `bri://` invite, and needs only the game port: a
first join accepts the certificate the host presents during the QUIC handshake
(its signature is still verified) and pins it in `trusted-hosts.json` (trust on
first use); an invite's key is checked instead. A pin that no longer matches is
reported as a changed identity and forgotten, so joining again trusts the new
one. Internet hosts open the game port on the router, check whether friends can
reach them and put an invite on the clipboard (`/invite` copies it again); LAN
and Internet hosts on Windows are offered a one-prompt Windows Firewall fix when
friends would be blocked. Details: `docs/architecture/hosting.md`. LAN hosts keep a persistent certificate/key pair in their state
directory so pins stay valid across restarts; single-player hosts use a
throwaway certificate. There is no insecure certificate fallback.

## Rendering and content

The map renderer shares the device, encoder and attachment with the native UI.
It uploads map meshes/materials once and updates a camera uniform thereafter.
Original interiors use diffuse/baked lighting; terrain retains eight texture
layers and both weight maps. Map and world-brick meshes have separate GPU
lifetimes. Opaque brick batches are coalesced; transparent batches retain
ordering. Replicated brick state drives native geometry, paint, visibility and
rotation through background mesh builds, including after joining a populated
world. The current four-million-triangle replacement budget rejects excessive
worlds explicitly. Bricks are meshed in 32-unit chunks (`world_chunks`) against
one shared material palette; a replica change rebuilds only the chunks it
touches, and chunks are frustum culled.

Scene textures are mipmapped (lightmaps and weight maps bind their base level
only) and follow v20's Trilinear, Sharp Filter and Anisotropy prefs. The world
pass uses 4x MSAA unless Anti-Aliasing is off. Cascaded sun shadows
(`bri_render::shadow`, Shadow Quality; Minimum is off) are cast by players,
vehicles and held/dropped items, like v20's projected shape shadows, and by
bricks only with Brick Shadows. Bricks that do not cast, interiors and terrain
render into a separate occluder depth map: a shadow is dropped wherever an
occluder lies between the caster and the receiving surface, so a player on a
brick tower shades the tower top and not the floor beneath it
(`crates/render/tests/shadow_occluders.rs`). Baked surfaces darken by a
bounded share of the mission's ambient/sun ratio; vertex-lit surfaces lose the
sun term.

The renderer now draws original sky faces and moving cloud layers with distance
fog for all 14 reference maps. Sky orientation/depth/translation and cloud
motion have offscreen tests; full environment fidelity is not accepted yet.
The renderer records remaining omissions: fog volumes/storm transitions,
water/snow, decorations, material detail, terrain holes/streaming and several
map-specific objects. Secondary lighting-cache provenance and the complete map
set are recorded in `vanilla-reference.md`.
The current terrain patch is `[-64, -64, 384, 384]` cells. Brick meshes bind the
five original top/side/bottom/ramp overlays and all 77 converted stock prints,
including original BLS print-name aliases. Texture alpha controls pigment
coverage separately from brick opacity. Signed paint-offset colors are adapted;
transparent-paint interaction and pumpkin literal RGB remain explicit fidelity
questions. Color/shape FX, lights and particles still need rendering. These
omissions remain mandatory alpha work.

The client content index validates package paths and reads the 35-world report
without reading all save payloads at startup. Map/simulation loading is lazy.
It supplies 166 stock brick choices, the recovered stock 36-color palette,
13 lights and 102 emitter choices. A joined world's palette retains its native
indices. Merely exposing a catalog is not implementation of its gameplay.

The avatar package verifies the native rig and 63 byte-preserved original PNGs
(27 faces, 28 decals and eight surface images). It resolves stock part choices,
hat/accent restrictions, skirt trims/leg colors and pack head poses. Original
surface masks and face/decal alpha overlay pigment on painted geometry; accent
transparency uses separate blend bindings. Outfit changes use server-validated
reliable replication, late join and same-process resume. First person hides the
local body; other players and the local third-person body use persistent buffers.
Movement animation is an initial state adapter, not accepted v20 timing parity.

The avatar editor receives a native 3D preview with a separate camera uniform,
matching portrait aspect and authored lighting/FOV fields. A transparent render
target retains the original UI backdrop. Preview requests never publish an outfit;
Done follows the existing acknowledgment/settings workflow. Exact orbit/FOV
interpretation and transparent preview compositing still need fidelity acceptance.

The client now connects ten brick slots/favorites and Hammer/Wrench/Printer tool
slots to a local query mirror, body-facing ghost deployment/shifts/rotation,
planting and server-owned tool commands. Ghost geometry remains separate from
accepted world state. Server rejection does not fabricate a placed brick.
Replicated collision queries update only when brick state changes, not on every
pose packet. The ghost has a translucent original-material preview.

Ordinary wrench properties, print selection and the implemented brick-event
subset use server inspections and reject stale edits. Catalog IDs, print aspects,
target reach and ownership are checked by the server. Opaque imported records
remain read-only and server-owned. Ctrl+Z follows v20's per-client undo queue
(511 entries): plants break like hammered bricks, and spray paint, FX paint and
prints revert. Sound/vehicle/item wrench behaviors, additional events and other
unfinished adapters still report explicit errors.

Remaining building fidelity includes exact ghost re-centering/snapping, the
terrain-only placement offset, projectile tool-flight timing,
held tools/animations/audio and complete trust/minigame/equipment rules. Reliable
actions capture body aim when dispatched, independently of movement datagrams.
The server validates this aim and still owns position, reach and permissions;
an action neither rewinds movement nor overwrites the current body orientation.
Minigames, vehicles, weapons, audio and the full vanilla contract remain required.

Save/Load now connects the native dialogs to authoritative snapshots and local
files under `<state-dir>/saves/map-<map-id-sha256>/`. Converted original saves are
read-only templates in the same listing. Saving with a template's name requires
overwrite confirmation and creates a local copy; it never edits converted content.
Overwriting a local save first retains its prior bytes under `.history/`, then
publishes a flushed replacement. Publication requires filesystem hard-link support;
failure leaves the prior save intact. Names are case-insensitive across platforms.

Native `.world.json` build files wrap a versioned world. The world's owner
table records which player (public-key principal) each owner number is. A
player who joins gets back the number the world has for their principal, so
their bricks are theirs again after a restart. Loading a build with ownership
gives each recorded builder's bricks to that player's number on this server;
owner numbers with no principal (imports, anonymous builds) map to fresh,
unclaimed numbers. Loading without ownership assigns the loading host. Future
joins cannot claim unclaimed or recorded numbers. Queued actions are omitted; authored events, prints and retained source
records survive. Save options can exclude events/ownership and their corresponding
legacy records. Both wrapped saves and earlier converted world files are readable;
dedicated startup also accepts these wrapped builds.

Loading appends atomically after validating every definition/collision footprint.
It keeps existing players and builds, allocates new brick IDs and merges exact
colors (including event color parameters) within the native 256-color bound.
It does not apply hand-placement reach/support rules to a restored build. Only
the authenticated host/administrator can load. Native hosting obtains a private
in-process credential; being first to connect or sharing an IP grants no authority.
This is the intentional modern LAN trust policy, not stock v20's unrestricted LAN.

File work runs off the UI thread with a bounded serial queue. A pending read
cannot load into a different session after reconnect/rehost. Working admission
limits are 63 MiB per build, eight local file operations, four server save/load
requests per 120 ticks, and a listing scan of 1,000 local saves/512 MiB. A corrupt
save currently produces an explicit listing error. Large loads still perform
planning/collision publication/replication on the authority loop; asynchronous
chunked loading, smooth large-world rendering and persistent identity remain work.

Current client and dedicated host use a matching full native content identity
covering geometry/map bindings, materials/prints, effects and the avatar rig,
customization catalog and all declared avatar images. A changed print, effect or
avatar must not silently join the same session. Legacy three-package fingerprints
remain only for diagnostic probes.

## Verification and commands

```powershell
cargo test --workspace --locked
cargo test -p bri-client --test app_flow --release --locked -- --ignored --nocapture
cargo test -p bri-client --lib local_native_content_index_and_lazy_maps --release --locked -- --ignored --nocapture
cargo test -p bri-render --test persistent_scene --release --locked -- --ignored --nocapture
cargo test -p bri-client --test brick_material_gallery --locked -- --ignored --nocapture
cargo build -p bri-client --release --locked
```

The ignored tests require the generated local assets and/or an offscreen GPU.
They never create a visible window or send OS input. Windows is verified.
Linux x86_64 builds and passes `--check` (see "Linux" below); its window, input
and audio are not yet verified, nor is macOS. Maxwell performs interactive
playtests when the complete alpha is ready.

Evidence:

- `artifacts/native-client-flow/`: actual App → QUIC host → authoritative poses
  and chat → persistent scene plus original HUD; ghost/plant/print/events/wrench/
  undo through the real server, cancellation/rehost and stale inspection races,
  settings replacement/reload and disconnect cleanup. Native save/undo/reload
  preserves print, events and owner; confirmed overwrite and a load-dialog render
  are covered. A same-batch look/fire/
  look-away sequence verifies that the click retains its own aim. Third-person
  rendering and a camera sweep against the original Bedroom floor also pass.
- `artifacts/native-camera/stock-shapes.json`: 4,080 camera sweeps across all
  170 stock collision definitions, four rotations and six approach directions;
  4,013 hits, no unsupported shape queries. Authored openings can remain clear.
- `artifacts/native-client-content/integration.json`: all three empty native
  simulations and Demo House's 150-brick lazy load.
- `artifacts/persistent-scene/`: six map frames on one device with one upload
  per map and distinct camera uniforms; original material/terrain rendering and
  explicit omissions. Smaller tests cover depth, blending and odd-size resize.
- `artifacts/brick-materials-gallery/`: all 77 native prints bound to compatible
  brick meshes, plus painted special bricks; remaining color/FX gaps retained.
- `artifacts/native-dedicated-content/`: real native tool catalog and bounded
  dedicated Demo House server run with final save.

The development executable accepts `--run [content-root] [client-state-dir]`.
When the state directory is omitted, it uses the current user's application data:
`%LOCALAPPDATA%/BlocklandReImagined` on Windows,
`~/Library/Application Support/BlocklandReImagined` on macOS, and
`$XDG_DATA_HOME/blockland-reimagined` (or `~/.local/share/blockland-reimagined`)
on Linux. An explicit directory still overrides this, including for isolated
headless tests. Existing development `client-state/` directories are not moved;
pass that directory explicitly to retain those settings/saves.
It has deliberately not been launched visibly during this work. Native settings
are versioned, atomically replaced and corruption is reported without erasing
the existing file. Alt+Enter is currently transient; Options display changes
use acknowledgment before committing preferences.

## Linux

`bri-client` builds and runs on x86_64 Linux with the same code; the only
platform branches are the state directory above and the identity file (Windows
user data protection there; a `0600` file on Linux).

Build requirements beyond Rust: a C compiler and the ALSA headers
(`pacman -S base-devel alsa-lib` on Arch/CachyOS, `apt install build-essential
libasound2-dev pkg-config` on Debian/Ubuntu). Windowing uses Wayland or X11
through libraries loaded at run time; graphics need a Vulkan driver (Mesa or
the vendor driver) since wgpu picks Vulkan on Linux.

```sh
cargo build -p bri-client --release --locked
python tools/regenerate_content.py --v20 "/path/to/Blockland v20"   # docs/content-regeneration.md
target/release/bri-client --check content
target/release/bri-client --run content
tools/package_playtest.sh --version a8 --sha256 "$(sha256sum target/release/bri-client | cut -d' ' -f1)"
```

`package_playtest.sh` is the Linux counterpart of `package_playtest.ps1`: it
copies the release client and the packs the package list selects into
`dist/BlocklandReImagined-alpha-<version>-linux/` with `launch.sh` and a
checksummed `MANIFEST.json`; `--validate-only` and `--verify <dir>` work as on
Windows.

Verified on 2026-09-27 from Windows: the client compiles for
`x86_64-unknown-linux-gnu` without warnings, links against an Ubuntu 24.04
sysroot, and the linked binary passes `--check` under WSL Ubuntu against the
full content folder. Not yet verified: opening a window, input, audio output,
and GPU rendering on a real Linux desktop; a CachyOS tester is the next step.
