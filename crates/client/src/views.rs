//! Every view of the world drawn this frame besides the player's own: the
//! mirrors' and portals' planes and the environment probe's faces. They all
//! see the same world from their own eye, so they share one path: one list
//! here, and one per-view prepare of what each eye picks for itself (sprites
//! sorted and turned to it, plants, weather, Add-On layers and skins). The world
//! around each eye (terrain tiles, effect lights) is chosen from all of
//! them together, since every view draws it from the same buffers.
//!
//! Only the player's own view leaves out what a first-person player hides
//! from themselves (their own jets, their own body, third-person flares);
//! every view here sees the player from outside.

use anyhow::Result;
use glam::{Mat4, Vec3};

/// One other view: its render-target slot (1 and up; 0 is the player's) and
/// its camera.
#[derive(Clone, Copy, Debug)]
pub struct OtherView {
    pub view: usize,
    pub view_projection: Mat4,
    pub eye: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    /// How the eye moves, for weather streaks.
    pub velocity: Vec3,
}

/// The player's camera, which mirror planes reflect.
pub struct PlayerCamera {
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub velocity: Vec3,
}

/// The mirror and portal planes (views 1 and up, in plan order), then the
/// environment probe's faces.
pub fn other_views(
    planes: &[bri_render::reflection::PlannedPlane],
    probe: &[bri_render::environment_probe::ProbeView],
    player: &PlayerCamera,
) -> Vec<OtherView> {
    let planes = planes.iter().enumerate().map(|(i, plane)| {
        let turn = |v: Vec3| plane.reflect_direction(v);
        OtherView {
            view: 1 + i,
            view_projection: plane.view_projection,
            eye: plane.eye,
            forward: turn(player.forward),
            right: turn(player.right),
            up: turn(player.up),
            velocity: turn(player.velocity),
        }
    });
    let faces = probe.iter().map(|face| OtherView {
        view: face.view,
        view_projection: face.view_projection,
        eye: face.eye,
        forward: face.forward,
        right: face.right,
        up: face.up,
        velocity: Vec3::ZERO,
    });
    planes.chain(faces).collect()
}

/// The player's eye first, then each other view's (duplicates once): every
/// place the world is seen from this frame.
pub fn eyes(player: Vec3, others: &[OtherView]) -> Vec<Vec3> {
    let mut eyes = vec![player];
    for v in others {
        if !eyes.contains(&v.eye) {
            eyes.push(v.eye);
        }
    }
    eyes
}

/// What each view prepares for itself.
pub struct Layers<'a> {
    pub effects: [&'a bri_fx_runtime::EffectsWorld; 3],
    pub sprites: &'a mut bri_fx_runtime::gpu::EffectsRenderer,
    pub foliage: &'a mut crate::foliage::ClientFoliage,
    pub weather: &'a bri_weather::WeatherWorld,
    pub drops: &'a mut bri_weather::gpu::WeatherRenderer,
    pub client_code: &'a mut crate::client_code::ClientCode,
    pub item_skins: &'a mut crate::item_skins::ItemSkins,
    /// The fog's start and end, which bound what plants each view draws.
    pub fog: (f32, f32),
}

impl Layers<'_> {
    /// Prepare `view`'s sprites, plants, weather and Add-On layers from its
    /// own eye. Its effects come from
    /// [`bri_fx_runtime::EffectsWorld::snapshot_in_other_view`], so it shows
    /// the first-person player's own jets.
    pub fn prepare(
        &mut self,
        frame: &crate::platform::RenderContext<'_>,
        v: &OtherView,
    ) -> Result<()> {
        let camera = bri_fx_runtime::Camera {
            view_projection: v.view_projection,
            position: v.eye,
            right: v.right,
            up: v.up,
        };
        let [world, weapon, actor] = self.effects.map(|e| e.snapshot_in_other_view(&camera));
        let (sprites, _) = crate::app::combine_effect_frames(world, [weapon, actor], &[v.eye]);
        self.sprites
            .prepare_view(frame.device, frame.queue, v.view, &camera, &sprites)?;
        let (fog_start, fog_end) = self.fog;
        self.foliage.prepare_view(
            frame,
            v.view,
            &bri_foliage::Camera {
                position: v.eye,
                right: v.right,
                view_projection: v.view_projection,
                visible_distance: fog_end.max(1.),
            },
            fog_start,
            fog_end.max(fog_start + 0.001),
        )?;
        let drops = self.weather.snapshot_from(&bri_weather::CameraState {
            position: v.eye,
            forward: v.forward,
            right: v.right,
            up: v.up,
            velocity: v.velocity,
        });
        self.drops
            .prepare_view(frame.device, frame.queue, v.view, v.view_projection, &drops)?;
        self.client_code
            .prepare_view(frame.device, frame.queue, v.view, v.view_projection, v.eye);
        self.item_skins
            .prepare_view(frame.device, frame.queue, v.view, v.view_projection, v.eye);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_eye_once_the_players_first() {
        let view = |view, eye| OtherView {
            view,
            view_projection: Mat4::IDENTITY,
            eye,
            forward: Vec3::NEG_Z,
            right: Vec3::X,
            up: Vec3::Y,
            velocity: Vec3::ZERO,
        };
        let probe = Vec3::new(4., 1., 0.);
        let others = [
            view(1, Vec3::new(0., 1., -10.)),
            view(9, probe),
            view(10, probe),
        ];
        assert_eq!(
            eyes(Vec3::ZERO, &others),
            vec![Vec3::ZERO, Vec3::new(0., 1., -10.), probe]
        );
    }
}
