//! Draws an Add-On's render layer with wgpu. The engine owns the device,
//! the pass and the camera; the Add-On only fills its layer. Pipelines are
//! built from shaders [`crate::shader`] already checked and rewrote, and
//! wgpu adds its own runtime checks (bounds checks and loop bounding) on
//! top. A GPU error in an Add-On's pipeline stops that Add-On, not the game.
//!
//! GPU time is bounded three ways. [`calibrate`] measures how much shader
//! work the GPU does per millisecond; every frame the renderer sets each
//! Add-On's loop cap so its shaders fit the frame budget at the current
//! screen size ([`loop_limit`]), starting at
//! [`crate::shader::DEFAULT_LOOP_LIMIT`] until the speed is known. Where the
//! GPU has timestamps, each layer is timed: a frame over budget halves the
//! cap, and one frame far over it stops the Add-On
//! ([`AddOn::report_gpu_time`]).
use crate::host::{AddOn, Blend, Budgets, Frame, Layer, Space, Stopped, VERTEX_BYTES, Vertex};
use crate::shader::{DEFAULT_LOOP_LIMIT, MAX_LOOP_LIMIT, Shader};
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use glam::{Mat4, Vec3};
use std::borrow::Cow;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

/// `create_buffer_init` that survives a lost device: wgpu then hands back
/// an invalid buffer, which `create_buffer_init` panics mapping. Left
/// unfilled, it is replaced when the renderer restarts after the loss.
trait BufferInit {
    fn buffer_init(&self, descriptor: &wgpu::util::BufferInitDescriptor<'_>) -> wgpu::Buffer;
}
impl BufferInit for wgpu::Device {
    fn buffer_init(&self, descriptor: &wgpu::util::BufferInitDescriptor<'_>) -> wgpu::Buffer {
        let unpadded = descriptor.contents.len() as wgpu::BufferAddress;
        let align = wgpu::COPY_BUFFER_ALIGNMENT - 1;
        let size = ((unpadded + align) & !align).max(wgpu::COPY_BUFFER_ALIGNMENT);
        let buffer = self.create_buffer(&wgpu::BufferDescriptor {
            label: descriptor.label,
            size,
            usage: descriptor.usage,
            mapped_at_creation: true,
        });
        if let Ok(mut range) = buffer.get_mapped_range_mut(..) {
            range
                .slice(..unpadded as usize)
                .copy_from_slice(descriptor.contents);
            drop(range);
            buffer.unmap();
        }
        buffer
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    view_proj: [f32; 16],
    camera: [f32; 4],
    time: [f32; 4],
    limits: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawUniform {
    model: [f32; 16],
    params: [[f32; 4]; 4],
}
/// Dynamic uniform offsets must be 256-byte aligned.
const DRAW_STRIDE: u64 = 256;

/// Where the engine's camera is this frame, and the size of the target it
/// draws into.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub view_proj: Mat4,
    pub position: Vec3,
    /// Target width and height in pixels.
    pub size: [u32; 2],
    /// The player's normal horizontal field of view, degrees: what
    /// [`Space::View`] draws at, so a held gun keeps its size while the
    /// world zooms.
    pub normal_fov: f32,
}
impl Camera {
    pub fn pixels(&self) -> u64 {
        u64::from(self.size[0]) * u64::from(self.size[1])
    }
    fn aspect(&self) -> f32 {
        self.size[0].max(1) as f32 / self.size[1].max(1) as f32
    }
    /// Each space's view-projection, in [`Space::ALL`] order.
    fn projections(&self) -> [Mat4; 3] {
        let aspect = self.aspect();
        let fov = self.normal_fov.clamped(5.0, 140.0).to_radians();
        let fov_y = 2.0 * ((fov / 2.0).tan() / aspect).atan();
        [
            self.view_proj,
            reversed_perspective(fov_y, aspect, VIEW_NEAR, VIEW_FAR),
            glam::camera::rh::proj::directx::orthographic(-aspect, aspect, -1.0, 1.0, -1.0, 1.0),
        ]
    }
}
/// A perspective projection in the engine's depth, which runs reversed
/// (1 at the near plane, 0 at the far one: `bri_render::scene::perspective`).
/// World space draws into that depth with the engine's own camera.
pub fn reversed_perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    glam::camera::rh::proj::directx::perspective(fov_y, aspect, far, near)
}
/// What the engine clears its depth to: the far plane.
pub const DEPTH_CLEAR: f32 = 0.0;
/// View space's near and far planes, in world units from the eye.
const VIEW_NEAR: f32 = 0.01;
const VIEW_FAR: f32 = 100.0;
/// The slice of the depth range view space draws into: in front of all the
/// world, so walls never cut into a held gun, while its own parts still
/// hide each other. Screen space writes depth 1 (in front of both).
const VIEW_DEPTH: f32 = 0.001;

/// How much Add-On shader work this GPU does: expressions (as
/// [`crate::shader`] counts them) per millisecond, over all its cores.
/// Measured with transcendental-heavy work, so ordinary shaders run faster
/// than it predicts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuSpeed {
    pub work_per_ms: f64,
}

/// A timed layer is refused only when its shaders, with no loop running,
/// could take this long over the whole screen: measurement catches the
/// rest after one frame, and this keeps that frame far from the two-second
/// driver reset.
const WORST_FIRST_FRAME_MS: f64 = 500.0;

/// How many times [`too_heavy`] measures the GPU again before it believes a
/// refusal.
const REMEASURES: u32 = 2;

/// How long shaders doing `work` a frame (as [`base_work`] counts it) take
/// at `speed`, in ms, when that is over `refuse`; `None` when they fit.
/// Load from anything else on the machine only ever makes the GPU seem
/// slower, so one measurement taken while it was busy (shaders compiling at
/// startup, another program, other tests) could stop an Add-On that fits
/// easily. Before refusing, the GPU is measured again with `measure` and
/// the fastest speed seen is kept in `speed`; only shaders still too heavy
/// at that speed are refused.
fn too_heavy(
    work: f64,
    refuse: f64,
    speed: &mut GpuSpeed,
    mut measure: impl FnMut() -> Option<GpuSpeed>,
) -> Option<f64> {
    for _ in 0..REMEASURES {
        if work / speed.work_per_ms <= refuse {
            return None;
        }
        if let Some(fresh) = measure()
            && fresh.work_per_ms > speed.work_per_ms
        {
            *speed = fresh;
        }
    }
    let ms = work / speed.work_per_ms;
    (ms > refuse).then_some(ms)
}

/// Fragments per screen pixel the loop cap assumes an Add-On draws; more
/// overdraw than this is caught by timing.
pub const OVERDRAW: f64 = 2.0;

/// Work an Add-On's shaders do per frame at loop cap 0 (no loop runs).
fn base_work(cost: [u32; 2], pixels: u64, vertices: u64) -> f64 {
    let [vertex, fragment] = cost.map(f64::from);
    pixels as f64 * OVERDRAW * fragment + vertices as f64 * vertex
}

/// The loop cap that fits shaders of `cost` ([vertex, fragment]
/// expressions per iteration) into `target_ms` at this screen size, scaled
/// down by `scale` after slow frames. Before the speed is measured it is
/// [`DEFAULT_LOOP_LIMIT`].
pub fn loop_limit(
    speed: Option<GpuSpeed>,
    cost: [u32; 2],
    pixels: u64,
    vertices: u64,
    target_ms: f32,
    scale: f64,
) -> u32 {
    let Some(speed) = speed else {
        return DEFAULT_LOOP_LIMIT;
    };
    let work = base_work(cost, pixels, vertices).max(1.0);
    let iterations = speed.work_per_ms * f64::from(target_ms) / work * scale - 1.0;
    iterations.clamped(0.0, f64::from(MAX_LOOP_LIMIT)) as u32
}

/// Timestamps around an Add-On's draws, read back a few frames later.
struct GpuTimer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<(wgpu::Buffer, Arc<AtomicU8>)>,
    period_ns: f32,
    /// The slot this frame's timestamps go to, once written.
    writing: Cell<Option<usize>>,
}
const FREE: u8 = 0;
const COPIED: u8 = 1;
const MAPPING: u8 = 2;
const READY: u8 = 3;

impl GpuTimer {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        let needed =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
        if !device.features().contains(needed) {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("addon timer"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("addon timer resolve"),
            size: 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = (0..3)
            .map(|_| {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("addon timer readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                (buffer, Arc::new(AtomicU8::new(FREE)))
            })
            .collect();
        Some(Self {
            queries,
            resolve,
            slots,
            period_ns: queue.get_timestamp_period(),
            writing: Cell::new(None),
        })
    }

    /// Start reading slots whose frame has been submitted since, and
    /// return the times of those that finished.
    fn collect(&mut self, device: &wgpu::Device) -> Vec<f32> {
        for (buffer, state) in &self.slots {
            if state.load(Ordering::Acquire) == COPIED {
                state.store(MAPPING, Ordering::Release);
                let state = state.clone();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        state.store(if result.is_ok() { READY } else { FREE }, Ordering::Release);
                    });
            }
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let mut times = Vec::new();
        for (buffer, state) in &self.slots {
            if state.load(Ordering::Acquire) != READY {
                continue;
            }
            if let Ok(view) = buffer.slice(..).get_mapped_range() {
                let ticks: [u64; 2] = bytemuck::pod_read_unaligned(&view[..16]);
                drop(view);
                let ns = ticks[1].saturating_sub(ticks[0]) as f64 * f64::from(self.period_ns);
                times.push((ns / 1e6) as f32);
            }
            buffer.unmap();
            state.store(FREE, Ordering::Release);
        }
        times
    }

    fn begin(&self, pass: &mut wgpu::RenderPass<'_>) {
        let free = self
            .slots
            .iter()
            .position(|(_, s)| s.load(Ordering::Acquire) == FREE);
        self.writing.set(free);
        if free.is_some() {
            pass.write_timestamp(&self.queries, 0);
        }
    }

    fn end(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.writing.get().is_some() {
            pass.write_timestamp(&self.queries, 1);
        }
    }

    fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(slot) = self.writing.take() else {
            return;
        };
        let (buffer, state) = &self.slots[slot];
        encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, buffer, 0, 16);
        state.store(COPIED, Ordering::Release);
    }
}

/// What a [`LayerRenderer`] draws: an Add-On's own layer, or one the game
/// builds from Add-On shaders (the skins an Add-On gives its items, drawn
/// on every copy of them). Either is held to the same budgets and stopped
/// the same way.
pub trait LayerSource {
    fn shaders(&self) -> &[Shader];
    fn layer(&self) -> &Layer;
    fn budgets(&self) -> &Budgets;
    /// Stop drawing it, for `reason`, which is returned.
    fn stop(&mut self, reason: Stopped) -> Stopped;
    /// One frame's measured GPU time: an error once it is far over budget.
    fn report_gpu_time(&mut self, ms: f32) -> Result<(), Stopped>;
}

impl LayerSource for AddOn {
    fn shaders(&self) -> &[Shader] {
        AddOn::shaders(self)
    }
    fn layer(&self) -> &Layer {
        AddOn::layer(self)
    }
    fn budgets(&self) -> &Budgets {
        AddOn::budgets(self)
    }
    fn stop(&mut self, reason: Stopped) -> Stopped {
        AddOn::stop(self, reason)
    }
    fn report_gpu_time(&mut self, ms: f32) -> Result<(), Stopped> {
        AddOn::report_gpu_time(self, ms)
    }
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

pub struct LayerRenderer {
    color: wgpu::TextureFormat,
    depth: Option<wgpu::TextureFormat>,
    samples: u32,
    pipeline_layout: wgpu::PipelineLayout,
    /// One frame uniform per [`Space`], in [`Space::ALL`] order.
    frame_buffers: [wgpu::Buffer; 3],
    frame_groups: [wgpu::BindGroup; 3],
    frame_layout: wgpu::BindGroupLayout,
    /// World space seen from other views (mirrors), view 1 first.
    view_frames: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    /// The time the last frame was prepared for.
    time: [f32; 2],
    draw_buffer: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    draw_capacity: u64,
    /// Per shader, one pipeline per [`Space`] and [`Blend`] mode.
    pipelines: Vec<[[wgpu::RenderPipeline; 3]; 3]>,
    /// The target size the last frame was prepared for, for view space's
    /// viewport.
    size: [u32; 2],
    /// This frame's draws in drawing order: world, then view, then screen,
    /// each in the Add-On's order.
    order: Vec<usize>,
    meshes: Vec<GpuMesh>,
    speed: Option<GpuSpeed>,
    /// Lowered after frames over budget, raised back slowly after fast ones.
    scale: f64,
    limit: u32,
    timer: Option<GpuTimer>,
}

fn uniform_layout(device: &wgpu::Device, label: &str, dynamic: bool) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

impl LayerRenderer {
    /// `draws` is the most draws a frame may hold (the Add-On's budget);
    /// `samples` is the pass's multisample count. `speed` comes from
    /// [`calibrate`]. The layer is timed when the device has timestamp
    /// queries inside passes.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        speed: Option<GpuSpeed>,
        color: wgpu::TextureFormat,
        depth: Option<wgpu::TextureFormat>,
        samples: u32,
        draws: usize,
    ) -> Self {
        let frame_layout = uniform_layout(device, "addon frame", false);
        let draw_layout = uniform_layout(device, "addon draw", true);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("addon layer"),
            bind_group_layouts: &[Some(&frame_layout), Some(&draw_layout)],
            immediate_size: 0,
        });
        let frame_buffers = Space::ALL.map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("addon frame"),
                size: std::mem::size_of::<FrameUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let draw_capacity = draws.max(1) as u64;
        let draw_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("addon draws"),
            size: draw_capacity * DRAW_STRIDE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_groups = [0, 1, 2].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("addon frame"),
                layout: &frame_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame_buffers[i].as_entire_binding(),
                }],
            })
        });
        let draw_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("addon draw"),
            layout: &draw_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &draw_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<DrawUniform>() as u64),
                }),
            }],
        });
        Self {
            color,
            depth,
            samples,
            pipeline_layout,
            frame_buffers,
            frame_groups,
            frame_layout,
            view_frames: Vec::new(),
            time: [0.0; 2],
            draw_buffer,
            draw_group,
            draw_capacity,
            pipelines: Vec::new(),
            size: [1, 1],
            order: Vec::new(),
            meshes: Vec::new(),
            speed,
            scale: 1.0,
            limit: DEFAULT_LOOP_LIMIT,
            timer: GpuTimer::new(device, queue),
        }
    }

    /// The loop cap this frame's shaders run with.
    pub fn loop_limit(&self) -> u32 {
        self.limit
    }

    /// Whether this renderer times its layer on the GPU.
    pub fn timed(&self) -> bool {
        self.timer.is_some()
    }

    /// Take the layer's GPU times that have come back since the last call;
    /// returns the latest.
    pub fn read_times(
        &mut self,
        device: &wgpu::Device,
        addon: &mut dyn LayerSource,
    ) -> Result<Option<f32>, Stopped> {
        let times = match &mut self.timer {
            Some(timer) => timer.collect(device),
            None => return Ok(None),
        };
        for &ms in &times {
            self.measured(addon, ms)?;
        }
        Ok(times.last().copied())
    }

    /// Take a measured GPU time for the layer: stop the Add-On when it is
    /// far over budget, otherwise adjust the loop cap.
    pub fn measured(&mut self, addon: &mut dyn LayerSource, ms: f32) -> Result<(), Stopped> {
        addon.report_gpu_time(ms)?;
        let target = addon.budgets().gpu_ms_per_frame;
        if ms > target {
            self.scale = (self.scale * 0.5).max(1.0 / 1024.0);
        } else if ms < target * 0.5 {
            self.scale = (self.scale * 1.1).min(1.0);
        }
        Ok(())
    }

    /// Build what is new in the Add-On's layer and upload this frame's
    /// uniforms. A GPU error stops the Add-On.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        addon: &mut dyn LayerSource,
        frame: &Frame,
        camera: Camera,
        time: [f32; 2],
    ) -> Result<(), Stopped> {
        self.read_times(device, addon)?;
        if self.pipelines.len() < addon.shaders().len() {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            for shader in &addon.shaders()[self.pipelines.len()..] {
                let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&shader.name),
                    source: wgpu::ShaderSource::Naga(Cow::Owned(shader.module.clone())),
                });
                self.pipelines
                    .push(self.pipelines_for(device, &module, &shader.name));
            }
            if let Some(error) = pollster::block_on(scope.pop()) {
                return Err(addon.stop(Stopped::Gpu(format!("a shader was refused: {error}"))));
            }
        }
        for mesh in &addon.layer().meshes[self.meshes.len()..] {
            self.meshes.push(GpuMesh {
                vertices: device.buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("addon mesh"),
                    contents: bytemuck::cast_slice(&mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                indices: device.buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("addon mesh indices"),
                    contents: bytemuck::cast_slice(&mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                count: mesh.indices.len() as u32,
            });
        }
        let cost = addon.shaders().iter().fold([0, 0], |[v, f], s| {
            [v.max(s.vertex_cost), f.max(s.fragment_cost)]
        });
        let vertices = frame.triangles.saturating_mul(3);
        let budgets = addon.budgets();
        // Timed layers are corrected by measurement, so the cap can aim
        // high; untimed ones must fit the frame budget up front.
        let (target, refuse) = if self.timer.is_some() {
            (budgets.gpu_stop_ms / 4.0, WORST_FIRST_FRAME_MS)
        } else {
            (budgets.gpu_ms_per_frame, f64::from(budgets.gpu_stop_ms))
        };
        if let Some(speed) = &mut self.speed {
            let work = base_work(cost, camera.pixels(), vertices);
            if let Some(ms) = too_heavy(work, refuse, speed, || calibrate(device, queue)) {
                return Err(addon.stop(Stopped::Gpu(format!(
                    "its shaders would take about {ms:.0} ms a frame on this graphics card at this screen size"
                ))));
            }
        }
        self.limit = loop_limit(
            self.speed,
            cost,
            camera.pixels(),
            vertices,
            target,
            self.scale,
        );
        // View and screen space see from the origin: their shaders light
        // and fade from where the eye is in their own space.
        for (i, view_proj) in camera.projections().into_iter().enumerate() {
            let position = if i == 0 { camera.position } else { Vec3::ZERO };
            let uniform = FrameUniform {
                view_proj: view_proj.to_cols_array(),
                camera: position.extend(1.0).to_array(),
                time: [time[0], time[1], 0.0, 0.0],
                limits: [self.limit, 0, 0, 0],
            };
            queue.write_buffer(&self.frame_buffers[i], 0, bytemuck::bytes_of(&uniform));
        }
        self.size = camera.size.map(|v| v.max(1));
        self.time = time;
        if frame.draws.len() as u64 > self.draw_capacity {
            return Err(addon.stop(Stopped::Budget("more draws than the renderer holds".into())));
        }
        let mut bytes = vec![0u8; frame.draws.len() * DRAW_STRIDE as usize];
        let layer = addon.layer();
        self.order = (0..frame.draws.len()).collect();
        self.order
            .sort_by_key(|&i| layer.materials[frame.draws[i].material].space);
        for (i, draw) in frame.draws.iter().enumerate() {
            let uniform = DrawUniform {
                model: draw.model,
                params: draw.params.unwrap_or(layer.materials[draw.material].params),
            };
            let at = i * DRAW_STRIDE as usize;
            bytes[at..at + std::mem::size_of::<DrawUniform>()]
                .copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.draw_buffer, 0, &bytes);
        }
        Ok(())
    }

    /// A shader's pipelines, one per space and blend mode, in
    /// [`Space::ALL`] and [`Blend::ALL`] order.
    fn pipelines_for(
        &self,
        device: &wgpu::Device,
        module: &wgpu::ShaderModule,
        name: &str,
    ) -> [[wgpu::RenderPipeline; 3]; 3] {
        Space::ALL
            .map(|space| Blend::ALL.map(|blend| self.pipeline(device, module, name, space, blend)))
    }

    fn pipeline(
        &self,
        device: &wgpu::Device,
        module: &wgpu::ShaderModule,
        name: &str,
        space: Space,
        blend: Blend,
    ) -> wgpu::RenderPipeline {
        let opaque = blend == Blend::Opaque;
        // Screen space draws over everything already drawn; its viewport
        // puts it at depth 1 (the nearest, reversed), so what the world
        // draws later stays behind its solid parts.
        let compare = match space {
            Space::World | Space::View => wgpu::CompareFunction::Greater,
            Space::Screen => wgpu::CompareFunction::Always,
        };
        let colour = match blend {
            Blend::Opaque | Blend::Translucent => wgpu::BlendState::ALPHA_BLENDING,
            Blend::Additive => wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            },
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(name),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_BYTES as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: opaque.then_some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: self.depth.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(opaque),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: self.samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.color,
                    blend: Some(colour),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    /// Record the frame's draws into the engine's pass.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &Frame, layer: &Layer) {
        if frame.draws.is_empty() {
            return;
        }
        if let Some(timer) = &self.timer {
            timer.begin(pass);
        }
        let [w, h] = self.size.map(|v| v as f32);
        let mut current = None;
        for &i in &self.order {
            let Some(draw) = frame.draws.get(i) else {
                continue;
            };
            let material = &layer.materials[draw.material];
            let mesh = &self.meshes[draw.mesh];
            let blend = Blend::ALL
                .iter()
                .position(|b| *b == material.blend)
                .unwrap_or(0);
            if current != Some(material.space) {
                current = Some(material.space);
                let space = material.space as usize;
                pass.set_bind_group(0, &self.frame_groups[space], &[]);
                match material.space {
                    Space::World => {}
                    Space::View => pass.set_viewport(0.0, 0.0, w, h, 1.0 - VIEW_DEPTH, 1.0),
                    Space::Screen => pass.set_viewport(0.0, 0.0, w, h, 1.0, 1.0),
                }
            }
            pass.set_pipeline(&self.pipelines[material.shader][material.space as usize][blend]);
            pass.set_bind_group(1, &self.draw_group, &[(i as u64 * DRAW_STRIDE) as u32]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, 0..1);
        }
        // The engine's later draws keep the whole depth range.
        if current.is_some_and(|s| s != Space::World) {
            pass.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
        }
        if let Some(timer) = &self.timer {
            timer.end(pass);
        }
    }

    /// The layer's world space from another view of the same frame (a
    /// mirror's): view 1 and up, after [`Self::prepare`], drawn by
    /// [`Self::draw_view`]. View and screen space belong to the player's
    /// screen and are not drawn there.
    pub fn prepare_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: usize,
        view_proj: Mat4,
        position: Vec3,
    ) {
        if view == 0 {
            return;
        }
        // Views past the ones in use (an environment probe's after the
        // mirrors') get frames too; the ones between stay unwritten.
        while view > self.view_frames.len() {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("addon view frame"),
                size: std::mem::size_of::<FrameUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("addon view frame"),
                layout: &self.frame_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            self.view_frames.push((buffer, group));
        }
        let uniform = FrameUniform {
            view_proj: view_proj.to_cols_array(),
            camera: position.extend(1.0).to_array(),
            time: [self.time[0], self.time[1], 0.0, 0.0],
            limits: [self.limit, 0, 0, 0],
        };
        queue.write_buffer(
            &self.view_frames[view - 1].0,
            0,
            bytemuck::bytes_of(&uniform),
        );
    }

    /// [`Self::draw`]'s world-space draws from a view [`Self::prepare_view`]
    /// prepared.
    pub fn draw_view(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        frame: &Frame,
        layer: &Layer,
        view: usize,
    ) {
        let Some((_, group)) = view.checked_sub(1).and_then(|v| self.view_frames.get(v)) else {
            return;
        };
        pass.set_bind_group(0, group, &[]);
        for &i in &self.order {
            let Some(draw) = frame.draws.get(i) else {
                continue;
            };
            let material = &layer.materials[draw.material];
            if material.space != Space::World {
                continue;
            }
            let mesh = &self.meshes[draw.mesh];
            let blend = Blend::ALL
                .iter()
                .position(|b| *b == material.blend)
                .unwrap_or(0);
            pass.set_pipeline(&self.pipelines[material.shader][Space::World as usize][blend]);
            pass.set_bind_group(1, &self.draw_group, &[(i as u64 * DRAW_STRIDE) as u32]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, 0..1);
        }
    }

    /// After the pass that drew the layer ends: copy its timestamps out.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(timer) = &self.timer {
            timer.resolve(encoder);
        }
    }
}

/// One rendered frame, RGBA8, with the loop cap it ran at and its measured
/// GPU time where the device has timestamps.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub loop_limit: u32,
    pub gpu_ms: Option<f32>,
}

/// The features [`LayerRenderer`] times layers with, where the adapter has
/// them. Request them when creating the device.
pub fn timing_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    adapter.features()
        & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
}

/// A headless device for tests, previews and calibration, with timestamps
/// where the adapter has them.
pub fn headless_device() -> Result<(String, wgpu::Device, wgpu::Queue)> {
    open_device(true)
}

/// A headless device, with timestamps where the adapter has them when
/// `timed`, or never when not.
fn open_device(timed: bool) -> Result<(String, wgpu::Device, wgpu::Queue)> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .context("no GPU adapter")?;
    let name = adapter.get_info().name;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: if timed {
            timing_features(&adapter)
        } else {
            wgpu::Features::empty()
        },
        ..Default::default()
    }))?;
    Ok((name, device, queue))
}

/// The work [`calibrate`] times: 16 transcendental operations per loop
/// iteration, the heaviest kind of shader work, so the speed it measures is
/// a lower bound for ordinary shaders.
const CALIBRATION: &str = "
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(v: BriVertex) -> Out {
    var out: Out;
    out.clip = vec4<f32>(v.position.xy, 0.0, 1.0);
    out.uv = v.uv;
    return out;
}
@fragment fn fs_main(v: Out) -> @location(0) vec4<f32> {
    var a = v.uv.x + 0.5;
    var b = v.uv.y + 0.25;
    for (var i = 0u; i < 1000000u; i++) {
        a = sin(a) + cos(b) + exp2(fract(a)) - log2(abs(b) + 1.0);
        b = cos(a) - sin(b) + inverseSqrt(abs(a) + 1.0) + sqrt(abs(b));
        a = tan(fract(a)) + atan(b) + exp(fract(b)) - log(abs(a) + 1.0);
        b = sinh(fract(a)) + cosh(fract(b)) - tanh(a) + pow(abs(b) + 1.0, 0.5);
    }
    return vec4<f32>(fract(a), fract(b), 0.0, 1.0);
}";
const CALIBRATION_SIZE: u32 = 128;
/// Calibration stops raising the work once a pass takes this long.
const CALIBRATION_MS: f64 = 10.0;

/// A pass long enough to stop at is timed again until the fastest time has
/// held for this many passes in a row.
const CALIBRATION_SAMPLES: u32 = 3;

/// A pass at least this much faster than the fastest so far means the GPU is
/// still speeding up.
const CALIBRATION_STEADY: f64 = 0.97;

/// Calibration keeps the GPU busy for at least this long (GPU time) before
/// it believes a speed. A GPU idling at low clocks (entering a game after
/// menus) takes on the order of 100 ms of steady load to raise them, and
/// v0.2.8 session logs read up to 13 times slower at join than after a
/// graphics change on the same card.
const CALIBRATION_WARM_UP_MS: f64 = 150.0;

/// Calibration stops after this much GPU time whatever it has seen, and
/// after this many passes at one loop cap.
const CALIBRATION_MOST_MS: f64 = 500.0;
const CALIBRATION_MOST_PASSES: u32 = 64;

/// Measure how much Add-On shader work the GPU does per millisecond, by
/// timing a small offscreen pass at rising loop caps (4 to 4096 iterations
/// over 128x128 pixels) until one takes about 10 ms. Passes are timed on the
/// GPU where it has timestamps, so waiting behind other programs' work or
/// for this thread to be scheduled is not counted; otherwise by the clock.
/// The GPU is kept busy long enough to leave idle clocks first. Takes well
/// under a second on any GPU that can run the game. `None` when the pass
/// fails.
pub fn calibrate(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<GpuSpeed> {
    let shader = crate::shader::compile("calibration.wgsl", CALIBRATION).ok()?;
    let cost = f64::from(shader.fragment_cost);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("addon calibration"),
        size: wgpu::Extent3d {
            width: CALIBRATION_SIZE,
            height: CALIBRATION_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = LayerRenderer::new(device, queue, None, format, None, 1, 1);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("addon calibration"),
        source: wgpu::ShaderSource::Naga(Cow::Owned(shader.module)),
    });
    let pipeline = renderer.pipeline(device, &module, "calibration", Space::World, Blend::Opaque);
    let corner = |x: f32, y: f32| Vertex {
        position: [x, y, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [x * 0.5 + 0.5, y * 0.5 + 0.5],
    };
    let vertices = [corner(-1.0, -1.0), corner(3.0, -1.0), corner(-1.0, 3.0)];
    renderer.meshes.push(GpuMesh {
        vertices: device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("addon calibration"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        indices: device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("addon calibration"),
            contents: bytemuck::cast_slice(&[0u32, 1, 2]),
            usage: wgpu::BufferUsages::INDEX,
        }),
        count: 3,
    });
    if pollster::block_on(scope.pop()).is_some() {
        return None;
    }
    let draw = DrawUniform {
        model: Mat4::IDENTITY.to_cols_array(),
        params: [[0.0; 4]; 4],
    };
    queue.write_buffer(&renderer.draw_buffer, 0, bytemuck::bytes_of(&draw));
    let timer = device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
        .then(|| {
            let buffer = |label, usage| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: 16,
                    usage,
                    mapped_at_creation: false,
                })
            };
            (
                device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("addon calibration"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                }),
                buffer(
                    "addon calibration resolve",
                    wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                ),
                buffer(
                    "addon calibration readback",
                    wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                ),
            )
        });
    let period_ns = f64::from(queue.get_timestamp_period());
    let wait = || {
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .ok()
    };
    let run = |limit: u32| -> Option<f64> {
        let uniform = FrameUniform {
            view_proj: Mat4::IDENTITY.to_cols_array(),
            camera: [0.0; 4],
            time: [0.0; 4],
            limits: [limit, 0, 0, 0],
        };
        queue.write_buffer(&renderer.frame_buffers[0], 0, bytemuck::bytes_of(&uniform));
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("addon calibration"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: timer.as_ref().map(|(queries, _, _)| {
                    wgpu::RenderPassTimestampWrites {
                        query_set: queries,
                        beginning_of_pass_write_index: Some(0),
                        end_of_pass_write_index: Some(1),
                    }
                }),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &renderer.frame_groups[0], &[]);
            pass.set_bind_group(1, &renderer.draw_group, &[0]);
            let mesh = &renderer.meshes[0];
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..3, 0, 0..1);
        }
        if let Some((queries, resolve, readback)) = &timer {
            encoder.resolve_query_set(queries, 0..2, resolve, 0);
            encoder.copy_buffer_to_buffer(resolve, 0, readback, 0, 16);
        }
        let started = Instant::now();
        queue.submit([encoder.finish()]);
        wait()?;
        let clock_ms = started.elapsed().as_secs_f64() * 1000.0;
        let Some((_, _, readback)) = &timer else {
            return Some(clock_ms);
        };
        let slice = readback.slice(..);
        let (send, receive) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
        wait()?;
        receive.recv().ok()?.ok()?;
        let ticks: [u64; 2] = bytemuck::pod_read_unaligned(&slice.get_mapped_range().ok()?[..16]);
        readback.unmap();
        // A driver that writes no timestamps leaves the clock to time it.
        Some(if ticks[1] > ticks[0] {
            (ticks[1] - ticks[0]) as f64 * period_ns / 1e6
        } else {
            clock_ms
        })
    };
    let pixels = f64::from(CALIBRATION_SIZE * CALIBRATION_SIZE);
    speed_from_passes(pixels * cost, run)
}

/// The GPU's speed from `run(limit)`, the time in ms of a calibration pass
/// at loop cap `limit` doing `work` per loop iteration, raising the cap
/// fourfold until a pass takes [`CALIBRATION_MS`]. Load from anything else
/// on the machine only ever adds time, and one pass slowed by it at a low
/// cap would make the GPU look many times slower than it is (and stop
/// Add-Ons that fit easily), so a pass long enough to stop at is timed again
/// and the fastest time counts. A GPU at idle clocks is just as slow, so the
/// passes go on until the GPU has been busy for [`CALIBRATION_WARM_UP_MS`]
/// and the fastest time holds for [`CALIBRATION_SAMPLES`] passes.
fn speed_from_passes(work: f64, mut run: impl FnMut(u32) -> Option<f64>) -> Option<GpuSpeed> {
    let busy = Cell::new(0.0);
    let mut timed = |limit: u32| -> Option<f64> {
        let ms = run(limit)?;
        busy.set(busy.get() + ms);
        Some(ms)
    };
    // Warm up, then the cost of a pass that runs no loop at all.
    timed(0)?;
    let mut empty = timed(0)?.min(timed(0)?);
    let mut limit = 3;
    loop {
        let mut fastest = timed(limit)?;
        let last = limit >= MAX_LOOP_LIMIT;
        let (mut steady, mut passes) = (1, 1);
        while (fastest - empty >= CALIBRATION_MS || last)
            && (steady < CALIBRATION_SAMPLES || busy.get() < CALIBRATION_WARM_UP_MS)
            && busy.get() < CALIBRATION_MOST_MS
            && passes < CALIBRATION_MOST_PASSES
        {
            let ms = timed(limit)?;
            passes += 1;
            steady = if ms < fastest * CALIBRATION_STEADY {
                1
            } else {
                steady + 1
            };
            fastest = fastest.min(ms);
        }
        if fastest - empty >= CALIBRATION_MS || last {
            // The empty pass was first timed cold; a warm one is faster,
            // which only makes the speed lower.
            empty = empty.min(timed(0)?);
            // Timer noise on a very fast GPU only makes this lower.
            return Some(GpuSpeed {
                work_per_ms: work * f64::from(limit + 1) / (fastest - empty).max(0.5),
            });
        }
        limit = (limit + 1) * 4 - 1;
    }
}

/// Run `addon` headless for each time in `times` and render every frame
/// offscreen, for tests and previews. Fails when there is no GPU adapter.
/// The GPU is calibrated first, as the game does.
pub fn render_offscreen(
    addon: &mut AddOn,
    width: u32,
    height: u32,
    times: &[f32],
) -> Result<(String, Vec<Image>)> {
    render_offscreen_scene(
        addon,
        width,
        height,
        times,
        Vec3::new(2.4, 1.8, 3.2),
        Vec3::ZERO,
        |_| Default::default(),
    )
}

/// [`render_offscreen`] from a camera at `eye` looking at `target`, with
/// `world(time)` as what the game shows at each frame (for Add-Ons that
/// read the world).
pub fn render_offscreen_scene(
    addon: &mut AddOn,
    width: u32,
    height: u32,
    times: &[f32],
    eye: Vec3,
    target: Vec3,
    world: impl Fn(f32) -> Arc<crate::world::World>,
) -> Result<(String, Vec<Image>)> {
    render_offscreen_views(addon, width, height, times, eye, target, |t| {
        (world(t), crate::host::View::default())
    })
}

/// [`render_offscreen`] on a GPU taken to run `speed`, instead of measuring
/// it, and without timing the layer: every frame's loop cap is then
/// [`loop_limit`] of that speed at the frame budget
/// ([`Budgets::gpu_ms_per_frame`](crate::Budgets)), whatever else the
/// machine is doing. For checking the cap on a real GPU; the game measures.
pub fn render_offscreen_at_speed(
    addon: &mut AddOn,
    width: u32,
    height: u32,
    times: &[f32],
    speed: GpuSpeed,
) -> Result<(String, Vec<Image>)> {
    render_views(
        addon,
        width,
        height,
        times,
        Some(speed),
        Vec3::new(2.4, 1.8, 3.2),
        Vec3::ZERO,
        |_| Default::default(),
    )
}

/// [`render_offscreen_scene`] with the player's view at each frame too
/// (its size is the image's), for Add-Ons that draw by what the player
/// sees: aiming, first person, ammo.
pub fn render_offscreen_views(
    addon: &mut AddOn,
    width: u32,
    height: u32,
    times: &[f32],
    eye: Vec3,
    target: Vec3,
    input: impl Fn(f32) -> (Arc<crate::world::World>, crate::host::View),
) -> Result<(String, Vec<Image>)> {
    render_views(addon, width, height, times, None, eye, target, input)
}

/// Render offscreen at a `given` GPU speed with no timing, or (None) on a
/// calibrated, timed device as the game does.
#[allow(clippy::too_many_arguments)]
fn render_views(
    addon: &mut AddOn,
    width: u32,
    height: u32,
    times: &[f32],
    given: Option<GpuSpeed>,
    eye: Vec3,
    target: Vec3,
    input: impl Fn(f32) -> (Arc<crate::world::World>, crate::host::View),
) -> Result<(String, Vec<Image>)> {
    ensure!(
        width > 0 && height > 0 && width <= 4096 && height <= 4096 && width.is_multiple_of(64),
        "width must be a multiple of 64"
    );
    let (name, device, queue) = open_device(given.is_none())?;
    let speed = given.or_else(|| calibrate(&device, &queue));
    let color_format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let depth_format = wgpu::TextureFormat::Depth32Float;
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = |format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("addon preview"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let color = texture(
        color_format,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(depth_format, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let color_view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let mut renderer = LayerRenderer::new(
        &device,
        &queue,
        speed,
        color_format,
        Some(depth_format),
        1,
        2048,
    );
    let camera = Camera {
        view_proj: reversed_perspective(0.9, width as f32 / height as f32, 0.1, 100.0)
            * glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y),
        position: eye,
        size: [width, height],
        normal_fov: 90.0,
    };
    let stopped = |e: Stopped| anyhow::anyhow!("the Add-On stopped: {e}");
    let wait = || {
        device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
    };
    let mut images = Vec::new();
    let mut last = 0.0;
    for &time in times {
        let (world, mut view) = input(time);
        view.size = [width, height];
        let camera = Camera {
            normal_fov: view.normal_fov,
            ..camera
        };
        let frame = addon
            .frame(crate::host::FrameInput {
                time,
                dt: time - last,
                eye: eye.to_array(),
                forward: (target - eye).normalize().to_array(),
                world,
                view,
                ..Default::default()
            })
            .map_err(stopped)?
            .clone();
        last = time;
        renderer
            .prepare(&device, &queue, addon, &frame, camera, [time, 0.0])
            .map_err(stopped)?;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("addon readback"),
            size: u64::from(width * height * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("addon layer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.06,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(DEPTH_CLEAR),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            renderer.draw(&mut pass, &frame, addon.layer());
        }
        renderer.resolve(&mut encoder);
        encoder.copy_texture_to_buffer(
            color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
            },
            size,
        );
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (send, receive) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
        wait()?;
        receive.recv()??;
        let pixels = slice.get_mapped_range()?.to_vec();
        readback.unmap();
        // Start reading the timestamps, let the copy finish, then read them.
        let mut gpu_ms = renderer.read_times(&device, addon).map_err(stopped)?;
        if gpu_ms.is_none() && renderer.timed() {
            wait()?;
            gpu_ms = renderer.read_times(&device, addon).map_err(stopped)?;
        }
        images.push(Image {
            width,
            height,
            pixels,
            loop_limit: renderer.loop_limit(),
            gpu_ms,
        });
    }
    Ok((name, images))
}

/// Write an image as PNG.
pub fn write_png(image: &Image, path: &std::path::Path) -> Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&image.pixels)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A calibration pass at loop cap `limit` on a GPU doing `speed` work
    /// per ms, `work` per iteration, plus a small fixed cost.
    fn pass_ms(speed: f64, work: f64, limit: u32) -> f64 {
        0.05 + work * f64::from(limit + 1) / speed
    }

    #[test]
    fn one_pass_slowed_by_load_does_not_make_the_gpu_look_slow() {
        let (speed, work) = (1.0e7, 128.0 * 128.0 * 100.0);
        let mut busy = true;
        let measured = speed_from_passes(work, |limit| {
            let ms = pass_ms(speed, work, limit);
            // Another program's work lands on the first pass with a loop:
            // alone, it would pass for a GPU sixty times slower.
            Some(if limit > 0 && std::mem::take(&mut busy) {
                ms + 40.0
            } else {
                ms
            })
        })
        .unwrap();
        assert!(!busy, "the slow pass was taken");
        let error = measured.work_per_ms / speed - 1.0;
        assert!(error.abs() < 0.05, "measured {measured:?}");
    }

    /// [`speed_from_passes`] on a GPU whose speed after `busy` ms of
    /// calibration work is `speed(busy)`; the measured speed and the GPU time
    /// calibration took.
    fn calibrate_on(work: f64, speed: impl Fn(f64) -> f64) -> (f64, f64) {
        let mut busy = 0.0;
        let measured = speed_from_passes(work, |limit| {
            let ms = pass_ms(speed(busy), work, limit);
            busy += ms;
            Some(ms)
        })
        .unwrap();
        (measured.work_per_ms, busy)
    }

    /// v0.2.8 on an RTX 4070 SUPER: 3.7e8 at join, 4.8e9 after a graphics
    /// change.
    const COLD: f64 = 3.7e8;
    const WARM: f64 = 4.8e9;

    #[test]
    fn a_gpu_at_idle_clocks_is_measured_once_it_has_sped_up() {
        let work = 128.0 * 128.0 * 100.0;
        // Clocks rise steadily over the first 120 ms of load.
        let (measured, busy) =
            calibrate_on(work, |busy| COLD + (WARM - COLD) * (busy / 120.0).min(1.0));
        let error = measured / WARM - 1.0;
        assert!(
            error.abs() < 0.05,
            "measured {measured:.2e} for a GPU of {WARM:.2e} ({:.0}x low), in {busy:.0} ms",
            WARM / measured
        );
        assert!(busy <= CALIBRATION_MOST_MS + 50.0, "took {busy:.0} ms");
        // Clocks that jump all at once after 120 ms.
        let (measured, _) = calibrate_on(work, |busy| if busy < 120.0 { COLD } else { WARM });
        assert!(
            (measured / WARM - 1.0).abs() < 0.05,
            "measured {measured:.2e}"
        );
    }

    #[test]
    fn a_warm_gpu_is_measured_in_about_the_warm_up() {
        let work = 128.0 * 128.0 * 100.0;
        // Fast enough that the highest cap is short, as on the RTX 4070
        // SUPER, and slow enough that the cap stops at 10 ms.
        for speed in [WARM, COLD / 10.0] {
            let (measured, busy) = calibrate_on(work, |_| speed);
            assert!(
                (measured / speed - 1.0).abs() < 0.05,
                "measured {measured:.2e} for {speed:.2e}"
            );
            assert!(
                busy < CALIBRATION_WARM_UP_MS
                    + 4.0 * CALIBRATION_MS * f64::from(CALIBRATION_SAMPLES),
                "took {busy:.0} ms at {speed:.2e}"
            );
        }
    }

    #[test]
    fn a_failed_pass_fails_calibration() {
        assert_eq!(
            speed_from_passes(1.0, |limit| (limit == 0).then_some(0.1)),
            None
        );
    }

    #[test]
    fn a_refusal_from_a_busy_measurement_is_measured_again() {
        let real = GpuSpeed { work_per_ms: 1.0e7 };
        // The first measurement was taken while the GPU was busy: 40 times
        // too slow, so 50 ms of work looks like 2000.
        let mut speed = GpuSpeed {
            work_per_ms: real.work_per_ms / 40.0,
        };
        let work = real.work_per_ms * 50.0;
        let mut measures = 0;
        let refused = too_heavy(work, WORST_FIRST_FRAME_MS, &mut speed, || {
            measures += 1;
            Some(real)
        });
        assert_eq!(refused, None, "it fits at the GPU's real speed");
        assert_eq!(speed, real, "and the renderer keeps that speed");
        assert_eq!(measures, 1);
        // Once known, the next frame needs no measuring.
        assert_eq!(
            too_heavy(work, WORST_FIRST_FRAME_MS, &mut speed, || unreachable!()),
            None
        );
    }

    #[test]
    fn shaders_too_heavy_at_the_fastest_speed_seen_are_still_refused() {
        let mut speed = GpuSpeed { work_per_ms: 1.0e7 };
        let work = speed.work_per_ms * 2000.0;
        let mut measures = 0;
        let refused = too_heavy(work, WORST_FIRST_FRAME_MS, &mut speed, || {
            measures += 1;
            // A slower measurement never lowers the speed.
            Some(GpuSpeed { work_per_ms: 1.0e6 })
        });
        assert_eq!(refused, Some(2000.0));
        assert_eq!(measures, REMEASURES);
        assert_eq!(speed.work_per_ms, 1.0e7);
        let mut failing = GpuSpeed { work_per_ms: 1.0e7 };
        assert_eq!(
            too_heavy(work, WORST_FIRST_FRAME_MS, &mut failing, || None),
            Some(2000.0),
            "a measurement that fails refuses as before"
        );
    }
}
