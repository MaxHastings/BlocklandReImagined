//! The world as the CPU sees it: map scene, brick chunks, the world log and the query mirror.
use super::*;

/// The world as the CPU sees it: map scene, brick chunks, the world log and the query mirror.
pub(super) struct SceneState {
    pub(super) cpu_scene: Option<SceneData>,
    /// The map's backdrop: drawn before the map from its own eye (`bri_content::backdrop`).
    pub(super) backdrop: Option<bri_render::scene_loader::BackdropScene>,
    /// The map's regions with their own fog, and the viewer's fog across them.
    pub(super) regions: Option<bri_content::regions::RegionMap>,
    /// How the map's sounds carry, when its regions say ([`Self::set_regions`]).
    pub(super) sound_carry: Option<crate::sound_carry::SoundCarryModel>,
    /// The map's decals and the triangles they draw on.
    pub(super) decals: Option<bri_render::decals::LoadedDecals>,
    pub(super) receiver_index: Option<Arc<bri_render::decals::ReceiverIndex>>,
    /// Decals a game attaches while it runs (pictures and edits), applied before the next draw.
    pub(crate) decal_feed: bri_render::decals::DecalFeed,
    pub(super) fog_blend: bri_content::regions::FogBlend,
    /// Game time of the last fog step.
    pub(super) fog_time: f64,
    pub(super) cpu_terrain: Vec<Arc<bri_render::terrain_scene::TerrainScene>>,
    pub(super) cpu_water: Vec<Arc<bri_render::water_scene::WaterScene>>,
    /// The sky drawn over the map's own (the Environment window).
    pub(super) scenery: super::scenery::Scenery,
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
    pub(super) cpu_chunk_bricks:
        HashMap<crate::world_chunks::ChunkKey, Arc<crate::world_chunks::ChunkBricks>>,
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
    /// Chunk rebuild jobs started and chunks they rebuilt, since load.
    pub(super) chunk_jobs: u64,
    pub(super) chunks_rebuilt: u64,
}

impl SceneState {
    /// The map's regions, with the way its sounds carry worked out from them.
    pub(super) fn set_regions(&mut self, regions: Option<bri_content::regions::RegionMap>) {
        self.sound_carry = regions
            .as_ref()
            .and_then(crate::sound_carry::SoundCarryModel::new);
        self.regions = regions;
    }
}
