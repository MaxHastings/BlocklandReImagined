# Client sandbox: Add-On code on players' machines

Status: built 2026-09-28; `crates/client-sandbox` (`bri-client-sandbox`)
runs in the client's games (see "In the game"), and joining a server asks
before its sandboxed Add-On code runs.
Maxwell chose (2026-09-28) to let Add-Ons send joining players sandboxed
code, and set the guiding rule for it:

> Build the platform to allow as much potential as possible, keep people
> reasonably safe by default, and let them go beyond safety for more
> capability if they trust the other person.

This replaces "data to clients, never code" in
[platform-principles.md](platform-principles.md) (principle 10) with **trust
tiers**. Packages and their ids are in [packages.md](packages.md).

## Why data is not enough

The data-only path goes a long way, and it stays the default: a brick kind
declaring a material per face, flipbook textures, named states such as
"cracking" that a server script sets and every client animates, HUD panels
bound to package state. The stress campaign
([`docs/stress-lab/HANDOFF.md`](../stress-lab/HANDOFF.md), "Beyond the
Stress Lab", on PR #1) proves those cases without client code.

Data runs out where an Add-On needs behaviour the engine does not already
have a declaration for:

| Case | Why data cannot express it |
|---|---|
| A custom digging tool whose effect on a custom block is a shader (heat haze, dissolving voxels, a portal surface) | Every effect would need its own engine feature first. |
| A package-defined movement controller (player archetypes on PR #1: a horse, a spider, a jetpack) | Clients predict movement. Today controllers are chosen by name from a built-in list (ledger class W14); a new one needs code on both sides. See "Prediction" below. |
| A movie theatre, a jukebox visualiser, a security-camera wall | Needs a video or render-to-texture capability plus logic driving it. |
| Mini-maps, radar, fog of war drawn over the world, custom particle systems, procedural skies | Rendering logic, not a fixed list of parameters. |
| Complex UI widgets (a card table, a circuit editor) | Declarative panels cover forms and lists, not interactive canvases. |

## Trust tiers

| Tier | What it can contain | How a player gets it | Prompt |
|---|---|---|---|
| **1. Data** | Assets, declarative content, UI, effect and material definitions | Downloads on join, verified by hash | None |
| **2. Sandboxed code** | WebAssembly modules and WGSL shaders using only the sandbox capabilities below | Downloads on join after the player trusts the server | Once per server; again when the code changes |
| **3. Elevated** | Capabilities beyond the sandbox: `net.http` (URLs), `files.addon_folder` (a folder of its own on the player's PC), and `native` (a native plugin with full access to the PC) | Downloads on join after a separate, stronger choice; for `native`, the player also types the server's name | Per server and per Add-On; again when the code changes |

Defaults stay safe: tiers 2 and 3 are off until the player chooses them,
tier 3 is never part of the tier 2 prompt, and a native plugin can never be
clicked through by accident.

The player's own Add-Ons are their choice already. In a game they host,
every enabled Add-On's sandboxed code runs. On someone else's server their
own enabled `client` Add-Ons (only client code, nothing the server needs,
like the Ragdoll) run too, without a prompt: the server never sent them
and they only draw on the player's screen. A server's Add-Ons are always
`shared` and arrive in the download cache, so they still ask
(`ClientCode::start`, `trust_prompt`). An Add-On whose only content is
client code is `client` (`bri_package::library::side_for_package`).

**Native plugins are deferred, not forbidden.** Tier 3 already covers them:
the capability exists, the trust prompt asks for them with the strongest
wording and a typed confirmation, and grants are per server and per Add-On
hash like every other elevated grant. What is not built is running them:
`Sandbox::start` refuses `native` ("cannot run yet") even when fully
trusted, and PR #1's package sync still refuses `.dll` and other native
files. Building it needs: a stable plugin ABI (a C ABI with a versioned
host table, since Rust's ABI is not stable), loading into a separate
process so a crashing plugin cannot take the game down and revoking trust
can kill it, letting package sync carry native files only for packages the
player granted `native`, and deciding how Windows SmartScreen and antivirus
warnings are presented. None of that blocks tiers 1 and 2, and the most
wanted cases (custom rendering, effects, controllers, video) fit in tier 2
or in engine capabilities, so it waits for a mod that genuinely needs it.

Nothing escalates silently. A grant covers exactly the code hash and
capabilities the player saw (`CodeSummary`, `TrustStore::granted`): new
code, or the same code asking for more, asks again. The code hash covers the
module, every shader and the declared capabilities
(`AddOnCode::code_hash`). Code that needs a tier the player has not granted
never starts (`Sandbox::start` takes the granted level).

### The prompts

Shown as a question when the player enters the game, before any of the code
runs (see "In the game"). Text comes from `trust::TrustPrompt`; the UI draws it.

**Tier 2**, when a server has sandboxed code the player has not trusted:

> **Max's Server wants to run Add-On code**
> This server's Add-Ons include code that runs on your PC in a sandbox. They can:
> - Spinning Cube: Draw its own 3D shapes in the world; Use its own graphics effects (shaders)
>
> They cannot read your files, reach the internet, see your other Add-Ons or change the game's rules. You can take this back any time on the Add-Ons screen.
>
> [Trust and join] [Leave]

When the code changed since the player trusted it, the body says so and
the row is marked changed.

**Tier 3**, a separate prompt after tier 2, only for elevated Add-Ons:

> **Cinema wants full trust**
> These Add-Ons ask to go beyond the sandbox. Only allow this if you know and trust whoever runs this server. They can:
> - Movie Theatre: Load things from the internet, which shows your IP address to those sites
>
> Outside the sandbox, a harmful Add-On could see or change things on your PC, or show your IP address to other sites. You can take this back any time on the Add-Ons screen.
>
> ☐ I know who runs Cinema and I trust them with my PC
> [Fully trust and join] (enabled once ticked) [Join without them]

When a native plugin is on the prompt, its row reads "Run as a normal
program with full access to your PC", the footer starts "A native plugin
runs as a normal program: it can do anything you can do on this PC, and no
sandbox limits it", and accept also needs the player to type the server's
name (`TrustPrompt::type_to_confirm`).

Declining tier 3 still joins: client code is presentation only, so the game
plays without it, and those Add-Ons' code simply does not run.

Choices are saved in the player's settings folder (`addon-trust.json`),
keyed by the server's identity (its host key once servers have one, its
address until then), per Add-On. The Add-Ons screen lists trusted servers
and lets the player revoke a server or one Add-On
(`revoke_server`, `revoke_addon`).

## Runtime: Wasmtime

| | Wasmtime (chosen) | Wasmi | Browser-style JS, Lua, Rhai |
|---|---|---|---|
| Speed | Cranelift native code; close to native for render and prediction loops | Interpreter, roughly 5 to 20 times slower | Slower still, and dynamic |
| CPU limits | Fuel (deterministic) and epoch interruption (wall clock, cheap) | Fuel | Varies |
| Memory limits | `StoreLimits`, guard pages, bounds checks | Yes | Varies |
| Security record | Bytecode Alliance; continuously fuzzed; used for untrusted code in production (Fastly, Shopify, Fermyon); spectre mitigations on by default | Smaller surface, less exposure | Rhai is ours server-side, but interpreting render logic per frame is too slow |
| Languages | Anything that targets wasm32: Rust, C, C++, Zig, AssemblyScript | Same | One each |
| Cost | Large dependency (about 1 to 2 minutes of extra clean build, cached by sccache); Windows supported | Small | |

Wasmtime's JIT is more attack surface than an interpreter, and that is the
known trade: its compiler is one of the most fuzzed in existence and
presentation code needs the speed. Wasmi stays the fallback if Wasmtime ever
becomes a problem: the host API is plain imports and exports, so swapping
engines does not change Add-Ons.

Version: `wasmtime 45`, the newest line whose minimum Rust (1.93) matches the
workspace and CI. Features: `runtime`, `cranelift`, `std` only: no WASI, no
component model, no text-format parser on clients, no profiling or cache.

WebAssembly features allowed (`wasm_imports::FEATURES`): WebAssembly 2.0
(SIMD, bulk memory, multi-value, reference types). Refused: threads and
shared memory, 64-bit memory, multiple memories, GC, exceptions, stack
switching, tail calls, relaxed SIMD. Each is more host surface with no
presentation need yet; they can be added one at a time.

## The host API (platform API level 1)

A module exports `memory`, and optionally `init()` (run once after start)
and `frame(time: f32, dt: f32)` (run every rendered frame). It imports host
functions from module `bri`. Importing anything else, or a function whose
capability the Add-On did not declare, refuses the Add-On when it loads,
before anything compiles (`client.capability.denied`,
`client.import.unknown`). The host defines only the functions of declared
capabilities, so there is no path to an undeclared one at run time either.

Pointers are offsets into the module's own memory; the host copies in and
out and never hands out references. Handles are small integers private to
the Add-On. Floats must be finite.

| Function | Capability | Does |
|---|---|---|
| `log(ptr, len)` | none | A line in the Add-On's log (4 KiB a frame; the rest is dropped). |
| `random() -> i32` | none | Presentation randomness, seeded from the code hash. |
| `mesh_create(vptr, vcount, iptr, icount) -> mesh` | `render.layer` | An immutable mesh: vertices of position, normal and uv (8 f32), u32 triangle indices. |
| `material_create(shader) -> material` | `render.layer` | A material using one of the Add-On's shaders. |
| `material_set(material, slot, x, y, z, w)` | `render.layer` | One of four vec4 parameters (`bri_draw.params`). |
| `draw(mesh, material, matrix_ptr)` | `render.layer` | Draw this frame, with a column-major model matrix. |
| `draw_with(mesh, material, matrix_ptr, params_ptr)` | `render.layer` | Draw with this draw's own four vec4 parameters (16 f32) in place of the material's: one material draws many things. |
| `material_blend(material, mode)` | `render.layer` | 0 solid (the default), 1 glow (added over the scene), 2 see-through (alpha blended); glow and see-through write no depth and draw both faces. |
| `material_space(material, space)` | `render.layer` | 0 world (the default), 1 view, 2 screen. View space is camera-relative (x right, y up, looking down -z) at the player's normal field of view, drawn after the world at the front of the depth range, so a first-person model never clips into walls or stretches while zoomed. Screen space is flat: y from -1 to 1, x from -aspect to aspect, no depth test, drawn last. Each space has its own `bri_frame`. |
| `view(ptr)` | `render.layer` | Writes the player's view (12 f32): field of view now and normally (degrees), aspect, width and height in pixels, flags (1 first person, 2 aiming, 4 alive), 6 unused. |
| `camera(ptr)` | `render.layer` | Writes the camera's eye and forward direction (6 f32, world units, Y up). |
| `environment(ptr)` | `render.layer` | Writes the scene's lighting (12 f32): the direction sunlight travels, the sun's colour, the ambient colour, the fog and horizon colour. |
| `shader(name_ptr, len) -> shader` | `render.shader` | One of the Add-On's shader files, checked at load. |
| `sound_play(name_ptr, len, volume) -> i32` | `audio` | A sound file the Add-On lists, at the player's ears. |
| `sound_at(name_ptr, len, volume, x, y, z) -> i32` | `audio` | The same, placed in the world: full within 10 units, gone by 90, on the effects channel. |
| `key_down(key) -> i32` | `input.focused` | 1 if the key is down *and* the Add-On's panel has focus; always 0 otherwise. |
| `send(ptr, len) -> i32` | `net.message` | A message to the Add-On's own server script. |
| `recv(ptr, capacity) -> i32` | `net.message` | The next message from its server script: its length, -1 when none, or -2 - length when the buffer is too small. |
| `local_player() -> i32` | `world.read` | The viewing player's id. |
| `life(player) -> i32` | `world.read` | Which life the player's body is (the tick it spawned, wrapped to 31 bits); -1 when there is no such player. The id stays the same across respawns, so a change is a new body; a corpse keeps the life it died in. |
| `players(ptr, capacity) -> i32` | `world.read` | Writes up to `capacity` players as the game draws them, 16 f32 each: id, flags (1 the viewer, 2 alive, 4 crouched), feet xyz, eye xyz, look xyz, velocity xyz, archetype kind, held image kind (from `archetype_kind`/`image_kind`, -1 otherwise). Returns how many. |
| `archetype_kind(ptr, len) -> i32`, `image_kind(ptr, len) -> i32` | `world.read` | Name an archetype (`namespace:archetype/name`) or a weapon image (`namespace:image/name`) to find in `players()`; returns its kind number (64 of each at most). |
| `entities(ptr, capacity) -> i32` | `world.read` | Writes up to `capacity` Add-On creatures as drawn, 8 f32 each: id, feet xyz, yaw, 3 unused. Returns how many. |
| `vehicle_kind(ptr, len) -> i32` | `world.read` | Names a vehicle definition (`namespace:vehicle/name`) the Add-On wants to find; returns its kind number (64 at most). |
| `vehicles(ptr, capacity) -> i32` | `world.read` | Writes up to `capacity` vehicles as drawn, 16 f32 each: id, kind (from `vehicle_kind`, -1 otherwise), position xyz, rotation xyzw, velocity xyz, radius of a sphere round its box, 3 unused. Returns how many. |
| `state_num(pkg_ptr, pkg_len, key_ptr, key_len, player, index) -> f32` | `world.read` | A number of an Add-On's public state the player receives: a server-wide key (`player` -1) or that player's; an array gives its `index`th element, true and false are 1 and 0; NaN when there is none. |
| `rigid_create(ptr) -> body` | `physics.local` | A rigid body the game simulates on this PC only, from 28 f32: shape (0 box, 1 ball, 2 capsule), size xyz (box half extents; ball radius; capsule radius and half height), offset of the shape in the body's frame, position, rotation xyzw, velocity, spin, density, friction, bounce, group, linear and angular damping, shared (1 lets other Add-Ons find, push and hold it). Bodies with the same nonzero group never touch each other. Bricks, terrain and the map are solid to it; players and vehicles shove it; shots strike it. It never touches gameplay. |
| `rigid_joint(a, b, ptr) -> joint` | `physics.local` | A ball joint between two of its bodies, from 12 f32: world anchor, twist axis, swing limit and twist limit (radians, 0 for free), friction. The joined bodies do not touch each other. |
| `rigid_remove(body)` | `physics.local` | The body and its joints go. |
| `rigid_push(body, x, y, z)` | `physics.local` | Adds this velocity to one of its bodies or a shared one. |
| `rigid_get(body, ptr) -> i32` | `physics.local` | Writes a body as last simulated (16 f32): position, rotation xyzw, velocity, spin, flags (1 resting, 2 shared, 256 x group), mass, radius. 0 when there is none yet (bodies are simulated after the frame that makes them). |
| `rigid_find(ox, oy, oz, dx, dy, dz, reach, ptr) -> body` | `physics.local` | The nearest of its own or a shared body a ray passes within reach of (0 when none), writing 8 f32: distance, the hit in the world, the grab point in the body's frame, 1 unused. |
| `rigid_hold(body, px, py, pz, tx, ty, tz, vx, vy, vz, max_accel)` | `physics.local` | For this frame, draws the body's point (in its frame) to a world target moving at a velocity, like a spring that cancels gravity, up to `max_accel` (at most 2,000). Release by not holding; the body keeps its momentum. |
| `skeleton(player, ptr, capacity) -> i32` | `avatar.pose` | Writes up to `capacity` nodes of the player's body as drawn this frame, 16 f32 each: parent node (-1 for none), flags, world position, rotation xyzw, the bounds of what is drawn on it (min and max in its frame), 1 unused. Returns how many nodes it has, or -1 without a body. |
| `skeleton_node(player, ptr, len) -> i32`, `skeleton_part(player, ptr, len) -> i32` | `avatar.pose` | A node by name, or the node a body part (`chest`, `headskin`, `rarm`, ...) is drawn on; -1 when there is none. |
| `pose(player, ptr, count) -> i32` | `avatar.pose` | Places `count` nodes of the player's body for drawing, 8 f32 each: node, world position, rotation xyzw. Nodes under them follow. The eye stays where the game puts it, so the view and aim never change. |

`world.read` offers only what the player's own screen and HUD already show
(public state keys, poses the game draws), so it is sandboxed, not
elevated. Ids arrive as f32 (exact to 16 million). The game builds the
world for a frame only when a running Add-On declares the capability.

The Commando sample's `sample-commando-look` draws a box-model rifle in
view space only while its player is alive, in first person and holding the
rifle. While aiming it draws a scope in screen space (`crates/client-sandbox/tests/commando.rs`).

The showcase Add-Ons use these: `steel-ball-fx` draws one mirror-steel
sphere per Steel Ball, and `gravity-gun-fx` draws beams, force fields,
shockwaves and GPU particle systems from each player's `beam` state
(`packages/showcase`, tested in `crates/client-sandbox/tests/showcase.rs`,
which with `--ignored` renders them offscreen to PNGs). The `ragdoll`
Add-On turns a dead player's body into jointed shared bodies and poses the
body from them (`physics.local`, `avatar.pose`; tested in
`crates/client-sandbox/tests/ragdoll.rs` and `crates/client/src/addon_physics.rs`).

`physics.local` bodies are cosmetic, like brick debris: the game simulates
them on this PC in their own world (`crates/client/src/addon_physics.rs`,
sharing `local_physics` with the debris) with Torque's gravity, fixed
1/120 s steps and at most four steps a frame. Commands apply after the
frame that makes them and the next frame reads the result. Shared bodies
are how Add-Ons interact without seeing each other: any Add-On with
`physics.local` can find, push and hold them.

Planned, same shape: `ui.panel` (draw into a panel the engine places),
`render.texture` (images from the Add-On, render targets), `video.screen`
(below). Each lands as a capability with plain words for the prompt.

What client code can never do, whatever it declares: read or write files
outside tier 3's own folder, open sockets or URLs outside tier 3, see or call
another Add-On, read other players' secrets (the server never sends them:
state keys declare their audience, W13 on PR #1), or change gameplay state
except by asking its own server script, which decides.

## Shaders

Add-On WGSL is written against a fixed interface (`shader::PRELUDE`):
`bri_frame` (view-projection, camera, time) at group 0, `bri_draw` (model
matrix, four vec4 params) at group 1, and the `BriVertex` input. Every
shader is checked when the Add-On loads, before the trust prompt, so a
server cannot offer a shader that would be refused later:

- Parsed and validated by naga (the compiler wgpu itself uses), with no
  optional GPU capabilities.
- Only `@vertex fn vs_main` and `@fragment fn fs_main`; no compute, so no
  dispatch of any size.
- No resources but the prelude's: no storage buffers, atomics, textures,
  samplers or immediates, and no overridable constants. A shader can read
  only its own draw's uniforms.
- Vertex inputs only at the three locations the engine supplies; one
  `@location(0)` colour out.
- Size: 64 KiB of source, 256 functions, 16k expressions per function, and
  no type over 16 KiB (a huge local array would spill registers and can hang
  drivers).
- **Every loop is bounded.** naga's IR is rewritten so each invocation has
  one iteration allowance that every loop draws from, helper functions
  included; when it runs out, loops exit. Each entry point starts by
  loading the allowance from `bri_frame.limits.x`, which the engine sets
  every frame (below). Code that touches the allowance is refused.
- **Bounded cost.** WGSL has no recursion, but helpers can fan out (each
  calling the one before twice doubles the work with no loop at all). The
  compiler counts the expressions each entry point runs with every call
  expanded (`vertex_cost`, `fragment_cost`) and refuses shaders over 8,192
  (`shader.too_costly`). Work per invocation is at most cost x (allowance
  + 1).
- wgpu then adds its own runtime checks (bounds checks and loop bounding)
  when it compiles the module for the GPU.

## Budgets

Per Add-On (`host::Budgets`; defaults shown):

| Budget | Default | Enforced by | When exceeded |
|---|---|---|---|
| Instructions per `frame` | 20 million | Wasmtime fuel | Stopped: "it used too much processing time" |
| Instructions for start and `init` | 500 million | Fuel | Never starts |
| Wall clock per `frame` | 8 ms | Epoch interruption (1 ms tick) | Stopped: "it took too long to respond" |
| Wall clock for start and `init` | 1 s | Epoch | Never starts |
| Memory | 64 MiB, one memory, one instance, 100k table elements | `StoreLimits`, trap on failed growth | Stopped: "it used too much memory" |
| WebAssembly stack | 512 KiB | `max_wasm_stack` | Stopped: crashed (stack overflow) |
| Module shape | 4 MiB, 20,000 functions, 512 KiB per function (bounds compile time) | Load | Refused |
| Meshes | 1,024, 65,536 vertices each, 32 MiB total | Host | Stopped: asked for too much |
| Materials | 256 | Host | Stopped |
| Draws and triangles per frame | 2,048 and 1 million | Host | Stopped |
| Sounds per frame | 16 | Host | Extra sounds dropped |
| Messages per frame | 32, 16 KiB each | Host | Stopped |
| Log per frame | 4 KiB | Host | Extra lines dropped |
| Incoming messages queued | 256 | Host | Oldest kept, newer dropped |
| Local bodies and joints | 256 and 512 alive | Host | Stopped: asked for too much |
| Physics calls per frame | 1,024 | Host | Stopped |
| Physics time per frame | 4 ms of its bodies' simulation; 30 frames over it stops the Add-On | `AddOn::report_physics_time` | Stopped: "its physics were too heavy" |
| Players posed per frame | 64 | Host | Stopped |
| Shader loop allowance | 16 iterations until the GPU is measured; then fitted to the GPU's speed, the screen size and the shader's cost, at most 4,096 | `gpu::loop_limit`, set per frame in `bri_frame.limits.x` | Loops end early (the shader still draws) |
| GPU time per frame | 4 ms; over it the allowance halves; 20 frames in a row over it stops the Add-On | Timestamp queries around the layer, where the GPU has them | Stopped: "its graphics were too heavy" |
| One frame's GPU time | 100 ms | The same timestamps (`gpu_stop_ms`) | Stopped at once, naming the time |
| Shader cost with no loop running | Over the whole screen (twice over), must fit 500 ms when timed, 100 ms when not | Estimate from the measured speed, every frame | Stopped: "its shaders would take about N ms a frame" |
| Graphics card reset (device lost) | any | Platform's device-lost recovery | Every Add-On's code stops until the next join; the game rebuilds its renderer as before |
| GPU errors | any validation error in its pipelines | wgpu error scope | Stopped |

**Measuring the GPU.** The first time Add-On code draws on a device, the
engine times a small offscreen pass (128x128 pixels of a shader doing 16
transcendental operations per loop iteration, at rising allowances until
one pass takes about 10 ms; 50 to 130 ms in all on software rendering) and
keeps the speed as shader expressions per millisecond (`gpu::calibrate`).
Transcendentals are the most expensive operations, so ordinary shaders run
faster than it predicts. Each frame the allowance is the largest that
keeps the Add-On's shaders, over the whole screen twice (overdraw), inside
a quarter of the 100 ms stop when the layer is timed, or inside the 4 ms
budget when it is not. Timing then lowers it to what the layer really
costs: on llvmpipe the endless-loop shader over 512x512 went from 15
iterations (23 ms) to 7 (5.8 ms) to 3 (4.6 ms) in three frames, where it
used to run 4,096 iterations (0.96 s). A server with no client code never
calibrates, times or draws anything.

A stopped Add-On is dropped for the session: its meshes and materials are
released, its layer stops drawing, and the player sees one line naming the
Add-On and the reason. Nothing else stops. Misuse (a pointer outside its
memory, a bad handle, a non-finite number) stops it the same way, with the
rule it broke.

## Authority

Client code is presentation. The server simulates everything that decides
the game. A client Add-On can only:

1. draw, play sounds, show panels on the player's own screen;
2. send messages to its own server script, which treats them as untrusted
   input from that player, exactly like a typed command (capability checks
   in `bri-package-runtime`'s `ops::authorize`, per-principal budgets);
3. predict, when it provides a movement controller (below), which the
   server re-simulates and corrects.

A modified client can run anything it likes locally; that was always true.
The sandbox protects the *player* from the *server*, and the server's rules
protect everyone from a modified client.

## Prediction: package movement controllers

PR #1's player archetypes pick a movement controller by name from a built-in
list because clients predict movement (ledger class W14). Tier 2 removes
that limit:

- A controller Add-On ships one WebAssembly module used on **both** sides:
  the server runs it (Wasmtime server-side, with the same budgets) as the
  authority; the client runs the same bytes to predict.
- Interface: `step(state_ptr, input_ptr, dt) -> state`, where state and
  input are fixed-layout records the engine owns (position, velocity, the
  controller's own bytes up to a small limit; buttons, look direction). No
  host functions except collision queries against the world
  (`sweep`, `overlap`), which both sides answer from the same world data.
- Determinism: the module must be a pure function of its inputs. Wasmtime
  with NaN canonicalisation on, no relaxed SIMD, no threads, no clocks and
  no randomness makes that true for the same bytes on every x86-64 machine;
  the step runs at the fixed simulation tick, not per frame.
- Disagreement is normal networking: the server's state wins, the client
  rewinds to it and replays its unacknowledged inputs through the same
  module, as it does for built-in movement today. A controller whose client
  and server results keep diverging (a determinism bug) only costs smoothness
  for that player; it can never move them anywhere the server did not.
- Budget: a tick's steps for all players must fit the tick; a controller
  that exceeds it is replaced by the default controller on that server and
  reported to the host.

## Video screens (planned capability)

`video.screen` renders a video onto a surface the Add-On draws (a movie
theatre wall), decoded by the engine, never by the Add-On:

- Files shipped in the Add-On (for example VP9 or AV1 in WebM) are tier 2:
  they are data, hashed and downloaded like any asset.
- Streaming from a URL is tier 3 (`net.http`): it reveals the player's IP
  address to whoever serves the URL, so it needs full trust, and the player
  sees the host name on the prompt. YouTube's terms make embedding its
  streams awkward; a server hosting its own files, or a direct media URL, is
  the supported path.
- Audio follows the world's sound rules (positional, volume settings).

## Distribution and the join flow

Code travels inside ordinary packages (PR #1's package sync: content
addressed, hash verified, data only for native types). The sync's refusal of
native code types (`CODE_EXTENSIONS`: `.exe`, `.dll`, scripts, ...) stays
until native plugins are built (tier 3, above); `.wasm` and `.wgsl` are
allowed because nothing runs them but this sandbox.

Order on join:

1. Environment check (protocol, shared packages), as today.
2. For packages the client lacks, fetch their `package.json` first (small).
   From each `client` section and listing, build the `CodeSummary` list:
   id, name, code hash, capabilities.
3. `TrustStore::decide`: `Join`, or a prompt. Today the prompt is a question
   shown as the game is entered, after the download (see "In the game");
   asking before the download is not built yet.
4. Download the rest, as today.
5. `AddOnCode::load` re-checks everything from the verified bytes (hash,
   imports, shaders) and compiles modules on a worker thread, so a large
   module never stalls a frame. If the code hash does not match what the
   player trusted, it does not start.
6. `Sandbox::start` with the granted level; `frame` each rendered frame;
   `LayerRenderer` draws its layer inside the world pass.

The Add-Ons screen (PR #4) shows, for each Add-On with code, where it runs
and its capabilities in the same plain words as the prompt, and the list of
servers the player trusts.

## Package format

The `client` section of an Add-On's `package.json`
([packages.md](packages.md)):

```json
"client": {
  "module": "client/main.wasm",
  "capabilities": ["render.layer", "render.shader"],
  "shaders": ["client/cube.wgsl"],
  "sounds": []
}
```

Unknown fields are errors. `native` loads as an elevated capability but
does not run yet. The sample is `packages/samples/spinning-cube`: a cube
with an animated WGSL shader, written as WebAssembly text so it needs no
toolchain; the test suite checks `main.wasm` is built from `main.wat`.
`bri-addon-preview <add-on folder> <output folder>` renders it headless.

## Red team

Round 1 (2026-09-28) attacked the prototype for escape, denial of service,
resource exhaustion and hostile shaders. Each row is a test in
`crates/client-sandbox/tests/sandbox.rs` or a measurement noted here.

| Attack | Result |
|---|---|
| Import WASI (files, clocks, sockets), or anything outside `bri` | Refused at load (`client.import.unknown`). |
| Import a host function whose capability is undeclared | Refused at load, before compiling (`client.capability.denied`); the linker also lacks it. |
| Ask the host for a memory, table or global | Refused at load. |
| Shared memory, threads, 64-bit memory, other post-2.0 features | Refused at load by the validator. |
| Garbage bytes, a Windows executable renamed `.wasm` | Refused at load (`client.module.malformed`). |
| Module path `../../secret.wasm`, or through a link | Refused at load (`client.path`). |
| `native` capability | Needs full trust plus the typed server name; does not run in this build even then. |
| Elevated capability without full trust; with it but not built yet | Never starts. |
| Infinite loop in `frame`, in `init`, in the `start` function | Stopped by fuel. |
| Infinite loop with unlimited fuel | Stopped by the wall-clock deadline. **Found and fixed:** the epoch ticker lived on the `Sandbox`, so dropping the sandbox while Add-Ons ran stopped the wall clock and the loop ran forever; every Add-On now holds the ticker. |
| Grow memory past the budget, or declare a huge initial memory | Stopped / never starts. |
| 100,000 draws a frame (cheap in fuel, expensive in host work) | Stopped at the draw budget; layer released. |
| Pointer at the end of memory, or `ptr + len` overflowing | Stopped (misuse); checked arithmetic. |
| Non-finite vertices or matrices, indices past the last vertex | Stopped (misuse). |
| Flood logs, sounds, messages | Bounded per frame; logs and sounds dropped, messages stop it. |
| Read keys while the player types elsewhere | Always 0 unless the Add-On's panel has focus. |
| Shader reads a storage buffer, texture or another resource | Refused (`shader.resource`). |
| Compute shader (a huge dispatch) | Refused (`shader.entry_point`). |
| `loop {}` in a fragment shader; nested and helper-function loops | Bounded by the shared per-invocation allowance. |
| Shader resets the allowance | Refused (`shader.reserved`). |
| 100,000-element local array | Refused (`shader.type_too_large`). |
| `loop {}` in a fragment shader drawn over the whole screen, on a real device | **Found and fixed:** with a fixed 4,096-iteration allowance, Maxwell's RTX 4070 SUPER took about 2.4 s a frame at 1080p and 4K for the heaviest allowed loop body (16 transcendentals), and the 4 ms x 30-strike budget let that run for about 75 s. The allowance now starts at 16, is fitted to the measured GPU speed and screen size, and halves on every slow frame; on llvmpipe the same shader settles at about 4.6 ms a frame (`an_endless_shader_loop_finishes_on_the_gpu`). |
| Helpers that call each other twice, 20 deep (a million times the work, no loop) | **Found and fixed:** was unbounded by the loop rewrite. Refused (`shader.too_costly`). |
| One frame far over budget | Stopped at once above 100 ms (`one_frame_far_over_the_gpu_budget_stops_the_addon_at_once`). |
| Sustained heavy fragment work | The allowance drops to fit; stopped after 20 frames over budget if even no loops are too slow. |
| Pipeline-overridable constants the engine does not set | Refused (`shader.override`). |
| Compile-time bomb: 60,000 tiny functions (1.2 MB), or one 4 MB function | **Found and fixed:** 5 s and 3 s to compile on one core before any budget applied. The module's shape is now bounded at load (20,000 functions, 512 KiB each, 4 MiB total) and compilation is parallel: the worst the limits allow compiles and starts in 0.5 to 0.7 s on 4 cores (`compile_time.rs`). |
| New code, or the same code asking for more, on a trusted server | Asks again; old grant does not cover it. |

Open, to close before this ships in a playtest build:

- **Re-measure on Maxwell's PC** with the fitted allowance (the fix
  above). Residual risk: heavy overdraw (up to 2,048 full-screen draws) on
  the very first frame, before timing has come back, on a GPU without
  timestamps. The static estimate assumes overdraw 2; timing catches the
  rest one to three frames later, and a device reset stops every Add-On.
- **Compile off the frame loop.** The worst allowed module compiles in
  under a second, but that is still a stall if done on the render thread.
  Compile during the join's loading step on a worker thread, and cache
  compiled code by hash (`Module::serialize`) so each version compiles once.
- **Spectre-class side channels** inside one process. Wasmtime's
  mitigations are on; Add-Ons get no clock finer than the frame time and
  share nothing with each other, which limits what they could learn and
  from whom.

## In the game

`crates/client/src/client_code.rs` runs it. When the client starts, every
`shared` and `client` package in `packages.json` with a `client` section is
loaded and checked (`ClientCode::load`). When the player enters a game:

- in a game they host, their own enabled Add-Ons' code starts (enabling an
  Add-On is their trust decision);
- on someone else's server, only code `addon-trust.json` grants for that
  server's host key and exactly that code hash starts; the rest is named in
  chat as off. When some of it is sandboxed code not trusted yet, the game
  asks as it is entered, before any of it runs: the prompt's title, words,
  one line per Add-On with what it can do, and "Trust and join" / "Leave".
  Trust and join saves the grant and starts that code; Leave disconnects.
  The code has already downloaded by then (data only; nothing runs it
  until the player trusts it). Elevated code is not asked about, since
  nothing elevated runs in this build: it is named in chat as asking for
  more than the sandbox allows. The Add-Ons screen's Forget Trust clears
  every grant, so each server asks again.

Each rendered frame runs every Add-On's `frame` with the camera, then its
layer draws in the world's last pass (after particles and weather, before
the UI), with the scene's depth and multisampling. Log lines and stop
reasons appear in chat. Leaving the game stops everything.

To try the sample: copy `packages/samples/spinning-cube` into the content
root and add `{ "id": "spinning-cube", "version": "1.0.0", "side":
"client", "dir": "spinning-cube" }` to `content/packages.json`, then host
a game. The cube appears three units in front of where you spawn.

## Not built yet

In order: asking before the code downloads rather than after, and a
per-server list on the Add-Ons screen instead of Forget Trust's all at
once; the elevated prompt (tick box, typed server name) once elevated
capabilities run;
`ui.panel`; a Rust guest crate (`bri-addon-guest`) with safe wrappers so
Add-On authors never write raw pointers; the prediction interface; then
`video.screen`.
