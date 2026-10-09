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
holds it. A `detail9999` detail is what the holder sees in first person
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
    { "skin": { "shader": "skins/gravity.wgsl", "color": [0.045, 0.83, 1.0],
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
`skins/gravity.wgsl` in `gravity-gun-tool` is the Gravity Gun's: it reads
which part of the model a face is from the model's texture coordinates
and makes the seams and core glow and pulse, leaving the rest to the model.

A tool whose image has `"paint_tint": true` (below) is dropped in the
colour it was held in.

An icon can instead be drawn from the item's own model on each player's
machine, so it matches the stock icons without shipping a picture of
base game art. Put `<icon>.render.json` beside it:

```json
{ "schema_version": 1, "pose_like": "v20.weapon.printgun",
  "look": { "textured": true } }
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
optional `skin` is a dark oily `shell` with glowing
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
| item | `ui_name` | the name players see |
| image | `zoom` | `{ "fov": 20, "on_jet": true, "crosshair": false, "first_person": true }`: aim with the zoom key (and the right mouse button with `on_jet`), hide the crosshair, force first person while aiming. More under "Scopes" below |
| image | `eye_offset`, `eye_rotation` | where the weapon sits in first person: exactly there, relative to the camera, as Torque places it, so a scope whose sight is on the eye line stays centred at any zoom |
| image | `hide_nodes` | the holder's body nodes hidden while the image is held, shown again when it goes: `["lhand", "rhand", "lhook", "rhook"]` for a model that draws its own hands (v20's `hideNode` in `onMount`). Up to 16 |
| image | `both_arms` | `true` holds it up with both arms (`armReadyBoth`), not the mount hand's arm alone |
| image | `scripts` | what each state `script` does, by lower-case name: `{ "onfire": { "arm": "spearThrow", "fire": true, "use_up": true } }` swings the arm, launches the image's projectile (or the entry's own `projectile`) and uses the item up, as a thrown grenade ([torque-equivalents.md](torque-equivalents.md#image-state-scripts-as-data)) |
| image | `follow_arm` | `true` also moves a first-person `eye_offset` image with the arm's actions (shift, plant, swing), as the base game's brick, hammer and spray cans do; off by default |
| image | `rope` | `{ "projectile": "your-id:projectile/chain", "speed": 160 }`: while the holder hangs on a rope (`tether`), every player's game draws it with that projectile's trail, swept from the muzzle to the rope's end as densely as the projectile flying it at `speed` would lay it, as v20 rope tools did by firing a stream of them; nothing is sent for it |
| pack | `sounds` | `{ "your-id:shot": { "file": "sounds/shot.wav", "volume": 0.8 } }`: your own `.wav`/`.ogg` files, named by a state's `sound` and by rules; `local` for sounds only the holder hears, `looping` for a state-long hum |

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
| `levels` | further fields of view the mouse wheel steps through while aiming (back on rolling the other way); each narrower than the last. The step resets when the aim ends | up to 8, 5 to 85 degrees |
| `sensitivity` | look speed while aimed, on top of the usual slowing with the field of view | 0.1 to 4, default 1 |
| `overlay` | your own PNG (named without `.png`, relative to `assets/`) drawn over the whole view while aimed in first person, fitted to the screen's height with black either side, under the HUD. Its transparent middle is the lens; the held model is hidden meanwhile | up to 2048 px a side and 4 MB |
| `sway` | the aim drifts on a figure of eight, `degrees` to each side, once every `seconds`; `crouched` and `moving` scale it. Only on foot, only on the holder's screen, and the mouse turns freely under it; it eases in and out | 0 to 5 degrees, 0.5 to 30 seconds, crouched 0 to 1, moving 1 to 4 |

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
Every gun loading the same `ammo` shares that reserve; each gun keeps its
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

An image's `shot` can say more of how it fires. `hitscan` (`range`, and
`moving_range`) lands each projectile at once along a ray, with its
damage and push; `explosion` names another projectile exploded there in
place of its own, `player_sound` and `other_sound` play there by what it
hit, `flown` names a projectile flown from the muzzle to that point, and
`tracer` (`color`, `width`, `seconds`) draws a streak to it. A ray from
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
`moving_spread` and `moving_projectile` replace the spread and the
projectile while the shooter moves faster than `moving_speed`; `rested`
(`after_ticks`, `spread`, and optionally `still` and `projectile`) is
the truer first shot after a pause. An image's `state_shots` fire on
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
