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
        let mut change = |id: u64, old: Option<&Brick>, new: Option<&Brick>| {
            let (out_before, out_now) = (was_out.contains(&id), left_out.contains(&id));
            if let (Some(old), Some(new)) = (old, new)
                && same_appearance(old, new)
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
            .filter(|previous| next.palette.starts_with(&previous.palette));
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
        // Validate and count every brick being rebuilt before allocating;
        // each distinct mesh is validated once, not once per placement.
        let mut counts = BTreeMap::new();
        let mut validated = std::collections::HashSet::new();
        for (key, ids) in &staged {
            let mut count = 0usize;
            for id in ids {
                let brick = &next.bricks[id];
                brick
                    .validate(next.palette.len())
                    .with_context(|| format!("Invalid replicated brick {id}"))?;
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
                count += mesh.quads.len() * 2;
            }
            counts.insert(*key, count);
        }
        let total = self.total_triangles
            - dirty
                .iter()
                .filter_map(|key| self.triangles.get(key))
                .sum::<usize>()
            + counts.values().sum::<usize>();
        ensure!(
            total <= max_triangles,
            "World requires {total} or more brick triangles, exceeding configured render budget {max_triangles}; no bricks were omitted"
        );
        let jobs: Vec<_> = staged.iter().filter(|(_, ids)| !ids.is_empty()).collect();
        let built = build_chunks(&jobs, &next, meshes, palette, materials)?;
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
        Ok(changes)
    }
}

/// Build chunks in parallel when a load or repaint touches many of them.
fn build_chunks(
    jobs: &[(&ChunkKey, &BTreeSet<u64>)],
    world: &PublicWorld,
    meshes: &BTreeMap<String, BrickMesh>,
    palette: &BrickPalette,
    materials: Option<&BrickMaterials>,
) -> Result<Vec<SceneData>> {
    let build = |(key, ids): &(&ChunkKey, &BTreeSet<u64>)| {
        build_chunk(**key, ids, world, meshes, palette, materials)
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
    world: &PublicWorld,
    meshes: &BTreeMap<String, BrickMesh>,
    palette: &BrickPalette,
    materials: Option<&BrickMaterials>,
) -> Result<SceneData> {
    let mut scene = SceneData {
        id: format!("{}/replicated-bricks/{key:?}", world.map_id),
        name: format!("{} bricks {key:?}", world.name),
        images: vec![],
        materials: palette.scene.materials.clone(),
        ..Default::default()
    };
    for id in ids {
        crate::world_scene::append_world_brick(
            &mut scene,
            *id,
            &world.bricks[id],
            &world.palette,
            meshes,
            palette.surfaces,
            materials,
            true,
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
mod tests {
    use super::*;
    use crate::network::WorldChanges;
    use bri_content::brick::{Face, Quad, Surface, Vertex};

    fn meshes() -> BTreeMap<String, BrickMesh> {
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
