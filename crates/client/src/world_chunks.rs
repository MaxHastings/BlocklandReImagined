//! Chunked replicated-brick geometry. A brick change rebuilds only the chunks
//! whose membership or appearance changed, instead of the whole world mesh.
//! Every chunk indexes one shared material palette, so brick textures and
//! bind groups are uploaded once per session and chunk uploads are geometry.
use crate::materials::BrickMaterials;
use anyhow::{Context, Result, ensure};
use bri_content::brick::Brick as BrickMesh;
use bri_net::protocol::PublicWorld;
use bri_render::scene::{AlphaMode, SceneData};
use bri_world::{Brick, ContentRef};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

/// Chunk edge in world units (64 studs, 160 plates); chunks are 3D so tall
/// builds split vertically as well.
pub const CHUNK_SIZE: f32 = 32.0;
pub type ChunkKey = [i32; 3];

/// Bricks belong to the chunk holding their origin. Large bricks may extend
/// past it; chunk bounds come from the built vertices, not the key.
pub fn chunk_key(position: [f32; 3]) -> ChunkKey {
    position.map(|v| (v / CHUNK_SIZE).floor() as i32)
}

/// Every material a replicated brick can bind: the five surfaces, the blank
/// print surface, every stock print, and a blended copy of each for
/// translucent paint and the flashing color effect.
pub struct BrickPalette {
    pub scene: SceneData,
    surfaces: [usize; 6],
}
impl BrickPalette {
    pub fn new(materials: &BrickMaterials) -> Result<Self> {
        let mut scene = SceneData {
            id: "replicated-bricks/palette".into(),
            name: "Brick material palette".into(),
            ..Default::default()
        };
        let surfaces = materials.surface_materials(&mut scene);
        for print in &materials.bundle.prints {
            materials.print_material(&mut scene, &print.id)?;
        }
        for index in 0..scene.materials.len() {
            let mut copy = scene.materials[index].clone();
            if copy.alpha != AlphaMode::Blend {
                copy.alpha = AlphaMode::Blend;
                if !scene.materials.contains(&copy) {
                    scene.materials.push(copy);
                }
            }
        }
        scene.validate()?;
        Ok(Self { scene, surfaces })
    }
    /// The material-free development path: one white vertex-lit material.
    pub fn development() -> Self {
        let mut scene = SceneData {
            id: "replicated-bricks/development-palette".into(),
            name: "Development brick palette".into(),
            ..Default::default()
        };
        scene
            .materials
            .push(bri_render::scene::Material::vertex_lit(
                "Development brick color",
                0,
            ));
        let mut blend = scene.materials[0].clone();
        blend.alpha = AlphaMode::Blend;
        scene.materials.push(blend);
        Self {
            scene,
            surfaces: [0; 6],
        }
    }
}

/// Only fields that change a brick's pixels; events, names and ownership do not.
fn same_appearance(a: &Brick, b: &Brick) -> bool {
    a.visible == b.visible
        && (!a.visible
            || (a.position == b.position
                && a.quarter_turns == b.quarter_turns
                && a.color == b.color
                && a.color_effect == b.color_effect
                && a.shape_effect == b.shape_effect
                && a.definition == b.definition
                && a.print == b.print))
}
fn visible_key(brick: Option<&Brick>, left_out: bool) -> Option<ChunkKey> {
    brick
        .filter(|b| b.visible && !left_out)
        .map(|b| chunk_key(b.position))
}

/// CPU chunk membership for the last applied replica. Owned by one builder
/// at a time (moved into the background job and back).
#[derive(Default)]
pub struct ChunkedWorld {
    source: Option<Arc<PublicWorld>>,
    members: HashMap<ChunkKey, BTreeSet<u64>>,
    triangles: HashMap<ChunkKey, usize>,
    total_triangles: usize,
    /// Visible bricks drawn elsewhere for now (easing to a new colour).
    left_out: BTreeSet<u64>,
    /// Bricks left out for failing validation; any change to one rebuilds
    /// its chunk, since it may now be valid.
    invalid: BTreeSet<u64>,
    /// Grid bounds of every brick placed in a chunk, for the faces its
    /// neighbours hide (`crate::brick_cover`).
    cover: bri_sim::grid::Index,
    /// Set while an update is under way; a failed update leaves the cover
    /// index unknown, so the next one rebuilds everything.
    cover_stale: bool,
}

/// Rebuilt chunks; `None` removes a chunk that no longer holds visible bricks.
pub type ChunkChanges = Vec<(ChunkKey, Option<SceneData>)>;

impl ChunkedWorld {
    pub fn source(&self) -> Option<&Arc<PublicWorld>> {
        self.source.as_ref()
    }
    pub fn chunk_count(&self) -> usize {
        self.members.len()
    }
    pub fn chunk_bricks(&self, key: ChunkKey) -> usize {
        self.members.get(&key).map_or(0, BTreeSet::len)
    }
    pub fn triangles(&self) -> usize {
        self.total_triangles
    }

    /// Bring the chunks up to `next`. Only chunks gaining, losing or holding a
    /// changed brick are rebuilt. `known` lists every brick that may differ
    /// from the last applied world; without it, whole worlds are compared.
    /// Atomic: on error no state changes, and the budget rejects the whole
    /// replacement before any chunk is built.
    pub fn update(
        &mut self,
        next: Arc<PublicWorld>,
        known: Option<&crate::network::WorldChanges>,
        meshes: &BTreeMap<String, BrickMesh>,
        palette: &BrickPalette,
        materials: Option<&BrickMaterials>,
        max_triangles: usize,
    ) -> Result<ChunkChanges> {
        let left_out = self.left_out.clone();
        self.update_leaving_out(
            next,
            known,
            &left_out,
            meshes,
            palette,
            materials,
            max_triangles,
        )
    }
    /// `update`, leaving the `left_out` bricks out of their chunks while
    /// something else draws them. Bricks entering or leaving that set
    /// rebuild their chunks like any other change.
    #[allow(clippy::too_many_arguments)] // `update` plus the left-out set
    pub fn update_leaving_out(
        &mut self,
        next: Arc<PublicWorld>,
        known: Option<&crate::network::WorldChanges>,
        left_out: &BTreeSet<u64>,
        meshes: &BTreeMap<String, BrickMesh>,
        palette: &BrickPalette,
        materials: Option<&BrickMaterials>,
        max_triangles: usize,
    ) -> Result<ChunkChanges> {
        ensure!(
            !next.palette.is_empty()
                && next.palette.len() <= 256
                && next
                    .palette
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "Invalid replicated world paint palette"
        );
        let mut dirty = BTreeSet::new();
        let mut moves = Vec::new();
        let was_out = &self.left_out;
        let was_invalid = &self.invalid;
        let mut change = |id: u64, old: Option<&Brick>, new: Option<&Brick>| {
            let (out_before, out_now) = (was_out.contains(&id), left_out.contains(&id));
            if let (Some(old), Some(new)) = (old, new)
                && same_appearance(old, new)
                && !was_invalid.contains(&id)
                && out_before == out_now
            {
                return;
            }
            let (from, to) = (visible_key(old, out_before), visible_key(new, out_now));
            dirty.extend(from);
            dirty.extend(to);
            if from.is_some() || to.is_some() {
                moves.push((id, from, to));
            }
        };
        // Palette updates are append-only; anything else repaints everything.
        let incremental = self
            .source
            .as_ref()
            .filter(|previous| next.palette.starts_with(&previous.palette))
            .filter(|_| !self.cover_stale);
        if let (Some(previous), Some(known)) = (incremental, known) {
            for id in known
                .bricks
                .iter()
                .chain(was_out.symmetric_difference(left_out))
            {
                change(*id, previous.bricks.get(id), next.bricks.get(id));
            }
        } else if let Some(previous) = incremental {
            let mut old = previous.bricks.iter().peekable();
            let mut new = next.bricks.iter().peekable();
            loop {
                match (old.peek(), new.peek()) {
                    (None, None) => break,
                    (Some((a, _)), Some((b, _))) if a == b => {
                        let ((id, a), (_, b)) = (old.next().unwrap(), new.next().unwrap());
                        change(*id, Some(a), Some(b));
                    }
                    (Some((a, _)), Some((b, _))) if a > b => {
                        let (id, b) = new.next().unwrap();
                        change(*id, None, Some(b));
                    }
                    (Some(_), _) => {
                        let (id, a) = old.next().unwrap();
                        change(*id, Some(a), None);
                    }
                    (None, Some(_)) => {
                        let (id, b) = new.next().unwrap();
                        change(*id, None, Some(b));
                    }
                }
            }
        } else {
            for (id, brick) in &next.bricks {
                change(*id, None, Some(brick));
            }
        }
        // Keep the cover index to what is placed, and rebuild the chunks of
        // every brick touching a changed one: its hidden faces may change.
        self.cover_stale = true;
        if incremental.is_none() {
            self.cover = Default::default();
        }
        let mut touched = BTreeSet::new();
        for (id, _, to) in &moves {
            if let Some(old) = self.cover.get(*id) {
                crate::brick_cover::neighbours(&self.cover, old, &mut touched);
                self.cover.remove(*id);
            }
            if to.is_some()
                && let Some(new) = next
                    .bricks
                    .get(id)
                    .and_then(|b| crate::brick_cover::bounds(b, meshes))
            {
                self.cover.insert(*id, new);
                crate::brick_cover::neighbours(&self.cover, new, &mut touched);
            }
        }
        for id in touched {
            dirty.extend(visible_key(next.bricks.get(&id), left_out.contains(&id)));
        }
        let mut staged: BTreeMap<ChunkKey, BTreeSet<u64>> = if incremental.is_some() {
            dirty
                .iter()
                .map(|key| (*key, self.members.get(key).cloned().unwrap_or_default()))
                .collect()
        } else {
            // Every existing chunk is replaced or removed.
            dirty.extend(self.members.keys().copied());
            dirty.iter().map(|key| (*key, BTreeSet::new())).collect()
        };
        for (id, from, to) in moves {
            if let Some(from) = from {
                staged
                    .get_mut(&from)
                    .context("Chunk move source")?
                    .remove(&id);
            }
            if let Some(to) = to {
                staged.get_mut(&to).context("Chunk move target")?.insert(id);
            }
        }
        // Leave out bricks that fail validation, then count every brick being
        // rebuilt before allocating; each distinct mesh is validated once,
        // not once per placement.
        let mut validated = std::collections::HashSet::new();
        let mut invalid = self.invalid.clone();
        invalid.retain(|id| next.bricks.contains_key(id));
        for ids in staged.values_mut() {
            ids.retain(|id| {
                let drawable =
                    crate::world_scene::drawable(*id, &next.bricks[id], next.palette.len());
                if drawable {
                    invalid.remove(id);
                } else {
                    invalid.insert(*id);
                }
                drawable
            });
        }
        for ids in staged.values() {
            for id in ids {
                let brick = &next.bricks[id];
                let ContentRef::Resolved(definition) = &brick.definition else {
                    anyhow::bail!(
                        "Visible brick {id} has an unresolved definition: {:?}",
                        brick.definition
                    );
                };
                let mesh = meshes.get(definition).with_context(|| {
                    format!("Visible brick {id} definition {definition} has no native render mesh")
                })?;
                if validated.insert(definition.as_str()) {
                    mesh.validate()
                        .with_context(|| format!("Brick definition {definition} mesh"))?;
                }
            }
        }
        let jobs: Vec<_> = staged.iter().filter(|(_, ids)| !ids.is_empty()).collect();
        let covers = crate::brick_cover::Covers {
            index: &self.cover,
            world: &next,
            meshes,
            left_out,
            invalid: &invalid,
        };
        let built = build_chunks(&jobs, &covers, palette, materials)?;
        // The budget counts triangles drawn, after covered faces are culled.
        let counts: BTreeMap<ChunkKey, usize> = jobs
            .iter()
            .zip(&built)
            .map(|((key, _), scene)| (**key, scene.indices.len() / 3))
            .collect();
        let total = self.total_triangles
            - dirty
                .iter()
                .filter_map(|key| self.triangles.get(key))
                .sum::<usize>()
            + counts.values().sum::<usize>();
        ensure!(
            total <= max_triangles,
            "World requires {total} brick triangles, exceeding configured render budget {max_triangles}; no bricks were omitted"
        );
        let mut changes: ChunkChanges = Vec::with_capacity(staged.len());
        let mut built = built.into_iter();
        for (key, ids) in staged {
            if ids.is_empty() {
                self.members.remove(&key);
                self.triangles.remove(&key);
                changes.push((key, None));
            } else {
                self.triangles.insert(key, counts[&key]);
                self.members.insert(key, ids);
                changes.push((key, Some(built.next().context("Chunk build count")?)));
            }
        }
        self.total_triangles = total;
        self.source = Some(next);
        self.left_out = left_out.clone();
        self.invalid = invalid;
        self.cover_stale = false;
        Ok(changes)
    }
}

/// Build chunks in parallel when a load or repaint touches many of them.
fn build_chunks(
    jobs: &[(&ChunkKey, &BTreeSet<u64>)],
    covers: &crate::brick_cover::Covers<'_>,
    palette: &BrickPalette,
    materials: Option<&BrickMaterials>,
) -> Result<Vec<SceneData>> {
    let build = |(key, ids): &(&ChunkKey, &BTreeSet<u64>)| {
        build_chunk(**key, ids, covers, palette, materials)
    };
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(jobs.len().div_ceil(4));
    if threads <= 1 {
        return jobs.iter().map(build).collect();
    }
    let per = jobs.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let workers: Vec<_> = jobs
            .chunks(per)
            .map(|part| scope.spawn(move || part.iter().map(build).collect::<Result<Vec<_>>>()))
            .collect();
        let mut out = Vec::with_capacity(jobs.len());
        for worker in workers {
            out.extend(
                worker
                    .join()
                    .map_err(|_| anyhow::anyhow!("Chunk builder panicked"))??,
            );
        }
        Ok(out)
    })
}

fn build_chunk(
    key: ChunkKey,
    ids: &BTreeSet<u64>,
    covers: &crate::brick_cover::Covers<'_>,
    palette: &BrickPalette,
    materials: Option<&BrickMaterials>,
) -> Result<SceneData> {
    let world = covers.world;
    palette_scene(
        format!("{}/replicated-bricks/{key:?}", world.map_id),
        format!("{} bricks {key:?}", world.name),
        ids.iter().map(|id| (*id, &world.bricks[id])),
        &world.palette,
        covers.meshes,
        palette,
        materials,
        Some(covers),
    )
}

/// One brick in its own frame against the shared palette, for per-brick
/// models (knocked-out brick debris) that upload geometry only.
pub fn build_brick(
    brick: &Brick,
    colors: &[[f32; 4]],
    meshes: &BTreeMap<String, BrickMesh>,
    palette: &BrickPalette,
    materials: Option<&BrickMaterials>,
) -> Result<SceneData> {
    brick.validate(colors.len())?;
    palette_scene(
        "replicated-bricks/single".into(),
        "Single brick".into(),
        [(0, brick)],
        colors,
        meshes,
        palette,
        materials,
        None,
    )
}

#[allow(clippy::too_many_arguments)] // bricks plus the shared palette context
fn palette_scene<'a>(
    id: String,
    name: String,
    bricks: impl IntoIterator<Item = (u64, &'a Brick)>,
    colors: &[[f32; 4]],
    meshes: &BTreeMap<String, BrickMesh>,
    palette: &BrickPalette,
    materials: Option<&BrickMaterials>,
    // Chunk builds (meshes validated once) hide covered faces.
    covers: Option<&crate::brick_cover::Covers<'_>>,
) -> Result<SceneData> {
    let mut scene = SceneData {
        id,
        name,
        images: vec![],
        materials: palette.scene.materials.clone(),
        ..Default::default()
    };
    for (id, brick) in bricks {
        let hidden = covers.map_or(0, |covers| {
            crate::brick_cover::mesh(brick, meshes).map_or(0, |mesh| covers.hidden(id, brick, mesh))
        });
        crate::world_scene::append_world_brick(
            &mut scene,
            id,
            brick,
            colors,
            meshes,
            palette.surfaces,
            materials,
            covers.is_some(),
            hidden,
        )?;
    }
    ensure!(
        scene.materials.len() == palette.scene.materials.len(),
        "Brick chunk needed a material outside the shared palette"
    );
    scene.coalesce_opaque_batches()?;
    scene.omissions.sort();
    scene.omissions.dedup();
    Ok(scene)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::network::WorldChanges;
    use bri_content::brick::{Face, Quad, Surface, Vertex};

    pub(crate) fn meshes() -> BTreeMap<String, BrickMesh> {
        let quad = Quad {
            face: Face::Omni,
            surface: Surface::Side,
            vertices: [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ]
            .map(|position| Vertex {
                position,
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            }),
            colors: None,
        };
        BTreeMap::from([(
            "definition/a".into(),
            BrickMesh {
                schema_version: 1,
                id: "mesh/a".into(),
                footprint_studs: [1, 1],
                height_plates: 1,
                attachment_rows: vec!["b".into()],
                collision_boxes: vec![],
                needs_external_collision: false,
                coverage: None,
                quads: vec![quad],
            },
        )])
    }
    fn brick(position: [f32; 3]) -> Brick {
        Brick::new(ContentRef::Resolved("definition/a".into()), position, 1)
    }
    fn world(bricks: impl IntoIterator<Item = (u64, Brick)>) -> Arc<PublicWorld> {
        Arc::new(PublicWorld {
            name: "Test".into(),
            map_id: "map/test".into(),
            palette: vec![[0.9, 0.2, 0.1, 1.0], [0.2, 0.4, 0.8, 0.5]],
            bricks: bricks.into_iter().collect(),
        })
    }
    fn try_update(
        state: &mut ChunkedWorld,
        next: &Arc<PublicWorld>,
        known: Option<&[u64]>,
    ) -> Result<BTreeMap<ChunkKey, Option<usize>>> {
        let known = known.map(|ids| WorldChanges {
            bricks: ids.iter().copied().collect(),
            palette: false,
        });
        Ok(state
            .update(
                next.clone(),
                known.as_ref(),
                &meshes(),
                &BrickPalette::development(),
                None,
                100,
            )?
            .into_iter()
            .map(|(key, scene)| (key, scene.map(|s| s.vertices.len())))
            .collect())
    }
    fn update(
        state: &mut ChunkedWorld,
        next: &Arc<PublicWorld>,
        known: Option<&[u64]>,
    ) -> BTreeMap<ChunkKey, Option<usize>> {
        try_update(state, next, known).unwrap()
    }

    #[test]
    fn one_brick_change_rebuilds_only_its_chunks() {
        let far = CHUNK_SIZE * 2.5;
        let base = world([(1, brick([1.0; 3])), (2, brick([far, 1.0, 1.0]))]);
        // Identical results from the replica's change list and a full compare.
        for known in [false, true] {
            let mut state = ChunkedWorld::default();
            let initial = update(&mut state, &base, None);
            assert_eq!(initial.len(), 2);
            assert_eq!(state.chunk_count(), 2);
            let ids = |ids: &'static [u64]| known.then_some(ids);

            // Names (like events and ownership) do not touch geometry.
            let mut same = (*base).clone();
            same.bricks.get_mut(&1).unwrap().name = Some("named".into());
            assert!(update(&mut state, &Arc::new(same), ids(&[1])).is_empty());

            // Repaint: only brick 1's chunk.
            let mut painted = (*base).clone();
            painted.bricks.get_mut(&1).unwrap().color = 1;
            let painted = Arc::new(painted);
            let changes = update(&mut state, &painted, ids(&[1]));
            assert_eq!(changes, BTreeMap::from([(chunk_key([1.0; 3]), Some(4))]));

            // Plant beside brick 2: only brick 2's chunk.
            let mut planted = (*painted).clone();
            planted.bricks.insert(3, brick([far + 1.0, 1.0, 1.0]));
            let planted = Arc::new(planted);
            let changes = update(&mut state, &planted, ids(&[3]));
            assert_eq!(
                changes,
                BTreeMap::from([(chunk_key([far, 1.0, 1.0]), Some(8))])
            );

            // Move brick 1 next to brick 2; its old chunk empties.
            let mut moved = (*planted).clone();
            moved.bricks.get_mut(&1).unwrap().position = [far + 2.0, 1.0, 1.0];
            let changes = update(&mut state, &Arc::new(moved.clone()), ids(&[1]));
            assert_eq!(
                changes,
                BTreeMap::from([
                    (chunk_key([1.0; 3]), None),
                    (chunk_key([far, 1.0, 1.0]), Some(12)),
                ])
            );
            assert_eq!((state.chunk_count(), state.triangles()), (1, 6));

            // Hiding and removing both drop geometry.
            moved.bricks.get_mut(&2).unwrap().visible = false;
            moved.bricks.remove(&3);
            let changes = update(&mut state, &Arc::new(moved), ids(&[2, 3]));
            assert_eq!(
                changes,
                BTreeMap::from([(chunk_key([far, 1.0, 1.0]), Some(4))])
            );
            assert_eq!(state.triangles(), 2);
        }
    }

    /// A 1x1 brick three plates tall: one quad per face, every face
    /// hiding its neighbour and hidden once fully covered (v20 COVERAGE).
    fn box_meshes() -> BTreeMap<String, BrickMesh> {
        use bri_content::brick::Coverage;
        let (x, y, z) = (0.25f32, 0.3f32, 0.25f32);
        let quad = |face: Face, normal: [f32; 3], corners: [[f32; 3]; 4]| Quad {
            face,
            surface: Surface::Side,
            vertices: corners.map(|position| Vertex {
                position,
                normal,
                uv: [0.0, 0.0],
            }),
            colors: None,
        };
        let quads = vec![
            quad(Face::Top, [0., 1., 0.], [[-x, y, -z], [-x, y, z], [x, y, z], [x, y, -z]]),
            quad(Face::Bottom, [0., -1., 0.], [[-x, -y, -z], [x, -y, -z], [x, -y, z], [-x, -y, z]]),
            quad(Face::North, [0., 0., -1.], [[-x, -y, -z], [-x, y, -z], [x, y, -z], [x, -y, -z]]),
            quad(Face::East, [1., 0., 0.], [[x, -y, -z], [x, y, -z], [x, y, z], [x, -y, z]]),
            quad(Face::South, [0., 0., 1.], [[x, -y, z], [x, y, z], [-x, y, z], [-x, -y, z]]),
            quad(Face::West, [-1., 0., 0.], [[-x, -y, z], [-x, y, z], [-x, y, -z], [-x, -y, -z]]),
        ];
        let cover = |required_area| Coverage {
            hides_adjacent: true,
            required_area,
        };
        BTreeMap::from([(
            "definition/box".into(),
            BrickMesh {
                schema_version: 1,
                id: "mesh/box".into(),
                footprint_studs: [1, 1],
                height_plates: 3,
                attachment_rows: vec!["b".into(); 3],
                collision_boxes: vec![],
                needs_external_collision: false,
                coverage: Some([cover(1.), cover(1.), cover(3.), cover(3.), cover(3.), cover(3.)]),
                quads,
            },
        )])
    }

    #[test]
    fn neighbours_hide_covered_faces_as_v20_coverage_does() {
        let meshes = box_meshes();
        let at = |x: f32, color: u8| {
            let mut b = Brick::new(ContentRef::Resolved("definition/box".into()), [x, 0.3, 0.25], 1);
            b.color = color;
            b
        };
        let quads = |changes: ChunkChanges| -> BTreeMap<ChunkKey, usize> {
            changes
                .into_iter()
                .map(|(key, scene)| (key, scene.map_or(0, |s| s.vertices.len() / 4)))
                .collect()
        };
        let run = |state: &mut ChunkedWorld, world: &Arc<PublicWorld>, known: Option<&[u64]>| {
            let known = known.map(|ids| WorldChanges {
                bricks: ids.iter().copied().collect(),
                palette: false,
            });
            quads(
                state
                    .update(
                        world.clone(),
                        known.as_ref(),
                        &meshes,
                        &BrickPalette::development(),
                        None,
                        1000,
                    )
                    .unwrap(),
            )
        };
        // Two opaque bricks side by side: each loses the face they share.
        let mut state = ChunkedWorld::default();
        let pair = world([(1, at(0.25, 0)), (2, at(0.75, 0))]);
        assert_eq!(run(&mut state, &pair, None), BTreeMap::from([(chunk_key([0.25, 0.3, 0.25]), 10)]));
        // A translucent neighbour hides nothing, but is hidden itself.
        let mut clear = (*pair).clone();
        clear.bricks.get_mut(&2).unwrap().color = 1;
        let clear = Arc::new(clear);
        assert_eq!(run(&mut state, &clear, Some(&[2])), BTreeMap::from([(chunk_key([0.25, 0.3, 0.25]), 11)]));
        // Across a chunk boundary, removing one brick rebuilds the other's
        // chunk so its face comes back.
        let (a, b) = (CHUNK_SIZE - 0.25, CHUNK_SIZE + 0.25);
        let mut state = ChunkedWorld::default();
        let edge = world([(1, at(a, 0)), (2, at(b, 0))]);
        let first = run(&mut state, &edge, None);
        assert_eq!(first.values().sum::<usize>(), 10);
        let mut gone = (*edge).clone();
        gone.bricks.remove(&2);
        assert_eq!(
            run(&mut state, &Arc::new(gone), Some(&[2])),
            BTreeMap::from([
                (chunk_key([a, 0.3, 0.25]), 6),
                (chunk_key([b, 0.3, 0.25]), 0),
            ])
        );
        // Placing it back hides both faces again, in both chunks.
        assert_eq!(
            run(&mut state, &edge, Some(&[2])),
            BTreeMap::from([
                (chunk_key([a, 0.3, 0.25]), 5),
                (chunk_key([b, 0.3, 0.25]), 5),
            ])
        );
    }

    /// An easing brick leaves its chunk while drawn apart and comes back
    /// once settled, both by rebuilding just that chunk.
    #[test]
    fn left_out_bricks_leave_and_rejoin_their_chunk() {
        let base = world([(1, brick([1.0; 3])), (2, brick([2.0, 1.0, 1.0]))]);
        let mut state = ChunkedWorld::default();
        update(&mut state, &base, None);
        let key = chunk_key([1.0; 3]);
        let rebuild = |state: &mut ChunkedWorld, next: &Arc<PublicWorld>, out: &[u64]| {
            let known = WorldChanges::default();
            state
                .update_leaving_out(
                    next.clone(),
                    Some(&known),
                    &out.iter().copied().collect(),
                    &meshes(),
                    &BrickPalette::development(),
                    None,
                    100,
                )
                .unwrap()
                .into_iter()
                .map(|(key, scene)| (key, scene.map(|s| s.vertices.len())))
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(
            rebuild(&mut state, &base, &[1]),
            BTreeMap::from([(key, Some(4))])
        );
        assert_eq!(state.chunk_bricks(key), 1);
        // Nothing changed: nothing rebuilt.
        assert!(rebuild(&mut state, &base, &[1]).is_empty());
        assert_eq!(
            rebuild(&mut state, &base, &[]),
            BTreeMap::from([(key, Some(8))])
        );
        assert_eq!(state.chunk_bricks(key), 2);
        // A repaint while left out still keeps it out; `update` keeps the set.
        let mut painted = (*base).clone();
        painted.bricks.get_mut(&1).unwrap().color = 1;
        rebuild(&mut state, &base, &[1]);
        let painted = Arc::new(painted);
        update(&mut state, &painted, Some(&[1]));
        assert_eq!(state.chunk_bricks(key), 1);
    }

    /// An invalid replicated brick stays out of its chunk instead of ending
    /// the game, and joins it once a later update makes it valid.
    #[test]
    fn invalid_bricks_are_left_out_of_their_chunk() {
        let mut state = ChunkedWorld::default();
        let mut bad = brick([2.0, 1.0, 1.0]);
        bad.events = vec![crate::world_scene::tests::set_color(2)];
        let base = world([(1, brick([1.0; 3])), (2, bad.clone())]);
        update(&mut state, &base, None);
        let key = chunk_key([1.0; 3]);
        assert_eq!((state.chunk_count(), state.chunk_bricks(key)), (1, 1));
        bad.events = vec![crate::world_scene::tests::set_color(1)];
        let fixed = world([(1, brick([1.0; 3])), (2, bad)]);
        update(&mut state, &fixed, Some(&[2]));
        assert_eq!(state.chunk_bricks(key), 2);
    }

    /// Reported load failure ("Unresolved native print NOPRINT"): a save
    /// whose bricks name a print this client does not have. Every such
    /// brick draws with the blank print surface, exactly like a print-less
    /// brick, and the rest of the world loads.
    #[test]
    fn unknown_prints_draw_blank_instead_of_failing_the_world() {
        let mut meshes = meshes();
        let mesh = meshes.get_mut("definition/a").unwrap();
        mesh.quads[0].surface = Surface::Print;
        let materials = BrickMaterials::in_memory();
        let palette = BrickPalette::new(&materials).unwrap();
        let printed = |print: Option<ContentRef>| {
            let mut b = brick([1.0; 3]);
            b.print = print;
            b
        };
        let unresolved = |namespace: &str, name: &str| {
            Some(ContentRef::Unresolved {
                namespace: namespace.into(),
                name: name.into(),
            })
        };
        let drawn = |brick: &Brick| {
            let scene =
                build_brick(brick, &[[1.0; 4]], &meshes, &palette, Some(&materials)).unwrap();
            let batch = &scene.batches[0];
            format!("{:?}", (&scene.vertices, &scene.materials[batch.material]))
        };
        let blank = drawn(&printed(None));
        let letter = drawn(&printed(unresolved("print", "Letters/A")));
        assert_ne!(blank, letter);
        let unknown = [
            unresolved("print", "NOPRINT"),
            unresolved("print", "Letters/NoSuchLetter"),
            unresolved("print", "Community_Prints/Violin"),
            unresolved("light_ui", "Letters/A"),
            Some(ContentRef::Resolved("print/print_not_installed/a".into())),
        ];
        for print in &unknown {
            assert_eq!(drawn(&printed(print.clone())), blank, "{print:?}");
        }
        // The whole world, as a load delivers it: every brick is placed.
        let loaded = world(
            unknown
                .into_iter()
                .chain([unresolved("print", "letters/a")])
                .enumerate()
                .map(|(i, print)| {
                    let mut b = printed(print);
                    b.position = [1.0 + i as f32, 1.0, 1.0];
                    (i as u64 + 1, b)
                }),
        );
        let mut state = ChunkedWorld::default();
        state
            .update(loaded, None, &meshes, &palette, Some(&materials), 100)
            .unwrap();
        assert_eq!(state.chunk_bricks(chunk_key([1.0; 3])), 6);
    }

    #[test]
    fn budget_and_invalid_bricks_reject_without_changing_state() {
        let mut state = ChunkedWorld::default();
        let base = world([(1, brick([1.0; 3]))]);
        update(&mut state, &base, None);
        let crowded = world((1..=60).map(|id| (id, brick([id as f32, 1.0, 1.0]))));
        let error = try_update(&mut state, &crowded, None).unwrap_err();
        assert!(error.to_string().contains("budget"));
        let mut unknown = (*base).clone();
        unknown.bricks.insert(2, brick([1.0; 3]));
        unknown.bricks.get_mut(&2).unwrap().definition = ContentRef::Resolved("missing".into());
        assert!(try_update(&mut state, &Arc::new(unknown), Some(&[2])).is_err());
        assert!(Arc::ptr_eq(state.source().unwrap(), &base));
        assert_eq!((state.chunk_count(), state.triangles()), (1, 2));
    }
}
