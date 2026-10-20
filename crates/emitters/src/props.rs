//! The authored properties of a particle emitter. The runtime that owns the emitter fills this; this crate neither reads packages nor
//! knows where the values came from.
use crate::rotation::Rotator;
use glam::Vec3;

/// `Range { Min, Max }`: drawn as `Max + r (Min - Max)`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Range {
    pub min: f32,
    pub max: f32,
}
impl Range {
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }
    pub const fn fixed(v: f32) -> Self {
        Self { min: v, max: v }
    }
}

/// `RangeVector { X, Y, Z }`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RangeVector {
    pub x: Range,
    pub y: Range,
    pub z: Range,
}
impl RangeVector {
    pub const fn fixed(v: Vec3) -> Self {
        Self {
            x: Range::fixed(v.x),
            y: Range::fixed(v.y),
            z: Range::fixed(v.z),
        }
    }
}

/// 8-bit B, G, R, A, as the original stores a colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8,
}
impl Color {
    pub const WHITE: Color = Color {
        b: 255,
        g: 255,
        r: 255,
        a: 255,
    };
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { b, g, r, a }
    }
    /// 0..1 R, G, B, A.
    pub fn to_unit(self) -> [f32; 4] {
        let k = 1.0 / 255.0;
        [
            f32::from(self.r) * k,
            f32::from(self.g) * k,
            f32::from(self.b) * k,
            f32::from(self.a) * k,
        ]
    }
}

/// How a particle's picture combines with the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DrawStyle {
    #[default]
    Regular,
    AlphaBlend,
    Modulated,
    Translucent,
    AlphaModulated,
    Darken,
    Brighten,
}

/// Which frame a particle's position is kept in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CoordinateSystem {
    #[default]
    Independent,
    Relative,
    Absolute,
}

/// The shape a particle's start point is drawn from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartLocationShape {
    #[default]
    Box,
    Sphere,
    Polar,
    All,
}

/// How a particle's starting velocity is aimed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VelocityDirection {
    #[default]
    None,
    StartPositionAndOwner,
    OwnerAndStartPosition,
    AddRadial,
}

/// Where a particle's orientation comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RotationSource {
    #[default]
    None,
    Actor,
    Offset,
    Normal,
}

/// The axis an emitter's effect points along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EffectAxis {
    #[default]
    NegativeX,
    PositiveZ,
}

/// `SpriteEmitter.EParticleDirectionUsage`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DirectionUsage {
    #[default]
    None,
    Up,
    Right,
    Forward,
    Normal,
    UpAndNormal,
    RightAndNormal,
    Scale,
}

/// How start points are taken from a mesh.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeshSpawning {
    #[default]
    None,
    Linear,
    Random,
}

/// How a skeletal owner's bones move a particle's start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkelLocationUpdate {
    #[default]
    None,
    SpawnOffset,
    Location,
}

/// How a sound is chosen for a collision or a spawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoundSelection {
    #[default]
    None,
    LinearGlobal,
    LinearLocal,
    Random,
}

/// One `ParticleSound`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticleSound {
    /// The host's id for the sound asset.
    pub sound: u32,
    pub radius: Range,
    pub pitch: Range,
    pub weight: i32,
    pub volume: Range,
    pub probability: Range,
}

/// The static mesh a `MeshSpawning` emitter takes start points from.
#[derive(Clone, Debug, Default)]
pub struct SpawnMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    /// Per-vertex colours (B, G, R, A), for `UseColorFromMesh`.
    pub colors: Vec<Color>,
}

/// One `ColorScale` entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorScale {
    pub relative_time: f32,
    pub color: Color,
}
/// One `SizeScale` entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeScale {
    pub relative_time: f32,
    pub relative_size: f32,
}
/// One `VelocityScale` / `RevolutionScale` entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorScale {
    pub relative_time: f32,
    pub value: Vec3,
}

/// A plane as the original stores a fade factor: x, y, z, w.
pub type Plane = [f32; 4];

#[derive(Clone, Debug)]
pub struct ParticleEmitterProps {
    // Booleans.
    pub use_collision: bool,
    pub use_collision_planes: bool,
    pub use_max_collisions: bool,
    pub use_spawned_velocity_scale: bool,
    pub use_color_scale: bool,
    pub fade_out: bool,
    pub fade_in: bool,
    pub use_actor_forces: bool,
    pub respawn_dead_particles: bool,
    pub auto_destroy: bool,
    pub auto_reset: bool,
    pub disabled: bool,
    pub disable_fogging: bool,
    pub velocity_from_mesh: bool,
    pub uniform_mesh_scale: bool,
    pub uniform_velocity_scale: bool,
    pub use_color_from_mesh: bool,
    pub spawn_only_in_direction_of_normal: bool,
    pub alpha_test: bool,
    pub accepts_projectors: bool,
    pub z_test: bool,
    pub z_write: bool,
    pub use_revolution: bool,
    pub use_revolution_scale: bool,
    pub spin_particles: bool,
    pub damp_rotation: bool,
    pub use_size_scale: bool,
    pub use_absolute_time_for_size_scale: bool,
    pub use_regular_size_scale: bool,
    pub uniform_size: bool,
    pub determine_velocity_by_location_difference: bool,
    pub scale_size_x_by_velocity: bool,
    pub scale_size_y_by_velocity: bool,
    pub scale_size_z_by_velocity: bool,
    pub automatic_initial_spawning: bool,
    pub blend_between_subdivisions: bool,
    pub use_subdivision_scale: bool,
    pub use_random_subdivision: bool,
    pub trigger_disabled: bool,
    pub reset_on_trigger: bool,
    pub use_velocity_scale: bool,
    pub add_velocity_from_owner: bool,
    pub b_only_bsp_collision: bool,
    pub rotate_velocity_loss_range: bool,
    pub b_fit_sizing: bool,
    pub b_rest_on_land: bool,
    pub b_weird_spin_on_collide: bool,
    // Numbers.
    pub min_frames_before_death: i32,
    pub b_fade_on_collide: bool,
    pub damage_amount: i32,
    pub damage_momentum: f32,
    pub collision_planes: Vec<[f32; 4]>,
    pub b_play_collision_sound_from_owner: bool,
    pub spawn_from_other_emitter: i32,
    pub spawn_amount: i32,
    pub add_location_from_other_emitter: i32,
    pub add_velocity_from_other_emitter: i32,
    pub mesh_spawning: MeshSpawning,
    pub mesh: Option<SpawnMesh>,
    pub velocity_scale_range: RangeVector,
    pub mesh_scale_range: RangeVector,
    pub mesh_normal: Vec3,
    pub mesh_normal_threshold_range: Range,
    pub use_skeletal_location_as: SkelLocationUpdate,
    pub skeletal_scale: Vec3,
    pub relative_bone_index_range: Range,
    pub sounds: Vec<ParticleSound>,
    pub spawning_sound: SoundSelection,
    pub spawning_sound_index: Range,
    pub spawning_sound_probability: Range,
    pub collision_sound: SoundSelection,
    pub collision_sound_index: Range,
    pub collision_sound_probability: Range,
    pub low_detail_factor: f32,
    pub acceleration: Vec3,
    pub extent_multiplier: Vec3,
    pub damping_factor_range: RangeVector,
    pub max_collisions: Range,
    pub spawned_velocity_scale_range: RangeVector,
    pub color_scale: Vec<ColorScale>,
    pub color_scale_repeats: f32,
    pub color_multiplier_range: RangeVector,
    pub opacity: f32,
    pub fade_out_factor: Plane,
    pub fade_out_start_time: f32,
    pub fade_in_factor: Plane,
    pub fade_in_end_time: f32,
    pub coordinate_system: CoordinateSystem,
    pub max_particles: i32,
    pub effect_axis: EffectAxis,
    pub auto_reset_time_range: Range,
    pub start_location_offset: Vec3,
    pub start_location_range: RangeVector,
    pub start_location_shape: StartLocationShape,
    pub sphere_radius_range: Range,
    pub start_location_polar_range: RangeVector,
    pub start_mass_range: Range,
    pub alpha_ref: i32,
    pub revolution_center_offset_range: RangeVector,
    pub revolutions_per_second_range: RangeVector,
    pub revolution_scale: Vec<VectorScale>,
    pub revolution_scale_repeats: f32,
    pub use_rotation_from: RotationSource,
    pub rotation_offset: Rotator,
    pub spin_ccw_or_cw: Vec3,
    pub spins_per_second_range: RangeVector,
    pub start_spin_range: RangeVector,
    pub rotation_damping_factor_range: RangeVector,
    pub rotation_normal: Vec3,
    pub size_scale: Vec<SizeScale>,
    pub size_scale_repeats: f32,
    pub start_size_range: RangeVector,
    pub scale_size_by_velocity_multiplier: Vec3,
    pub scale_size_by_velocity_max: f32,
    pub particles_per_second: f32,
    pub initial_particles_per_second: f32,
    pub draw_style: DrawStyle,
    pub texture_u_subdivisions: i32,
    pub texture_v_subdivisions: i32,
    pub subdivision_scale: Vec<f32>,
    pub subdivision_start: i32,
    pub subdivision_end: i32,
    pub seconds_before_inactive: f32,
    pub min_squared_velocity: f32,
    pub initial_time_range: Range,
    pub lifetime_range: Range,
    pub initial_delay_range: Range,
    pub spawn_on_trigger_range: Range,
    pub spawn_on_trigger_pps: f32,
    pub start_velocity_range: RangeVector,
    pub start_velocity_radial_range: Range,
    pub max_abs_velocity: Vec3,
    pub velocity_loss_range: RangeVector,
    pub add_velocity_multiplier_range: RangeVector,
    pub get_velocity_direction_from: VelocityDirection,
    pub velocity_scale: Vec<VectorScale>,
    pub velocity_scale_repeats: f32,
    pub warmup_ticks_per_second: f32,
    pub relative_warmup_time: f32,
    pub landed_min_squared_velocity: f32,
    pub rest_reset: Vec3,
    pub rest_reset_flags: [bool; 3],
    pub weird_bounce: f32,
}

impl Default for ParticleEmitterProps {
    fn default() -> Self {
        Self {
            use_collision: false,
            use_collision_planes: false,
            use_max_collisions: false,
            use_spawned_velocity_scale: false,
            use_color_scale: false,
            fade_out: false,
            fade_in: false,
            use_actor_forces: false,
            respawn_dead_particles: true,
            auto_destroy: false,
            auto_reset: false,
            disabled: false,
            disable_fogging: false,
            velocity_from_mesh: false,
            uniform_mesh_scale: true,
            uniform_velocity_scale: false,
            use_color_from_mesh: false,
            spawn_only_in_direction_of_normal: false,
            alpha_test: true,
            accepts_projectors: true,
            z_test: true,
            z_write: false,
            use_revolution: false,
            use_revolution_scale: false,
            spin_particles: false,
            damp_rotation: false,
            use_size_scale: false,
            use_absolute_time_for_size_scale: false,
            use_regular_size_scale: true,
            uniform_size: false,
            determine_velocity_by_location_difference: false,
            scale_size_x_by_velocity: false,
            scale_size_y_by_velocity: false,
            scale_size_z_by_velocity: false,
            automatic_initial_spawning: true,
            blend_between_subdivisions: false,
            use_subdivision_scale: false,
            use_random_subdivision: false,
            trigger_disabled: true,
            reset_on_trigger: false,
            use_velocity_scale: false,
            add_velocity_from_owner: false,
            b_only_bsp_collision: false,
            rotate_velocity_loss_range: false,
            b_fit_sizing: false,
            b_rest_on_land: false,
            b_weird_spin_on_collide: false,
            min_frames_before_death: 0,
            b_fade_on_collide: false,
            damage_amount: 0,
            damage_momentum: 0.0,
            collision_planes: Vec::new(),
            b_play_collision_sound_from_owner: false,
            spawn_from_other_emitter: -1,
            spawn_amount: 0,
            add_location_from_other_emitter: -1,
            add_velocity_from_other_emitter: -1,
            mesh_spawning: MeshSpawning::None,
            mesh: None,
            velocity_scale_range: RangeVector::default(),
            mesh_scale_range: RangeVector::fixed(Vec3::ONE),
            mesh_normal: Vec3::Z,
            mesh_normal_threshold_range: Range::fixed(0.0),
            use_skeletal_location_as: SkelLocationUpdate::None,
            skeletal_scale: Vec3::ONE,
            relative_bone_index_range: Range::new(0.0, 1.0),
            sounds: Vec::new(),
            spawning_sound: SoundSelection::None,
            spawning_sound_index: Range::fixed(0.0),
            spawning_sound_probability: Range::fixed(1.0),
            collision_sound: SoundSelection::None,
            collision_sound_index: Range::fixed(0.0),
            collision_sound_probability: Range::fixed(1.0),
            low_detail_factor: 1.0,
            acceleration: Vec3::ZERO,
            extent_multiplier: Vec3::ONE,
            damping_factor_range: RangeVector::fixed(Vec3::ONE),
            max_collisions: Range::fixed(0.0),
            spawned_velocity_scale_range: RangeVector::fixed(Vec3::ONE),
            color_scale: Vec::new(),
            color_scale_repeats: 0.0,
            color_multiplier_range: RangeVector::fixed(Vec3::ONE),
            opacity: 1.0,
            fade_out_factor: [1.0; 4],
            fade_out_start_time: 0.0,
            fade_in_factor: [1.0; 4],
            fade_in_end_time: 0.0,
            coordinate_system: CoordinateSystem::Independent,
            max_particles: 10,
            effect_axis: EffectAxis::NegativeX,
            auto_reset_time_range: Range::fixed(0.0),
            start_location_offset: Vec3::ZERO,
            start_location_range: RangeVector::default(),
            start_location_shape: StartLocationShape::Box,
            sphere_radius_range: Range::fixed(0.0),
            start_location_polar_range: RangeVector::default(),
            start_mass_range: Range::fixed(0.0),
            alpha_ref: 0,
            revolution_center_offset_range: RangeVector::default(),
            revolutions_per_second_range: RangeVector::default(),
            revolution_scale: Vec::new(),
            revolution_scale_repeats: 0.0,
            use_rotation_from: RotationSource::None,
            rotation_offset: Rotator::new(0, 0, 0),
            spin_ccw_or_cw: Vec3::splat(0.5),
            spins_per_second_range: RangeVector::default(),
            start_spin_range: RangeVector::default(),
            rotation_damping_factor_range: RangeVector::fixed(Vec3::ONE),
            rotation_normal: Vec3::Z,
            size_scale: Vec::new(),
            size_scale_repeats: 0.0,
            start_size_range: RangeVector::fixed(Vec3::ONE),
            scale_size_by_velocity_multiplier: Vec3::ZERO,
            scale_size_by_velocity_max: 0.0,
            particles_per_second: 0.0,
            initial_particles_per_second: 0.0,
            draw_style: DrawStyle::Regular,
            texture_u_subdivisions: 1,
            texture_v_subdivisions: 1,
            subdivision_scale: Vec::new(),
            subdivision_start: 0,
            subdivision_end: 0,
            seconds_before_inactive: 0.0,
            min_squared_velocity: 0.0,
            initial_time_range: Range::fixed(0.0),
            lifetime_range: Range::fixed(4.0),
            initial_delay_range: Range::fixed(0.0),
            spawn_on_trigger_range: Range::fixed(0.0),
            spawn_on_trigger_pps: 0.0,
            start_velocity_range: RangeVector::default(),
            start_velocity_radial_range: Range::fixed(0.0),
            max_abs_velocity: Vec3::ZERO,
            velocity_loss_range: RangeVector::default(),
            add_velocity_multiplier_range: RangeVector::fixed(Vec3::ONE),
            get_velocity_direction_from: VelocityDirection::None,
            velocity_scale: Vec::new(),
            velocity_scale_repeats: 0.0,
            warmup_ticks_per_second: 1.0,
            relative_warmup_time: 0.0,
            landed_min_squared_velocity: 0.0,
            rest_reset: Vec3::ZERO,
            rest_reset_flags: [false; 3],
            weird_bounce: 0.0,
        }
    }
}

/// What a sprite emitter adds to the base emitter's properties.
#[derive(Clone, Debug, Default)]
pub struct SpriteExtra {
    pub use_direction_as: DirectionUsage,
    pub projection_normal: Vec3,
    /// `ProjectionNormal` normalised by `initialize` (zero when it is nearly zero).
    pub real_projection_normal: Vec3,
}

impl SpriteExtra {
    /// `USpriteEmitter::Initialize`: normalise the projection normal; a sprite with no
    /// direction usage is uniformly sized (this sets `base.uniform_size` for that case).
    pub fn initialize(&mut self, base: &mut ParticleEmitterProps) {
        let n = self.projection_normal;
        self.real_projection_normal = crate::rotation::safe_normal(n);
        if self.use_direction_as == DirectionUsage::None {
            base.uniform_size = true;
        }
    }
}
