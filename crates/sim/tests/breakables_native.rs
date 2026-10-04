//! The converted Bedroom and Kitchen missions keep v20's glass shapes.
use bri_sim::map::NativeMap;
use std::path::PathBuf;

fn content() -> PathBuf {
    std::env::var_os("BRI_CONTENT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

/// (datablock, sound, indestructable) per breakable, in scene order.
fn shapes(map: &str) -> Vec<(String, Option<String>, bool, usize)> {
    let native = NativeMap::load(
        &bri_package::testing::pack_dir(&content(), "map_bundle"),
        map,
    )
    .unwrap();
    native
        .breakables
        .iter()
        .map(|b| {
            assert!(b.colliders.end <= native.colliders.len());
            assert!(b.center.is_finite());
            assert_eq!(b.explosion.as_deref(), Some("glassExplosion"));
            (
                b.datablock.clone(),
                b.sound.clone(),
                b.indestructable,
                b.colliders.len(),
            )
        })
        .collect()
}

#[test]
#[ignore = "requires generated v20 content (map-bundle-017, or BRI_CONTENT)"]
fn bedroom_windows_and_bulb_break_but_kitchen_windows_do_not() {
    for map in [
        "v20/add-ons/map_bedroom/bedroom.mis",
        "v20/add-ons/map_bedroomdark/bedroomdark.mis",
    ] {
        let shapes = shapes(map);
        eprintln!("{map}: {shapes:?}");
        assert_eq!(shapes.len(), 5);
        let windows: Vec<_> = shapes.iter().filter(|s| s.0 == "glassA").collect();
        assert_eq!(windows.len(), 4);
        assert!(
            windows
                .iter()
                .all(|s| { s.1.as_deref() == Some("glassExplosionSound") && !s.2 && s.3 > 0 })
        );
        // The bulb has no explosionSound: it shatters silently in v20.
        let bulb = shapes.iter().find(|s| s.0 == "lightBulbA").unwrap();
        assert!(bulb.1.is_none() && !bulb.2 && bulb.3 > 0);
    }
    for map in [
        "v20/add-ons/map_kitchen/kitchen.mis",
        "v20/add-ons/map_kitchendark/kitchendark.mis",
    ] {
        let shapes = shapes(map);
        eprintln!("{map}: {shapes:?}");
        assert_eq!(shapes.len(), 11);
        assert!(shapes.iter().filter(|s| s.0 == "glassA").all(|s| s.2));
        let lights: Vec<_> = shapes
            .iter()
            .filter(|s| s.0 == "fluorescentLight")
            .collect();
        assert_eq!(lights.len(), 4);
        assert!(lights.iter().all(|s| !s.2 && s.1.is_none()));
    }
    for map in [
        "v20/add-ons/map_slate/slate.mis",
        "v20/add-ons/map_skylands/skylands.mis",
    ] {
        assert!(shapes(map).is_empty());
    }
}

#[test]
#[ignore = "requires generated v20 content (map-bundle-017, or BRI_CONTENT)"]
fn a_player_thrown_at_each_bedroom_shape_hits_its_glass_hard_enough() {
    use bri_sim::{
        player::{MoveInput, Player, PlayerTuning},
        simulation::Simulation,
    };
    use glam::Vec3;
    let native = NativeMap::load(
        &bri_package::testing::pack_dir(&content(), "map_bundle"),
        "v20/add-ons/map_bedroom/bedroom.mis",
    )
    .unwrap();
    let breakables = native.breakables.clone();
    let mut sim = Simulation::new(
        bri_world::World::new("Bedroom".into(), "bedroom".into(), vec![[1.0; 4]; 2]),
        bri_sim::definitions::Definitions {
            entries: Default::default(),
        },
        native.colliders,
    )
    .unwrap();
    for shape in &breakables {
        let mut smashed = false;
        // Sideways into the windows. The bulb sits in its lamp's shade,
        // which leaves room for a player only just beside it.
        let starts = [4.0, 2.5]
            .into_iter()
            .flat_map(|d| [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z].map(|v| (v, d)));
        for (direction, distance) in starts {
            let tuning = PlayerTuning::default();
            let feet = shape.center - direction * distance - Vec3::Y;
            let Ok(mut player) = Player::spawn(&mut sim.physics, 1, feet, tuning) else {
                continue;
            };
            sim.physics.detect_collisions(&(), &());
            player.push(direction * 40.0);
            for _ in 0..30 {
                let motion = player.step(&mut sim.physics, MoveInput::default()).unwrap();
                sim.physics.step();
                if motion.hits.iter().any(|(handle, speed)| {
                    *speed > 30.0
                        && sim
                            .map_collider_index(*handle)
                            .is_some_and(|c| shape.colliders.contains(&c))
                }) {
                    smashed = true;
                }
            }
            player.despawn(&mut sim.physics);
            if smashed {
                break;
            }
        }
        assert!(
            smashed,
            "{} at {} was never hit",
            shape.datablock, shape.center
        );
    }
}
