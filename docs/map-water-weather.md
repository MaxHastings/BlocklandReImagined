# Water and weather integration evidence

Water conversion, basic rendering and host player forces are integrated. Exact
water fidelity and weather remain alpha work.
The source is the designated E: v20 installation; native scene metadata from
map-bundle-014 preserves all authored fields without executing mission scripts.

## Reference coverage

- Nine WaterBlock placements: Slate Desert (one), Slate Sea Revised (two),
  Slate Storm Revised (two), Slopes (one), Tutorial (three).
- Two precipitation placements: Slate Storm's HeavyRain (5,000 drops) and
  Slopes' SnowA (500 drops). Their authored collision flag is enabled.
- Bedroom also has two procedural foliage replicators: grass and beargrass.
  These are distinct from the now-rendered tree meshes.

WaterBlock is also used for flat textured scenery: the Desert object refers to
Tutorial sand and the second Sea/Storm objects reference sand/dirt materials.
Do not classify every water-class object as visually blue water. Preserve source
resource paths, including legacy tilde references requiring explicit resolution.

## Geometry and behavior evidence

Pinned OpenMBG waterBlock.cc at commit
`9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7` sets the water surface to source
position.z plus scale.z. UpdateFluidRegion resets rotation and uses an XY origin
and size; the renderer's internal +1024 terrain offset is not a world translation.
The source unit box is [0,1] in each axis. This means source scale/origin must be
converted together into native Y-up coordinates; position alone is not the surface.

The point-submerged helper checks the top plane and a terrain-aware fluid XY mask,
without an explicit bottom check. The same pinned engine's shapeBase.cc
waterFind/updateContainer separately use body/water world-box overlap for forces.
The native bounded-volume query follows that separation, although exact edge
coverage/multiple-volume ordering and the classic terrain accept masks remain
fidelity work.
Native water waves and texture flow follow the family's fluidQuadTree/fluidRender
formulas; their classic time origin and terrain-space offset are explicit. GPU
time updates avoid reupload. A generated terrain mask drives surface/shore alpha;
original surface/shore images and available reflection resources are bound.
Two surface alpha passes are combined analytically. Original multi-pass lighting,
reflection mapping/specular appearance, edge masks and underwater composition
still need fidelity work. Rendered repeated tiles are a finite neighborhood;
camera-following water LOD/streaming remains required.

Host simulations load the same native liquid regions used for rendering. Player
forces use source density 0.7, drag 0.1, water density/viscosity and coverage;
the >=0.1 force threshold and >=0.9 underwater speed selection follow the family
code. Stock speeds are 8.4/7.8/7.8. Full movement acceleration while in liquid is
a native motor adaptation, not proved exact Torque behavior. Headless checks
cover rising from the floor, movement/drag, density-based floating equilibrium
and force removal after leaving water. Client prediction still needs these inputs
when prediction is integrated. No visible playtest or feel acceptance is claimed.

Vehicles expose a host-supplied surface-height callback in native Y-up space;
the vehicle module does not parse WaterBlock declarations. Root still needs to
connect vehicle forces, projectile crossings/splashes, gameplay liquid events and
underwater audio/visual transitions to the shared native environment.

Bundle 012 was rejected because Tutorial names a nonexistent reflection texture
while its reflection intensity is zero. Disabled reflections now need no image;
enabled missing reflections remain errors. Bundle 013 exposed the missing default
repeat behavior in Desert. Bundle 014 follows the classic repeated terrain period
(2048 units), including implicit repeat; exact Blockland toggle defaults remain
qualified engine-family evidence. A visual check verifies Desert's sand now spans
the view, and Sea has its original water and sand layers.

The actual Sea session test also found that its marker center touches the map
collider. Shared loading now provides collision-checked candidates within authored
spawn regions, with floor tracing for obstructed marker centers. Hosting receives
multiple candidates instead of one unchecked center. The stable center/spiral
selection is a native adaptation; exact legacy weighted random selection remains
work. The headless Sea session joins successfully and floats at its 9-unit surface.

## Precipitation evidence limits

The recovered core's SnowA declaration uses dropTexture/dropSize/splashMS and
true billboards. Older OpenMBG precipitation instead implements the earlier Snow
material-list fields; it must not be used as the SnowA runtime baseline.
Pinned OpenMBU precipitation.cpp at commit
`3d6516e1c9cb43e61aead3369d1f7210d08b83ef` has the later drop/splash system and
many matching placement fields, but some properties moved from datablock to
instance. Treat it as engine-family evidence, not exact Blockland behavior.

Required implementation includes camera-local rain/snow volumes, original textures,
bounded drop work, collision/roof occlusion, authored turbulence/speed/size and
settings. Generic particle emitters alone do not establish weather fidelity.

Sources: [OpenMBG waterBlock.cc](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/terrain/waterBlock.cc),
[older precipitation.cc](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/fx/precipitation.cc),
[later OpenMBU precipitation.cpp](https://github.com/MBU-Team/OpenMBU/blob/3d6516e1c9cb43e61aead3369d1f7210d08b83ef/engine/source/game/fx/precipitation.cpp).
