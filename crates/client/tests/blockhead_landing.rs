//! Real player motor -> rendered-state sampling -> stock landing policy.
//! Headless; no device/window or original game assets.
use anyhow::Result;
use bri_client::footsteps::{Footsteps, Walk};
use bri_sim::{
    archetype::Archetypes,
    player::{MoveInput, Player, PlayerTuning},
};
use glam::Vec3;
use rapier3d::prelude::*;

#[test]
fn stock_jump_and_falls_sound_once_at_touchdown_for_each_observer_rate() -> Result<()> {
    for (height, jumping) in [(0.05, true), (2.0, false), (8.0, false)] {
        let mut world = bri_physics::new_world();
        world.insert_collider(
            ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            None,
        );
        world.detect_collisions(&(), &());
        let mut p = Player::spawn(
            &mut world,
            1,
            Vec3::new(0.0, height, 0.0),
            PlayerTuning::default(),
        )?;
        let mut states = vec![p.state().clone()];
        for tick in 0..720 {
            p.step(
                &mut world,
                MoveInput {
                    jump: jumping && tick == 60,
                    ..Default::default()
                },
            )?;
            world.step();
            states.push(p.state().clone());
        }
        assert!(states.last().unwrap().grounded);
        let archetypes = Archetypes::default();
        for fps in [30, 60, 144] {
            let mut footsteps = Footsteps::default();
            let mut heard = [0; 2];
            for frame in 0..fps * 6 {
                let state = &states[frame * 120 / fps];
                let local = Walk::from(state);
                let mut remote = local;
                remote.owner = 2;
                remote.feet[0] += 20.0;
                for fall in footsteps.update(1.0 / fps as f32, [local, remote], &archetypes, |_| {
                    "default".into()
                }) {
                    assert_eq!(fall.sound, "bri:foley/blockhead/land_01");
                    assert!(state.grounded, "airborne landing at {fps} fps");
                    let index = usize::from(fall.at[0] > 10.0);
                    assert_eq!(fall.at, if index == 0 { local.feet } else { remote.feet });
                    heard[index] += 1;
                }
            }
            assert_eq!(heard, [1, 1], "height={height}, jump={jumping}, fps={fps}");
        }
    }
    Ok(())
}
