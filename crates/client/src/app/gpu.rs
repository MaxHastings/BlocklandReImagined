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
    pub(super) region_lines: Option<bri_render::lines::LineRenderer>,
    pub(super) region_outlines: crate::rule_regions::Outlines,
    /// The Environment window's vignette over the world.
    pub(super) vignette: Option<bri_render::vignette::VignetteRenderer>,
    /// An Add-On's selection box (`Notice::SelectionBox`), and the box it
    /// last uploaded.
    pub(super) selection_lines: Option<bri_render::lines::LineRenderer>,
    pub(super) selection_uploaded: Option<Option<([f32; 3], [f32; 3])>>,
    /// None: rebuild the outlines whatever `hidden_outlines` finds.
    pub(super) hidden_uploaded: Option<bool>,
    /// The hidden bricks the outlines draw, kept from world changes.
    pub(super) hidden_outlines: crate::hidden_outlines::HiddenOutlines,
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
    pub(super) gpu_chunk_bricks:
        HashMap<crate::world_chunks::ChunkKey, Arc<crate::world_chunks::ChunkBricks>>,
    pub(super) chunk_uploads: BTreeSet<crate::world_chunks::ChunkKey>,
    /// Rebuild GPU renderers before the next frame (the map changed while
    /// no device was open).
    pub(super) gpu_restart: bool,
    /// The device the renderers were built on (`gpu_ready`), until it
    /// stops. A map change rebuilds them on it at once rather than on the
    /// next frame: a window that draws no frames (minimized) still
    /// finishes the change, and the compile starts as early as it can.
    pub(super) device: Option<(wgpu::Device, wgpu::Queue, wgpu::TextureFormat)>,
    /// The ghost built at the origin and the one transform that places it.
    pub(super) ghost_gpu: Option<(GpuScene, bri_render::scene::GpuInstances)>,
    /// What `ghost_gpu` was built from: moving the ghost only moves it.
    pub(super) ghost_look: Option<GhostLook>,
    pub(super) ghost_uploaded: u64,
    pub(super) gpu_name: String,
    /// A device has been given (`gpu_ready`): the world waits for its
    /// pipelines. Headless Apps never draw and never wait.
    pub(super) opened: bool,
    /// GPU time per world pass, in ms, from the latest timed frame: while
    /// the expanded performance overlay shows, or always once
    /// `time_gpu_passes` asks.
    pub(super) gpu_passes: Vec<(&'static str, f32)>,
    pub(super) time_passes: bool,
    /// Render Scale below 100%: the smaller texture the world draws into
    /// and the pass that stretches it over the window.
    pub(super) upscale: Option<bri_render::upscale::Upscale>,
    /// The window's size this frame, which the interface (name tags) is
    /// laid out in, even while the world draws smaller.
    pub(super) display_size: (u32, u32),
}
