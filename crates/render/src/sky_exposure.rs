//! Local, geometry-derived skylight visibility. Nine upper-hemisphere depth
//! maps are cached per view, independent of the sun and Shadow Quality. Moving
//! receivers sample the same world-space field; geometry edits invalidate it.
//! Depth is copied to a storage buffer to stay within the renderer's existing
//! sixteen sampled-texture limit. No authored map names or indoor flags.
//!
//! The field is rebuilt when the eye crosses into another `STEP`-unit cell or
//! the static geometry changes. Drawing all nine maps of a large build in
//! one frame was a hitch every few steps, so the rebuild is staged as
//! engines stage their cached shadow and probe updates: one direction a
//! frame into a pending set of maps, which replaces the field (maps,
//! matrices and receiver uniform together) only once all nine are drawn.
//! Until then receivers keep reading the previous field, whose fade region
//! still covers the eye's surroundings.
use crate::buffer_init::BufferInit;
use glam::{Mat4, Vec3};

pub(crate) const DIRECTIONS: usize = 9;
pub(crate) const SIZE: u32 = 512;
const RADIUS: f32 = 96.0;
const STEP: f32 = 8.0;
const STRIDE: u64 = 256;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Receiver {
    matrices: [[f32; 16]; DIRECTIONS],
    directions: [[f32; 4]; DIRECTIONS],
    centre: [f32; 4],
    params: [f32; 4],
}

/// A field being drawn: where it is centred and from what, its matrices
/// and receiver data, and how many directions are drawn so far.
struct Pending {
    target: (Vec3, Vec<u64>),
    matrices: [Mat4; DIRECTIONS],
    receiver: Receiver,
    done: usize,
}

pub(crate) struct SkyExposure {
    texture: wgpu::Texture,
    /// The field being drawn, a direction a frame (`plan`).
    pending_texture: wgpu::Texture,
    pub pending_layers: Vec<wgpu::TextureView>,
    working: wgpu::Texture,
    pub working_layers: Vec<wgpu::TextureView>,
    pub matrices: std::cell::Cell<[Mat4; DIRECTIONS]>,
    pub had_moving: std::cell::Cell<bool>,
    pub depths: wgpu::Buffer,
    pub receiver: wgpu::Buffer,
    /// The current field's projectors, then the pending field's.
    caster: wgpu::Buffer,
    pub group: wgpu::BindGroup,
    pub enabled: bool,
    pub prepared: std::cell::Cell<bool>,
    cache: std::cell::RefCell<Option<(Vec3, Vec<u64>)>>,
    pending: std::cell::RefCell<Option<Pending>>,
}

impl SkyExposure {
    pub fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, enabled: bool) -> Self {
        let size = if enabled { SIZE } else { 1 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky exposure depth"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: DIRECTIONS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let layer_views = |texture: &wgpu::Texture| -> Vec<wgpu::TextureView> {
            (0..DIRECTIONS)
                .map(|i| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: i as u32,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect()
        };
        let scratch = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: texture.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let pending_texture = scratch("pending sky exposure depth");
        let pending_layers = layer_views(&pending_texture);
        let working = scratch("moving sky exposure depth");
        let working_layers = layer_views(&working);
        let depths = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky exposure depths"),
            size: u64::from(size * size) * DIRECTIONS as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let receiver = device.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sky exposure receiver"),
            contents: bytemuck::bytes_of(&<Receiver as bytemuck::Zeroable>::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let caster = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky exposure projectors"),
            size: STRIDE * 2 * DIRECTIONS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky exposure caster"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &caster,
                        offset: 0,
                        size: wgpu::BufferSize::new(80),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self {
            texture,
            pending_texture,
            pending_layers,
            working,
            working_layers,
            matrices: std::cell::Cell::new([Mat4::IDENTITY; DIRECTIONS]),
            had_moving: std::cell::Cell::new(false),
            depths,
            receiver,
            caster,
            group,
            enabled,
            prepared: std::cell::Cell::new(false),
            cache: Default::default(),
            pending: Default::default(),
        }
    }

    /// The directions to draw this frame into `pending_layers` (each with
    /// the projector at its `pending_offset`), while the field the eye needs
    /// is not the one held: none while it is. At most `budget` a frame,
    /// except the first field (nothing stands in for it), which draws
    /// whole. Conservative depth along each direction includes distant
    /// ceilings and walls. The cached receiver cube stays fixed as the
    /// camera rotates. Call `finish` after drawing.
    pub fn plan(
        &self,
        queue: &wgpu::Queue,
        eye: Vec3,
        mut key: Vec<u64>,
        bounds: Option<(Vec3, Vec3)>,
        budget: usize,
    ) -> Option<std::ops::Range<usize>> {
        self.prepared.set(true);
        if !self.enabled {
            return None;
        }
        key.sort_unstable();
        let centre = (eye / STEP).floor() * STEP;
        let target = (centre, key);
        if self.cache.borrow().as_ref() == Some(&target) {
            *self.pending.borrow_mut() = None;
            return None;
        }
        let mut pending = self.pending.borrow_mut();
        if pending.as_ref().is_none_or(|p| p.target != target) {
            let (matrices, receiver) = Self::fit(centre, bounds);
            for (i, matrix) in matrices.iter().enumerate() {
                let mut caster = [0.0f32; 20];
                caster[..16].copy_from_slice(&matrix.to_cols_array());
                queue.write_buffer(
                    &self.caster,
                    u64::from(Self::pending_offset(i)),
                    bytemuck::cast_slice(&caster),
                );
            }
            *pending = Some(Pending {
                target,
                matrices,
                receiver,
                done: 0,
            });
        }
        let pending = pending.as_mut().expect("set above");
        let budget = if self.cache.borrow().is_none() {
            DIRECTIONS
        } else {
            budget.clamp(1, DIRECTIONS)
        };
        let first = pending.done;
        pending.done = (first + budget).min(DIRECTIONS);
        Some(first..pending.done)
    }
    /// After the direction `plan` gave was drawn: once every direction of
    /// the pending field is, it becomes the field receivers read (its maps
    /// copied over the current ones on `encoder`, its matrices and receiver
    /// data written), and this returns true.
    pub fn finish(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder) -> bool {
        let mut pending = self.pending.borrow_mut();
        let Some(done) = pending.take_if(|p| p.done >= DIRECTIONS) else {
            return false;
        };
        encoder.copy_texture_to_texture(
            self.pending_texture.as_image_copy(),
            self.texture.as_image_copy(),
            self.texture.size(),
        );
        queue.write_buffer(&self.receiver, 0, bytemuck::bytes_of(&done.receiver));
        for (i, matrix) in done.matrices.iter().enumerate() {
            let mut caster = [0.0f32; 20];
            caster[..16].copy_from_slice(&matrix.to_cols_array());
            queue.write_buffer(
                &self.caster,
                u64::from(Self::offset(i)),
                bytemuck::cast_slice(&caster),
            );
        }
        *self.cache.borrow_mut() = Some(done.target);
        self.matrices.set(done.matrices);
        true
    }
    /// The nine projectors around `centre`, reaching past `bounds`, and the
    /// receiver data that reads them.
    fn fit(centre: Vec3, bounds: Option<(Vec3, Vec3)>) -> ([Mat4; DIRECTIONS], Receiver) {
        let directions: [Vec3; DIRECTIONS] = std::array::from_fn(|i| {
            if i == 0 {
                Vec3::Y
            } else {
                let angle = (i - 1) as f32 * std::f32::consts::TAU / 8.0;
                Vec3::new(angle.cos() * 0.8660254, 0.5, angle.sin() * 0.8660254)
            }
        });
        let span = RADIUS * 3.0f32.sqrt();
        let reach = bounds
            .map_or(RADIUS * 2.0, |(a, b)| {
                a.distance(centre).max(b.distance(centre)) + span
            })
            .max(span);
        let matrices = directions.map(|toward| {
            let up = if toward.y > 0.99 { Vec3::Z } else { Vec3::Y };
            let view = glam::camera::rh::view::look_at_mat4(centre + toward * reach, centre, up);
            glam::camera::rh::proj::directx::orthographic(
                -span,
                span,
                -span,
                span,
                0.0,
                reach + span,
            ) * view
        });
        let receiver = Receiver {
            matrices: matrices.map(|m| m.to_cols_array()),
            directions: directions.map(|d| [d.x, d.y, d.z, 0.0]),
            centre: [centre.x, centre.y, centre.z, RADIUS],
            params: [SIZE as f32, span * 2.0 / SIZE as f32, 1.0, 0.0],
        };
        (matrices, receiver)
    }

    pub fn offset(i: usize) -> u32 {
        (i as u64 * STRIDE) as u32
    }
    pub fn pending_offset(i: usize) -> u32 {
        ((DIRECTIONS + i) as u64 * STRIDE) as u32
    }
    /// The projector at a caster buffer `offset` (`offset` or
    /// `pending_offset`), for culling what it draws.
    pub fn projector(&self, offset: u32) -> Mat4 {
        let slot = u64::from(offset) / STRIDE;
        let i = slot as usize % DIRECTIONS;
        if slot as usize >= DIRECTIONS {
            self.pending
                .borrow()
                .as_ref()
                .map_or(Mat4::IDENTITY, |p| p.matrices[i])
        } else {
            self.matrices.get()[i]
        }
    }

    pub fn merge(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_texture(
            self.texture.as_image_copy(),
            self.working.as_image_copy(),
            self.texture.size(),
        );
    }

    pub fn copy(&self, encoder: &mut wgpu::CommandEncoder, moving: bool) {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: if moving { &self.working } else { &self.texture },
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::DepthOnly,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.depths,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 4),
                    rows_per_image: Some(SIZE),
                },
            },
            wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: DIRECTIONS as u32,
            },
        );
    }
}
