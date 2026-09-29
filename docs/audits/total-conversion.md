# Total conversion audit

Date: 2026-09-29. Branch `claude/total-conversion-addons-jw91uo`, from main
570ae86.

Max's ask: Add-Ons should be able to change the whole experience, the way
Minecraft mods turn it into Mario or Call of Duty, and modders should find
the seams already there when they arrive. The Discord modder lpsroo adds
that many engine variables still need exposing.

This audit takes what a total conversion needs, area by area, and grades
each seam as **present**, **partial** or **missing**. Every gap is either
built on this branch, planned as next work with the reason it waits, or
marked **not worth it** with the reason. Seams this branch built are
marked **new**.

The Commando sample (`packages/samples/sample-commando*`) is the proof. It
is a small Call of Duty style conversion made only of Add-On packages:

- a soldier body with more health and no jet;
- a scoped rifle with a clip, a reserve and reloads, aim down sights,
  view kick and its own sounds;
- a rifle and a scope drawn in first person by client code;
- an ammo counter and a score panel;
- target dummies, and a sentry that shoots back;
- kill rewards, and a Start Game mode that names all of it.

It runs headless in `crates/sim/tests/commando.rs`, and the client code
runs and renders offscreen in `crates/client-sandbox/tests/commando.rs`. It
uses no engine code written for it and no copyrighted assets: every model
is a box model, and the sounds come from `tools/make_commando_sounds.py`.

## How a conversion is put together

A conversion is several Add-Ons, because who needs a file decides where it
runs:

| Package | Kind | Runs on |
|---|---|---|
| `sample-commando-rifle` | `weapons` (with sounds) | everyone |
| `sample-commando-look` | `model` and client code | each player |
| `sample-commando` | `behaviour`, `script`, `archetype`, `entity` | the host |
| `sample-commando-hud` | `hud` | each player |
| `sample-commando-mode` | `mode` | the host (Start Game) |

The engine refuses a package that mixes host kinds with `model` or `hud`,
and refuses dependency cycles. The split keeps rules on the host and
cosmetics on the client, as the network rule asks.

## Seams by area

### Player model and animation

| Seam | Status | Notes |
|---|---|---|
| Replace the player's body with a package box model (`archetype.model`) | present | Any archetype, picked per player by `set_archetype`. |
| No drawn body (`"model": "none"`) | **new** | A conversion can draw its own bodies with client code, for example animated ones from `players()`. |
| Scaled bodies (archetype or entity `scale`) | **new** | Package models draw at the body's scale: giants, Mario's tiny form. Entities take `scale` 0.2 to 4. |
| Other v20 shapes as bodies (horse) | present | The horse is now detected by look (`Look::is_horse`), not by archetype id, so an Add-On archetype can ride on it. |
| Animated package box models (walk bob, turn) | missing, **next** | HANDOFF next step 4. Until then, client code can animate bodies itself with `"model": "none"`. It waits on a small keyframe format for box models. That is renderer work, and the brick-perf lane owns the renderer. |
| Any v20 shape as an archetype body | partial, **next** | Only the Blockhead and horse have v20 animations wired. The other shapes need their own thread maps. |

### First-person view model and weapon feel

| Seam | Status | Notes |
|---|---|---|
| Magazines, reserve, reloads (`ammo`, `use_ammo`, `refill`, `reload`, `ammo`/`no_ammo` states) | **new** | The v20 image state machine, extended. Reserve is per item and survives switching. Rules call `reload`, `give_ammo`, and read `clip`/`reserve`. |
| Aim zoom and scopes (`zoom`: `fov`, `on_jet` right-click aim, `crosshair`, `first_person`) | **new** | Aiming narrows the view smoothly, can hide the crosshair (the scope draws its own) and forces first person until released. |
| View kick (`shot.kick`) | **new** | The shooter's own game turns their aim up per shot, as a player would. No server work, no bandwidth. |
| Weapon sounds shipped in the Add-On (`sounds`) | **new** | `.wav`/`.ogg` files named by key from state `sound` fields and from rules. They are part of the content identity, so everyone has the same files. `local` sounds are heard only by the shooter. |
| First-person offset and rotation (`eye_offset`, `eye_rotation`) | present (rotation **new**) | `eye_rotation` was read by the importer but not used for Add-On images. |
| Draw a custom view model in first person | **new** | Client code draws in view space (below). The Commando rifle is a box-model rifle drawn this way, kicking back when the clip drops. |
| Held weapon seen by others (third person) | partial | The image's DTS model shows. An Add-On weapon with no model shows nothing held. Client code can draw one from `players()` slot 15 (the held image's kind). A native box-model item format waits for the box-model animation work. |
| Weapons that fire from rules and creatures (`fire`) | **new** | A rule launches a projectile of its own weapons: turrets, creature guns, fireballs, traps. |
| Spread, several projectiles, recoil push (`shot`) | present | Already in schema 2. |

### HUD, UI and camera

| Seam | Status | Notes |
|---|---|---|
| JSON HUD panels (rows bound to rule state, key buttons) | present | Four corners, up to 16 rows and 8 keys. |
| Center and bottom prints from rules (`center_print`, `bottom_print`) | **new** | v20's `centerPrint`/`bottomPrint`, per player or to everyone, budgeted per Add-On. |
| Ammo counter | **new** | Drawn by the game for any image with `ammo`, red when empty and amber at a quarter; `ammo.counter: false` leaves it to the Add-On (client code reads the rounds through `view`). |
| Hide the crosshair | **new** | Through `zoom.crosshair: false` while aiming. |
| Draw anything on the screen (client code screen space) | **new** | `material_space(m, 2)` draws in screen space, from -1 to 1 with y up and x scaled by aspect: scopes, hit markers, damage vignettes. |
| Draw over the world in the view (view space) | **new** | `material_space(m, 1)`: camera-relative, always in front of the world, at the normal field of view, so a gun does not stretch while zoomed. |
| Read the player's view (`view`: fov, aspect, size, first person, aiming, alive, ammo) | **new** | Lets client code draw sights only while aiming, a gun only in first person, a counter of its own. |
| Text drawn by client code | missing, **next** | Planned `ui.panel`. Until then, JSON panels and prints carry text. |
| Hide or replace the base HUD (health, chat, tool bar) | missing, **next** | Needs a per-slot "replaced by" in the HUD slots. It is small, but it touches UI the first-impressions work is changing. The crosshair and prints cover the Commando needs. |
| Camera distance per body (`camera_distance`) | present | |
| Forced first person while aiming | **new** | |
| Scripted cameras (top-down, fixed, side-scroller) | missing, **next** | A Mario-style side view needs the camera and the movement axes both changed. That design belongs with package movement controllers (below), not a camera-only override that leaves controls wrong. |

### Movement and physics

| Seam | Status | Notes |
|---|---|---|
| Every motor constant per archetype (`movement`: 45 names, gravity, jump, speeds, jet, swim, step, slope, crouch, air control…) | present | This is most of lpsroo's list. Changes are predicted on the client, because archetypes are content everyone has. |
| Switch a player's constants at run time | present | `set_archetype` to a sibling archetype (a moon-gravity body, a sprint body). |
| Tweak one constant for one player at run time | missing, **next** | Needs a replicated per-player tuning delta, so prediction stays correct. Swapping archetypes covers modes today. |
| Collision `box` or `ball` bodies, riding, mount points | present | |
| Push, tumble, hold, throw players, vehicles and entities | present | `physics` capability. |
| New vehicles and physics objects without code | present | `vehicles.json`. |
| Package movement controllers (wall-run, double jump, grapple) | missing, **next** | Tier-2 wasm controllers that run on host and client for prediction (client-sandbox "Prediction"). This is the right home for Mario-style movement. It is a large design, not a quick seam. |

### Rules and modes

| Seam | Status | Notes |
|---|---|---|
| Rhai rules with commands, state, capabilities and budgets | present | |
| Hooks: `on_join`, `on_tick`, `on_death`, `on_loadout` | present | |
| Hooks: `on_spawn`, `on_leave`, `on_damage` (change or cancel any player damage) | **new** | Friendly fire, armour, fall damage, headshot rules. |
| Hooks: `on_entity_damage`, `on_entity_death` | **new** | Scoring creatures, bosses, drops. |
| Loadouts (`give_item`, `on_loadout`), heal, ammo | present (`heal`, ammo **new**) | |
| Game modes in Start Game (`mode`: Add-Ons plus a map) | present | |
| Engine decisions a package answers (`respawn`, `build` policies) | present | |
| Round lifecycle and win conditions | partial | Rules build rounds from `on_tick`, state and `respawn`, as the samples do. A built-in round seam waits for a second real mode that needs the same shape, per the stress-lab rule that two systems justify a seam. |
| `schedule` (run later) | not worth it now | `on_tick` with a tick counter in state does the same, deterministically, with no new queue to budget. |
| `random.seeded` | present | `seed()`, `noise`, `hash3`. |
| `projectiles.spawn` | **new** | As `fire`. |

### Sounds

| Seam | Status | Notes |
|---|---|---|
| Add-On sound files for weapons | **new** | |
| Sounds from rules (`play_sound` at a player's ears, `sound_at` in the world) | **new** | Weapon-pack keys or v20 profile names. |
| Sounds from client code (`sound_play`, `sound_at`) | present | |
| Mode music and ambient loops | missing, **next** | Client code can play one-shots. A looping music channel needs the audio mixer's music bus, which the base game has no use for yet. |

### Entities and NPCs

| Seam | Status | Notes |
|---|---|---|
| Scripted creatures (`entity`: model, think, speed, health, labels, steering, driving with `control`) | present | |
| Creatures hit by guns, hammers and blasts | **new** | `TargetId::Entity`. The player driving one cannot shoot it. Fire does not burn creatures, because the hook decides. |
| Creatures that shoot (`fire` from a think) | **new** | The sentry in the sample. |
| Scaled creatures | **new** | |
| Pathfinding | missing, not worth it now | Steering and `aim()`/object queries cover chasers, turrets and patrols. Grid pathfinding over a brick world that changes every tick is a large system that no Add-On has asked for yet. |
| Crowd separation | missing, **next** | HANDOFF item 6: shoulder-to-shoulder creatures jam. A separation term in entity steering, owned by the entity-perf lane, which is changing entity stepping. |
| Driven creature prediction | missing, **next** | HANDOFF item 3: a driven creature lags by the round trip. It needs per-tick body replication like player poses. |

### Engine variables (lpsroo)

Exposed today, by name, as data:

- the 45 motor constants and health per archetype;
- every weapon field, including ammo, zoom, kick and sounds;
- every vehicle field (flight, steering, threads, smash, shove);
- entity speed, health, think rate and scale;
- server settings, through the admin screen.

Missing, and next:

- per-player live overrides (above);
- the world environment: sky, fog, sun and time of day from a package or a
  rule. This is HANDOFF item 6. It waits because the weather and day-cycle
  code is being reworked in the renderer. The seam would be a replicated
  `environment` value a rule sets.
- world gravity for loose physics bodies.

### Blocks and the world

| Seam | Status | Notes |
|---|---|---|
| Generated chunk worlds (`world` with `generate`) | present | |
| Place and remove bricks from rules | present | |
| Block content (per-face textures, flipbooks, `set_block_state`) | partial, **next** | HANDOFF item 1: it loads, saves and replicates but is not drawn. Drawing it is renderer work on `world_scene`, which the brick-perf lane is rebuilding now. It should land there, not collide with it. |
| Native brick format without v20 files | missing, **next** | Listed in the modding guide's "Still being built". |

### Client code

| Seam | Status | Notes |
|---|---|---|
| Sandboxed WebAssembly and WGSL, trust once per server | present | |
| World, view and screen spaces | **new** | |
| The player's view and ammo | **new** | |
| Each player's archetype, held image and crouch in `players()` | **new** | Slots 14 and 15 are kind numbers from `archetype_kind`/`image_kind`; flag 4 means crouched. |
| Input beyond the focused panel | missing, **next** | Reading movement keys in client code is a keylogging question. The answer is declared bindings the player sees and can rebind, which is also how a Reload key should work. The Commando uses a HUD panel key (G) and `/reload`. |
| Elevated code (`net.http`, `files.addon_folder`) offered to joiners | missing, **next** | Needs the stronger prompt (client-sandbox "Not built yet"). A conversion does not need it. |
| Text, textures and render targets | missing, **next** | `ui.panel`, `render.texture`. |

## Not worth it

- **A general TorqueScript VM.** The product contract forbids it. Imported
  v20 Add-Ons are ported, not emulated.
- **Native (unsandboxed) mod code.** A total conversion should never need
  more trust than "Trust and join". Everything above works inside the
  sandbox or on the host's Rhai.
- **A skeletal animation format of our own.** Box models plus client code
  cover original conversions, and v20's DTS covers the classic look.
- **`schedule`.** It duplicates `on_tick` (above).

## What the Commando sample proves, and how to run it

```sh
cargo test -p bri-sim --test commando
cargo test -p bri-client-sandbox --test commando
cargo test -p bri-client-sandbox --test commando -- --ignored   # offscreen PNGs
cargo test -p bri-package-runtime --test samples
```

The tests cover:

- a joiner becomes a commando: the archetype, 150 health, no jet, and the
  rifle in hand with 8 + 24 rounds;
- two shots and a reload leave 8 + 22;
- two hits drop a dummy, which scores and pays 8 rounds through
  `on_entity_death`;
- a kill in a minigame scores, counts the streak and refills;
- a sentry's own rounds hurt players outside any minigame and credit nobody;
- the client code draws the rifle in view space only in first person while
  holding it and alive, kicks it back when the clip drops, and draws the
  scope in screen space while aiming;
- with `--ignored`, it renders offscreen on a GPU adapter (checked here on
  Mesa's software Vulkan).
