# HUD panels and regions

A `hud` file is one of two things: a panel (rows of text from a rule's
state, below) or a region layout, which claims a part of the screen and
draws your own textures there ([HUD regions](#hud-regions)).

## Panels

A HUD panel is a JSON file each player's game draws from state it
receives. A rule's Add-On runs only on the host, so a HUD is always its own
Add-On that depends on the rule, as `sample-points-hud` depends on
`sample-survival-points`.

```json
{
  "schema_version": 1,
  "slot": "hud.overlay",
  "anchor": "top_left",
  "title": "SURVIVAL POINTS",
  "background": [0.05, 0.08, 0.12, 0.8],
  "accent": [0.35, 0.85, 0.45, 1.0],
  "text": [0.92, 0.95, 1.0, 1.0],
  "rows": [
    { "label": "Points", "bind": "sample-survival-points:player/points" },
    { "label": "Awarded to everyone", "bind": "sample-survival-points:global/awarded" }
  ],
  "keys": [
    { "key": "N", "label": "Leaderboard", "package": "sample-survival-points", "command": "top" }
  ]
}
```

- `bind` is `rule-id:player/key` (the viewing player's value),
  `rule-id:players/key` (every player's value, one line each) or
  `rule-id:global/key`. The viewer must receive the key (`owner` or
  `everyone` for its own player value, `everyone` otherwise), or the Add-On
  is refused at load.
- `anchor` is `top_left`, `top_right`, `bottom_left` or `bottom_right`.
- Up to 16 rows and 8 keys. A key is one letter `A`-`Z` that sends the
  rule's command (`package` is the rule's id) with no arguments. The game
  refuses letters it already uses.
- Colors are RGBA from 0 to 1.
- `holding` (optional, up to 32) shows the panel only while the viewer
  holds one of these: an Add-On id (any of its images) or an image id.
  An ammo panel listing `["my-guns"]` appears with one of that Add-On's
  guns out and nowhere else.

## HUD regions

A file with a `"region"` claims one named part of the screen and draws
textured elements into it from the Add-On's own PNGs: a Mario-style power
meter, a sprite-font counter, a reticle. It needs no code. The Playground's
[Runner HUD](../../packages/playground/playground-runner-hud) is a
complete example.

```json
{
  "schema_version": 1,
  "region": "top_center",
  "when": { "body": ["playground-runner:archetype/runner"] },
  "pixelated": true,
  "elements": [
    { "type": "image", "texture": "playground-runner-hud:texture/power", "rect": [80, 12, 80, 80],
      "frames": { "count": 9, "value": "health" } },
    { "type": "text", "at": [120, 96], "text": "POWER", "align": "center" }
  ]
}
```

List each layout as `{ "kind": "hud", ... }` and each PNG as
`{ "kind": "texture", ... }` in `package.json`
`provides`. A layout may use only its own Add-On's textures. Like a panel,
it is a client Add-On: put it beside the body or rule it dresses, depending
on it, as `playground-runner-hud` depends on `playground-runner`.

Texture PNGs may be up to 4 MiB encoded, 4096 pixels per edge and 4,194,304
pixels total. All textures in one Add-On share a 64 MiB decoded RGBA budget.
This allows wide sprite atlases without rescaling their frames.

**Units.** The screen is 480 HUD units tall and as wide as its aspect makes
it (640 at 4:3, about 853 at 16:9), so a layout keeps its proportions at
any size; an N64 layout drawn at 320x240 doubles straight into it. Each
region is a fixed box; positions are from its top left corner, x right, y
down. Elements may reach outside the box.

| Region | Box (x, y, width, height) | Hides |
|---|---|---|
| `top_left` | 0, 0, 240, 120 | |
| `top_center` | centred, 0, 240, 120 | |
| `top_right` | right edge, 0, 240, 120 | |
| `left` | 0, 180, 120, 120 | |
| `right` | right edge, 180, 120, 120 | |
| `bottom_left` | 0, 360, 240, 120 | |
| `bottom_center` | centred, 360, 240, 120 | |
| `bottom_right` | right edge, 360, 240, 120 | |
| `crosshair` | centred, 208, 64, 64 | the crosshair |
| `energy` | right edge, 240, 48, 120 | the jet energy bar |
| `center_print` | centred, 160, 480, 120 | the center print |
| `bottom_print` | 0, 400, full width, 80 | the bottom print, whatever it shows |
| `ammo` | right edge, 400, 240, 80 | the bottom print only while it is the ammo display |
| `chat` | 0, 0, 320, 120 | chat's passing lines; they show again while typing or paging up |
| `tools` | right edge, 0, 64, 480 | the tool bar |
| `name_tags` | 0, 0, full width, 480 | the names over players' heads |

A region nobody claims shows the stock HUD as before. Claiming one of the
last eight hides that stock element while the claim applies; a layout with
no elements just hides it, or saves the region for your client code.

**Names over heads.** A `name_tags` claim hides the stock player names;
give it `name_tags` to draw them your own way (item and shape labels stay):

```json
{ "schema_version": 1, "region": "name_tags",
  "when": { "body": ["your-id:archetype/soldier"] },
  "name_tags": { "teammates": "always", "others": "aimed",
                 "color": [0.55, 0.8, 1.0, 1.0], "outline": false, "height": 0.6 } }
```

`teammates` (on the viewer's team in their mini-game) and `others` each
take `always` (the stock), `aimed` (only while the viewer's aim is on the
body) or `never`. `color` (RGBA 0..1) paints every name one colour instead
of the team's; `outline` (default true) is the stock dark edge; `height`
is units above the eye (default 0.85, -8 to 8).

**Carrying fewer things.** A `tools` claim hides the five-slot tool bar.
With rules that keep two guns (`on_pickup` returning false to swap with
`drop_item` and `give_item`, or `set_tools`), and a layout showing the
held weapon's `ammo`, that is a two-weapon game.

**A game with no building.** A `building` claim hides the whole building
interface (brick bar, "Press B for more bricks", paint box, tool bar) and
turns off its keys and wheel: B, the number keys, the use-bricks, use-tools
and spray-can keys, the inventory arrows, the ghost brick shifts, the
Drop Camera at Player and Drop Player at Camera keys and the wheel's brick,
paint and tool scrolling. A claim that applies to a control profile's player
types also leaves those actions out of that game's Options > Controls list. An Add-On declares it once for its
bodies (`"region": "building"`, `"when": { "body": [...] }`, no elements);
the engine enforces it, so nothing of Blockland's building HUD or binds shows
in a game that does not claim it. `docs/architecture/seams.md` lists the owner.

**When.** `when` limits the claim to a body (`body`: archetype ids), a held
item (`holding`: a weapons Add-On's id or an image id) and a vehicle seat
(`vehicle`: vehicle ids; `seat`: seat numbers from 0, the driver's first, of the vehicle they ride), and to playing in a mini-game or not
(`minigame`: true or false). Every condition given must match. The HUD switches
with the body, the weapon and the seat the moment they change, and goes
back to the stock HUD when nothing claims a region.

**Elements**, drawn back to front, at most 64:

- `image`: `texture`, `rect` [x, y, w, h], optional `tint` (RGBA 0..1).
  With `turn` `{ "value", "scale" }` it is turned about the middle of its
  `rect` by the value times `scale`, in degrees clockwise: a compass rose
  that swings as the viewer turns is `"turn": { "value": "heading", "scale": -1 }`.
  With `frames` `{ "count", "columns", "value", "max" }` it shows one frame
  of a strip (left to right, then down), picked by the value's share of its
  maximum: the first frame at 0, the last when full.
- `bar`: `rect`, `value`, optional `max`, `fill` and `back` (each a `color`
  and an optional `texture` revealed from one end), `direction` `right`
  (fills from the left, the default), `left`, `up` or `down`.
- `text`: `at` [x, y] (the left, centre or right of the line by `align`,
  and its top), `text` (a fixed line, `{}` where the value goes), `value`,
  `digits` (zero padding), `color`. Without `glyphs` it uses the game's
  font. With `glyphs` `{ "texture", "charset", "columns", "size": [w, h],
  "spacing" }` it draws from your sprite font, one equal cell per character
  of `charset`; a character not in it leaves a space.
- `kill_feed`: the latest deaths, one line each from `at` (`align` as
  for text), the newest at the bottom: `lines` (most shown, 4), `seconds`
  (how long a line stays, 6; it fades over its last half second),
  `spacing` (units between lines, 16; negative stacks upward), `format`
  (`"{killer} [{weapon}] {victim}"`) and `death` for a death with no killer
  (`"[{weapon}] {victim}"`), `color`, `team_colors` (names in their team's
  colour, true), `icons` (a damage type's name, such as `"Gun"`, to one of
  your textures drawn for `{weapon}`, at `icon_size` [24, 12]) and `names`
  (a damage type's name to the word written for it). While a layout with a
  kill feed is drawn, deaths stay out of chat.
- `message_feed`: lines a rule posts with `hud_line`, one each from `at`
  (`align` as for text), the newest at the bottom: `feed` (the name the rule
  posts to: 1 to 32 letters, digits, `_` or `-`), `lines` (most shown, 4),
  `spacing` (units between lines, 16; negative stacks upward), `color` (a
  line's colour when its rule gave none) and `icons` (names to your
  textures, drawn before a line's text at `icon_size` [24, 12]). Each line
  stays for the seconds its rule gave it and fades out over its last half
  second. This is the radio's lines, a countdown's words or a match notice,
  and a layout may draw several feeds. A rule posts with
  `hud_line(player, feed, text, seconds[, options])` (`()` for everyone;
  `seconds` 0.5 to 60; options `color` [r, g, b] or [r, g, b, a] and `icon`,
  one of the element's icon names; the `chat` capability), and a feed no
  layout draws shows nothing.
- `tracker`: a motion tracker. A blip for each other player within `range`
  units, placed by bearing and distance from the middle of `rect` (straight
  ahead is up, `range` at its edge). `blip` is the texture, with optional
  `blip_above` and `blip_below` for players more than `height` (3) units
  above or below you, and `blip_ally`, `blip_enemy` and `blip_neutral` per
  side; `ally`, `enemy` and `neutral` tint them. A player shows while moving
  faster than `min_speed` (1 unit a second), except crouched (unless
  `crouched` is true), or within `fired_within` seconds (2) of firing,
  however they move. `allies`, `enemies` and `others` (all true) pick which
  sides show: your team, the other teams of your game, and everyone else.
  Put a background image or a sweep under it as ordinary `image` elements.
- `edge_arrow`: a hit indicator. Draws `texture` into `rect` only while the
  last hit came from its `edge` (`top`, `bottom`, `left` or `right`). `x` and
  `z` are rule values giving where the hit came from as a horizontal world
  offset from the viewer (any length: the rule writes the attacker's position
  minus the victim's, or a shot's direction reversed), and `strength` is a
  0 to 1 rule value the rule counts down, which fades the arrow (`tint` alpha
  times `strength`; 0 draws nothing). The edge is the nearest of ahead (top),
  behind (bottom), left and right by the viewer's facing, so the arrow follows
  the camera. Claim `top_center`, `bottom_center`, `left` and `right` with one
  arrow each. Flashing a meter needs nothing more than `show` on a value the
  rule flips.
- `strip`: a picture strip scrolled by the way the viewer faces, as a
  compass. `textures` (one to eight, equal width) are laid end to end to make
  one whole turn, and `rect` shows `span` degrees (a number, or a value such
  as `fov`) centred on `heading` (the value `heading`, degrees clockwise from
  north, which is straight ahead along the world's -z), wrapping round.
- `strip_marker`: a picture on a `strip` at the bearing of a point in the
  world, for objective letters on a compass. Give it the strip's own `rect`,
  `heading` and `span`, the picture as `texture` with its `size` [w, h], and
  `x` and `z`, rule values giving the point's world position. It draws
  nothing while either value is missing, while the viewer stands on the point,
  or while the point lies outside the `span` degrees the strip shows. Use one
  per point.
- `dial_marker`: a picture on a dial at the place of a point in the world,
  for teammates and objective letters on a round compass or radar. Give it
  the dial's `rect`, the `range` in units from the middle to the edge, the
  picture as `texture` with its `size` [w, h], and `x` and `z`, rule values
  giving the point's world position. With `heading` (the value `heading`) the
  viewer's facing is up, as a rotating radar; without it north is up (turn the
  dial's own `image` against the heading to match). A point beyond `range`
  stays on the edge, or is left out when `edge` is false. It draws nothing
  while either value is missing or the viewer stands on the point. Use one
  per point; put a ring or disc `image` under or over the dial for its look.
- Any element takes `show` `{ "value", "above", "below" }` to appear only
  while a value is in range: a low-health flash, a reload prompt.

**Values** (`value`, `max`, `show`): `health`, `health_max`, `vehicle_health` (what is left of the hull of the vehicle the viewer rides, 0 to 1; missing on foot), `energy` (jet
energy 0 to 1), `alive` (1 or 0), `center_print` and `bottom_print` (the
stock prints' text, without markup), or a rule's state as a panel binds it,
`rule-id:player/key` or `rule-id:global/key`. The held gun gives `ammo`
(rounds ready to fire; for a gun counted from its reserve, such as
grenades, the reserve), `ammo_max` (a full magazine), `ammo_reserve`
(rounds carried beyond it) and `reloading` (1 while it reloads). They are
missing with no gun that has a magazine, and `ammo` and `ammo_reserve` are
missing when they never run out. A missing value reads as `-` in text and
0 in a bar or `show`, so `"show": { "value": "ammo_max", "above": 0 }`
shows an element only with such a gun out. A `max` left out is the value's
own (`health_max` for `health`, `ammo_max` for `ammo`, else 1). `stance` is the viewer's posture: 0 standing, 1 crouching, 2 lying prone, 3 holding a ladder (`frames` with `"max": 3` and four frames shows one picture each). `aiming` is 1 while the viewer aims down a gun's sights, and `zoom_level` is the scope's step while they do (1 for the gun's
own field of view, 2 and up for each wheel level of `Zoom::levels`, 0 when not aiming), so a level marker can show with `show` ranges.
A layout reading a rule's
state is not drawn on a server that does not run that rule.

`respawn` is the seconds until you may respawn while dead (0 once a click
respawns you), missing while alive.

**Stock prompts.** `texts` replaces the game's own words while the layout
is drawn: `click_to_respawn`, `respawn_countdown` (`{}` is the seconds),
`respawn_countdown_one` (one second left; `respawn_countdown` if left out)
and `building_disabled`. An empty text hides that prompt, so a layout can
draw its own from `respawn`:

```json
{
  "schema_version": 1, "region": "chat", "when": { "minigame": true },
  "texts": { "click_to_respawn": "", "respawn_countdown": "", "building_disabled": "" },
  "elements": [
    { "type": "kill_feed", "at": [8, 8], "icons": { "Gun": "my-hud:texture/rifle" } },
    { "type": "text", "at": [160, 100], "align": "center", "text": "Respawn in {}",
      "value": "respawn", "show": { "value": "respawn", "above": 0 } }
  ]
}
```

`pixelated: true` draws images with sharp pixels: pixel art and sprite
fonts.

**Two Add-Ons, one region.** One claim draws. The most specific wins (a
vehicle seat beats a held item, which beats a body, which beats no
condition; they add up), then the higher `priority` (-100 to 100), then the
package id in alphabetical order. The game names the losing layout once in
the Add-On's problems; it is not drawn. So a weapon's ammo counter
claiming `ammo` with `holding` shows over a body's `ammo` claim while that
weapon is out, and the body's comes back when it is put away.

**Drawing from client code.** An Add-On with client code and
`render.layer` draws its own HUD in screen space. Its `hud` layout claims
the region (with no elements, or with some under the code's drawing), and
`hud_region(name, ptr)` gives the region's box in screen space, the size of
one unit and whether this Add-On owns the region now. Draw only while it
does ([client sandbox](../architecture/client-sandbox.md)).

**Not yet.** Region boxes are fixed; there is no free placement or new
region names.

Tests: `crates/content/src/hud.rs` (validation, conflict ranking, frames,
sprite fonts, kill feeds, message feeds, trackers, stock texts), `crates/client/src/match_hud.rs`, `crates/client/src/hud_regions.rs`, and the render check
`crates/client/tests/hud_regions_render.rs` (including the Runner's ammo
counter), and `crates/sim/tests/script_api.rs`
`a_held_guns_ammo_reaches_its_holder_for_hud_counters`.

**A layout's own frame.** A layout that sets `"basis": [800, 600]` is
authored in a frame of that size instead of the region's box: every
position, size and spacing in the file is in those units, from the frame's
top left. The frame is as tall as the screen and sits against the edge its
region touches (left for `top_left`, `left`, `bottom_left` and `chat`;
right for `top_right`, `right`, `bottom_right`, `energy`, `ammo` and
`tools`; the middle for the rest), so a game drawn at 800x600 keeps its
numbers at any aspect. Left out, units are HUD units from the region's
corner as above.
