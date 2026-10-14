# 2026-10-14 Map Add-Ons design

pharzedia asked how to bring the converted Bedroom into Blender. They want to
extend it, keep its recovered lights, and ship the result as an Add-On,
because map2dif produced broken `.dif` files. Wilfred asked for new maps
straight from Blender or TrenchBroom. pharzedia also noted that no native
format can add a new map with terrain and interiors to the map list, and that
only converted-map lamps cast shadows.

Finding: no doc or importer exists for `*.interior.json` or `*.shape.json`
into Blender. More importantly, maps can only come from v20 conversion: there
is no `map` content kind, `LOADABLE_MAPS` hard-codes the list, and joiners
never download maps. A Blender reader alone would not let anything come back
in, so Maxwell asked for the long-term fix instead of the literal request.

Decision: proposed only, in
[docs/architecture/map-packages.md](../architecture/map-packages.md), with a
row in `seams.md`. Maps become `shared` Add-On content (`*.map.json` holding
native models, spawns, lights and heightfield terrain). The import tools are
glTF (Blender) and TrenchBroom. Converted maps get an export to the same format
with a round-trip test. Brick, image and map lights become one engine light
with spot cones, curves and a shadow flag. Authored maps show their lamps in Dynamic and sun and ambient in Classic.
The order of work and the open questions for Maxwell are in the doc. No code
changed; nothing was built or tested.

Scope cut (Maxwell, same day: avoid technical debt): the first version is the
map kind, glTF map import, export of converted maps to `.glb`, and map lights
on the existing light definition and lamp-shadow path. Deferred: TrenchBroom
import, spot lights, shadows on brick lights, the lightmap bake, and a Blender
reader for `*.interior.json`. The two map loading paths stay temporary only if
vanilla moves onto the kind before beta.

Next: Maxwell approves or amends the open questions; work starts after the
release blockers.

Approved (Maxwell, same day): one place for maps and one pipeline, with the
vanilla maps moved onto it. The format keeps optional lightmapped
architecture (`*.interior.json`) beside plain models, so converted maps keep
their Classic and Unified look; Blender maps light live in Dynamic and fall back to sun and ambient in Classic (Sauce asked that no map force shaders on). Building
starts with the `map` kind and loader.

Light types (pharzedia, same day): map2dif's lights (Omni, Spot, Strobe,
Pulse, Pulse2, Flicker, Runway) were a separate system from brick
`FxLightData`. The design now keeps them separate: map lights use map2dif's
types and timing on the lamp-shadow path, and brick lights stay
`FxLightData`. Spot lights move into the first version.
