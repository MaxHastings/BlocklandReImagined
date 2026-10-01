# Old v20 Add-Ons and new bricks

Put an old Blockland Add-On (a `.zip` or a folder with `server.cs`) in the
game's `content/Add-Ons/` folder (inside `content`, found as in section
2), open **Start Game > Add-Ons**, pick it
and press **Import**. Its scripts are never run: datablocks for bricks,
weapons and vehicles become data, and `IMPORT-REPORT.md` in the new Add-On
lists what came across and what did not. An Add-On without a licence file
is imported as `proprietary`; for your own work, set `license` in the new
`package.json`. From a checkout the same importer runs as:

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- Weapon_Example.zip out/weapon_example --installed content
```

`--installed` names the game's content folder. An Add-On builds on the base
game: a dirt brick declared `brick1x1DirtData : brick1x1Data` inherits the
stock 1x1's icon and fields, an image names `weaponSwitchSound`. The
importer reads those base datablocks' names and fields from the installed
game's brick catalog, weapons, sounds and effects (Import in the Add-Ons
screen always passes it); without it they are unknown and the report says so.

Many v20 Add-Ons keep part of what they do in scripts: a shotgun's spread,
a slash command. The report lists each such function under **Needs
behaviour**. When someone has made a native **port** of that Add-On, the
importer applies it and the report says **Ported**. The ports so far, and the
recipe for making one (or having your agent make one), are in
[porting.md](porting.md).

For weapons the importer brings across items, images, projectiles,
explosions, damage types and the Add-On's own look and sound: the particle
emitters its image states, projectile trails and explosions use (with an
explosion's burst and light), its `AudioProfile`s (a sound whose
description is not 3D is heard by its holder alone), and its `DebrisData`
with its model, so its casings and explosion debris are its own. v20 datablocks the engine
would have corrected on load (an emitter's period or angles) are corrected
the same way and noted. An `ItemData` with no `uiName` is hidden in v20,
so it is left out and its image kept for rules to mount; a
`ParticleEmitterData` with a `uiName` is offered in the wrench's emitter
list, as v20 listed it, and so is a named light; one with a
`uiName` and no image becomes a pickup nobody holds. A kill icon the
Add-On forgot to ship is left out of its messages. Its particles may draw
its own textures; players load at most 64 of them from all Add-Ons, and
fit each within 256 pixels a side.

That is also how you make **new bricks** today: write a small v20-style
brick Add-On and import it. A folder `Brick_Tall` holding:

```text
server.cs         datablock fxDTSBrickData(brick3x3x2Data)
                  {
                      brickFile = "./3x3x2.blb";
                      category = "Bricks";
                      subCategory = "Tall";
                      uiName = "3x3x2 Block";
                  };
3x3x2.blb         3 3 6
                  BRICK
description.txt   Title: Tall Bricks
                  Author: You
```

imports into an Add-On with one 3x3 brick, two bricks tall. A `.blb`'s
first line is its size in studs, studs and plates (three plates to a
brick); `BRICK` gives a plain box with studs. v20's own brick Add-Ons show
the longer form for other shapes.

**Mirrors.** Any brick can have mirror sides. A brick Add-On can reuse the
game's window as a mirror with no model of its own (a brick that inherits a
base game brick's `brickFile` is imported without a copy of that shape; the
game lends it the base brick's shape and menu icon). The default **Mirror**
Add-On (`packages/brick_mirror`) is this brick:

```text
server.cs         datablock fxDTSBrickData(brickMirror1x4x5Data : brick4x1x5windowData)
                  {
                      uiName = "1x4x5 Mirror";
                      reflectionFaces = "north south";
                      reflectionDepth = 0.5;
                  };
                  datablock fxDTSBrickData(brickMirror1x14x10Data : brickMirror1x4x5Data)
                  {
                      uiName = "1x14x10 Mirror";
                      stretchSize = "14 1 30";
                  };
```

`stretchSize = "width depth height"` (studs, studs, plates) makes another
size of the shape, as a nine-slice picture stretches: the frame, sill and
stud edges keep their size and the middle grows. Its mirror, collision,
portal openings and pairing all follow the new size, so a 1x14x10 Mirror
is the same window brick, big.

The mirror spans the whole side and the window's frame, drawn in front of
it, hides its edges; the window's see-through glass is not drawn.

| Field | Meaning | Default |
|---|---|---|
| `reflectionFaces` | Which sides mirror: `north south east west top bottom` | required |
| `reflectionDepth` | How far in the mirror sits, from that side (0) toward the opposite side (1); 0.5 is the middle of the brick | 0 |
| `reflectionInset` | How far in from the side's edges the glass stops, in world units (leaves a frame) | 0 |
| `reflectionTint` | Colour the reflection is multiplied by, `"r g b"` from 0 to 1 (a real mirror is about `"0.95 0.95 0.95"`) | `"1 1 1"` |
| `reflectionStrength` | 1 is a full mirror, which also stops drawing the brick's own see-through surfaces across its mirrored sides (a window's glass); below 1 the painted brick shows through | 1 |

Mirrors are drawn only on each player's computer, never sent over the
network. Players pick how many mirrors show live reflections with
**Options > Graphics > Mirrors** (Off, Low, Medium, High: 0 to 3 at once,
the biggest on screen first, counting mirrors seen inside another
mirror's reflection); a mirror seen deeper than that repeats what it last
showed, so two facing mirrors make an endless tunnel, each bounce dimmed
by `reflectionTint`. The rest, and all of them with Mirrors off, show plain
silver. Mirrors facing the same way in one flat wall count as
one. Reflections show everything the world draws: bricks, the map and its
sky and water, players, vehicles, items, particles, plants, weather and
Add-On code's world-space layers (not its view- or screen-space ones, which
belong to the player's screen). Name tags and hidden-brick outlines are
screen aids and stay out of mirrors.

**Portals (linked bricks).** A brick can also be a window onto another
brick: each of its `linkFaces` shows the view out of its partner, and with
`linkPass` players, vehicles, items and projectiles that go in come out of
the partner, turned the way the partner faces. The optional **Portal**
Add-On (`packages/brick_portal`, off until a player turns it on) is the
game's window again:

```text
server.cs         datablock fxDTSBrickData(brickPortal1x4x5Data : brick4x1x5windowData)
                  {
                      uiName = "1x4x5 Portal";
                      linkFaces = "north south";
                      linkName = "Portal";
                      linkDepth = 0.5;
                      linkPass = 1;
                      linkFrame = "0.05 0.05 0.2";
                  };
                  datablock fxDTSBrickData(brickPortal1x14x10Data : brickPortal1x4x5Data)
                  {
                      uiName = "1x14x10 Portal";
                      stretchSize = "14 1 30";
                  };
```

Two bricks of one kind, placed by one player, with the same brick **Name**
(the wrench's Name box every brick has; case does not matter) are a pair.
Placing two in a row names them to match (`Portal_1a2b3`), as Teledoors do.
Three or more of one name form a ring, each leading to the next in the
order they were placed. A brick with no partner shows its own glass and,
with `linkPass`, is shut. Going in through one side comes out of the
partner's opposite side when that side is open too (a doorway), else out of
the same side (a wall portal). Pairing follows from the bricks themselves,
so nothing extra is sent; each player's game draws the views, and the host
decides who goes through.

Bots know portals too, with nothing for a bot kind to set: a bot sees and
shoots through an opening at whoever stands beyond its partner, its paths
lead through openings where walking through is the way, and it follows an
enemy it watched go in.

| Field | Meaning | Default |
|---|---|---|
| `linkFaces` | The open sides: `north south east west top bottom` | required |
| `linkName` | Stem of the names placing a pair gives: up to 16 letters, digits or underscores, starting with a letter | required |
| `linkDepth` | How far in the opening sits, as `reflectionDepth` | 0 |
| `linkInset` | Frame left around each view, in world units | 0 |
| `linkTint` | Colour the view is multiplied by, `"r g b"` | `"1 1 1"` |
| `linkIdle` | Colour a linked side shows when its view is not drawn live | `"0.35 0.42 0.55"` |
| `linkPass` | Whether things pass through; the brick's collision becomes a frame around each opening | 0 |
| `linkFrame` | Width of that frame, in world units: one number for every edge, or `"sides top bottom"` (the bottom is a sill bodies step over) | 0 |

The Add-On also has a 1x14x10 (6.9 by 5.75 inside: a tank or a jeep
drives through with room to spare) and a 1x20x12 (the Stunt Plane, wings
and all). Each size of portal is its own kind, so a 1x4x5 never pairs
with a 1x14x10 of the same name. A big one costs no more to draw than a small one
the same size on screen: each view is drawn only over the part of the
screen its opening covers.

**Another size of a brick (`stretchSize`).** Any brick can be its
`brickFile`'s shape at another size, `"width depth height"` in studs,
studs and plates. Half a stud of every edge keeps its size and moves out
with the edge while the middle stretches, as a nine-slice picture does:
a window's frame stays as thin round a bigger pane, studs on top stay one
stud each (there are more of them), and the brick's attachment grid and
collision boxes grow to match. A shape with no collision boxes of its
own (and no `linkPass` frame) needs the Add-On to give it collision.

Views share the mirrors' **Options > Graphics > Mirrors** budget, and a
portal seen through a portal repeats what it last showed, like facing
mirrors.
