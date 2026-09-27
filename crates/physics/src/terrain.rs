//! Authoritative streamed terrain collision.
//!
//! Terrain is divided into square tiles of `tile_cells` cells aligned to the
//! block period. Every focus (player, moving body, projectile, spawn anchor)
//! keeps the tiles around it loaded independently of any renderer; distant
//! foci load distant tiles. Tile shapes are exact heightfields built from the
//! shared `TerrainField` (same checkerboard split, same heights, primary-block
//! holes as removed cells) and are shared (`Arc`) between all period copies.
//! Loading/unloading only inserts/removes colliders; `clear` removes all of
//! them for map changes. Memory is bounded by `2 * (side / tile_cells)²`
//! cached shapes per terrain plus one collider per active tile.
use anyhow::{Result, ensure};
use bri_content::terrain_field::TerrainField;
use glam::Vec3;
use rapier3d::parry::shape::{HeightFieldCellStatus, HeightFieldFlags};
use rapier3d::parry::utils::Array2;
use rapier3d::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

/// A region that must have authoritative terrain collision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Focus {
    pub center: Vec3,
    /// Horizontal half-extent (world units) that must be covered.
    pub radius: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct StreamingConfig {
    /// Tile edge in cells; must divide the terrain side. Larger tiles have
    /// fewer seams; smaller tiles load less area per focus.
    pub tile_cells: i32,
    /// Extra distance a tile stays loaded after leaving every focus.
    pub hysteresis: f32,
    /// Upper bound on simultaneously active tile colliders.
    pub max_active_tiles: usize,
}
impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            tile_cells: 256,
            hysteresis: 32.0,
            max_active_tiles: 4096,
        }
    }
}

/// Per-body focus policy: covers the body's bounding sphere plus motion over
/// `lookahead` seconds plus `margin`.
#[derive(Clone, Copy, Debug)]
pub struct BodyFocusPolicy {
    pub margin: f32,
    pub lookahead: f32,
}
impl Default for BodyFocusPolicy {
    fn default() -> Self {
        Self {
            margin: 16.0,
            lookahead: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamStats {
    pub loaded: usize,
    pub unloaded: usize,
    pub active: usize,
    pub shapes_built: usize,
    pub cached_shapes: usize,
    /// Wanted tiles that were not loaded because `max_active_tiles` was hit.
    pub deferred: usize,
}
impl StreamStats {
    pub fn changed(&self) -> bool {
        self.loaded > 0 || self.unloaded > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileKey {
    pub field: usize,
    pub x: i32,
    pub y: i32,
}

pub struct TerrainColliders {
    fields: Vec<Arc<TerrainField>>,
    config: StreamingConfig,
    tag: u128,
    shapes: HashMap<(usize, i32, i32, bool), SharedShape>,
    active: BTreeMap<TileKey, ColliderHandle>,
    anchors: Vec<Focus>,
    totals: StreamStats,
}

impl TerrainColliders {
    pub fn new(fields: Vec<Arc<TerrainField>>, config: StreamingConfig, tag: u128) -> Result<Self> {
        ensure!(config.tile_cells > 0, "Invalid terrain tile size");
        ensure!(
            config.hysteresis.is_finite() && config.hysteresis >= 0.0,
            "Invalid terrain hysteresis"
        );
        for field in &fields {
            ensure!(
                field.side() % config.tile_cells == 0,
                "Terrain tile size must divide the block side"
            );
        }
        Ok(Self {
            fields,
            config,
            tag,
            shapes: HashMap::new(),
            active: BTreeMap::new(),
            anchors: Vec::new(),
            totals: StreamStats::default(),
        })
    }
    pub fn fields(&self) -> &[Arc<TerrainField>] {
        &self.fields
    }
    /// Foci that stay loaded permanently (e.g. authored spawn regions).
    pub fn set_anchors(&mut self, anchors: Vec<Focus>) {
        self.anchors = anchors;
    }
    pub fn active_tiles(&self) -> impl Iterator<Item = (&TileKey, &ColliderHandle)> {
        self.active.iter()
    }
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
    pub fn cached_shapes(&self) -> usize {
        self.shapes.len()
    }
    pub fn totals(&self) -> StreamStats {
        self.totals
    }
    pub fn is_terrain_collider(&self, handle: ColliderHandle) -> bool {
        self.active.values().any(|h| *h == handle)
    }
    /// Tiles whose horizontal extent is within `pad` of a focus square.
    fn tiles_for(&self, focus: &Focus, pad: f32, out: &mut BTreeSet<TileKey>) {
        if !(focus.center.is_finite() && focus.radius.is_finite() && focus.radius >= 0.0) {
            return;
        }
        let r = focus.radius + pad;
        for (index, field) in self.fields.iter().enumerate() {
            let (lo, hi) = field.height_range();
            // Foci far above/below all terrain need no terrain collision.
            if focus.center.y + r < lo || focus.center.y - r > hi {
                continue;
            }
            let t = self.config.tile_cells;
            let [gx0, gy0] = field.grid(focus.center.x - r, focus.center.z + r);
            let [gx1, gy1] = field.grid(focus.center.x + r, focus.center.z - r);
            if !(gx0.abs() < 1.0e7 && gy0.abs() < 1.0e7 && gx1.abs() < 1.0e7 && gy1.abs() < 1.0e7) {
                continue;
            }
            let (tx0, tx1) = (
                (gx0.floor() as i32).div_euclid(t),
                (gx1.floor() as i32).div_euclid(t),
            );
            let (ty0, ty1) = (
                (gy0.floor() as i32).div_euclid(t),
                (gy1.floor() as i32).div_euclid(t),
            );
            // A focus covering an absurd area is clipped; it still gets its
            // nearest tiles rather than exhausting memory.
            if i64::from(tx1 - tx0 + 1) * i64::from(ty1 - ty0 + 1) > 4096 {
                continue;
            }
            let blocks = field.side() / t;
            for y in ty0..=ty1 {
                for x in tx0..=tx1 {
                    if !(field.repeat || (0..blocks).contains(&x) && (0..blocks).contains(&y)) {
                        continue;
                    }
                    out.insert(TileKey { field: index, x, y });
                }
            }
        }
    }
    fn shape(&mut self, key: TileKey) -> (SharedShape, bool) {
        let field = &self.fields[key.field];
        let t = self.config.tile_cells;
        let blocks = field.side() / t;
        let primary = (0..blocks).contains(&key.x) && (0..blocks).contains(&key.y);
        let has_holes = primary && field.empty_square_count() > 0;
        let cache = (
            key.field,
            key.x.rem_euclid(blocks),
            key.y.rem_euclid(blocks),
            has_holes,
        );
        if let Some(shape) = self.shapes.get(&cache) {
            return (shape.clone(), false);
        }
        let (c0, r0) = (key.x * t, key.y * t);
        let n = t as usize;
        let mut heights = Array2::<Real>::repeat(n + 1, n + 1, 0.0);
        for i in 0..=n {
            let row = r0 + t - i as i32;
            for j in 0..=n {
                let flat = heights.flat_index(i, j);
                heights.data_mut()[flat] = field.sample(c0 + j as i32, row);
            }
        }
        let mut hf = rapier3d::parry::shape::HeightField::with_flags(
            heights,
            Vector::new(t as Real * field.spacing, 1.0, t as Real * field.spacing),
            HeightFieldFlags::FIX_INTERNAL_EDGES,
        );
        for i in 0..n {
            let row = r0 + t - i as i32 - 1;
            for j in 0..n {
                let column = c0 + j as i32;
                let mut status = HeightFieldCellStatus::empty();
                if (column ^ row) & 1 != 0 {
                    status |= HeightFieldCellStatus::ZIGZAG_SUBDIVISION;
                }
                if has_holes && field.is_empty_square(column, row) {
                    status |= HeightFieldCellStatus::CELL_REMOVED;
                }
                hf.set_cell_status(i, j, status);
            }
        }
        let shape = SharedShape::new(hf);
        self.shapes.insert(cache, shape.clone());
        (shape, true)
    }
    fn tile_translation(&self, key: TileKey) -> Vector {
        let field = &self.fields[key.field];
        let t = self.config.tile_cells as f32;
        Vector::new(
            field.origin.x + (key.x as f32 * t + t * 0.5) * field.spacing,
            field.origin.y,
            field.origin.z - (key.y as f32 * t + t * 0.5) * field.spacing,
        )
    }
    /// Load tiles required by `foci` (plus anchors); unload tiles no focus
    /// needs within the hysteresis band. Call `PhysicsWorld::detect_collisions`
    /// (or step) before issuing queries when the result reports a change.
    pub fn update(&mut self, physics: &mut PhysicsWorld, foci: &[Focus]) -> StreamStats {
        let mut wanted = BTreeSet::new();
        let mut keep = BTreeSet::new();
        for focus in foci.iter().chain(self.anchors.iter()) {
            self.tiles_for(focus, 0.0, &mut wanted);
            self.tiles_for(focus, self.config.hysteresis, &mut keep);
        }
        let mut stats = StreamStats::default();
        let stale: Vec<_> = self
            .active
            .keys()
            .filter(|k| !keep.contains(k))
            .copied()
            .collect();
        for key in stale {
            if let Some(handle) = self.active.remove(&key) {
                physics.remove_collider(handle);
                stats.unloaded += 1;
            }
        }
        for key in wanted {
            if self.active.contains_key(&key) {
                continue;
            }
            if self.active.len() >= self.config.max_active_tiles {
                stats.deferred += 1;
                continue;
            }
            let (shape, built) = self.shape(key);
            stats.shapes_built += usize::from(built);
            let collider = ColliderBuilder::new(shape)
                .translation(self.tile_translation(key))
                .user_data(self.tag);
            let handle = physics.insert_collider(collider, None);
            self.active.insert(key, handle);
            stats.loaded += 1;
        }
        stats.active = self.active.len();
        stats.cached_shapes = self.shapes.len();
        self.totals.loaded += stats.loaded;
        self.totals.unloaded += stats.unloaded;
        self.totals.shapes_built += stats.shapes_built;
        self.totals.deferred += stats.deferred;
        self.totals.active = stats.active;
        self.totals.cached_shapes = stats.cached_shapes;
        stats
    }
    /// Remove every terrain collider (map change/teardown). Cached shapes are
    /// dropped as well; the instance can be discarded afterwards.
    pub fn clear(&mut self, physics: &mut PhysicsWorld) -> usize {
        let count = self.active.len();
        for (_, handle) in std::mem::take(&mut self.active) {
            physics.remove_collider(handle);
        }
        self.shapes.clear();
        self.totals.active = 0;
        self.totals.cached_shapes = 0;
        count
    }
}

/// Foci for every non-fixed rigid body (players, dropped items, vehicles,
/// physical bricks). Sleeping bodies stay covered so they can be woken.
pub fn body_foci(physics: &PhysicsWorld, policy: BodyFocusPolicy) -> Vec<Focus> {
    let mut out = Vec::new();
    for (handle, body) in physics.bodies.iter() {
        if body.is_fixed() {
            continue;
        }
        let mut extent = 0.0f32;
        for collider in body.colliders() {
            if let Some(c) = physics.colliders.get(*collider) {
                let aabb = c.compute_aabb();
                let center = body.translation();
                let half = (aabb.maxs - aabb.mins) * 0.5;
                let offset = (aabb.mins + aabb.maxs) * 0.5 - center;
                extent = extent.max(offset.length() + half.length());
            }
        }
        let _ = handle;
        let p = body.translation();
        let v = body.linvel();
        let speed = v.length();
        out.push(Focus {
            center: Vec3::new(p.x, p.y, p.z),
            radius: policy.margin + extent + speed * policy.lookahead,
        });
    }
    out
}

/// Exact ray query against all terrain fields (independent of loaded tiles),
/// for long queries such as hitscan weapons. Returns the nearest hit.
pub fn cast_ray_fields(
    fields: &[Arc<TerrainField>],
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<bri_content::terrain_field::TerrainRayHit> {
    fields
        .iter()
        .filter_map(|f| f.cast_ray(origin, direction, max_distance))
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::terrain_field::*;
    use bri_content::{Terrain, TerrainLayer};

    fn field(repeat: bool, empty_runs: Vec<[u32; 2]>) -> Arc<TerrainField> {
        let side = 16u32;
        let count = (side * side) as usize;
        let elevations = (0..count)
            .map(|i| {
                10.0 + ((i % 16) as f32 * 0.7).sin() * 3.0 + ((i / 16) as f32 * 0.45).cos() * 2.0
            })
            .collect();
        let terrain = Terrain {
            schema_version: 1,
            id: "fixture".into(),
            side,
            elevations,
            primary_layers: vec![0; count],
            layers: vec![TerrainLayer {
                slot: 0,
                material: "t".into(),
                weights: vec![255; count],
            }],
        };
        let instance = TerrainInstance {
            schema_version: TERRAIN_INSTANCE_SCHEMA,
            node: 0,
            terrain: "fixture".into(),
            square_size: 2.0,
            origin: [-16.0, 5.0, 16.0],
            repeat,
            repeat_source: RepeatSource::Authored,
            empty_runs,
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

    fn down(physics: &PhysicsWorld, x: f32, z: f32) -> Option<f32> {
        let ray = Ray::new(Vector::new(x, 100.0, z), Vector::new(0.0, -1.0, 0.0));
        physics
            .query_pipeline()
            .cast_ray(&ray, 1000.0, true)
            .map(|(_, t)| 100.0 - t)
    }

    #[test]
    fn heightfield_tiles_match_field_across_negative_periodic_seams_and_holes() {
        let f = field(true, vec![[3 * 16 + 4, 2]]);
        let mut physics = PhysicsWorld::new();
        let config = StreamingConfig {
            tile_cells: 8,
            hysteresis: 4.0,
            max_active_tiles: 64,
        };
        let mut stream = TerrainColliders::new(vec![f.clone()], config, 7).unwrap();
        // Two distant players, far outside the primary block in opposite directions.
        let a = f.vertex(-37, 21);
        let b = f.vertex(90, -55);
        let stats = stream.update(
            &mut physics,
            &[
                Focus {
                    center: a,
                    radius: 12.0,
                },
                Focus {
                    center: b,
                    radius: 12.0,
                },
            ],
        );
        assert!(stats.loaded >= 2 && stats.changed());
        physics.detect_collisions(&(), &());
        for (center, span) in [(a, 10.0f32), (b, 10.0)] {
            for i in 0..121 {
                let x = center.x - span + (i % 11) as f32 * 2.0 * span / 10.0 + 0.137;
                let z = center.z - span + (i / 11) as f32 * 2.0 * span / 10.0 + 0.291;
                let expected = f.height(x, z).unwrap();
                let got = down(&physics, x, z).expect("streamed terrain under focus");
                assert!(
                    (got - expected).abs() < 2e-3,
                    "{got} vs {expected} at {x},{z}"
                );
            }
        }
        // Between the players nothing is loaded: coverage follows foci, not a view.
        let mid = (a + b) * 0.5;
        assert!(down(&physics, mid.x, mid.z).is_none());
        // Primary-block hole is removed only in the primary block.
        let hole = f.vertex(4, 3) + Vec3::new(0.5 * f.spacing, 0.0, -0.5 * f.spacing);
        let repeat = hole + Vec3::new(16.0 * f.spacing, 0.0, 0.0);
        stream.update(
            &mut physics,
            &[
                Focus {
                    center: hole,
                    radius: 4.0,
                },
                Focus {
                    center: repeat,
                    radius: 4.0,
                },
            ],
        );
        physics.detect_collisions(&(), &());
        assert!(down(&physics, hole.x, hole.z).is_none());
        let expected = f.height(repeat.x, repeat.z).unwrap();
        assert!((down(&physics, repeat.x, repeat.z).unwrap() - expected).abs() < 2e-3);
        // Old foci unloaded; clear removes every terrain collider.
        assert!(stream.totals().unloaded > 0);
        assert_eq!(physics.colliders.len(), stream.active_count());
        let removed = stream.clear(&mut physics);
        assert!(removed > 0 && physics.colliders.is_empty() && stream.cached_shapes() == 0);
    }

    #[test]
    fn non_repeating_terrain_and_body_foci() {
        let f = field(false, vec![]);
        let mut physics = crate::new_world();
        physics.gravity = Vector::new(0.0, -9.81, 0.0);
        let config = StreamingConfig {
            tile_cells: 8,
            hysteresis: 0.0,
            max_active_tiles: 64,
        };
        let mut stream = TerrainColliders::new(vec![f.clone()], config, 7).unwrap();
        let inside = f.vertex(5, 5);
        let outside = f.vertex(40, 5);
        let (body, _) = physics.insert(
            RigidBodyBuilder::dynamic().translation(Vector::new(
                inside.x,
                inside.y + 2.0,
                inside.z,
            )),
            ColliderBuilder::ball(0.5),
        );
        let (far, _) = physics.insert(
            RigidBodyBuilder::dynamic().translation(Vector::new(outside.x, 20.0, outside.z)),
            ColliderBuilder::ball(0.5),
        );
        physics.step();
        let foci = body_foci(
            &physics,
            BodyFocusPolicy {
                margin: 2.0,
                lookahead: 0.5,
            },
        );
        assert_eq!(foci.len(), 2);
        let stats = stream.update(&mut physics, &foci);
        assert!(stats.loaded >= 1);
        // Only primary-block tiles exist without repetition.
        assert!(
            stream
                .active_tiles()
                .all(|(k, _)| (0..2).contains(&k.x) && (0..2).contains(&k.y))
        );
        for _ in 0..600 {
            let foci = body_foci(&physics, BodyFocusPolicy::default());
            if stream.update(&mut physics, &foci).changed() {
                physics.detect_collisions(&(), &());
            }
            physics.step();
        }
        let rest = physics.bodies[body].translation().y;
        let ground = f
            .height(
                physics.bodies[body].translation().x,
                physics.bodies[body].translation().z,
            )
            .unwrap();
        assert!(
            rest > ground && rest < ground + 1.0,
            "ball rests on terrain: {rest} vs {ground}"
        );
        assert!(
            physics.bodies[far].translation().y < 0.0,
            "no terrain outside the single block"
        );
        let hit = cast_ray_fields(
            std::slice::from_ref(&f),
            Vec3::new(inside.x, 100.0, inside.z),
            Vec3::NEG_Y,
            500.0,
        )
        .unwrap();
        assert!((hit.point.y - f.height(inside.x, inside.z).unwrap()).abs() < 1e-3);
    }
}
