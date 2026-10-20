//! Light coronas (Engine.dll corona manager at VA 0x10933618: `Render` 0x1c8f70,
//! `UpdateActor` 0x1c68e0, `IsOccluded` 0x1c5310; see `specs/coronas-lens-flare-laser.md`).
//! A corona is a camera-facing, additively composed quad whose fade rises and falls with a
//! randomly rate-limited visibility trace, sized by distance.
#![allow(clippy::collapsible_if, clippy::manual_clamp)]
use crate::draw::{BlendFactor, DrawState, StageOp};
use crate::props::Color;
use crate::random::Rand;
use crate::ribbon::pack_color;
use crate::rotation::{Coords, Rotator, rotate_angle_axis, safe_normal};
use crate::sprite::{SpriteDraw, SpriteVertex};
use glam::Vec3;

const SLOTS: usize = 64;

/// What the manager needs of a corona actor (a Light or any actor with `bCorona`).
#[derive(Clone, Debug)]
pub struct CoronaActor {
    pub id: u32,
    pub location: Vec3,
    pub rotation: Rotator,
    /// The actor that owns it (excluded from the directional trace).
    pub owner: Option<u32>,
    pub hue: u8,
    pub saturation: u8,
    pub scale_glow: f32,
    pub draw_scale: f32,
    pub draw_scale3d: Vec3,
    pub clamp_distance: f32,
    pub near_clamp_distance: f32,
    pub directional: bool,
    pub night_vision_only: bool,
    pub normal_vision_only: bool,
    pub has_texture: bool,
    pub deleted: bool,
    /// The first skin is a texture with an alpha channel (`bAlphaTexture`).
    pub alpha_texture: bool,
    pub glow_descriptor: Option<u32>,
    pub light: Option<LightCorona>,
}

/// The extra fields of an `ALight`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LightCorona {
    pub min_size: f32,
    pub max_size: f32,
    pub rotation: f32,
    pub rotation_offset: f32,
    pub use_own_final_blend: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct CoronaScene {
    pub camera: Vec3,
    /// World directions of one screen pixel to the right and upward.
    pub right: Vec3,
    pub up: Vec3,
    /// The viewport clock in seconds.
    pub time: f64,
    pub behind_view: bool,
    pub view_actor: Option<u32>,
    pub night_vision: bool,
    pub glow_filter: Option<u32>,
}

pub trait CoronaWorld {
    /// A line trace that only the actors with `bBlockCoronaTraces` stop; `source` is
    /// ignored. The id of what it hit, `None` when the line is clear.
    fn trace(&mut self, start: Vec3, end: Vec3, source: Option<u32>) -> Option<Option<u32>>;
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    actor: Option<u32>,
    fade: f32,
    last_trace_time: f64,
    last_occluded: bool,
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            actor: None,
            fade: 0.0,
            last_trace_time: 0.0,
            last_occluded: false,
        }
    }
}

/// One drawn corona: a four-vertex fan under the translucent final blend.
#[derive(Clone, Debug)]
pub struct CoronaQuad {
    pub actor: u32,
    pub vertices: [SpriteVertex; 4],
    pub draw: SpriteDraw,
    /// The light supplies its own material blend (`UseOwnFinalBlend`) instead of the
    /// shared translucent one.
    pub use_own_final_blend: bool,
}

impl CoronaQuad {
    pub fn indices(&self) -> [u32; 6] {
        [0, 1, 2, 0, 2, 3]
    }
}

#[derive(Clone, Debug)]
pub struct CoronaManager {
    entries: [Entry; SLOTS],
    ring: [Option<u32>; SLOTS],
    count: usize,
    cursor: usize,
    last_frame_time: f64,
}

impl Default for CoronaManager {
    fn default() -> Self {
        Self {
            entries: [Entry::default(); SLOTS],
            ring: [None; SLOTS],
            count: 0,
            cursor: 0,
            last_frame_time: 0.0,
        }
    }
}

/// `FColor(FPlane)`: floor of the scaled channel, clamped to a byte.
fn quantise(c: f32) -> u8 {
    let v = (c * 255.0).floor();
    if v < 0.0 {
        0
    } else if v > 255.0 {
        255
    } else {
        v as u8
    }
}

fn hue_rgb(hue: u8, sat: u8) -> [f32; 3] {
    let h = f32::from(hue);
    let mut c = if hue < 86 {
        [(85.0 - h) / 85.0, h / 85.0, 0.0]
    } else if hue < 171 {
        [0.0, (170.0 - h) / 85.0, (h - 85.0) / 85.0]
    } else {
        [(h - 170.0) / 85.0, 0.0, (255.0 - h) / 84.0]
    };
    let s = f32::from(sat) / 255.0;
    for v in &mut c {
        *v += (1.0 - *v) * s;
    }
    c
}

impl CoronaManager {
    /// `Reset`: forget every entry and pending actor.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `Add`: an actor the scene collector found this frame. The ring holds 64 and
    /// overwrites the oldest after that.
    pub fn add(&mut self, id: u32) {
        if self.ring[..self.count.min(SLOTS)].contains(&Some(id)) {
            return;
        }
        self.ring[self.count % SLOTS] = Some(id);
        self.count = (self.count + 1) % SLOTS;
    }

    /// `FlushActor` (view target change): drop every live corona.
    pub fn clear_actors(&mut self) {
        for e in &mut self.entries {
            e.actor = None;
        }
    }

    pub fn fade(&self, actor: u32) -> Option<f32> {
        self.entries
            .iter()
            .find(|e| e.actor == Some(actor))
            .map(|e| e.fade)
    }

    fn entry_index(&self, id: u32) -> Option<usize> {
        self.entries.iter().position(|e| e.actor == Some(id))
    }

    /// `IsOccluded`: true when the corona is hidden this frame.
    fn is_occluded(
        &mut self,
        scene: &CoronaScene,
        a: &CoronaActor,
        world: &mut dyn CoronaWorld,
        rand: &mut Rand,
    ) -> bool {
        let entry = self.entry_index(a.id);
        if let Some(i) = entry {
            let elapsed = self.last_frame_time - self.entries[i].last_trace_time;
            let threshold = f64::from(rand.unit()) * 0.33;
            if threshold > elapsed {
                return self.entries[i].last_occluded;
            }
        }
        let hidden = if a.directional {
            let dir = safe_normal(a.location - scene.camera);
            let forward = Coords::UNIT.mul_rotator(a.rotation).x;
            if dir.dot(forward) > 0.0 {
                self.cache(entry, true);
                return true;
            }
            match world.trace(a.location, scene.camera, a.owner) {
                Some(hit) => scene.behind_view || hit != scene.view_actor,
                None => false,
            }
        } else {
            let source = if scene.behind_view {
                None
            } else {
                scene.view_actor
            };
            world.trace(scene.camera, a.location, source).is_some()
        };
        self.cache(entry, hidden);
        hidden
    }

    fn cache(&mut self, entry: Option<usize>, hidden: bool) {
        if let Some(i) = entry {
            self.entries[i].last_occluded = hidden;
            self.entries[i].last_trace_time = self.last_frame_time;
        }
    }

    /// `UpdateActor`: raise or create the actor's fade.
    fn update_actor(
        &mut self,
        scene: &CoronaScene,
        a: &CoronaActor,
        dt: f32,
        world: &mut dyn CoronaWorld,
        rand: &mut Rand,
    ) {
        if !a.has_texture || a.deleted {
            return;
        }
        if self.is_occluded(scene, a, world, rand) {
            return;
        }
        if a.glow_descriptor != scene.glow_filter {
            return;
        }
        if a.night_vision_only && !scene.night_vision {
            return;
        }
        if a.normal_vision_only && scene.night_vision {
            return;
        }
        if let Some(i) = self.entry_index(a.id) {
            let e = &mut self.entries[i];
            e.fade = (e.fade + 2.0 * dt * a.scale_glow).min(1.0);
            return;
        }
        while self.cursor < SLOTS && self.entries[self.cursor].actor.is_some() {
            self.cursor += 1;
        }
        if self.cursor >= SLOTS {
            return;
        }
        self.entries[self.cursor] = Entry {
            actor: Some(a.id),
            fade: (2.0 * dt * a.scale_glow).min(1.0),
            last_trace_time: self.last_frame_time,
            last_occluded: false,
        };
    }

    /// `Render`, once per frame after the scene is drawn: age, update, and return the quads.
    pub fn frame(
        &mut self,
        scene: &CoronaScene,
        lookup: &dyn Fn(u32) -> Option<CoronaActor>,
        world: &mut dyn CoronaWorld,
        rand: &mut Rand,
    ) -> Vec<CoronaQuad> {
        let dt = ((scene.time - self.last_frame_time) * 3.0).min(1.0) as f32;
        self.last_frame_time = scene.time;
        for i in 0..SLOTS {
            if let Some(id) = self.entries[i].actor {
                let glow = lookup(id).map_or(0.0, |a| a.scale_glow);
                self.entries[i].fade -= glow * dt;
                if self.entries[i].fade < 0.0 {
                    self.entries[i].actor = None;
                }
            }
        }
        for slot in 0..SLOTS {
            if let Some(id) = self.ring[slot] {
                if let Some(a) = lookup(id) {
                    self.update_actor(scene, &a, dt, world, rand);
                }
            }
        }
        self.cursor = 0;
        let mut quads = Vec::new();
        for i in 0..SLOTS {
            let Some(id) = self.entries[i].actor else {
                continue;
            };
            let Some(a) = lookup(id) else {
                self.entries[i].actor = None;
                continue;
            };
            if a.deleted || !a.has_texture {
                continue;
            }
            if a.glow_descriptor != scene.glow_filter {
                continue;
            }
            quads.push(quad(scene, &a, self.entries[i].fade));
        }
        quads
    }
}

/// The quad of one corona at `fade` (sections 1.7 and 1.8).
pub fn quad(scene: &CoronaScene, a: &CoronaActor, fade: f32) -> CoronaQuad {
    let d = (a.location - scene.camera).length();
    let view = safe_normal(scene.right.cross(scene.up));
    let mut color = hue_rgb(a.hue, a.saturation).map(|c| c * fade);
    // Distance factor.
    let dd = d.max(a.near_clamp_distance);
    let mut s = if dd * 0.001 < 0.001 {
        0.001
    } else {
        (dd * 0.001).min(a.clamp_distance * 0.001)
    };
    if s <= 1.0 {
        s *= s;
    }
    let actor_oriented = a.draw_scale3d.x != a.draw_scale3d.y && a.draw_scale3d.z > 0.0;
    let (mut rv, mut uv);
    if actor_oriented {
        let c = Coords::UNIT.mul_rotator(a.rotation);
        rv = c.x;
        uv = c.y;
        let t = ((1.0 - rv.dot(view).abs()) * (1.0 - uv.dot(view).abs())).sqrt();
        color = color.map(|v| f32::from(quantise(v)) / 255.0 * t);
    } else {
        rv = scene.right;
        uv = scene.up;
        if let Some(l) = a.light {
            if l.rotation != 0.0 || l.rotation_offset != 0.0 {
                let angle = (s * l.rotation * 16384.0 + l.rotation_offset) as i32;
                rv = rotate_angle_axis(rv, angle, view);
                uv = rotate_angle_axis(uv, angle, view);
            }
        }
    }
    let clamp_i = a.clamp_distance as i32;
    if clamp_i != 0 {
        let rem = (d as i32) % (2 * clamp_i);
        let angle2 = ((rem as f32 / a.clamp_distance) * 0.5 * 65535.0) as i32;
        rv = rotate_angle_axis(rv, angle2, view);
        uv = rotate_angle_axis(uv, angle2, view);
    }
    let mut x = a.draw_scale3d.x * a.draw_scale * s * 128.0;
    let mut y = a.draw_scale3d.y * a.draw_scale * s * 128.0;
    if a.directional {
        let dir = safe_normal(a.location - scene.camera);
        let forward = Coords::UNIT.mul_rotator(a.rotation).x;
        let att = (-dir.dot(forward)).clamp_unit();
        x *= att;
        y *= att;
    }
    if let Some(l) = a.light {
        x = x.max(l.min_size).min(l.max_size);
        y = y.max(l.min_size).min(l.max_size);
    }
    rv *= x;
    uv *= y;
    let rgba = Color {
        r: quantise(color[0]),
        g: quantise(color[1]),
        b: quantise(color[2]),
        a: 255,
    };
    let c = pack_color(rgba);
    let l = a.location;
    let v = |p: Vec3, u: f32, vv: f32| SpriteVertex {
        position: p,
        color: c,
        uv: [u, vv],
        uv2: [u, vv],
    };
    CoronaQuad {
        actor: a.id,
        vertices: [
            v(l - rv - uv, 0.0, 0.0),
            v(l + rv - uv, 1.0, 0.0),
            v(l + rv + uv, 1.0, 1.0),
            v(l - rv + uv, 0.0, 1.0),
        ],
        draw: SpriteDraw {
            state: DrawState {
                // FB_Translucent: one, inverse source colour.
                blend: Some((BlendFactor::One, BlendFactor::InvSrcColor)),
                stage: StageOp::Modulate,
                fog_color: None,
            },
            blend_subdivisions: false,
            alpha_test: a.alpha_texture,
            alpha_ref: 0,
            z_test: false,
            z_write: false,
            two_sided: true,
            accepts_projectors: false,
            local_to_world: None,
        },
        use_own_final_blend: a.light.is_some_and(|l| l.use_own_final_blend),
    }
}

trait ClampUnit {
    fn clamp_unit(self) -> f32;
}
impl ClampUnit for f32 {
    fn clamp_unit(self) -> f32 {
        if self < 0.0 {
            0.0
        } else if self > 1.0 {
            1.0
        } else {
            self
        }
    }
}

/// The CullDistance test of the scene collector that registers coronas (Engine.dll
/// 0x1eaf00): with `cull_distance > 0` the actor is considered only when
/// `distance * tan(clamp(fov, 10, 85) * pi / 360) <= cull_distance`. Zero disables it.
/// Registration also needs the actor to pass the filter (a corona light passes even when
/// hidden), to lie in a leaf the visibility walk reaches and inside that zone's frustum,
/// and, for non-static actors, a nonzero camera zone number; the host applies those and
/// calls [`CoronaManager::add`]. An actor then stays in the ring and is re-tested every
/// frame whatever its visibility.
pub fn cull_distance_allows(cull_distance: f32, distance: f32, fov_degrees: f32) -> bool {
    if cull_distance <= 0.0 {
        return true;
    }
    let fov = fov_degrees.max(10.0).min(85.0);
    let g = (f64::from(fov) * std::f64::consts::PI / 360.0).tan() as f32;
    distance * g <= cull_distance
}
