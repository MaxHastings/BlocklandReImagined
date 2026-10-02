//! Everything uploaded to the graphics card, and the renderers that draw it.
use super::*;

/// Everything uploaded to the graphics card, and the renderers that draw it.
pub(super) struct GpuState {
    pub(super) weather_renderer: Option<bri_weather::gpu::WeatherRenderer>,
    pub(super) renderer: Option<crate::gpu_build::Building<SceneRenderer>>,
    pub(super) shell_gpu: Option<(GpuScene, bri_render::scene::GpuInstances)>,
    /// Outlines of non-rendering bricks, drawn only while a building tool is
    /// out, and whether the uploaded lines are the shown ones (None: stale).
    pub(super) hidden_lines: Option<bri_render::lines::LineRenderer>,
    /// The Environment window's vignette over the world.
    pub(super) vignette: Option<bri_render::vignette::VignetteRenderer>,
    /// An Add-On's selection box (`Notice::SelectionBox`), and the box it
    /// last uploaded.
    pub(super) selection_lines: Option<bri_render::lines::LineRenderer>,
    pub(super) selection_uploaded: Option<Option<([f32; 3], [f32; 3])>>,
    pub(super) hidden_uploaded: Option<bool>,
    /// `BrickFades::outlined` when the outlines were built: bricks fading
    /// in or out gain or lose theirs as they pass v20's alpha 0.1.
    pub(super) hidden_fading: Vec<(u64, bool)>,
    pub(super) effects_renderer: Option<bri_fx_runtime::gpu::EffectsRenderer>,
    pub(super) gpu_scene: Option<GpuScene>,
    pub(super) gpu_broken: BTreeSet<u32>,
    pub(super) gpu_terrain: Vec<bri_render::terrain_scene::GpuTerrain>,
    /// World-pass depth and, with MSAA, the multisampled color attachment
    /// that the last world pass resolves into the frame target.
    pub(super) depth: Option<(wgpu::Texture, Option<wgpu::Texture>, (u32, u32))>,
    pub(super) gpu_palette: Option<GpuScene>,
    pub(super) gpu_chunks: HashMap<crate::world_chunks::ChunkKey, GpuScene>,
    /// The same for each chunk as uploaded, which may be older.
    pub(super) gpu_chunk_bricks: HashMap<crate::world_chunks::ChunkKey, Arc<crate::world_chunks::ChunkBricks>>,
    pub(super) chunk_uploads: BTreeSet<crate::world_chunks::ChunkKey>,
    /// Rebuild GPU renderers before the next frame (the map changed).
    pub(super) gpu_restart: bool,
    /// The ghost built at the origin and the one transform that places it.
    pub(super) ghost_gpu: Option<(GpuScene, bri_render::scene::GpuInstances)>,
    /// What `ghost_gpu` was built from: moving the ghost only moves it.
    pub(super) ghost_look: Option<GhostLook>,
    pub(super) ghost_uploaded: u64,
    pub(super) gpu_name: String,
    /// GPU time per world pass, in ms, from the latest timed frame: while
    /// the expanded performance overlay shows, or always once
    /// `time_gpu_passes` asks.
    pub(super) gpu_passes: Vec<(&'static str, f32)>,
    pub(super) time_passes: bool,
}
