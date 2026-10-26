# Weapons

A weapon Add-On is an `assets/weapons.json` file, listed in `provides` as
`{ "kind": "weapons", "id": "your-id:weapons/main", "file":
"assets/weapons.json" }`. It is the same format **Import Add-On** writes
for v20 weapons: `items` (what players hold), `images` (the held model and
its firing states), `projectiles`, `damage_types` and `explosions`. Model
and icon paths may point at base game files; the sample reuses
`Add-Ons/Weapon_Gun/pistol.dts`. An item's `icon` may also be your own
PNG (up to 512 pixels a side), named without `.png` relative to
`assets/`: the Gravity Gun's `"icon": "icons/gravity_gun"` is
`assets/icons/gravity_gun.png`. With neither, the item shows its first
letter.

An item, image or projectile `model` may also be your own model: a native
model file (`*.shape.json`, the format of `bri_content::shape`: x right, y
up, -z forward) relative to `assets/`, such as
`"model": "models/pick.shape.json"`. Nodes `muzzlePoint` and
`ejectPoint` say where a gun fires and throws its casings. Each material names a PNG
beside the model (`pick_wood` draws `pick_wood.png`, up to 1,024 pixels a
side), as a vehicle model's do. A node called `mountPoint` is where the hand
holds it. A node called `supportPoint` is where a second hand holds a two-handed
image (`aim_both`) on imported bodies; without it that hand goes halfway
from `mountPoint` to `muzzlePoint`. A `detail9999` detail is what the holder sees in first person
and the lower details what everyone else sees, so an image state's
`sequence` (the pick's `"fire"`) can swing the first-person copy alone.
Its box is its bounds for dropping; `bri-addon-check` names a model or
texture it cannot find. `bri-client`'s `own_model_tool` test fixture writes a
small example.

An item has one look wherever it is: in a hand (first or third person),
dropped, on a spawn brick, in a mirror and as its icon. The look is its
image's: the image's `model`, its `color` when `color_shift` is on, and
its skin. An item whose image has no model draws the item's own `model`.

A skin is your own shader drawn over every copy of one of your images,
puffed over its model. Put `looks.json` in `assets/`:

```json
{ "schema_version": 1,
  "images": { "gravity-gun-tool:image/gravitygun":
    { "skin": { "shader": "skins/alien.wgsl", "color": [0.3, 0.95, 1.0],
                "energy_states": ["Grab"] } } } }
```

The shader is a WGSL file in your Add-On, written and limited like an
Add-On's own client shaders (see
[client-sandbox.md](../architecture/client-sandbox.md)): the game
draws it in world space with your image's model at rest, and gives each
copy `params[0]` = `color` and its energy (1 while its holder's image is
in one of `energy_states`, else 0), `params[1]` = the direction sunlight
travels and a seed for that copy, `params[2]` = the sun's colour and
`params[3]` = the ambient light. Skins share an Add-On's GPU budget; one
that runs far over it stops, and items draw their plain models. A skin
that names an image that is not yours, a colour outside 0 to 1, or a
shader that is missing or does not compile is logged, and the item draws
without it. A skin is WGSL, so it is held to the same trust as Add-On code
([What players are asked to trust](trust.md)): a server's skins draw on a joiner's screen only once they
trust that server's code, and the item draws plain until then.
`skins/alien.wgsl` in `gravity-gun-tool` is the Gravity Gun's.

A tool whose image has `"paint_tint": true` (below) is dropped in the
colour it was held in.

An icon can instead be drawn from the item's own model on each player's
machine, so it matches the stock icons without shipping a picture of
base game art. Put `<icon>.render.json` beside it:

```json
{ "schema_version": 1, "pose_like": "v20.weapon.printgun",
  "look": { "skin": {} } }
```

`clockwise_quarter_turns` optionally rotates the icon on screen by 0 to 3
clockwise quarter turns (default 0), preserving its model, lighting and skin.

`pose_like` names a stock item: its model is fitted to its own icon's
outline to find the side profile it was drawn in (which way its nose
points across the picture, and how far it is tipped and turned). Your model
is drawn in that profile on its own axes: forward is +Y and up is +Z, as
item models are held, or from `mountPoint` towards `muzzlePoint` when it has
both. So its nose and grip point the way the stock item's do. It is sized
to its own bounds to fill the box the stock drawing fills, with a clear
border on every side, on a clear background. The model is the item's in
play. `look.base` is its colour, by default the item's colour in play
(its image's tint); `"textured": true` draws the model's own
textures and colours instead (times `base`), so a tool of wood and iron
shows both. The
optional `skin` is the Gravity Gun's alien shell: a dark `shell` with glowing
`veins` (by default the colour of the item's skin in `looks.json`), puffed
out by `puff` (default 0.012) as it is in play. If the icon
cannot be drawn, the item keeps its PNG or letter and the log says why.
The icon is drawn once on a background thread while the game loads (the
PNG or letter shows until it is ready) and kept in the client state folder
under `item-icons/`, named by a hash of the models, the stock icon and the
request, so later runs show it at once.

An image with `"paint_tint": true` is held in its holder's spray colour,
the palette colour they last picked with the paint keys, as a colour spray
can is: a tool that paints with that colour shows it.

An image with `"light": { "radius": 20, "color": [1, 1, 1] }` lights the
world around it while a player holds or wears it, as a v20 image's
`hasLight` with `lightType = ConstantLight` did; the importer reads those
fields (`lightRadius` up to 100, `lightColor`). Worn in a paint colour it
lights in that colour: Capture the Flag's flag glows in its team's colour on
the carrier's back. Other light types are noted, not drawn yet.

The fields you are most likely to change:

| Where | Field | Meaning |
|---|---|---|
| projectile | `speed`, `gravity`, `lifetime_ticks` | how fast, how much it drops, how long it lives (120 ticks = 1 s) |
| projectile | `damage`, `impulse`, `vertical` | hurt, and how hard it shoves |
| projectile | `ballistic`, `elasticity` | bounces, and how much |
| projectile | `elasticity_surfaces`, `bounce_surfaces` | what a bounce keeps and the effect it makes, by authored surface kind: `{ "metal": 1.25, "sand": 0.375 }` multiplies `elasticity`, `{ "wood": "pack:explosion/knock-wood" }` names the bounce's effect or sound-only explosion in place of `bounce_effect` |
| projectile | `decal` | the mark it leaves where it hits a brick or the map (its meshes and ground), a bullet hole: `{ "size": 0.15, "color": [0.04, 0.035, 0.03, 0.9], "seconds": 30 }`, a soft disc `size` across (0.02 to 12, times the projectile's scale) on the face it hit, darkest in the middle, gone after `seconds` (0.5 to 600, fading over the last one) or when the brick goes (a mark on the map stays until it fades; one on a map mover's collision, a door, goes where the door goes); each player keeps the newest 512. `color` and `seconds` may be left out (near black, 30). None leaves no mark, as v20's own projectiles. A projectile that goes off in the air or by its `fuse` marks the surface below it, within its blast radius (at most 6 units), with the same decal: a grenade's scorch |
| projectile | `pass_source` | `true` lets it fly through its shooter's body (a thrown grenade, which meets every other body and bounces off it with `collide_players`, as `ThrowWeaponPickup.Touch` skips the Instigator). Off: it may meet the shooter once it has left their body |
| projectile | `settle_speed` | a bounce that would leave it slower than this (native units a second) stops it and plays no bounce effect, sound or `on_bounce` children, so a grenade lying on the floor is not an impact every tick (a thrown grenade under 20 source units a second stops, `ThrowWeaponPickup.HitWall`). 0 leaves every bounce an impact. Test: `crates/weapons/tests/addon_seams.rs` |
| projectile | `arm_ticks`, `arm_ticks_max` | a ballistic projectile bursts on contact once it is `arm_ticks` old, and bounces before. With `arm_ticks_max` above `arm_ticks`, each shot arms at a tick drawn between the two (AA's M203: 0.1 to 0.7 s), the same on the host and every machine |
| projectile | `stick_to_bodies` | `true` sticks it to any player or vehicle it hits, whatever its speed or angle, and it rides along on them until it goes off (a sticky grenade: pair it with `ballistic`, an `arm_ticks` as long as its `lifetime_ticks` and `explode_death`). If the body leaves the game it stays where it was. Off: v20's sticking, in place, only to what it meets fast and steep enough (`min_stick_speed`, `bounce_angle`) |
| projectile | `fuse` | a fuse that starts at a moment of the flight: `{ "start": "bounce", "ticks": 60, "not_before_ticks": 180, "effect": "fuseLit" }` goes off `ticks` after its first bounce (`start: "rest"`: after a bounce leaves it slower than `rest_speed`, or it sticks to anything), never before the projectile is `not_before_ticks` old, and plays the explosion effect `effect` where it starts. A frag grenade's timer starts at its first bounce, a plasma grenade's when it lies still. Pair it with `ballistic`, an `arm_ticks` as long as `lifetime_ticks` so it does not burst on contact, and let `lifetime_ticks` outlast the fuse. Test: `crates/weapons/tests/tactical_seams.rs` |
| projectile | `explosion.core_radius`, `explosion.edge_damage` | a blast whose damage holds, then falls in a line: full `damage` out to `core_radius` (units), straight down to `edge_damage` times `damage` at `radius`, and nothing past it (Halo's grenades: 120 near the middle, 80 at the edge). `edge_damage` left at 0 keeps v20's quadratic fall from the middle |
| projectile | `explosion.kill_radius` | within this distance (units, scaled with the projectile) of the blast's centre whatever the blast reaches takes a lethal hit, whatever the falloff gives: a rocket's kill radius. 0 is none | 0 to 100000 (default 0) |
| projectile | `impacts` | the explosion effect it makes by what it lands on, in place of `explosion.effect`: `{ "vehicle": "sparks", "map": "dust", "brick": "chips", "player": "puff" }`, each an explosion effect as `explosion.effect` names one (`player` also covers Add-On creatures, `map` the map's terrain, buildings and moving shapes). One left out, and a burst in the air, play `explosion.effect`; the blast's damage and push stay `explosion`'s. A ray lands it as a flying round does |
| image state | `ticks` | how long a state (`Fire` is the reload time) lasts |
| image | `shot` | several projectiles per shot, their spread and the recoil ([porting.md](porting.md#the-image-shot-field)); `scale` sizes the projectiles against their holder (0.1 to 10) |
| image | `volleys` | up to 4 more sets each shot fires after its own: `[{ "projectile": "...", "projectiles": 1, "spread": 0.0005 }]`, a shotgun's slug after its pellets |
| image | `last_shot` | `{ "shot": {...}, "volleys": [...] }`: what the magazine's last `last_rounds` rounds fire instead (a two-barrel gun's single barrel) |
| image | `state_shots` | `{ "onfire2": {...} }`: a shot fired on entering a state with that script, as onFire's is, so a gun can spread wider as it keeps firing |
| image state | `arm`, `gesture` | the holder's animation on entering it: `arm` on thread 2 (`shiftright`), `gesture` on thread 3, the other hand; `"arm_once": true` does not play `arm` again as the state times out into itself (an arm raised while a throw waits) |
| image state | `cues` | up to 16 things a while after it is entered, as scripts scheduled them: `[{ "after_ms": 450, "thread": 2, "sequence": "plant", "sound": "your-id:sound/pump" }]`, each played even if the state has moved on |
| projectile | `children` | smaller projectiles it throws out as it flies, bounces or explodes: one set or a list of up to 4, each its own `projectile`, `count`, `speed`; `fuse_ticks: [0, 48]` sets each child off after a random time in that range, as cluster bomblets; `max_count` throws a random number from `count` to it; `steps` (`low`, `high`, `offset`, `step`, each `[x, up, back]`) sets each axis of a child's velocity to a whole number from `low` to `high`, plus `offset`, times `step`, as a script that threw embers with `getRandom`; `on_hit` throws them at each thing it hits, bouncing or bursting, never as it dies in the air; `angles` flings each along `(cos a, cos b, sin a)` at `speed`, `a` and `b` whole degrees at random (Tier+Tactical's `PrjLoop_emitPrj`); `redraw` draws the count's limit again before each child past `count`, as a `for(%i = 0; %i < getRandom(3, 5); %i++)` loop; `max_times` stops `every_ticks` after so many |
| projectile | `aura` | hurts what is within `radius` every `every_ticks` as it flies: `damage`, `players_only` to pass vehicles by, `effect` played on each one hurt at its scale, `target_sound` heard by that player alone, `max_pulses` to stop after so many (it flies on), `max_targets` to hurt only the first so many each pulse, `ally_damage` (0 to 100) dealt instead to a teammate or ally of its thrower in a mini-game with weapon damage on, friendly fire or not |
| image | `shot.lob` | `{ "speed": 18.25, "range": 300, "otherwise": 100, "distance_divisor": 3.75, "jitter_steps": [3, 3], "jitter_divisor": [6, 6] }`: lobbed to come down about where the holder looks, as Tier+Tactical's mortar: `speed` along the aim, plus upward the distance from their feet to where the look lands (`otherwise` past `range` times their scale) over `distance_divisor`, plus a little along the world's x and -z |
| image | `cook` | a grenade whose fuse burns from a state in the hand (below) |
| image | `guard` | a shield raised in some states (below) |
| projectile | `fixed_damage` | its direct damage stays as authored at any scale |
| projectile | `falloff` | `{ "near": 10, "far": 50, "far_damage": 5 }`: its direct damage is `damage` up to `near` units from where it was fired, `far_damage` from `far` on, and moves in a line between (a Halo projectile's air damage range) |
| projectile | `homing` | `{ "turn_rate": 0.576, "range": 165, "angle": 0.21 }`: as it is made it picks the nearest player or vehicle (not its shooter) within `range` units and `angle` radians of its heading, then turns toward it by at most `turn_rate` radians a second at the same speed; straight on without one or once it is gone |
| item | `ui_name` | the name players see |
| image | `zoom` | `{ "fov": 20, "on_jet": true, "crosshair": false, "first_person": true }` (or `"magnification": 2`: zoom to that many times the player's own view by tangent, 90 to 53.1 and 70 to 38.6, with `fov` kept as a fallback): aim with the zoom key (and the right mouse button with `on_jet`), hide the crosshair, force first person while aiming. More under "Scopes" below |
| image | `eye_offset`, `eye_rotation` | where the weapon sits in first person: exactly there, relative to the camera, as Torque places it, so a scope whose sight is on the eye line stays centred at any zoom |
| image | `hide_nodes` | the holder's body nodes hidden while the image is held, shown again when it goes: `["lhand", "rhand", "lhook", "rhook"]` for a model that draws its own hands (v20's `hideNode` in `onMount`). Up to 16 |
| image | `both_arms` | `true` holds it up with both arms (`armReadyBoth`), not the mount hand's arm alone |
| image | `scripts` | what each state `script` does, by lower-case name: `{ "onfire": { "arm": "spearThrow", "fire": true, "use_up": true } }` swings the arm, launches the image's projectile (or the entry's own `projectile`) and uses the item up, as a thrown grenade ([torque-equivalents.md](torque-equivalents.md#image-state-scripts-as-data)) |
| image | `follow_arm` | `true` also moves a first-person `eye_offset` image with the arm's actions (shift, plant, swing), as the base game's brick, hammer and spray cans do; off by default |
| image | `movement` | `{ "reloading": 0.75, "aiming": 0.75 }`: the share of the holder's speeds the image leaves (0.05 to 1 each, 1 slows nothing) while its magazine reloads and while the holder aims it with the right mouse button (`zoom.on_jet`: the host sees the jet button held; a zoom on the zoom key alone is the holder's own view and does not slow them). They multiply with each other, a rule's `set_speed_scale` and a bullet's slowdown. | 0.05 to 1 each |
| image | `aim_assist` | `{ "angle": 0.05, "range": 120, "slowdown": 0.5 }`: help aiming at a body the holder could hurt (not a teammate, inside its mini-game's rules). A shot turns onto the middle of the nearest such body in sight whose middle is within `angle` radians of the aim and `range` units (at the holder's scale), before spread; the holder's own look slows to `slowdown` while their aim is on one. Bots firing the gun get the same help. Test: `crates/weapons/tests/aim_assist.rs` | angle 0 to 0.35, range 1 to 1000, slowdown 0.1 to 1 (default 1); off by default |
| image | `rope` | `{ "projectile": "your-id:projectile/chain", "speed": 160 }`: while the holder hangs on a rope (`tether`), every player's game draws it with that projectile's trail, swept from the muzzle to the rope's end as densely as the projectile flying it at `speed` would lay it, as v20 rope tools did by firing a stream of them; nothing is sent for it |
| pack | `sounds` | `{ "your-id:shot": { "file": "sounds/shot.wav", "volume": 0.8 } }`: your own `.wav`/`.ogg` files, named by a state's `sound` and by rules; `local` for sounds only the holder hears, `looping` for a state-long hum or a vehicle's `engine_sound` ([content-kinds.md](content-kinds.md)), `music` for a track rules play with `play_music` (up to 10 minutes, [rules.md](rules.md#music)). A pack may hold 4096 sounds. Do not copy a sound a package you depend on already has: name it by its full id (`other:sound/shot`) in a state, cue, projectile or explosion and it plays that one once the packs are merged. `bri-addon-check` decodes each file as the game does and fails one it cannot play |

**Scopes.** `zoom` takes more for a proper scope:

```json
"zoom": { "fov": 22, "on_jet": true, "jets": false, "crosshair": false,
          "first_person": true, "levels": [10],
          "overlay": "scope/scope",
          "sway": { "degrees": 0.35, "seconds": 4.5, "crouched": 0.3, "moving": 2.5 } }
```

| Field | Meaning | Limits |
|---|---|---|
| `jets` | `false`: with `on_jet`, the right mouse button only aims and the player does not jet while holding it | default `true` |
| `allowed_states` | Optional image-state names that permit visual aiming. Omitted allows all states; `[]` allows none. A restricted profile without a known current state does not aim. | Up to 64 unique ASCII names, 1 to 128 bytes each; each must name exactly one state of this image, matched without ASCII case |
| `levels` | further fields of view the mouse wheel steps through while aiming (back on rolling the other way); each narrower than the last. The step resets when the aim ends | up to 8, 5 to 85 degrees |
| `model_fov` | the field of view the held image is drawn with at each aim step (first for `fov`, then one per `levels` entry, the last repeating), so a scope does not magnify the gun. The image moves between this and its `first_person_view` field as the world zooms in and out. Images without a `first_person_view` are unchanged | up to 9, 1 to 150 degrees |
| `instant` | the view jumps to the aim's field of view and back with no ease, as a scope that snaps into place. The held image's `sights` motion keeps its own `seconds` | `true` or `false` (default `false`) |
| `sensitivity` | look speed while aimed, on top of the usual slowing with the field of view | 0.1 to 4, default 1 |
| `overlay_fit` | optional: `height_black` (default, the fit below), `height` (fitted to the height and centred with the world showing either side) or `stretch` (over the whole screen whatever its shape) | one of the three words |
| `overlay` | your own PNG (named without `.png`, relative to `assets/`) drawn over the whole view while aimed in first person, fitted to the screen's height with black either side, under the HUD. Its transparent middle is the lens; the held model is hidden meanwhile | up to 2048 px a side and 4 MB |
| `sway` | the aim drifts on a figure of eight, `degrees` to each side, once every `seconds`; `crouched` and `moving` scale it. Only on foot, only on the holder's screen, and the mouse turns freely under it; it eases in and out | 0 to 5 degrees, 0.5 to 30 seconds, crouched 0 to 1, moving 1 to 4 |

For a scope permitted only in ready/firing states, add for example
`"allowed_states": ["Ready", "Fire", "WaitRelease"]` using the actual names in
that image's `states`. The client's accepted mounted state gates its scope,
FOV, aim sensitivity, wheel levels, sway and forced first-person view. A barred
state removes the scope overlay so the held model can be seen, and the existing
FOV ramp returns toward normal. Holding aim when an allowed state returns aims
again; the wheel step is kept across this temporary gate. A keyboard Zoom key
cannot bypass the gate by falling back to generic camera zoom.

**Screen flash.** An image's `"screen_flash": { "texture": "fx/flash", "center": [0.255, 0.32], "scale": 1.4, "seconds": 0.023, "flip": true }` shows a picture over its holder's own screen, in first person, each time a state with `"screen_flash": true` is entered (the firing state). `texture` is a PNG named without `.png`, relative to `assets/`, up to 2048 px a side. It is added to the light of the screen under it (an additive blend, black adds nothing) and drawn under the HUD. `center` is where its middle lies as offsets from the screen's middle, in fractions of the screen's width and height (-1 to 1); `scale` multiplies its pixels on a screen 640 wide and grows with the screen's width (0.01 to 16); `seconds` is how long it shows (0.001 to 2); `flip` turns each showing over left to right and top to bottom at random. Other players do not see it, and in third person it does not show.

The profile stays installed while barred: `"on_jet": true, "jets": false`
continues to suppress jet input from RMB. State eligibility is presentation only;
it does not decide whether the host accepts firing, reload or movement. Rules
and native weapon states still own those decisions. Omitting `allowed_states`
preserves the existing scope behavior. This adds no mount identity or same-ID
remount/reset guarantee.

Sway changes only where the holder looks, so other players see nothing
new and it costs no bandwidth.

**Your own model.** A weapons pack may bring its models and textures in
`assets/presentation.json` (beside `weapons.json`, the same format the
base game's item presentation uses: `models` and `textures` keyed by your
own ids, each with its file and sha256) and `assets/item-physics.json`
(each item's box). An item's or image's `model` in `weapons.json` then
names one of those keys, such as `your-add-on:model/rifle`, and a
particle's `texture` may name a texture key. Leave `items`, `images` and
`projectiles` empty in it: the game presents them from `weapons.json`.
Models are the native `shape.json` format (nodes, meshes, materials,
animations); name a `mountPoint` node where the hand holds it and a
`muzzlePoint` where shots leave, and an image state's `sequence` plays one
of its animations (a rifle's `Bolt`).

**Magazines.** An image's `magazine` gives it rounds, a reload and a
reserve, with nothing in a rule:

```json
"magazine": { "size": 30, "ammo": "rifle", "reload_ticks": 240, "reserve": 90,
              "max_reserve": 180, "reload_sequence": "shiftDown",
              "reload_sound": "mag:reload", "empty_sound": "mag:click",
              "display": "Rifle Rounds" }
```

Each shot takes `per_shot` rounds (1 when left out); a shot without them
clicks with `empty_sound` and reloads. The last round, the light key and
`reload(p)` start a reload that lasts `reload_ticks` (120 a second, up to
1200) and fills the magazine from the holder's reserve of its `ammo`.
A reload that starts with nothing loaded may take its own `empty_reload_ticks`, `empty_reload_image_sequence` and `empty_reload_sound` in place of the ordinary ones (a gun whose empty reload also works its slide or bolt). Every gun loading the same `ammo` shares that reserve; each gun keeps its
own rounds, by the tool slot it sits in (two of one gun each keep theirs),
also when thrown and picked up by someone else. `one_by_one`
loads a round per `reload_ticks` (`per_load` rounds with a two-barrel
gun), as a shotgun's shells, and a pull of the
trigger stops the loading and fires. The first gun of an ammo type a
player draws brings `reserve` rounds, never above `max_reserve` (100000 at
most); a new life brings full magazines and starting reserves again. The
holder sees `display  rounds / reserve` at the bottom of their screen,
sent to them alone and only when it changes; with `display_ticks` (up to
7200) it stays up that long each time, and the states whose scripts
`display_scripts` names show it again (a dry pull), as does the light key
when there is nothing to load. A size is 1 to 1000 rounds;
an ammo name is 1 to 32 letters, digits, `.`, `_` or `-`.

A magazine with `"cool_ticks": 30` gets a round back every 30 ticks while the gun is held and the magazine is neither full nor empty, as a gun whose rounds are heat (a Halo plasma gun): nothing is refunded from the reserve, and an empty one reloads as usual.

A magazine with `"from_reserve": true` (size 1) has no rounds of its own:
each throw takes `per_shot` straight from the reserve and nothing
reloads, as Tier+Tactical's counted grenades. With none left the image
leaves the hand while its tool stays selected; more ammo of its kind
(a grenade bag) puts it back, or with `"clear_when_out": true` its tool
goes too. The display shows the reserve alone, and the light key works the
light.

`"remount": true` draws the gun afresh (out of the hand and back, its
draw states again) when the holder picks another copy of it from another
slot; without it the gun stays up and takes that copy's rounds.

A magazine's `supply` says where its rounds come from, as Tier+Tactical's
ammo systems did: `reserve` (the default: shots take the magazine's rounds,
a reload fills it from the reserve), `endless` (a reload fills it from
nothing, the reserve untouched; the display shows `rounds / size`),
`unlimited` (nothing is used, no display), `counted` (shots take the
reserve, no reloads; the display shows the reserve) or `both` (shots take
the magazine and the reserve, a reload fills the magazine free while there
is reserve). `"hide_display": true` shows no display at all.

**Fields from server settings.** A pack's `bindings` let a server setting
decide any field of its items, images and projectiles, as v20 scripts read
a `$Pref::Server::*` global where the field was used:

```json
"bindings": [
  { "setting": "$Pref::Server::TT::Recoil",
    "field": ["images", "mag:image/rifle", "shot", "kick"],
    "values": { "false": null } },
  { "setting": "tier-rules:tt_displaytime",
    "field": ["images", "mag:image/rifle", "magazine", "display_ticks"],
    "scale": 120 },
  { "setting": "$Pref::Server::TT::Ammo",
    "field": ["images", "mag:image/rifle", "magazine", "supply"],
    "values": { "2": "endless", "3": "both" },
    "when": { "$Pref::Server::TT::AlwaysReloadEx": "true" } }
]
```

`setting` is `<package>:<key>` or the v20 global of a server setting
([Game rules](rules.md)'s `scope: "server"`), whichever running Add-On declares it.
`field` is the kind, one of the pack's own ids and the path in it (not its
`id`, `states`, `image` or `item`). `values` maps the setting's value, as
text, to the field's (`null` leaves an optional field out; a value not
listed leaves the field as authored), or `scale` multiplies a number
setting. `when` applies a binding only while other settings have those
values; a later binding of the same field wins. The host plays the pack
with the settings applied and derives it again when the host changes one;
players get the values with the world and derive the same pack. The
result is checked as any pack, so a value that takes a field out of its
range is refused when the host sets it. A gun keeps its rounds and
reserve across a change; new shots, reloads and spawns follow the new
fields. A restart setting changes them at the next start. At most 8192
bindings, 10 steps deep.

A magazine can instead follow the image's own states, as Tier+Tactical's
guns did with their check scripts: `checks` names the flags each state
script sets on entering its state (`"TT_onFireCheck": { "loaded": ["shot"],
"ammo": ["reserve"] }`, each flag `true`, `false` or true when any listed
fact holds: `shot`, `empty`, `full`, `not_full`, `reserve`, `no_reserve`),
`on_reload` and `on_loaded` the flags as a reload starts and as its rounds
arrive, and `reload_state` the state script the rounds arrive with; its
`light_states` (below) are then also the only states a reload starts in.
A check with `"spend": true` also takes the shot's rounds as it loads one
(a burst's later rounds, fired by states that do not run `onFire`), and
one with `"keeps_reload": true` leaves a reload under way unloaded, so a
forced reload is not cut short by the next check. A reload these states
run stops when a tool is drawn or put away.

A pack may fire projectiles of a package it depends on: list their ids in
`external_projectiles` (`"external_projectiles": ["tier1:projectile/tracer"]`)
and name them as usual. They are found when the game loads the packs
together; an image whose projectile no loaded package declares is left
out, with a note.

An image's `shot` can say more of how it fires. `hitscan` (`range`, 1 to 8192 units, and
`moving_range`) lands each projectile at once along a ray, with its
damage and push. The ray meets other players where the shooter's own game
showed them when they fired (lag compensation, up to half a second back),
so a shot that was on target on their screen hits; a flying projectile
meets them where they are; `explosion` names another projectile exploded there in
place of its own, `player_sound` and `other_sound` play there by what it
hit, `flown` names a projectile flown from the muzzle to that point, and
`tracer` (`color`, `width`, `seconds`, and `every`, 1 to 1000, to draw only each Nth round fired since the image came up, as a belt with a tracer every fifth round does) draws a streak to it. A ray from
the muzzle (`from_eye` false) starts at the eye instead when something
stands within `eye_within` units in front of it, so a muzzle poking
through a wall does not shoot past it; with `converge` it heads for the
point the eye looks at rather than along the muzzle. `sounds` (up to 8
pairs of `player` and `other`) has each shot draw one pair, as melee
scripts picked one of two hit sounds per swing; a side a pair leaves out
keeps `player_sound` or `other_sound`. `damage` (-100 to 100) is what
each ray deals in place of its projectile's. `ricochet` (`times` 1 to 8,
`damage`, `shooter`) turns the ray off whatever it meets, mirrored about
the face it hit, up to `times` more landings over the range it has left:
each landing deals `damage` (-100 to 100) more for every one before it,
and once turned it can come back into its shooter, who takes `shooter`
(0 to 1, default 1) of its damage. Every turn is drawn as a streak in the
tracer's look, and `on_damage`'s `info.bounces` says how many turns came
before the hit.
`trace` (not with `ricochet`) gives the ray a strength that starts at 1 and
that each landing on something other than a player spends, as Army Ops'
bullets do: `{ "max_landings": 4, "loss": 0.9, "floor": 0.1, "scatter": 0.5, "surfaces": { "wood":
{ "ricochet": 0.5, "penetration": 0.6 } }, "other": {} }`. At a landing the ray
glances off, mirrored about the face, when the cosine of the angle it struck
at is under the surface's `ricochet` times a draw between 0 and 1 (so shallow
hits glance more often and 0 never does), keeping `loss` of its strength; it
leaves mirrored about the face, less `scatter` times that cosine along the
normal, plus the same along a random direction. Otherwise it goes on through
with the surface's `penetration` of its strength (0 stops it), strayed by
`scatter` times the strength it has lost; otherwise it stops. The range left
after a landing is multiplied by the strength then. The draws are seeded by
the tick and the shooter, so a shot repeats. Which shapes glance or let a ray
through, and a ray that starts inside a shape, are told apart by the shape: a
collision detail of a shape model may carry `"shot": { "ricochet_ok": true,
"penetrable": false, "ricochet": 0.4, "thickness": 8 }`. `ricochet_ok` (default
true) lets a ray glance off it, `penetrable` (default false) lets one go
through it (still only at the surface's `penetration` share), `ricochet` takes
the place of the surface's, 0 to 1, and `thickness` (0 to 64 units) is how far
a ray is taken to travel inside it, in place of the way it measures to the
far face. A ray that goes through a shape without a `thickness` meets the
shape's far face as no landing: it spends the range and goes on, unless the trace sets
`exit_lands` (default off), which makes that face a landing like any other, with
its own glance draw and the surface's `penetration` share again. A detail
with no `shot` leaves everything to the surface name. A surface's `depth` (units, 0 to
1000, default 0 for none) makes thickness cost strength too: through `t` units the ray
keeps a further `1 - t * (1 - penetration) / depth`, `t` being the shape's `thickness` or the way
measured to its far face. A landing on a player stops it unless `trace.players` (a surface:
`penetration`, `depth`) gives a share: the body takes its damage, then the ray goes on out of its
far side with that share of its strength, never glancing off it; with something right behind the
body (within 3 units) the ray ends in it. A strength under `floor` or `max_landings` (1 to 16) landings stops it.
`surfaces` names kinds as map materials and `surface_kind` give them (up to
32); `other` is any kind not named, which stops the ray by default. Each
landing deals the ray's `damage` times its strength then.
`moving_spread` and `moving_projectile` replace the spread and the
projectile while the shooter moves faster than `moving_speed`; `rested`
(`after_ticks`, `spread`, and optionally `still` and `projectile`) is
the truer first shot after a pause. `crouched_spread` is the spread
while the shooter crouches and is not moving (used when tighter than their
standing spread), and `airborne_spread` the spread while they are off the
ground and not riding, in place of every other: a tactical rifle that is
steadiest crouched and wild mid-jump. `error` (`min`, `max`, `rise_ticks`, `fall_ticks`, `recover_ticks`) is a spread that builds as the holder keeps firing and settles when they stop, in place of all of those: the holder's error rises by `1 / rise_ticks` every tick the trigger is held or a shot was fired in the last `recover_ticks`, falls by `1 / fall_ticks` otherwise (0 to 1), and a shot's spread is `min` at 0 and `max` at 1. An image's `state_shots` fire on
entering a state whose script is not `onFire` (`"onfire2": { ... }`),
each a shot of its own, hitscan or not (a knife's weaker stab beside its
slash); a shot with `"free": true` takes no rounds. `recoil` pushes the shooter back along
the aim as they fire (units a second); `recoil_vertical` sets the push
along the aim's vertical part apart, so a machine gun can push only up or
down. `kick` shakes the holder's view
with each shot (`amplitude` 0 to 1, `frequency`, `seconds`); with a
`radius` (up to 100) other players within it feel it too, weaker with
distance, as a v20 recoil blast's camera shake. The image's
`volleys` fire more projectiles after its own (a shotgun's close blast),
and `left_image` holds a second image in the left hand that shares the
holder's ammo; a state script `onFireAkimbo` pulls its trigger. A
projectile's `slow` (`{ "divisor": 2 }`) slows the player it hits for a
moment, more with each hit down to a floor. An item with `"hidden": true`
is put in the world only by rules (`drop_item`): no spawn list, loadout or
`/give` offers it.

The light key reloads a gun with a magazine unless its image gives the key
its own command (below). With `"light_states": ["Ready", "Empty"]` it
reloads only from those states, and works the light as usual whenever it
cannot reload (a full magazine, no reserve), as the hl2 ammo system did.

**Cooked grenades.** An image's `cook` lights a fuse in the hand as a
state script runs, and the shot it fires next carries what is left:

```json
"cook": { "script": "onpindrop", "fuse_ticks": 480, "burst_height": 2.0,
          "print": "{seconds} second{s} left", "print_ticks": 12, "print_seconds": 0.15 }
```

The holder reads `print` in the middle of their screen every
`print_ticks` while it burns (`{seconds}` the time left to a tenth, `{s}`
an `s` unless it is exactly 1; `first_print` replaces the first). Held for
the whole `fuse_ticks`, the image's projectile goes off `burst_height`
above their feet and they put it away, keeping the grenade. Putting it
away first puts the fuse out. The
[Commando rifle](../../packages/samples/sample-commando-rifle/assets/weapons.json)
is a plain scoped rifle: raise it, fire, let go, fire again.

**Shields.** An image's `guard` protects its holder while their right
hand's image is in one of its `states`:

```json
"guard": { "states": ["Ready"], "front": { "up": 0.7, "above": 3, "down": 0.8, "below": 4 },
           "projectile_damage": 0.1, "damage": 0.25, "push": 0.5,
           "reflect": true, "reflect_kill": "Reflected", "hit_explosion": "ns:explosion/clang",
           "sounds": ["ns:sound/bing"], "durability": 20, "break_explosion": "ns:projectile/pieces" }
```

It covers what strikes the side the holder faces. With `front`, looking up
past `up` it covers hits landing higher than `above` (times their scale)
below the middle of their body, and looking down past `down` those lower
than `below`. A projectile it stops does `projectile_damage` of its damage
and `push` of its push, plays `hit_explosion` and one of up to 8 `sounds`
at the holder, and with `reflect` flies back the way they look as theirs
(a kill by it reads as the damage type `reflect_kill` when the pack has
one). A ray it stops does `projectile_damage` and is not sent back; any
other harm it covers does `damage`. After `durability` stops the shield
breaks: `break_explosion` goes off and the item leaves their tools; with
`"bots_keep": true` a bot's never wears out. With `fall_damage` (0 to 1),
a fall or crash the holder meets looking the way they were going (down,
for a fall) does that share, with `hit_explosion` at twice their scale.

Shots hit players, vehicles, bricks and Add-On creatures. A creature's
own rule decides what the hit does (`on_entity_damage`).

An image with a projectile and an `onfire` command (in `commands.states`)
does both: the round flies and the command runs, as a v20 gun's `onFire`
that called `Parent::onFire` did. That is how a rule counts a magazine.

A tool rather than a gun: give its image `"command": "your-rule:command"`
and no projectile. Its `onFire` state then runs that command of your rule
Add-On for the holder, with `aim()` resolved where they look (declare
`aim_reach` on the command). The Duplicator's `duplicator-tool` does this.
A state's `"arm"` swings the holder's arm as the image enters it
(`"armattack"` to strike, `"root"` to rest); v20 chose the swing from the
image's name in script, so give your own tools this instead, for example
a swing on `PreFire` and a rest on `StopFire`.

A package-fired gun may keep its actual `projectile` reference alongside
`command`. The top-level `command` replaces the native shot; it does not
launch a second projectile. For a constant-speed shot, that reference lets
v0.2.8's existing bot executor read flight, range and blast. Use
`"bot": {"fire": "hold"}` for an authored automatic cycle, or `"tap"`
for a semi-automatic cycle. The package still owns ammo and reloads through
the ordinary image states. This supplies basic use of an assigned gun;
the native attack planner does not model arbitrary package commands.
Variable-charge shots and cooked grenades require richer descriptions.

To let bots see that such a gun is empty, say where the package keeps its
rounds, in per-player state the behaviour declares:

```json
"bot": {
  "fire": "hold",
  "ammo": {
    "package": "rallypack-rules",
    "loaded": { "key": "rounds" },
    "spare": [{ "key": "spares" }]
  }
}
```

`loaded` is a number, or the field of an object value (`"field": "n"`);
each `spare` entry is a number or a list of numbers (spare magazines). A
bot with no loaded and no spare rounds treats the gun as no weapon: it
takes up another one it holds, or does not attack, and never fires the
empty one. With spare rounds left it still fires, since the gun's own
dry-fire and reload states fetch them; the bot never reloads or tops up
anything itself. A count the package has not stored yet is unknown, never
empty. `bot.ammo` is only valid on an image the package fires (`command`),
and it makes the downloaded weapons data newer than older clients read.

More moments can run commands through the image's `commands`:

```json
"commands": {
  "states": { "ongrab": "gravity-gun:grab", "onrelease": "gravity-gun:release" },
  "wheel": "gravity-gun:reel"
}
```

`"light": "your-rule:reload"` there runs when the holder presses the light
key with the image in hand, instead of turning on their light, as v20
Add-Ons did by packaging `serverCmdLight`. `"cancel": "your-rule:mode"`
runs when the holder presses the cancel key (v20 Add-Ons packaged
`serverCmdCancelBrick` for a rifle's grenade launcher or the next kind of
round); the key still clears their ghost brick. Declare such commands
`tool_only` ([Game rules](rules.md)).

`states` maps a state's `script` (lowercase) to a command, run as the
image enters that state: a state with `"down"` to a charging state whose
`"up"` leads to the firing state gives a press-and-hold charge.
`jet` runs when the holder presses jet (the right mouse button) with the
tool in hand, as v20's `onTrigger` slot 4 did; players who can jet still
jet. `wheel` runs while the trigger is held with the mouse wheel's notches
as its one `int` argument (positive rolled forward, away from you), and
the wheel then does not change tools; declare `"args": ["int"]` on that
command. `mount` runs as the image goes into the holder's hand and
`unmount` as it leaves (v20's `onMount` and `onUnMount`), so a scope that
slows its holder can push a slower player type and pop it again. The
Gravity Gun's `gravity-gun-tool` uses `states` and `wheel`: hold left
click to grab, roll to reel, let go to drop or fling.

`light` and `cancel` take those keys. `shift`, `rotate` and `plant` take
the brick keys whenever the player has no ghost brick out and holds no
copy to place with the tool (a copy moves and plants with them as
always), with v20's `serverCmdShiftBrick` arguments: declare `"args":
["int", "int", "int", "bool"]` (studs away from and to the left of the
player's facing, plates up, and whether it was the super shift),
`["int"]` for `rotate` (1 clockwise seen from above, or -1), and none for
`plant`. `seat` takes the next and previous seat keys on foot, with 1 or
-1 (`["int"]`). A duplicator's selection box uses them. `mount` and
`unmount` run as the image comes into and leaves the holder's hand
(v20's `onMount` and `onUnMount`), however it happens; declare the
`unmount` command `while_dead`.

An item with no `image` is picked up but held by nobody: an ammo box or a
health pack whose `on_pickup` answers `"take"`. An item's `label` (up to
32 characters) shows above it where it lies, as v20's `setShapeName` on an
item: an ammo box's round count. One with `"rotate": true` turns slowly
where it lies, once every three seconds, as a v20 item whose `onAdd` set
`%obj.rotate`. Every item needs a
`ui_name`, the name players pick it by.

`effects` holds the pack's own particles, emitters and lights in the base
game's effects library format (ids in your namespace, such as
`your-id:emitter/flash`; a particle's `texture` is the base game's, such
as `base/data/particles/cloud`, or a key of your item presentation's
`textures`), and
`explosions`, each explosion's effect: `{ "id": "your-id:explosion/boom",
"lifetime": 0.3, "emitters": [...], "light": ..., "burst": [emitter,
count, radius] }`. An image state's `emitter` and a projectile's `trail`
name an emitter by id; an explosion's effect is found by its explosion's
name (`boom`), as the base game's are. Import Add-On writes these from a
v20 Add-On's datablocks.

**A flash that lights the ground.** An image state's `emitter` may name
one of your explosions' effects by id (`your-id:explosion/flash`)
instead of an emitter: the state then plays it whole, its emitters and
its `light` for the effect's `lifetime`, at the muzzle. A muzzle flash
with a `lifetime` of 0.05 and a warm light of radius 6 lights the grass
around the gun for each shot. A gun on a vehicle (the image a vehicle's
`weapon.effect` names) does the same, and its states with
`eject_shell` throw the image's `casing` from its muzzle, carried on
with the vehicle, as a held gun's do.

**Worked example: a rifle with a burst mode.** Two Add-Ons, as the
Commando sample splits them: `mag` provides the weapons pack (the rifle
`mag:weapon/rifle`, whose image has a `magazine` of `"ammo": "rifle"` and
fires `mag:projectile/round`, an ammo box `mag:weapon/ammo` with no
`image`, and a sound `mag:ping`), and `mag-rules` provides the rule, lists
`mag` in its `dependencies` (so the item hooks hear `mag`'s items and
rounds) and asks for the `player`, `damage`, `chat` and `effects`
capabilities. The magazine counts, reloads and shows its rounds itself;
the rule adds what it does not. Its `behaviour.json`:

```json
{
  "schema_version": 1,
  "script": "mag.rhai",
  "on_pickup": true,
  "on_projectile_hit": true,
  "commands": [
    { "name": "fired", "tool_only": true },
    { "name": "mode", "tool_only": true }
  ],
  "state": { "player": { "burst": { "default": false } } }
}
```

The rifle's image sends its moments to the rule: `"commands": { "states":
{ "onfire": "mag-rules:fired" }, "cancel": "mag-rules:mode" }`.

```rhai
fn cmd_fired(p) {                      // the image fired one round
    let me = player(p);
    let m = me.magazine;
    if get_player(p, "burst") && m != () && m.rounds > 0 {
        // A second round from the muzzle, out of the same magazine.
        fire("mag:projectile/round", me.mx, me.my, me.mz,
             me.lx * 200.0, me.ly * 200.0, me.lz * 200.0, p);
        set_rounds(p, m.item, m.rounds - 1);
    }
}
fn cmd_mode(p) {                       // the cancel key
    set_player(p, "burst", !get_player(p, "burst"));
    center_print(p, if get_player(p, "burst") { "Burst" } else { "Single" }, 1.0);
}
fn on_pickup(p, item, info) {
    if item == "mag:weapon/ammo" {     // used up, never held
        give_ammo(p, "rifle", 30);
        return "take";
    }
    ()
}
fn on_projectile_hit(hit) {
    if hit.kind == "brick" { sound_at("mag:ping", hit.x, hit.y, hit.z); }
}
```

`take_item(p, "mag:weapon/rifle")` takes the rifle back (a thrown weapon),
`drop_item("mag:weapon/ammo", x, y, z)` leaves a box in the world, and
`player(p).tools` lists what someone carries by slot, for a rule that
refuses a second rifle. Typing `/mode` in chat is refused because the
commands are `tool_only`; the image still runs them.

Keys of `damage_types` and `explosions` are their `name` in lowercase, and
a projectile names its damage type as `$DamageType::<name>`. A damage type
with `"special": true` is a special kill (Support_SpecialKills): its
message's `%3` is replaced by the killing weapon's icon, so `"%2 [sent
back]%3%1"` shows both. Two Add-Ons may declare the same name: each
keeps its own. The pack loaded later has its declaration kept as
`<its id>:<Name>`, and its own projectiles, images and rules that name the
bare `<Name>` (`damage`, an `on_damage` answer) get that one. The earlier
one is replaced for everyone only when the later Add-On depends on its
package, as v20's re-declaration did; the same declaration twice is one.
Everyone in a
game needs the same weapons, so give the Add-On to the people you play
with.

## Idle fidget

A state may carry `"fidget": { "seconds": [10, 15], "to": 7, "skip": 0.25 }`: once the
image has rested in that state for a random time in `seconds`, it enters state `to` (by index),
unless the draw falls under `skip` (0 to 0.95), which waits again for a new time. A trigger
transition or timeout out of the state comes first and ends the wait, and the state `to` goes
where any state goes (back to the rest by `timeout`, to a shot by `down`). The draws are
deterministic in the world's tick and the holder. Halo's rifle waits 10 to 15 seconds and
plays its 118-frame posing clip.

## Timing the check reports before the game runs

`bri-addon-check` (and the port tool) reads each weapon image's state machine
in the order the game runs it (loaded, ammo, trigger, then timeout once the
time is up) and names what would misbehave in play:

| Code | Meaning |
|---|---|
| `weapon.zero_time_cycle` | a loop of zero-time states longer than the 16-transition budget: the game unmounts the image mid-use |
| `weapon.idle_cycle` | a loop through package-command states that never waits, so the command fires every tick |
| `weapon.callback_timing` | a state's command runs in the same step as the transition that reads what it changes; package operations commit after the weapon step, so the transition sees the old value |
| `state.value_too_big` / `state.value_large` | a declared per-value default over (or near) the 4096-byte limit |
| `state.scope_too_big` / `state.scope_large` | declared player (64 KiB) or global (256 KiB) state over (or near) its limit |

Each message names the image or key and the states involved.


## Authored contact and ejection effects

A projectile's `impacts.surfaces` maps authored surface kinds to effect IDs.
A matching kind wins over `player`, `vehicle`, `brick` and `map`; an unknown
kind falls back to those target categories and the ordinary explosion effect.
For example, `"surfaces": {"metal":"kit:explosion/sparks", "dirt":"kit:explosion/dust"}`
uses the same mechanism on every map and weapon.

A projectile's `decal` may name a `texture` from item presentation, plus
`surfaces` mapping kinds to texture keys. `blend` is `alpha` (default),
`additive`, `multiply` or `double_multiply`. `size`, `color` and `seconds`
control the mark; textured marks use the entire image and its alpha. Static
map and brick marks share one bounded pool. Declared missing artwork is
reported and omitted. Content without a declared image keeps the stock disc.

Use `profiles` for a surface's complete style, including size and lifetime
ranges, fade duration and rotation. A null profile leaves no mark. Set
`require_profile` to omit marks on unknown surfaces. For example:

```json
"decal": {
  "size": 0.2, "seconds": 30, "require_profile": true,
  "profiles": {
    "wood": null,
    "stone": {
      "texture": "kit/holes/stone", "blend": "double_multiply",
      "size": [0.2, 0.4], "color": [1, 1, 1, 1],
      "seconds": [60, 90], "fade_seconds": 10,
      "rotation_degrees": [0, 360]
    }
  }
}
```

Clients sample the same ranges from the host's shared cue ID. The profile's
art, blend and timing override the whole default style. Default marks can also
set `fade_seconds` and a fixed `rotation_degrees`; omitting them preserves
the stock one-second fade and zero rotation. Fade duration is capped at the
selected lifetime. Profiles use the same texture, size and lifetime bounds as
default marks and enter ordinary content-reference validation.

Particles in `effects.particles` may specify an `animation` with `frames`
(texture keys), a `rate` range in frames/second, and optional `looping`,
`random_start` and `reverse` flags. Without looping, the last frame holds.
Optional `variation` has two RGBA `color` endpoints and a `size` range;
seeded per-particle sampling multiplies the ordinary color/size keys.
Optional `initial_spin_degrees: [min, max]` samples a starting sprite angle
independently of angular velocity; omitting it keeps zero starting rotation.
All frames must be declared presentation textures. The renderer packs original
pixels into atlas pages rather than spending a texture-array layer per frame.

Composite `effects.explosions` can contain up to 32 independently placed
`bursts`. Each has `emitter`, a `count` range, a `radius` range, local `offset`
and local `direction`. The source's +Y axis follows the impact normal or
attachment rotation. Existing `burst` and continuous `emitters` still work;
all burst groups share the ordinary live-particle/source budgets.

`effects.shells` are glowing layers a rule draws over a body with
`body_shell(p, shell, strength, seconds)` (a shield taking a hit). A shell has an
`id`, `facing` and `edge` colours (`[r, g, b, brightness]`: the colour where the
body faces the eye and where it turns edge on), `stand_off` (units off the body at
full strength, squared with the strength, 0 to 2), `ease` (the strength shown is
`1 - (1 - s)^ease` of the fade's remaining share `s`; 1 is a straight fade, more
holds the glow, 1 to 8) and two `noise` layers `{ texture, repeats, scroll: [u, v, depth],
seconds }`: the layers' red channels multiply into the pattern, each scrolling
`scroll` every `seconds` (u and v in repeats). A texture taller than it is wide
is a volume of square slices stacked down it; `depth` moves through it, in
volumes, blending the neighbouring slices. Textures are named as particles name theirs.
Every player sees it, apart from the owner in first person. Tests:
`crates/render/tests/plasma_shell.rs`, `crates/client/src/body_shell.rs`.

An image's `shell_effect` names an authored emitter/composite to play for
`eject_shell` states. It replaces mesh `casing` presentation and uses the posed
`ejectPoint` (or `muzzlePoint` when absent), carrying actor velocity. If the
reliable cue arrives before its image pose, it waits up to half a second.
These sprite casings can opt into the cosmetic point contact/rest and local
contact audio policies described below.

An image's `shell_mesh` throws a static mesh instead, from the posed `ejectPoint`
for `eject_shell` states (not with `casing` or `shell_effect`). It is client-only
and cosmetic: nothing crosses the network, and every client throws its own.
All numbers are the pack's, in native units (y up); `exit_direction` is in the
eject point's frame, the rest in the world's:

```json
"shell_mesh": {
  "model": "your-id:model/casing", "exit_direction": [1, 1, 0],
  "exit_offset": [0, 0, 0], "exit_variance": [5, 15], "velocity": 3, "velocity_variance": 0.8,
  "acceleration": [0, -30, 0], "damping": [0.45, 0.45, 0.45],
  "spin": [1, 1, 1], "respin_on_bounce": true,
  "bounce": 0.4, "rest_speed": 4, "lifetime": 10,
  "sound": "your-id:casing_hit"
}
```

`exit_variance` is the most each throw turns the direction, in degrees, about the eject point's X (pitch) and Y (yaw) axes, and `velocity_variance` the most the speed differs from `velocity`, either way. `damping` is the fraction of each axis's velocity lost per second; `spin` is the
most it turns about each axis, in turns per second, either way, picked per throw
(and again on each bounce with `respin_on_bounce`); `bounce` is the share of the
speed along the surface normal a bounce keeps; a bounce that leaves it slower than
`rest_speed` comes to rest; it is gone after `lifetime` seconds; `sound` is a
sound profile played where it lands. `landing_sounds` maps a surface kind (`brick`, `terrain`, a map's own kinds, `default`) to a sound profile that replaces `sound` on that surface. `owner_frame: true` takes `exit_direction` and `exit_offset` in the owner's yaw frame instead of the eject node's; `inherit_velocity` (default 1, up to 2) is the share of the owner's velocity the casing starts with; `clip_start: true` sweeps from the eject point to the start and stops the casing at a wall in between. The model must be one the pack loads.


Particle `contact` is optional. A definition can supply `restitution` and
`friction` in [0,1], `rest_speed` in native units/second, and `die_on_contact` /
`die_at_rest`, plus `stop_animation_at_rest` to hold its last sprite frame. The weapon adapter sweeps points against the actual map/terrain/
brick collision geometry. These cosmetic contacts never damage or move anything.
Friction removes the authored fraction of sideways velocity per contact; it is
not a continuous tire friction coefficient. Rest requires a supporting surface
opposed to acceleration. Attached flash groups keep their attachment motion and
do not collide. Unset contact preserves stock particle integration.

Contact paths use at most 1/60-second trajectory segments, 128 segments per
particle and 32,768 shared queries per advance. Unprocessed motion waits in place
when this budget is exhausted; particles continue aging. Resting particles expire
normally. These clouds do not sleep/refill, since reconstructing them would require
historical world collisions. Water transitions and attachment to a moving
support are not supplied by this seam.


Optional particle `contact_audio` requires `contact` and explicit package policy:

```json
"contact_audio": {
  "surfaces": {"example_surface": "example:contact_sound"},
  "fallback": null,
  "min_inward_speed": 0.5,
  "cooldown_seconds": 0.1,
  "max_impacts": 8,
  "max_lateness_seconds": 0.05
}
```

These example numbers are invented, not recovered original-game rules. All four
eligibility fields are required. Speed is the inward normal velocity in native
units/second; cooldown uses particle simulation age, and `max_impacts` counts
all real inward contacts, including contacts that fail eligibility or reporting
budgets. Eligibility history advances even when reporting drops an event. Cooldown
is measured between eligible inward contacts before surface lookup or mixer
audibility;
this is an explicit native policy, not a recovered original audibility order.
Lateness is simulated time from the impact to the end of this advance (including
emission pre-age). Expired particles do not generate historical contact bursts.
No repeat/promotion law is inferred from profile names or promotion counts.
Packages must recover that policy or label an explicit native adaptation.

The client resolves the exact original sweep against the same world snapshot.
`surfaces` keys are authored map surface kinds, `brick`, `terrain`, or `default`;
an unmapped/unknown kind is silent unless `fallback` explicitly names a profile.
Profiles use existing prepared sounds and weighted leaf variants, including
per-profile gains and spatial placement at the impact. Unknown or looping sounds
are silent and counted as drops. There are no gameplay or network events.

Only inward physical hits report: support probes, separation, resting frames and
render views never request sounds. Reports cap at 64 per effects advance and
local sound starts at 16 per audio tick, sharing the existing mixer voice budget.
Overflow is counted and discarded; no deferred backlog is replayed. Variant seeds
come from world seed, source/emission identity and impact ordinal, independent of
visual RNG, unrelated audio, vector compaction, report drops and render views.
The optional field retains the existing version-1 effects schema policy; omission
preserves legacy motion and silence. Original repeat/promotion fidelity remains a
separate package-evidence requirement.


An image may set `first_person_origin: "eye"` or `"mount"`. Omission keeps the
existing inference: a nonzero eye offset or rotation selects the eye, and zero
eye fields select the host mount. Explicit `eye` selects eye-local placement even
when its local transform is identity. With `follow_arm: false`, identity eye
placement is exactly the supplied eye transform and does not query the host hand
or apply the model's automatic mount correction. `follow_arm` remains a separate
choice for carrying arm actions. Explicit `mount` uses the ordinary host mount,
image offset/rotation and model correction even with nonzero eye fields. Third
person always retains that ordinary mount path. Base images without an explicit
origin retain stock forced arm following; explicit origins keep their authored
`follow_arm` choice. Native image and authored
presentation selectors must agree; unknown selector values are rejected.


## An independent local held-image view

An image can opt into `first_person_view`:

```json
"first_person_view": {
  "horizontal_fov": 62,
  "near": 0.03,
  "far": 400,
  "scale": 0.7,
  "eye_offset_reference_fov": 100
}
```

These are invented example values. Author the projection and camera-relative
native-unit conversion from your own reference. Horizontal FOV is in degrees;
`near` and `far` must be positive and ordered. `scale` defaults to 1. Values
must also produce a finite, invertible projection and a nondegenerate native
transform. Omission of the entire block keeps the existing view and serialization.

The local held image uses the current stock eye/rotation and this independent
projection, after the world, with fresh depth. Its model and posed attachment
nodes receive the uniform visual scale; its eye translation does not. Optional
`eye_offset_reference_fov` multiplies eye translation by that reference divided
by the same frame's resolved stock horizontal FOV. Leaving it out keeps the
existing eye translation. It does not change third-person scale or placement.

Current custom Add-On camera shots suppress these opted local layers. Composing
an independent held-image view with arbitrary custom camera callbacks requires
separate qualification. World-space effects, diagnostic geometry and other
world-depth consumers finish first; the screen vignette remains above local
images and the HUD follows. MSAA resolves after the final local layer.

The image's skin and explicitly attached local weapon sprites use its local
view. Their immutable owner/slot/image/policy affinity survives particle/source
expiry and holstering. A particle without an eligible exact current image stays
hidden while its original lifetime drains; it is never redirected into another
weapon's projection or into the world. At most the existing image-slot count
allocates current views. A same-image policy mismatch is diagnosed. Shared
sprite and skin budgets still apply across world and local layers. Point lights
remain in the world once; particle size/light radius do not inherit mesh scale.

This rendering policy does not recover source pose, ADS state selection,
lighting or interpolation laws. Those still need an ordinary integrated consumer
and original-reference comparison.
