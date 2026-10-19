//! Native map collision loading shared by headless servers and local sessions.
use anyhow::{Context, Result, ensure};
use bri_content::{
    Terrain,
    interior::Interior,
    scene::{Kind, Scene},
    terrain_field::{TerrainField, TerrainInstance},
};
use bri_physics::terrain::{BodyFocusPolicy, Focus, StreamingConfig, TerrainColliders};
use glam::Vec3;
use rapier3d::prelude::{ColliderBuilder, PhysicsWorld};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Native map loading coverage; each map still needs full gameplay/fidelity acceptance.
pub const LOADABLE_MAPS: &[&str] = &[
    "v20/add-ons/map_bedroom/bedroom.mis",
    "v20/add-ons/map_kitchen/kitchen.mis",
    "v20/add-ons/map_slopes/slopes.mis",
    "v20/add-ons/map_slate/slate.mis",
    "v20/add-ons/map_bedroomdark/bedroomdark.mis",
    "v20/add-ons/map_construct/construct.mis",
    "v20/add-ons/map_destruct/destruct.mis",
    "v20/add-ons/map_halloween_slate/halloweenslate.mis",
    "v20/add-ons/map_kitchendark/kitchendark.mis",
    "v20/add-ons/map_skylands/skylands.mis",
    "v20/add-ons/map_slate_desert/slatedesert.mis",
    "v20/add-ons/map_slate_sea_revised/slatesearevised.mis",
    "v20/add-ons/map_slate_storm_revised/slatestormrevised.mis",
    crate::tutorial::MAP_ID,
];

/// Classification for read-only client environment queries. Simulation replaces
/// these tags with its own map authority tag when inserting map collision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u128)]
pub enum MapSurface {
    Terrain = 1,
    Interior = 2,
    Static = 3,
}
impl MapSurface {
    pub fn from_tag(tag: u128) -> Option<Self> {
        match tag {
            1 => Some(Self::Terrain),
            2 => Some(Self::Interior),
            3 => Some(Self::Static),
            _ => None,
        }
    }
}

pub struct NativeMap {
    pub scene: Scene,
    /// Interior and static model collision. Terrain is streamed separately.
    pub colliders: Vec<ColliderBuilder>,
    /// Exact, unbounded terrain placements shared by collision, queries and rendering.
    pub terrain: Vec<Arc<TerrainField>>,
    pub waters: Vec<bri_content::water::Water>,
    /// Retained scene objects which this adapter does not yet give collision.
    pub pending_objects: Vec<String>,
    /// Static shapes a fast player smashes (v20 `Glass` class).
    pub breakables: Vec<Breakable>,
    /// Teleporter entrances and exits (`Scene::teleporters`).
    pub teleporters: Vec<bri_content::scene::Teleporter>,
    /// The plate a flat map stands on, where the Environment window's
    /// ground and water go; None on every other map.
    pub plate: Option<bri_content::scene::Plate>,
    /// What each collider's triangles are made of, for footsteps and
    /// impacts ([`MapSurfaces`]).
    pub surfaces: MapSurfaces,
    /// Static models that follow keyed motions (doors): their nodes and
    /// the colliders that move with them.
    pub movers: Vec<MapMover>,
}

/// A static model a package moves along authored keys (a door). Its
/// colliders are in [`NativeMap::colliders`] like any other, built about
/// the model's origin and placed at `placement` (its pose at key 0).
#[derive(Clone, Debug, PartialEq)]
pub struct MapMover {
    /// Scene node index: the stable identity clients draw.
    pub node: u32,
    pub def: bri_content::mover::MoverDef,
    /// The node's world transform at rest.
    pub placement: glam::Mat4,
    /// Indices into `NativeMap::colliders`.
    pub colliders: std::ops::Range<usize>,
}
impl MapMover {
    /// Where the model's origin is at `tick` in `state`.
    pub fn transform(&self, state: &bri_content::mover::MoverState, tick: u64) -> glam::Mat4 {
        state.offset(&self.def, tick).apply(self.placement)
    }
    /// [`Self::transform`] between ticks, for drawing.
    pub fn transform_at(&self, state: &bri_content::mover::MoverState, tick: f64) -> glam::Mat4 {
        state.offset_f(&self.def, tick).apply(self.placement)
    }
}

/// The surface kind a body stands on or a shot strikes, a short name a map
/// gives its materials (`dirt`, `metal`, `grass`). The engine gives kinds
/// no meaning; bodies and effects say what each sounds and looks like.
/// A map names them in its bundle's `surfaces`, by map id: material name
/// (any case) to kind. Brick hits are [`BRICK_SURFACE`], terrain is
/// [`TERRAIN_SURFACE`], and anything unnamed is [`DEFAULT_SURFACE`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapSurfaces {
    /// The kinds the map names, which `triangles` index.
    pub kinds: Vec<String>,
    /// For each of [`NativeMap::colliders`] in order, each triangle's kind
    /// (an index into `kinds`, [`NO_KIND`] for none), or None when none of
    /// its triangles has one.
    pub triangles: Vec<Option<Arc<[u16]>>>,
}
/// A triangle the map names no kind for.
pub const NO_KIND: u16 = u16::MAX;
pub use bri_content::surface::{
    BRICK as BRICK_SURFACE, DEFAULT as DEFAULT_SURFACE, TERRAIN as TERRAIN_SURFACE,
    valid_name as surface_kind_name,
};
/// The most kinds a map may name.
pub const MAX_SURFACE_KINDS: usize = 64;
impl MapSurfaces {
    /// The kind of triangle `triangle` of collider `collider`, when the map
    /// names one.
    pub fn kind(&self, collider: usize, triangle: u32) -> Option<&str> {
        let kinds = self.triangles.get(collider)?.as_ref()?;
        let kind = *kinds.get(usize::try_from(triangle).ok()?)?;
        self.kinds.get(usize::from(kind)).map(String::as_str)
    }
    /// The bundle's `surfaces` for `map_id`: material name to kind.
    fn table(bundle: &serde_json::Value, map_id: &str) -> Result<BTreeMap<String, String>> {
        let Some(table) = bundle.get("surfaces").and_then(|s| s.get(map_id)) else {
            bri_console::fallback::report(
                bri_console::fallback::kind::DEFAULTED_FIELD,
                map_id,
                "surfaces",
                "no surface kinds: footsteps and impacts use the default",
            );
            return Ok(BTreeMap::new());
        };
        let table: BTreeMap<String, String> =
            serde_json::from_value(table.clone()).context("Invalid map surfaces")?;
        ensure!(
            table.len() <= 4096
                && table.iter().all(|(material, kind)| {
                    !material.is_empty() && material.len() <= 128 && surface_kind_name(kind)
                }),
            "Map surfaces name materials with kinds of up to 32 letters, digits, `_` or `-`"
        );
        Ok(table
            .into_iter()
            .map(|(material, kind)| (material.to_ascii_lowercase(), kind))
            .collect())
    }
    /// Each triangle's kind for a collider made of `materials`, or None.
    fn push(
        &mut self,
        map_id: &str,
        table: &BTreeMap<String, String>,
        shape: &bri_content::shape::Shape,
        materials: Option<&[Option<usize>]>,
    ) -> Result<()> {
        let mut any = false;
        let ids = materials
            .filter(|_| !table.is_empty())
            .map(|materials| {
                materials
                    .iter()
                    .map(|m| {
                        let Some(kind) = m
                            .and_then(|m| shape.materials.get(m))
                            .and_then(|m| table.get(&m.name.to_ascii_lowercase()))
                        else {
                            bri_console::fallback::report(
                                bri_console::fallback::kind::DEFAULTED_FIELD,
                                map_id,
                                &format!(
                                    "surface of material {}",
                                    m.and_then(|m| shape.materials.get(m))
                                        .map_or("(none)", |m| m.name.as_str())
                                ),
                                "the default surface kind",
                            );
                            return Ok(NO_KIND);
                        };
                        any = true;
                        let at = match self.kinds.iter().position(|k| k == kind) {
                            Some(at) => at,
                            None => {
                                ensure!(
                                    self.kinds.len() < MAX_SURFACE_KINDS,
                                    "A map names at most {MAX_SURFACE_KINDS} surface kinds"
                                );
                                self.kinds.push(kind.clone());
                                self.kinds.len() - 1
                            }
                        };
                        Ok(u16::try_from(at)?)
                    })
                    .collect::<Result<Arc<[u16]>>>()
            })
            .transpose()?;
        self.triangles.push(ids.filter(|_| any));
        Ok(())
    }
}

/// A v20 `Glass`-class static shape (`glassA`, `lightBulbA`,
/// `fluorescentLight`). `Armor::onImpact` calls `StaticShape::explode` on one
/// unless the mission marks it `indestructable`.
#[derive(Clone, Debug, PartialEq)]
pub struct Breakable {
    /// Scene node index: the stable identity clients hide.
    pub node: u32,
    pub datablock: String,
    /// `ExplosionData` that `ShapeBase::blowUp` plays on destruction.
    pub explosion: Option<String>,
    /// `explosionSound`, played by `explode` at the shape's origin. Only
    /// `glassA` has one; the bulb and fluorescent lights break silently.
    pub sound: Option<String>,
    pub position: Vec3,
    /// `blowUp`'s explosion point: the object box center added to the
    /// position without rotation or scale, as the engine does.
    pub center: Vec3,
    pub indestructable: bool,
    /// Indices into `NativeMap::colliders`.
    pub colliders: std::ops::Range<usize>,
}

/// `StaticShapeData` declarations whose `className` is `Glass`, by lowercase
/// name: (explosion, explosionSound).
fn glass_datablocks(
    bundle: &serde_json::Value,
) -> std::collections::BTreeMap<String, (Option<String>, Option<String>)> {
    let field = |fields: &serde_json::Value, key: &str| {
        fields[key]
            .as_str()
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    bundle["static_datablocks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| {
            d["fields"]["classname"]
                .as_str()
                .is_some_and(|c| c.eq_ignore_ascii_case("glass"))
        })
        .filter_map(|d| {
            let fields = &d["fields"];
            Some((
                d["name"].as_str()?.to_ascii_lowercase(),
                (field(fields, "explosion"), field(fields, "explosionsound")),
            ))
        })
        .collect()
}
/// TorqueScript truth: a field is true when it reads as a nonzero number.
fn script_true(value: &str) -> bool {
    value.trim().parse::<f64>().is_ok_and(|v| v != 0.0)
}
fn native_file(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..",
        "Invalid native asset filename"
    );
    Ok(root.join(name))
}
impl NativeMap {
    /// Permanently loaded terrain around every authored spawn region, so
    /// spawn selection and newly joined players always stand on collision.
    pub fn spawn_anchors(&self) -> Result<Vec<Focus>> {
        self.scene
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Spawn))
            .map(|node| {
                let radius = node
                    .properties
                    .get("radius")
                    .map(|v| v.parse::<f32>())
                    .transpose()?
                    .unwrap_or(0.0);
                ensure!(
                    radius.is_finite() && (0.0..=10000.).contains(&radius),
                    "Invalid authored spawn radius"
                );
                Ok(Focus {
                    center: Vec3::new(node.transform[12], node.transform[13], node.transform[14]),
                    radius: radius + TerrainStream::BODY_MARGIN,
                })
            })
            .collect()
    }
    pub fn load(root: &Path, map_id: &str) -> Result<Self> {
        bri_console::fallback::with_owner(map_id, || Self::load_unscoped(root, map_id))
    }
    fn load_unscoped(root: &Path, map_id: &str) -> Result<Self> {
        let bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("bundle.json"))?)?;
        let entry = bundle["maps"]
            .as_array()
            .context("Missing native map index")?
            .iter()
            .find(|m| m["id"].as_str() == Some(map_id))
            .context("Unknown native map")?;
        let mut scene: Scene = bri_content::strict::from_slice(&std::fs::read(native_file(
            root,
            entry["file"].as_str().context("Missing scene filename")?,
        )?)?)?;
        ensure!(
            scene.schema_version == 1 && scene.id == map_id,
            "Native map schema/identity mismatch"
        );
        let mut interiors = std::collections::BTreeMap::new();
        for node in scene
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Interior))
        {
            let id = node.asset.as_ref().context("Missing interior reference")?;
            if !interiors.contains_key(id) {
                let file = native_file(
                    root,
                    bundle["assets"][id]
                        .as_str()
                        .context("Missing native interior file")?,
                )?;
                let interior: Interior = bri_content::strict::from_slice(&std::fs::read(file)?)?;
                interior.validate()?;
                interiors.insert(id.clone(), interior);
            }
        }
        let lift = scene.floor_lift(|id| interiors.get(id));
        scene.lift(lift);
        let plate = scene.plate(|id| interiors.get(id));
        let mut colliders = Vec::new();
        let mut waters: Vec<bri_content::water::Water> = bundle
            .get("waters")
            .and_then(|w| w.get(map_id))
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or_default();
        for water in &mut waters {
            water.min[1] += lift;
            water.max[1] += lift;
            water.validate()?;
            ensure!(
                scene
                    .nodes
                    .get(water.node)
                    .is_some_and(|n| matches!(n.kind, Kind::Water)),
                "Water refers to wrong scene node"
            );
        }
        if bundle.get("waters").and_then(|w| w.get(map_id)).is_some() {
            let expected: Vec<_> = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| matches!(n.kind, Kind::Water))
                .map(|(i, _)| i)
                .collect();
            ensure!(
                waters.iter().map(|w| w.node).collect::<Vec<_>>() == expected,
                "Native water index disagrees with map"
            );
        }
        let glass = glass_datablocks(&bundle);
        let surface_table = MapSurfaces::table(&bundle, map_id)?;
        let mut surfaces = MapSurfaces::default();
        let mut breakables = Vec::new();
        let mut movers = Vec::new();
        let mover_defs: std::collections::BTreeMap<usize, bri_content::mover::MoverDef> =
            scene.movers()?.into_iter().collect();
        let mut pending_objects: Vec<_> = scene
            .pending_scripts
            .iter()
            .map(|p| p.diagnostic())
            .collect();
        for (index, node) in scene.nodes.iter().enumerate() {
            ensure!(
                node.transform.iter().all(|v| v.is_finite()),
                "Non-finite map transform"
            );
            let transform = glam::Mat4::from_cols_array(&node.transform);
            match node.kind {
                Kind::Interior => {
                    let id = node.asset.as_ref().context("Missing interior reference")?;
                    let interior = &interiors[id];
                    colliders.push(
                        bri_physics::content::interior_collider(&interior.details[0], transform)?
                            .user_data(MapSurface::Interior as u128),
                    );
                    surfaces.triangles.push(None);
                }
                Kind::StaticModel | Kind::DatablockModel
                    if node.asset.is_some() && mover_defs.contains_key(&index) =>
                {
                    // A model a package moves: its collision is built about
                    // its own origin and put where the placement says, so
                    // the session can move it.
                    let def = mover_defs[&index].clone();
                    let id = node.asset.as_ref().context("Missing mover asset")?;
                    let path = native_file(
                        root,
                        bundle["assets"][id]
                            .as_str()
                            .context("Missing mover model file")?,
                    )?;
                    let shape: bri_content::shape::Shape =
                        bri_content::strict::from_slice(&std::fs::read(path)?)?;
                    let first = colliders.len();
                    if def.collide {
                        for (collider, materials) in
                            bri_physics::content::static_shape_collider_materials(
                                &shape,
                                glam::Mat4::IDENTITY,
                            )?
                        {
                            let kept = collider
                                .shape
                                .as_trimesh()
                                .is_some_and(|m| m.indices().len() == materials.len());
                            if !kept {
                                bri_console::fallback::report(
                                    bri_console::fallback::kind::DROPPED_ITEM,
                                    map_id,
                                    &format!("surface kinds of {}", node.name),
                                    "the default surface kind for the whole collider (triangle and material counts differ)",
                                );
                            }
                            surfaces.push(
                                map_id,
                                &surface_table,
                                &shape,
                                kept.then_some(&materials[..]),
                            )?;
                            colliders.push(
                                collider
                                    .position(mover_pose(transform))
                                    .user_data(MapSurface::Static as u128),
                            );
                        }
                    }
                    movers.push(MapMover {
                        node: u32::try_from(index)?,
                        def,
                        placement: transform,
                        colliders: first..colliders.len(),
                    });
                }
                Kind::StaticModel | Kind::DatablockModel if node.asset.is_some() => {
                    let id = node.asset.as_ref().context("Missing static model asset")?;
                    let path = native_file(
                        root,
                        bundle["assets"][id]
                            .as_str()
                            .context("Missing static model file")?,
                    )?;
                    let shape: bri_content::shape::Shape =
                        bri_content::strict::from_slice(&std::fs::read(path)?)?;
                    let first = colliders.len();
                    // A map Add-On's whole level is one model marked
                    // `surface: level`: bricks may not be buried in it or
                    // reach through it, as with an interior. Other models
                    // are props, which v20's plant never asks about.
                    let surface =
                        if node.properties.get("surface").map(String::as_str) == Some("level") {
                            MapSurface::Interior
                        } else {
                            MapSurface::Static
                        };
                    for (collider, materials) in
                        bri_physics::content::static_shape_collider_materials(&shape, transform)?
                    {
                        // Each triangle stays where it was built, so a hit's
                        // triangle is the material's.
                        let kept = collider
                            .shape
                            .as_trimesh()
                            .is_some_and(|m| m.indices().len() == materials.len());
                        if !kept {
                            bri_console::fallback::report(
                                bri_console::fallback::kind::DROPPED_ITEM,
                                map_id,
                                &format!("surface kinds of {}", node.name),
                                "the default surface kind for the whole collider (triangle and material counts differ)",
                            );
                        }
                        surfaces.push(
                            map_id,
                            &surface_table,
                            &shape,
                            kept.then_some(&materials[..]),
                        )?;
                        colliders.push(collider.user_data(surface as u128));
                    }
                    let datablock = node.properties.get("datablock");
                    if let Some((name, (explosion, sound))) =
                        datablock.and_then(|d| glass.get(&d.to_ascii_lowercase()).map(|g| (d, g)))
                    {
                        breakables.push(Breakable {
                            node: u32::try_from(index)?,
                            datablock: name.clone(),
                            explosion: explosion.clone(),
                            sound: sound.clone(),
                            position: transform.w_axis.truncate(),
                            center: transform.w_axis.truncate() + object_box_center(&shape)?,
                            indestructable: node
                                .properties
                                .get("indestructable")
                                .is_some_and(|v| script_true(v)),
                            colliders: first..colliders.len(),
                        });
                    } else if let Some(pending) = node.properties.get("native_behavior_pending") {
                        pending_objects.push(format!("{}: {pending}", node.name));
                    }
                }
                Kind::Climb => {
                    // Added below with the others, so each reads its own
                    // numbers.
                }
                Kind::StaticModel => anyhow::bail!("Missing static model asset: {}", node.name),
                Kind::DatablockModel | Kind::Water | Kind::Unadapted => {
                    if !matches!(node.kind, Kind::Water) {
                        bri_console::fallback::report(
                            bri_console::fallback::kind::DROPPED_ITEM,
                            map_id,
                            &node.name,
                            "no collision or behavior (not yet adapted)",
                        );
                    }
                    pending_objects.push(node.name.clone())
                }
                _ => {}
            }
        }
        // Ladders: sensor boxes the player motor finds by a body's middle.
        for climb in scene.climbs()? {
            colliders.push(bri_motor::climb::collider(
                climb.center,
                climb.half,
                climb.yaw,
                climb.speed,
                climb.strafe,
            ));
            surfaces.triangles.push(None);
        }
        let mut instances: Vec<TerrainInstance> = match bundle
            .get("terrains")
            .context("Map bundle has no converted terrain instances")?
            .get(map_id)
        {
            Some(value) => {
                serde_json::from_value(value.clone()).context("Invalid native terrain instances")?
            }
            None => Vec::new(),
        };
        for instance in &mut instances {
            instance.origin[1] += lift;
        }
        let terrain = bri_content::terrain_field::map_fields(&scene, instances, |id| {
            let file = native_file(
                root,
                bundle["assets"][id]
                    .as_str()
                    .context("Missing native terrain file")?,
            )?;
            Ok(bri_content::strict::from_slice::<Terrain>(&std::fs::read(
                file,
            )?)?)
        })?;
        ensure!(
            !colliders.is_empty() || !terrain.is_empty(),
            "Native map has no physical geometry"
        );
        let teleporters = scene.teleporters()?;
        Ok(Self {
            scene,
            colliders,
            terrain,
            waters,
            pending_objects,
            breakables,
            teleporters,
            plate,
            surfaces,
            movers,
        })
    }
}

/// The pose of a model placed at `transform`, for its collider.
fn mover_pose(transform: glam::Mat4) -> rapier3d::prelude::Pose {
    let (_, rotation, translation) = transform.to_scale_rotation_translation();
    rapier3d::prelude::Pose::from_parts(
        rapier3d::prelude::Vector::from_array(translation.to_array()),
        rapier3d::prelude::Rotation::from_array(rotation.to_array()),
    )
}

/// Center of the shape's bounds at its default pose, in object space.
fn object_box_center(shape: &bri_content::shape::Shape) -> Result<Vec3> {
    let mut pose = bri_content::animation::sample(shape, None, 0.0)?;
    pose.visibility.fill(1.0);
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for detail in 0..shape.details.len() {
        for triangle in bri_content::animation::triangles(shape, &pose, detail, |_| true)? {
            for vertex in triangle.vertices {
                min = min.min(vertex.position);
                max = max.max(vertex.position);
            }
        }
    }
    Ok(if min.cmple(max).all() {
        (min + max) * 0.5
    } else {
        Vec3::ZERO
    })
}

/// Terrain collision streamed around every moving body of one physics world
/// plus permanent anchors (authored spawns). Queries that may reach beyond the
/// loaded tiles use `cast_ray` against the exact fields instead.
pub struct TerrainStream {
    colliders: TerrainColliders,
    policy: BodyFocusPolicy,
}
impl TerrainStream {
    /// Tile edge in cells; Torque terrain blocks are 256 cells wide.
    const TILE_CELLS: i32 = 64;
    /// Covers the longest body-relative physics query (150-unit tool reach).
    const BODY_MARGIN: f32 = 160.0;

    pub fn new(fields: Vec<Arc<TerrainField>>, tag: u128, anchors: Vec<Focus>) -> Result<Self> {
        let tile_cells = fields
            .iter()
            .fold(Self::TILE_CELLS, |tile, f| gcd(tile, f.side()));
        let mut colliders = TerrainColliders::new(
            fields,
            StreamingConfig {
                tile_cells,
                ..Default::default()
            },
            tag,
        )?;
        colliders.set_anchors(anchors);
        Ok(Self {
            colliders,
            policy: BodyFocusPolicy {
                margin: Self::BODY_MARGIN,
                ..Default::default()
            },
        })
    }
    pub fn fields(&self) -> &[Arc<TerrainField>] {
        self.colliders.fields()
    }
    pub fn active_tiles(&self) -> usize {
        self.colliders.active_count()
    }
    /// Whether `handle` is one of the streamed terrain tiles (which share
    /// the map tag with interiors and static shapes).
    pub fn is_terrain_collider(&self, handle: rapier3d::prelude::ColliderHandle) -> bool {
        self.colliders.is_terrain_collider(handle)
    }
    /// Load tiles around every non-fixed body and refresh the query pipeline
    /// when the loaded set changed.
    pub fn update(&mut self, physics: &mut PhysicsWorld) {
        let foci = bri_physics::terrain::body_foci(physics, self.policy);
        if self.colliders.update(physics, &foci).changed() {
            bri_physics::detect_collisions(physics);
        }
    }
    /// Nearest exact terrain hit as (distance, normal), independent of loaded
    /// tiles. Distances are in units of the normalized direction.
    pub fn cast_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(f32, Vec3)> {
        cast_terrain(self.fields(), origin, direction, max_distance)
    }
}

/// Nearest exact terrain hit as (distance, normal) over `fields`. Distances
/// are in units of the normalized direction.
pub fn cast_terrain(
    fields: &[Arc<TerrainField>],
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<(f32, Vec3)> {
    bri_physics::terrain::cast_ray_fields(fields, origin, direction, max_distance).map(|hit| {
        // Fields are two-sided; report the face toward the ray like physics queries.
        let normal = if hit.normal.dot(direction) > 0.0 {
            -hit.normal
        } else {
            hit.normal
        };
        (hit.distance, normal)
    })
}

fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{MoveInput, Player, PlayerTuning};
    use bri_content::{TerrainLayer, terrain_field::*};

    fn field(repeat: bool) -> Arc<TerrainField> {
        let side = 128u32;
        let count = (side * side) as usize;
        let elevations = (0..count)
            .map(|i| {
                let (x, y) = ((i % 128) as f32, (i / 128) as f32);
                20.0 + (x * 0.1).sin() * 4.0 + (y * 0.07).cos() * 3.0
            })
            .collect();
        let terrain = Terrain {
            schema_version: 1,
            id: "t".into(),
            side,
            elevations,
            primary_layers: vec![0; count],
            layers: vec![TerrainLayer {
                slot: 0,
                material: "m".into(),
                weights: vec![255; count],
            }],
        };
        let instance = TerrainInstance {
            schema_version: TERRAIN_INSTANCE_SCHEMA,
            node: 0,
            terrain: "t".into(),
            square_size: 8.0,
            origin: [-512.0, 0.0, 512.0],
            repeat,
            repeat_source: RepeatSource::Authored,
            empty_runs: vec![],
            detail: None,
            bump: TerrainBump {
                texture: None,
                scale: 1.0,
                offset: 0.01,
                zero_scale: 8,
            },
            diagnostics: vec![],
        };
        Arc::new(TerrainField::new(terrain, &instance).unwrap())
    }

    /// A 2x4-footprint plate, one plate tall.
    fn plate_definitions() -> crate::definitions::Definitions {
        use bri_content::collision::{CollisionBody, Part};
        let mesh = bri_content::brick::Brick {
            schema_version: 1,
            id: "plate".into(),
            footprint_studs: [4, 2],
            height_plates: 1,
            attachment_rows: vec!["bbbb".into(), "bbbb".into()],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![],
        };
        let collision = CollisionBody {
            id: "plate".into(),
            parts: vec![Part::Box {
                center: [0.0; 3],
                size: [2.0, 0.2, 1.0],
            }],
        };
        let shape = bri_physics::content::collider(&collision)
            .unwrap()
            .build()
            .shared_shape()
            .clone();
        crate::definitions::Definitions {
            entries: [(
                "plate".into(),
                crate::definitions::Definition {
                    mesh,
                    collision,
                    shape,
                    indestructible: false,
                    special: Default::default(),
                    reflection: None,
                    link: None,
                    glass: [0.0; 4],
                    bot: None,
                },
            )]
            .into(),
        }
    }

    /// The Slopes: a level brick aimed at sloped terrain dips into the
    /// uphill side (v20 even sinks terrain ghosts 0.1). It plants; only a
    /// brick wholly under the surface is Buried.
    #[test]
    fn bricks_dipping_into_sloped_terrain_plant_and_only_buried_ones_fail() -> Result<()> {
        use crate::simulation::{Builder, PlantFailure};
        let field = field(false);
        let mut simulation = crate::simulation::Simulation::new(
            bri_world::World::new("Terrain".into(), "test".into(), vec![[1.0; 4]]),
            plate_definitions(),
            vec![],
        )?;
        let (x, z) = (1.0, -0.5);
        simulation.attach_terrain(
            vec![field.clone()],
            vec![Focus {
                center: Vec3::new(x, 20.0, z),
                radius: 50.0,
            }],
        )?;
        // The footprint spans 2 x 1 units; find the surface's low and high
        // points under it.
        let corners = [(0.0, -1.0), (2.0, -1.0), (0.0, 0.0), (2.0, 0.0)]
            .map(|(cx, cz)| field.height(cx, cz).unwrap());
        let low = corners.iter().copied().fold(f32::MAX, f32::min);
        let high = corners.iter().copied().fold(f32::MIN, f32::max);
        assert!(high - low > 0.05, "the test needs a slope: {corners:?}");
        let owner = bri_world::authority::Actor {
            owner: 1,
            ..Default::default()
        };
        let builder = Builder {
            actor: &owner,
            position: Vec3::new(x, high + 2.0, z),
            reach: 50.0,
        };
        let at = |bottom: f32| {
            bri_world::Brick::new(
                bri_world::ContentRef::Resolved("plate".into()),
                [x, bottom + 0.1, z],
                1,
            )
        };
        // Bottom on the plate grid just under the low side: the whole
        // footprint dips into the slope, the top still shows.
        let bottom = (low / 0.2).floor() * 0.2;
        assert!(bottom + 0.2 > low);
        simulation.plant(&builder, at(bottom))?;
        // Top under the surface everywhere: buried.
        let deep = ((low - 0.4) / 0.2).floor() * 0.2;
        let error = simulation.plant(&builder, at(deep)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<PlantFailure>(),
            Some(&PlantFailure::Buried),
            "{error:#}"
        );
        Ok(())
    }

    #[test]
    fn tool_rays_hit_terrain_where_no_tile_is_loaded() -> Result<()> {
        let field = field(true);
        let mut simulation = crate::simulation::Simulation::new(
            bri_world::World::new("Terrain".into(), "test".into(), vec![[1.0; 4]]),
            crate::definitions::Definitions::default(),
            vec![],
        )?;
        simulation.attach_terrain(vec![field.clone()], Vec::new())?;
        // Far from every body and anchor, so no collision tile is streamed.
        let (x, z) = (12_345.0, -9_876.0);
        let ground = field.height(x, z).unwrap();
        let origin = Vec3::new(x, ground + 50.0, z);
        let hit = simulation
            .target(origin, Vec3::NEG_Y, 100.0)?
            .expect("the ray reaches the terrain");
        assert!((hit.position.y - ground).abs() < 0.05, "{:?}", hit.position);
        assert!(hit.brick.is_none());
        Ok(())
    }

    #[test]
    fn players_stand_on_streamed_terrain_far_from_the_primary_block() -> Result<()> {
        let field = field(true);
        // Twelve blocks away, far beyond any finite patch around the origin.
        let (x, z) = (12_345.0, -9_876.0);
        let ground = field.height(x, z).unwrap();
        let mut physics = bri_physics::new_world();
        let mut stream = TerrainStream::new(vec![field.clone()], 7, Vec::new())?;
        let mut player = Player::spawn(
            &mut physics,
            1,
            Vec3::new(x, ground + 2.0, z),
            PlayerTuning::default(),
        )?;
        stream.update(&mut physics);
        assert!(stream.active_tiles() > 0);
        for _ in 0..240 {
            stream.update(&mut physics);
            player.step(&mut physics, MoveInput::default())?;
            physics.step();
        }
        let feet = Vec3::from(player.state().feet);
        assert!(player.state().grounded, "{:?}", player.state());
        assert!((feet.y - field.height(feet.x, feet.z).unwrap()).abs() < 0.1);
        // Walking a full block over keeps collision streaming under the body.
        for _ in 0..1200 {
            stream.update(&mut physics);
            player.step(
                &mut physics,
                MoveInput {
                    forward: 1.0,
                    ..Default::default()
                },
            )?;
            physics.step();
        }
        let moved = Vec3::from(player.state().feet);
        assert!(moved.distance(feet) > 20.0, "{moved} vs {feet}");
        assert!((moved.y - field.height(moved.x, moved.z).unwrap()).abs() < 0.5);
        // Exact rays reach terrain even where no tile is loaded.
        let far = Vec3::new(-40_000.0, 200.0, 33_000.0);
        let (distance, normal) = stream.cast_ray(far, Vec3::NEG_Y, 500.0).unwrap();
        assert!((far.y - distance - field.height(far.x, far.z).unwrap()).abs() < 0.01);
        assert!(normal.y > 0.5);
        Ok(())
    }

    /// A joined client predicts in a collision mirror that is queried but
    /// never stepped, so its kinematic body keeps its join pose while the
    /// motor walks on. Terrain must still stream under the walker, or a
    /// client that roams far from where it joined falls through the ground.
    #[test]
    fn predicted_players_stand_on_terrain_far_from_where_they_joined() -> Result<()> {
        let field = field(true);
        let (x, z) = (100.0, 100.0);
        let ground = field.height(x, z).unwrap();
        let state = {
            let mut scratch = bri_physics::new_world();
            let player = Player::spawn(
                &mut scratch,
                1,
                Vec3::new(x, ground + 0.5, z),
                PlayerTuning::default(),
            )?;
            player.state().clone()
        };
        let mut mirror = crate::prediction::CollisionMirror::new(
            crate::definitions::Definitions::default(),
            Vec::new(),
            Vec::new(),
        );
        mirror.attach_terrain(vec![field.clone()])?;
        let mut predictor = crate::prediction::Predictor::new(mirror, state, Default::default())?;
        for _ in 0..120 {
            predictor.step(MoveInput::default())?;
        }
        let start = Vec3::from(predictor.state().feet);
        assert!(predictor.state().grounded, "{:?}", predictor.state());
        // 7 units a second at 120 ticks: 800 units, across several of the
        // 512-unit tiles.
        for _ in 0..14_000 {
            predictor.step(MoveInput {
                forward: 1.0,
                ..Default::default()
            })?;
        }
        let feet = Vec3::from(predictor.state().feet);
        // Well past the tiles loaded around the join point (160-unit body
        // margin plus hysteresis).
        assert!(feet.distance(start) > 700.0, "{feet} vs {start}");
        let ground = field.height(feet.x, feet.z).unwrap();
        assert!((feet.y - ground).abs() < 0.5, "{feet} over ground {ground}");
        Ok(())
    }

    #[test]
    fn non_repeating_terrain_has_no_collision_outside_its_block() -> Result<()> {
        let field = field(false);
        let mut physics = bri_physics::new_world();
        let anchors = vec![Focus {
            center: Vec3::new(5_000.0, 20.0, 5_000.0),
            radius: 100.0,
        }];
        let mut stream = TerrainStream::new(vec![field], 7, anchors)?;
        stream.update(&mut physics);
        assert_eq!(stream.active_tiles(), 0);
        assert!(
            stream
                .cast_ray(Vec3::new(5_000.0, 200.0, 5_000.0), Vec3::NEG_Y, 500.0)
                .is_none()
        );
        Ok(())
    }
}
