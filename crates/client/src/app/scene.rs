//! The world as the CPU sees it: map scene, brick chunks, the world log and the query mirror.
use super::*;

/// The world as the CPU sees it: map scene, brick chunks, the world log and the query mirror.
pub(super) struct SceneState {
    pub(super) cpu_scene: Option<SceneData>,
    pub(super) cpu_terrain: Vec<Arc<bri_render::terrain_scene::TerrainScene>>,
    /// The easing bricks the applied chunks leave out.
    pub(super) chunks_left_out: BTreeSet<u64>,
    /// Map static shapes' index ranges, and the smashed ones `gpu_scene`
    /// no longer draws.
    pub(super) shape_indices: BTreeMap<u32, std::ops::Range<u32>>,
    pub(super) meshes: Option<Arc<Meshes>>,
    /// Mirror bricks' definitions, and where the world's mirrors are.
    pub(super) mirror_shapes: Arc<crate::mirrors::MirrorShapes>,
    pub(super) mirror_index: crate::mirrors::MirrorIndex,
    /// Replicated bricks as independently rebuilt chunks sharing one
    /// uploaded material palette. A running job owns `chunked`.
    pub(super) palette: Option<Arc<crate::world_chunks::BrickPalette>>,
    pub(super) chunked: crate::world_chunks::ChunkedWorld,
    pub(super) cpu_chunks: HashMap<crate::world_chunks::ChunkKey, SceneData>,
    /// Where each brick of a CPU chunk is in its vertices.
    pub(super) cpu_chunk_bricks: HashMap<crate::world_chunks::ChunkKey, Arc<crate::world_chunks::ChunkBricks>>,
    /// Dead bricks (thrown as debris or falling) the drawn chunks may still
    /// hold, with their chunk and whether its upload hides them yet. The
    /// rebuilt chunk without them lands later (100-200 ms on a big build);
    /// until then they are hidden inside the drawn chunk the frame they die.
    pub(super) chunk_hides: BTreeMap<bri_world::BrickId, (crate::world_chunks::ChunkKey, bool)>,
    /// This frame's liquids, rebuilt only when they or the paint change.
    pub(super) liquid_cache: Option<LiquidCache>,
    pub(super) world_source: Option<Arc<bri_net::protocol::PublicWorld>>,
    pub(super) world_revision: u64,
    pub(super) world_log: Option<Arc<network::WorldLog>>,
    pub(super) world_job: Option<WorldJob>,
    /// Map of the installed scene.
    pub(super) scene_map: Option<String>,
    pub(super) materials: Option<Arc<crate::materials::BrickMaterials>>,
    pub(super) query_source: Option<Arc<bri_net::protocol::PublicWorld>>,
    /// The replica log and revision `query_source` came from.
    pub(super) query_log: Option<(Arc<network::WorldLog>, u64)>,
}
