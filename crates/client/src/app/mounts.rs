//! Seats and riders: driven-vehicle prediction and mount poses.
use super::*;

/// Seats and riders: what the local player sits on and how riders are posed.
pub(super) struct Mounts {
    /// Heading of the vehicle the local player rides, last frame.
    pub(super) mount_heading: Option<f32>,
    /// The vehicle seat the local player sat in last frame.
    pub(super) seated_on: Option<(u64, u8)>,
    /// Sat down in a gunner's seat and not yet looking along its turret:
    /// done on the first frame the seat's view is known, which a seat
    /// change's own frame may not be.
    pub(super) takes_turret: bool,
    /// The seat this client's moves are shaped for, sent with them: set once
    /// a new seat's view is in place, so the host reads moves made for the
    /// old seat as the old seat's.
    pub(super) seat_report: Option<bri_sim::session::SeatSince>,
    /// This frame's seat rotation for every mounted player.
    pub(super) rider_rotations: BTreeMap<bri_world::OwnerId, glam::Quat>,
    /// This frame's first-person eye while the local player rides a vehicle
    /// or another player, or sits, from their posed `eye` node.
    pub(super) posed_eye: Option<Vec3>,
    /// The tumble vehicle the local player last started riding.
    pub(super) tumble: Option<u64>,
}

/// Whether `owner` is held in the `sit` pose: `/sit`, or a vehicle seat or
/// a body's mount point whose pose is `sit` (`mountThread`). The host gives
/// such a body the sit's eye, so its rays and name tag start at the drawn head.
pub(crate) fn sit_posed(
    view: &network::View,
    vehicle_assets: &crate::vehicles::VehicleAssets,
    presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
    owner: bri_world::OwnerId,
) -> bool {
    let Some(vitals) = view.vitals.get(&owner) else {
        return false;
    };
    vitals.sitting
        || vitals
            .mounted
            .and_then(|(vehicle, seat)| {
                let info = view.vehicles.get(&vehicle)?;
                let d = vehicle_assets.definition(&info.definition)?;
                Some(d.seats.get(usize::from(seat))?.pose == "sit")
            })
            .unwrap_or(false)
        || vitals
            .ride
            .and_then(|ride| {
                let mount = presented.get(&ride.mount)?;
                let kind = view.archetypes.resolve(mount.archetype);
                Some(kind.mount_points.get(usize::from(ride.seat))?.pose == "sit")
            })
            .unwrap_or(false)
}

impl App {
    /// Predict the vehicle this client drives, as Torque runs the moves of
    /// the object a client controls on that client: the host's own vehicle
    /// code against the collision mirror, corrected from each newer pose.
    /// Player-type mounts the rider controls are predicted the same way.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn predict_driven(
        motion: &mut crate::motion::Motion,
        vehicles: &mut crate::vehicles::ClientVehicles,
        assets: &crate::vehicles::VehicleAssets,
        prefs: &bri_ui::prefs::Prefs,
        faults: &mut crate::cosmetic::CosmeticFaults,
        view: &network::View,
        driven: Option<u64>,
    ) {
        // The host's copy of this driver's steering prefs, which it steers
        // their moves by: predicting with it keeps the two agreeing even
        // before (or without) the host hearing the client's own.
        let wanted = driven.and_then(|id| {
            let info = view.vehicles.get(&id)?;
            let pose = view.vehicle_poses.get(&id)?;
            motion
                .drive_anchor_ready(id, pose.tick, pose.driver_input, pose.passage_frame)
                .then_some(())?;
            let d = assets.definition(&info.definition)?;
            let target = drive_target(info, d, pose.driver_steering.0)?;
            (motion.drive_state.refused.as_ref() != Some(&target)).then_some(())?;
            Some((target, info, pose))
        });
        let steering = steering_in_use(wanted.as_ref().map(|(_, _, pose)| *pose), prefs);
        let prefs = (!steering.0, !steering.1);
        // A new vehicle, a respawn under a new id, a changed definition or
        // scale, or leaving the seat: start again or stop.
        let target = wanted.as_ref().map(|(t, ..)| t.clone());
        if target != motion.drive_state.target
            || target
                .as_ref()
                .is_some_and(|t| motion.driving() != Some(t.id))
        {
            motion.drive_state.target = target;
            let request = wanted.as_ref().map(|(target, info, pose)| {
                let owner = view.owner;
                (
                    target.id,
                    assets.pack().clone(),
                    bri_sim::prediction::DriveSpawn {
                        spawn: bri_vehicles::Spawn {
                            id: bri_vehicles::VehicleId(target.id),
                            owner: bri_vehicles::OwnerId(owner),
                            definition: info.definition.clone(),
                            transform: Default::default(),
                            spawn_id: None,
                            respawn_ticks: None,
                            scale: info.scale,
                        },
                        seat: 0,
                        prefs,
                    },
                    pose.motion(),
                )
            });
            if faults
                .absorb("vehicle prediction", motion.drive(request))
                .is_none()
            {
                // Show the host's poses for this vehicle instead.
                motion.drive_state.refused = motion.drive_state.target.take();
                let _ = motion.drive(None);
            }
        }
        motion.set_drive_prefs(prefs);
        if let Some((_, _, pose)) = wanted {
            let corrected = motion.observe_vehicle(pose);
            if faults.absorb("vehicle prediction", corrected).is_none() {
                motion.drive_state.refused = motion.drive_state.target.take();
                let _ = motion.drive(None);
            }
        }
        if driven.is_none() {
            motion.drive_state.refused = None;
        }
        vehicles.set_predicted(motion.driven_frame());
    }
    /// Pose each spawned horse with the horse rig from its interpolated
    /// frame: body in the brick's colour, dead ones in `death1`.
    pub(super) fn pose_mounts(
        mount_meshes: &mut BTreeMap<u64, crate::avatar::AvatarMesh>,
        avatar_assets: &crate::avatar::AvatarAssets,
        vehicle_assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        animation_time: f64,
        view: &network::View,
    ) -> Result<()> {
        let horses: Vec<_> = view
            .vehicles
            .values()
            .filter(|info| {
                vehicle_assets
                    .definition(&info.definition)
                    .is_some_and(|d| d.family == bri_vehicles::Family::Horse)
            })
            .cloned()
            .collect();
        mount_meshes.retain(|id, _| horses.iter().any(|h| h.id == *id));
        for info in horses {
            let Some(frame) = vehicles.frame(info.id).cloned() else {
                continue;
            };
            let mut appearance = avatar_assets.package.defaults.clone();
            let color = info.color.map_or([1.0; 4], |[r, g, b, _]| [r, g, b, 1.0]);
            appearance.colors.insert("chest".into(), color);
            if mount_meshes
                .get(&info.id)
                .is_none_or(|m| m.appearance != appearance)
            {
                let mesh = avatar_assets.horse_mesh(appearance)?;
                mount_meshes.insert(info.id, mesh);
            }
            let forward = frame.rotation * Vec3::NEG_Z;
            let state = bri_sim::player::PlayerState {
                owner: 1,
                feet: frame.position.to_array(),
                velocity: frame.velocity.to_array(),
                yaw: forward.x.atan2(-forward.z),
                pitch: 0.0,
                head_yaw: 0.0,
                grounded: frame.velocity.y.abs() < 0.5,
                crouched: false,
                jetting: false,
                jump: Default::default(),
                archetype: bri_sim::player_types::PlayerType::Horse.archetype(),
                scale: info.scale,
                energy: 0.0,
                speed_scale: 1.0,
                tick: Default::default(),
                tether: None,
            };
            let input = crate::avatar::AvatarAnimationInput {
                dead: info.destroyed,
                ..Default::default()
            };
            let mesh = mount_meshes.get_mut(&info.id).unwrap();
            mesh.instanced = true;
            mesh.straddle = vehicles.straddle(info.id).copied();
            mesh.pose_with_animation(avatar_assets, &state, animation_time, &input)?;
        }
        Ok(())
    }
}
