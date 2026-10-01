//! v20's third-person camera for a vehicle's riders.
//!
//! In third person a mounted player asks its mount for the camera
//! (`Player::getCameraTransform` 0x5ab7d0 hands off to
//! `Vehicle::getCameraTransform` 0x56cc10). Blockland rewrote Torque's
//! version: with `cameraRoll` off, as on every stock vehicle, the camera
//! stays level behind the vehicle's heading instead of following the
//! driver's look, rises with its distance and looks down by `cameraTilt`.
//! In first person riders see from their eye node through the seat; the rider
//! of a player-type mount sees from its mount node ([`driver_eye`]).
use anyhow::{Result, ensure};
use glam::{Quat, Vec2, Vec3};

/// The start of the camera ray sits this far above the vehicle's box center.
const RAY_LIFT: f32 = 2.0;
/// The ray reaches a tenth past the camera so a wall just behind it eases
/// the camera in rather than popping it.
const RAY_REACH: f32 = 1.1;
/// A hit backs the camera off by this share of `1 + normal . ray`: next to
/// nothing from a wall faced head on, most of it along a grazed one.
const BACK_OFF: f32 = 0.8;
const MIN_BACK_OFF_SHARE: f32 = 0.05;

/// Where the driver's camera sits and how it looks (yaw, pitch).
///
/// `position`/`rotation` is the vehicle, `bounds_center` its shape box
/// center in vehicle space, and `pos` how far the view has slid out to
/// third person. `free_look` is the head's turn while Free Look is held: only
/// then does v20 build the camera from the rider's head, turned by it and
/// pitched by `cameraTilt`, before levelling it. `solid` casts a ray from the
/// first point to the second against terrain, interiors and colliding bricks
/// (mask 0x300000c), returning the hit's distance and surface normal.
#[allow(clippy::too_many_arguments)]
pub fn driver_view(
    position: Vec3,
    rotation: Quat,
    bounds_center: Vec3,
    camera: &bri_vehicles::schema::VehicleCamera,
    free_look: Option<f32>,
    pos: f32,
    solid: impl FnOnce(Vec3, Vec3) -> Result<Option<(f32, Vec3)>>,
) -> Result<(Vec3, f32, f32)> {
    ensure!(
        position.is_finite()
            && rotation.is_finite()
            && bounds_center.is_finite()
            && camera.max_dist.is_finite()
            && camera.max_dist > 0.0
            && camera.offset.is_finite()
            && camera.tilt.is_finite()
            && pos.is_finite(),
        "Invalid vehicle camera"
    );
    let local = match free_look {
        Some(yaw) => {
            let pitch = -camera.tilt;
            Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            )
        }
        None => Vec3::NEG_Z,
    };
    // Level the eye's forward; a vehicle standing on its nose or tail
    // falls back to the roof's heading.
    let forward = rotation * local;
    let up = rotation * Vec3::Y;
    let heading = Vec3::new(forward.x, 0.0, forward.z)
        .try_normalize()
        .or_else(|| Vec3::new(-up.x, 0.0, -up.z).try_normalize())
        .unwrap_or(Vec3::NEG_Z);
    // `cameraMinDist` is 0 on every stock vehicle (the data default).
    let center = position + rotation * bounds_center;
    let back = center - heading * (camera.max_dist * pos.clamp(0.0, 1.0));
    // The camera's height over the vehicle's origin grows with its level
    // distance from it, reaching `cameraOffset` at `cameraMaxDist`.
    let rise = Vec2::new(back.x - position.x, back.z - position.z).length() / camera.max_dist;
    let end = Vec3::new(back.x, position.y + camera.offset * rise, back.z);
    let look = Vec3::new(heading.x, -camera.tilt, heading.z).normalize();
    let (yaw, pitch) = (heading.x.atan2(-heading.z), look.y.clamp(-1.0, 1.0).asin());
    let start = center + Vec3::Y * RAY_LIFT;
    let reach = end - start;
    let length = reach.length();
    if length < 0.001 {
        return Ok((end, yaw, pitch));
    }
    let direction = reach / length;
    let eye = match solid(start, start + reach * RAY_REACH)? {
        Some((distance, normal)) => {
            let back_off =
                -direction * (1.0 + normal.dot(direction)).max(MIN_BACK_OFF_SHARE) * BACK_OFF;
            if distance < length {
                start + direction * distance + back_off
            } else {
                // Only the extra tenth hit: ease in as the wall nears.
                let share =
                    (1.0 - (distance - length) / (length * (RAY_REACH - 1.0))).clamp(0.0, 1.0);
                end + back_off * share
            }
        }
        None => end,
    };
    Ok((eye, yaw, pitch))
}

/// Where the rider controlling a player-type mount (horse, rowboat, cannon)
/// sees from in first person.
///
/// `Player::getCameraTransform` (0x5ab7d0) at `pos` 0, for a rider whose
/// control object is the `PlayerObjectType` (0x4000) mount it sits on: the
/// seat's mount node position plus the rider's posed `eye` node, both in the
/// mount's frame (the mount node's own rotation is not applied to the eye),
/// placed by the mount's transform. `eye` is the rider's `eye` node in its
/// own shape space, already scaled by the rider's scale. Everyone else,
/// including every rider of a vehicle (`VehicleObjectType`, 0x10000), sees
/// from their own posed `eye` node through the seat
/// (`Player::getRenderEyeTransform` 0x5aafa0).
pub fn driver_eye(position: Vec3, rotation: Quat, mount_node: Vec3, eye: Vec3) -> Vec3 {
    position + rotation * (mount_node + eye)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn the_drivers_eye_adds_the_posed_eye_to_the_mount_node_in_the_vehicle_frame() {
        let turn = Quat::from_rotation_y(FRAC_PI_2);
        let eye = driver_eye(
            Vec3::new(10.0, 1.0, 0.0),
            turn,
            Vec3::new(0.0, 0.5, -1.0),
            Vec3::new(0.0, 1.2, -0.2),
        );
        assert!(eye.distance(Vec3::new(8.8, 2.7, 0.0)) < 1e-5, "{eye}");
    }

    /// The Jeep's authored camera.
    fn jeep() -> bri_vehicles::schema::VehicleCamera {
        bri_vehicles::schema::VehicleCamera {
            max_dist: 13.0,
            offset: 7.5,
            tilt: 0.4,
            lag: 0.0,
            decay: 0.75,
        }
    }
    fn open(_: Vec3, _: Vec3) -> Result<Option<(f32, Vec3)>> {
        Ok(None)
    }

    #[test]
    fn the_camera_sits_level_behind_the_heading_and_looks_down_by_the_tilt() {
        let at = Vec3::new(10.0, 5.0, -3.0);
        let (eye, yaw, pitch) =
            driver_view(at, Quat::IDENTITY, Vec3::ZERO, &jeep(), None, 1.0, open).unwrap();
        assert!(eye.distance(at + Vec3::new(0.0, 7.5, 13.0)) < 1e-4, "{eye}");
        assert!(yaw.abs() < 1e-6);
        assert!((pitch - (-0.4f32).atan()).abs() < 1e-6);
    }

    #[test]
    fn pitch_and_roll_of_the_vehicle_never_tilt_the_camera() {
        let at = Vec3::new(0.0, 2.0, 0.0);
        let level = driver_view(at, Quat::IDENTITY, Vec3::ZERO, &jeep(), None, 1.0, open).unwrap();
        for rotation in [
            Quat::from_rotation_x(0.6),
            Quat::from_rotation_x(-1.2),
            Quat::from_rotation_z(0.9),
            Quat::from_rotation_z(0.9) * Quat::from_rotation_x(0.3),
        ] {
            let (eye, yaw, pitch) =
                driver_view(at, rotation, Vec3::ZERO, &jeep(), None, 1.0, open).unwrap();
            assert!((pitch - level.2).abs() < 1e-5, "pitch {pitch}");
            // Rolling alone keeps the heading; pitching keeps it too.
            if rotation == Quat::from_rotation_x(0.6) || rotation == Quat::from_rotation_z(0.9) {
                assert!(
                    yaw.abs() < 1e-5 && eye.distance(level.0) < 1e-4,
                    "{eye} {yaw}"
                );
            }
            assert!((eye.y - (at.y + 7.5)).abs() < 1e-4, "height {eye}");
        }
    }

    #[test]
    fn the_camera_follows_the_vehicle_heading_not_the_mouse() {
        let turned = Quat::from_rotation_y(-FRAC_PI_2);
        let (eye, yaw, _) =
            driver_view(Vec3::ZERO, turned, Vec3::ZERO, &jeep(), None, 1.0, open).unwrap();
        // Facing +X (yaw right), the camera is 13 back along -X.
        assert!((yaw - FRAC_PI_2).abs() < 1e-5);
        assert!(eye.distance(Vec3::new(-13.0, 7.5, 0.0)) < 1e-4, "{eye}");
    }

    #[test]
    fn free_look_swings_the_camera_around_the_vehicle() {
        let (eye, yaw, pitch) = driver_view(
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ZERO,
            &jeep(),
            Some(FRAC_PI_2),
            1.0,
            open,
        )
        .unwrap();
        assert!((yaw - FRAC_PI_2).abs() < 1e-5);
        assert!(eye.distance(Vec3::new(-13.0, 7.5, 0.0)) < 1e-4, "{eye}");
        assert!(
            (pitch - (-0.4f32).atan()).abs() < 1e-6,
            "the head pitch is levelled away"
        );
    }

    #[test]
    fn the_camera_rises_as_it_slides_out() {
        let half = driver_view(
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ZERO,
            &jeep(),
            None,
            0.5,
            open,
        )
        .unwrap()
        .0;
        assert!(half.distance(Vec3::new(0.0, 3.75, 6.5)) < 1e-4, "{half}");
        let first = driver_view(
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ZERO,
            &jeep(),
            None,
            0.0,
            open,
        )
        .unwrap()
        .0;
        assert!(first.distance(Vec3::ZERO) < 1e-4);
    }

    #[test]
    fn the_box_center_moves_the_camera_but_the_rise_is_from_the_origin() {
        let center = Vec3::new(0.0, 1.5, -1.0);
        let (eye, ..) =
            driver_view(Vec3::ZERO, Quat::IDENTITY, center, &jeep(), None, 1.0, open).unwrap();
        // 13 behind the center is 12 behind the origin: 12/13 of the offset.
        assert!(
            eye.distance(Vec3::new(0.0, 7.5 * 12.0 / 13.0, 12.0)) < 1e-4,
            "{eye}"
        );
    }

    #[test]
    fn a_wall_pulls_the_camera_in_and_a_wall_just_behind_eases_it() {
        let start = Vec3::new(0.0, 2.0, 0.0);
        let end = Vec3::new(0.0, 7.5, 13.0);
        let length = start.distance(end);
        let direction = (end - start) / length;
        // A wall facing the ray head on stops the camera just in front of it.
        let (eye, ..) = driver_view(
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ZERO,
            &jeep(),
            None,
            1.0,
            |a, b| {
                assert_eq!(a, start);
                assert!(b.distance(start + (end - start) * 1.1) < 1e-4);
                Ok(Some((length * 0.5, -direction)))
            },
        )
        .unwrap();
        let hit = start + direction * length * 0.5;
        assert!(eye.distance(hit - direction * 0.05 * 0.8) < 1e-4, "{eye}");
        // A grazed floor backs it off further.
        let (eye, ..) = driver_view(
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ZERO,
            &jeep(),
            None,
            1.0,
            |_, _| Ok(Some((length * 0.5, Vec3::Y))),
        )
        .unwrap();
        let share = 1.0 + Vec3::Y.dot(direction);
        assert!(eye.distance(hit - direction * share * 0.8) < 1e-4, "{eye}");
        // A wall in the tenth past the camera: halfway in, half the back-off.
        let (eye, ..) = driver_view(
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ZERO,
            &jeep(),
            None,
            1.0,
            |_, _| Ok(Some((length * 1.05, -direction))),
        )
        .unwrap();
        assert!(
            eye.distance(end - direction * 0.05 * 0.8 * 0.5) < 1e-4,
            "{eye}"
        );
    }

    #[test]
    fn a_vehicle_on_its_nose_still_has_a_heading() {
        let (eye, yaw, pitch) = driver_view(
            Vec3::ZERO,
            Quat::from_rotation_x(-FRAC_PI_2),
            Vec3::ZERO,
            &jeep(),
            None,
            1.0,
            open,
        )
        .unwrap();
        assert!(eye.is_finite() && yaw.is_finite() && pitch.is_finite());
    }

    /// Every seat of the stock vehicles and the default Stunt Plane, seen
    /// from the rider's posed `eye` node as v20 places it, next to the old
    /// fixed 1.6 above the seat. Run with `--ignored --nocapture` for the table.
    #[test]
    #[ignore = "requires the converted avatar and vehicle packs"]
    fn every_seats_first_person_eye_comes_from_the_posed_eye_node() -> Result<()> {
        use crate::avatar::{AvatarAnimationInput, AvatarAssets};
        use bri_vehicles::schema::{Pack, SeatRole};
        use std::path::Path;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let assets = AvatarAssets::load(&root.join("content/avatar-pack-002"))?;
        let mut definitions =
            Pack::load(root.join("content/vehicles-pack-012/vehicles.json"))?.definitions;
        definitions.extend(
            Pack::load(root.join("packages/imported/vehicle_stunt_plane/assets/vehicles.json"))?
                .definitions,
        );
        let mut seats = 0;
        for d in &definitions {
            for (index, seat) in d.seats.iter().enumerate() {
                let node = Vec3::from(seat.transform.position);
                let turn = Quat::from_array(seat.transform.rotation).normalize();
                let mut rider = bri_sim::player::PlayerState {
                    owner: 1,
                    feet: node.to_array(),
                    velocity: [0.0; 3],
                    yaw: 0.0,
                    pitch: 0.0,
                    head_yaw: 0.0,
                    grounded: true,
                    crouched: false,
                    jetting: false,
                    jump: Default::default(),
                    archetype: Default::default(),
                    scale: 1.0,
                    energy: 100.0,
                    speed_scale: 1.0,
                    tick: Default::default(),
                };
                rider.yaw = {
                    let forward = turn * Vec3::NEG_Z;
                    forward.x.atan2(-forward.z)
                };
                let mut body = assets.mesh(assets.package.defaults.clone())?;
                // `mountThread`: held from the first frame.
                for time in [0.0, 1.0] {
                    body.pose_with_animation(
                        &assets,
                        &rider,
                        time,
                        &AvatarAnimationInput {
                            mount_rotation: Some(turn),
                            sitting: seat.pose == "sit",
                            ..Default::default()
                        },
                    )?;
                }
                let eye_node = body.model_node(&assets, "Eye").expect("eye node");
                let through_seat = body.world_node(&assets, "Eye").expect("eye node");
                // The body sits on the mount node's full transform.
                let expected = node + turn * eye_node.w_axis.truncate();
                assert!(
                    through_seat.w_axis.truncate().distance(expected) < 1e-4,
                    "{} seat {index}",
                    d.datablock
                );
                let driver = d.seat_role(index) == SeatRole::Actor;
                let eye = if driver {
                    driver_eye(Vec3::ZERO, Quat::IDENTITY, node, eye_node.w_axis.truncate())
                } else {
                    through_seat.w_axis.truncate()
                };
                let old = node + turn * Vec3::Y * 1.6;
                println!(
                    "{:<26} seat {index} {:<11} {:<4} node {:>6.3} {:>6.3} {:>6.3}  v20 eye {:>6.3} {:>6.3} {:>6.3}  old {:>6.3} {:>6.3} {:>6.3}",
                    d.datablock,
                    format!("{:?}", d.seat_role(index)),
                    seat.pose,
                    node.x,
                    node.y,
                    node.z,
                    eye.x,
                    eye.y,
                    eye.z,
                    old.x,
                    old.y,
                    old.z,
                );
                assert!(
                    eye.is_finite() && eye.y > node.y,
                    "{} seat {index}",
                    d.datablock
                );
                seats += 1;
            }
        }
        assert!(seats >= 30, "{seats} seats");
        Ok(())
    }
}
