//! The camera: whose eyes, which mode, where it looks.
use super::*;

/// What the camera shows beyond the controls: observer and rendered eyes, the drawn controls, crosshair and wheels.
pub(super) struct ViewState {
    /// Whether the UI was last told to hide the crosshair.
    pub(super) crosshair_hidden: bool,
    /// The held tool's `wheel` command: while its trigger is held, it takes
    /// the mouse wheel (`UiUpdate::ToolWheel`).
    pub(super) tool_wheel: Option<String>,
    /// The UI sends the wheel to an Add-On's zooming orbit camera.
    pub(super) camera_wheel: bool,
    /// The aim takes the mouse wheel (`Controls::aim_takes_wheel`).
    pub(super) aim_wheel: bool,
    /// The scope overlay shown (`ItemUi::scope_overlay`).
    pub(super) scope_overlay: Option<(u64, f32)>,
    /// Where the admin, spy or death camera was last drawn from, reported
    /// to the server as the camera's transform.
    pub(super) observer_eye: Option<Vec3>,
    /// The camera the last rendered frame was drawn from (eye, yaw, pitch).
    pub(super) rendered_camera: Option<(Vec3, f32, f32)>,
    /// Which driven vehicle is predicted, and one whose prediction failed.
    /// The rendered camera's roll about its forward axis (a rider's
    /// first-person view tilting with the seat), radians.
    pub(super) rendered_roll: f32,
    /// The controls as the last tick sampled them. The tick poses the body,
    /// the held items and the eye from these; the redraw must draw the camera
    /// from them too. Mouse motion the window loop delivers between the tick
    /// and the redraw would otherwise turn the camera by an amount the body
    /// never saw, a different amount each frame. Torque draws the control
    /// object and its camera from one move per frame (`Player::getRenderEyeTransform`,
    /// 0x5aafa0, places both the first-person camera and the mounted images).
    pub(super) drawn_controls: Option<Controls>,
}

impl App {
    pub(crate) fn driver_camera_ray(
        from: Vec3,
        to: Vec3,
        passages: &bri_content::passage::Passages,
        collision: Option<&bri_sim::prediction::CollisionMirror>,
        mut solid: impl FnMut(Vec3, Vec3) -> Result<Option<(f32, Vec3)>>,
    ) -> Result<(Option<(f32, Vec3)>, crate::portal_view::Through)> {
        crate::portal_view::ray(from, to, passages, |from, to| {
            let nearby = camera_segment_near_portal(passages, from, to);
            match collision.filter(|_| nearby) {
                Some(mirror) => {
                    mirror.portal_camera_hit(from, (from - to).normalize(), from.distance(to))
                }
                None => solid(from, to),
            }
        })
    }

    /// `shot.kick`: shake this player's own view when they shoot, and the
    /// view of anyone within a kick's `radius` of another player's shot,
    /// seen from the shot itself (a hitscan tracer, or a new projectile), so
    /// the kick costs nothing on the wire. One kick per hand per frame.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn view_kick(
        shot_kicks: &mut Vec<(u64, u8)>,
        kick_seen: &mut Option<u64>,
        actor_effects: &mut crate::actor_effects::ActorEffects,
        pack: &bri_weapons::Pack,
        weapons: &bri_sim::session::WeaponView,
        owner: bri_world::OwnerId,
        muzzle: impl Fn(u64, u8) -> Option<Vec3>,
        seed: u64,
    ) {
        let shots =
            crate::actor_effects::new_shots(&std::mem::take(shot_kicks), kick_seen, weapons);
        for shot in shots {
            let Some(kick) = weapons
                .images
                .get(&shot.actor)
                .and_then(|images| images.iter().find(|m| m.hand == shot.hand))
                .and_then(|m| pack.images.get(&m.image)?.shot.as_ref()?.kick)
            else {
                continue;
            };
            let seed = seed ^ shot.actor.rotate_left(8) ^ u64::from(shot.hand);
            if shot.actor == owner {
                actor_effects.kick(kick, seed);
            } else if let Some(at) = shot.from.or_else(|| muzzle(shot.actor, shot.hand)) {
                actor_effects.kick_near(kick, at, seed);
            }
        }
    }
    /// The authoritative local player is alive (or not yet known).
    /// Watching the game as a spectator: dead with a rule holding the
    /// respawn, or under a camera a rule gave (free, a point, a path).
    pub(super) fn spectating(&self) -> bool {
        self.network_view()
            .and_then(|v| v.vitals.get(&v.owner))
            .is_some_and(|v| {
                (!v.alive && v.respawn_held)
                    || matches!(
                        v.control,
                        bri_sim::session::ControlObject::Observer
                            | bri_sim::session::ControlObject::Point
                            | bri_sim::session::ControlObject::Path
                            | bri_sim::session::ControlObject::Orbit {
                                body: bri_sim::session::OrbitBody::Frozen,
                                ..
                            }
                    )
            })
    }
    pub(super) fn local_alive(&self) -> bool {
        self.network_view()
            .and_then(|v| v.vitals.get(&v.owner))
            .is_none_or(|v| v.alive)
    }
    /// The chase camera for a gunner seat with no turret player to look
    /// through: distance, pivot above the vehicle and downward view tilt.
    pub(super) fn chase_camera(
        assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        view: &network::View,
    ) -> Option<(f32, Vec3, f32)> {
        let (vehicle, _) = view.vitals.get(&view.owner)?.mounted?;
        let info = view.vehicles.get(&vehicle)?;
        let camera = &assets.definition(&info.definition)?.camera;
        let frame = vehicles.frame(vehicle)?;
        Some((
            camera.max_dist.clamp(1.0, 40.0),
            frame.position + Vec3::Y * camera.offset,
            camera.tilt,
        ))
    }
    /// `Player::getCameraTransform` (blocklandv20.exe 0x5ab7d0) on foot, as
    /// a horse or riding as a passenger: the pivot is the middle of the
    /// standing box over `feet` plus `cameraVerticalOffset` (0.75 while
    /// sliding in), the view is pitched down by `cameraTilt`, and the camera
    /// sits `cameraMaxDist` back along that tilted view. Box, offset and
    /// distance scale with the player. A package archetype keeps its own
    /// `camera_distance`.
    pub(super) fn player_camera(
        assets: &crate::vehicles::VehicleAssets,
        archetypes: &bri_sim::archetype::Archetypes,
        local: &bri_sim::player::PlayerState,
        feet: Vec3,
        pos: f32,
    ) -> (f32, Vec3, f32) {
        let horse = archetypes.resolve(local.archetype).look.is_horse();
        let (max_dist, offset, tilt) = match assets.definition("v20.vehicle.horsearmor") {
            Some(d) if horse => (d.camera.max_dist, d.camera.offset, d.camera.tilt),
            _ => (
                archetypes.resolve(local.archetype).look.camera_distance,
                PLAYER_CAMERA.1,
                PLAYER_CAMERA.2,
            ),
        };
        let scale = local.scale;
        let stand_height = archetypes.tuning(local.archetype, scale).stand_height;
        pivot_camera(stand_height, scale, (max_dist, offset, tilt), feet, pos)
    }
    /// The local first-person eye: the rider's while mounted, else the
    /// smoothed predicted eye.
    pub(super) fn local_eye(&self) -> Option<Vec3> {
        self.mounts.rider_eye.or(self.motion.local_eye())
    }
    /// Players and vehicles as drawn this frame, as boxes that shove
    /// client-only bodies (debris, Add-On bodies). Vehicle ids have the top
    /// bit set.
    pub(super) fn pushers(
        presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
        view: &network::View,
        vehicles: &crate::vehicles::ClientVehicles,
        vehicle_assets: &crate::vehicles::VehicleAssets,
    ) -> Vec<crate::local_physics::Pusher> {
        let mut pushers: Vec<_> = presented
            .iter()
            .map(|(owner, p)| {
                let t = view.archetypes.tuning(p.archetype, p.scale);
                let height = if p.crouched {
                    t.crouch_height
                } else {
                    t.stand_height
                };
                crate::local_physics::Pusher {
                    id: *owner,
                    center: Vec3::from(p.feet) + Vec3::Y * height * 0.5,
                    rotation: glam::Quat::IDENTITY,
                    half: Vec3::new(t.width * 0.5, height * 0.5, t.width * 0.5),
                }
            })
            .collect();
        for (id, info) in &view.vehicles {
            let (Some(frame), Some(d)) = (
                vehicles.frame(*id),
                vehicle_assets.definition(&info.definition),
            ) else {
                continue;
            };
            let (min, max) = (Vec3::from(d.bounds_min), Vec3::from(d.bounds_max));
            pushers.push(crate::local_physics::Pusher {
                id: id | 1 << 63,
                center: frame.position + frame.rotation * ((min + max) * 0.5),
                rotation: frame.rotation,
                half: (max - min) * 0.5,
            });
        }
        pushers
    }
    /// The local rider's first-person eye, from their posed `eye` node
    /// (`Player::getCameraTransform` at `pos` 0, blocklandv20.exe 0x5ab7d0).
    /// The rider controlling a player-type mount (a `PlayerObjectType`
    /// mount, 0x5ab856) sees from the seat's mount node plus the eye node in
    /// the mount's frame; every other rider, including a vehicle's driver,
    /// from the eye node through their seat (`getRenderEyeTransform`).
    /// `None` on foot.
    pub(super) fn rider_eye(
        avatars: &BTreeMap<bri_world::OwnerId, crate::avatar::AvatarMesh>,
        avatar_assets: &crate::avatar::AvatarAssets,
        vehicle_assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        view: &network::View,
        local: &bri_sim::player::PlayerState,
    ) -> Option<Vec3> {
        let vitals = view.vitals.get(&view.owner)?;
        if vitals.mounted.is_none() && vitals.ride.is_none() {
            return None;
        }
        let avatar = avatars.get(&view.owner)?;
        let driving = vitals.mounted.and_then(|(vehicle, seat)| {
            let info = view.vehicles.get(&vehicle)?;
            let d = vehicle_assets.definition(&info.definition)?;
            let seat = usize::from(seat);
            (d.seat_role(seat) == SeatRole::Actor).then_some(())?;
            Some((vehicles.frame(vehicle)?, d.seats.get(seat)?))
        });
        let eye = match driving {
            Some((frame, seat)) => crate::vehicle_camera::driver_eye(
                frame.position,
                frame.rotation,
                Vec3::from(seat.transform.position),
                avatar.model_node(avatar_assets, "Eye")?.w_axis.truncate() * local.scale,
            ),
            None => avatar
                .animated_world_node(avatar_assets, "Eye")?
                .w_axis
                .truncate(),
        };
        eye.is_finite().then_some(eye)
    }
    /// The eye in the body's space before a leading eye crosses a portal.
    pub(super) fn first_person_eye_here(
        controls: &Controls,
        local: &bri_sim::player::PlayerState,
        eye: Vec3,
    ) -> Vec3 {
        let middle =
            Vec3::from(local.feet) + Vec3::Y * bri_sim::player::nominal_middle(local.scale);
        middle + controls.portal_tilt() * (eye - middle)
    }
    /// Where the view camera is and how it looks (yaw, pitch, roll): first
    /// person, sliding out to the chase camera, or an observer camera. Only
    /// a rider's first-person view rolls, with its seat.
    #[allow(clippy::too_many_arguments)]
    /// [`Self::view_camera_here`], carried through any opening between the
    /// local body and the camera: a camera whose body's middle is not yet
    /// through (the eye leads it) or has just come out (the chase camera
    /// trails it) looks from the side the body is seen from, so walking
    /// through never cuts.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn view_camera(
        controls: &Controls,
        presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
        building: &crate::building::Building,
        collision: Option<&bri_sim::prediction::CollisionMirror>,
        assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        view: &network::View,
        local: &bri_sim::player::PlayerState,
        first_person_eye: Vec3,
        passages: &bri_content::passage::Passages,
        drawn_offset: Option<Vec3>,
    ) -> Result<(Vec3, f32, f32, f32)> {
        let (eye, yaw, pitch, roll, boom) = Self::view_camera_here(
            controls,
            presented,
            building,
            collision,
            assets,
            vehicles,
            view,
            local,
            first_person_eye,
            drawn_offset,
            passages,
        )?;
        // A free camera flies through openings itself (`Controls::fly`); an
        // orbit camera's boom went back through one, which turns its look.
        if controls.observer().is_some() {
            let (yaw, pitch, roll) = match boom {
                Some(carry) => crate::portal_view::carried_look((yaw, pitch, roll), &carry),
                None => (yaw, pitch, roll),
            };
            return Ok((eye, yaw, pitch, roll));
        }
        // A chase camera whose boom went through an opening is already
        // there; otherwise the eye leading the body's middle is carried.
        // The tilt a floor or ceiling opening left eases out after: the eye
        // starts where the carry put it and comes round.
        let middle =
            Vec3::from(local.feet) + Vec3::Y * bri_sim::player::nominal_middle(local.scale);
        let look = (yaw, pitch, roll);
        let eye = if boom.is_none() && controls.camera_pos() == 0.0 {
            Self::first_person_eye_here(controls, local, eye)
        } else {
            eye
        };
        let (eye, (yaw, pitch, roll)) =
            crate::portal_view::through(eye, look, boom, middle, passages);
        Ok((eye, yaw, pitch, roll))
    }
    /// The view camera where the body is, and the carry of any opening the
    /// chase camera's boom went back through.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn view_camera_here(
        controls: &Controls,
        presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
        building: &crate::building::Building,
        collision: Option<&bri_sim::prediction::CollisionMirror>,
        assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        view: &network::View,
        local: &bri_sim::player::PlayerState,
        first_person_eye: Vec3,
        drawn_offset: Option<Vec3>,
        passages: &bri_content::passage::Passages,
    ) -> Result<(Vec3, f32, f32, f32, Option<glam::Affine3A>)> {
        let look = |yaw: f32, pitch: f32| {
            Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            )
        };
        let pos = controls.camera_pos();
        let seated = view.vitals.get(&view.owner).and_then(|v| v.mounted);
        // The rider of a player-type mount looks along the mount as drawn,
        // in first and third person, so the two turn together.
        let mount = seated
            .filter(|_| controls.observer().is_none())
            .and_then(|(vehicle, seat)| {
                let info = view.vehicles.get(&vehicle)?;
                let d = assets.definition(&info.definition)?;
                (d.seat_role(usize::from(seat)) == SeatRole::Actor).then_some(())?;
                Some(vehicles.frame(vehicle)?.rotation)
            });
        let (yaw, pitch) =
            mount.map_or_else(|| controls.camera_angles(), |m| controls.mount_look(m));
        // `minLookAngle`/`maxLookAngle`: exactly straight down and up.
        let pitch = pitch.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
        if controls.observer().is_some() || pos == 0.0 {
            let ride = controls
                .ride_view()
                .filter(|_| controls.observer().is_none());
            let (yaw, pitch, roll) = match ride {
                Some(ride) => {
                    let (yaw, pitch) = crate::controls::angles(ride * Vec3::NEG_Z, ride * Vec3::Y);
                    (yaw, pitch, crate::controls::roll(ride))
                }
                // The roll a floor or ceiling opening left, easing out.
                None if controls.observer().is_none() => (yaw, pitch, controls.portal_roll()),
                None => (yaw, pitch, 0.0),
            };
            let (eye, boom) = camera_eye(
                controls,
                presented,
                &view.entities,
                drawn_offset,
                building,
                collision,
                first_person_eye,
                look(yaw, pitch),
                None,
                passages,
            )?;
            return Ok((eye, yaw, pitch, roll, boom));
        }
        let riding = seated.and_then(|(vehicle, seat)| {
            let info = view.vehicles.get(&vehicle)?;
            let d = assets.definition(&info.definition)?;
            Some((info, d, usize::from(seat), vehicles.frame(vehicle)?))
        });
        // In third person a player with a control object hands the camera to
        // it (`Player::getCameraTransform` 0x5ab80e): a vehicle's driver sees
        // its chase camera, swung round by the head's turn; the Tank gunner
        // and a horse's rider see their player-type mount's own camera.
        // Passengers have no control object and keep their own camera.
        let player_view = match riding {
            Some((_, d, seat, frame))
                if matches!(
                    d.seat_role(seat),
                    SeatRole::StrafeDriver | SeatRole::MouseDriver
                ) =>
            {
                let center = (Vec3::from(d.bounds_min) + Vec3::from(d.bounds_max)) * 0.5;
                // The boom goes back through any portal behind the vehicle,
                // as a player's chase camera's does.
                let mut boom = None;
                let (eye, yaw, pitch) = crate::vehicle_camera::driver_view(
                    frame.position,
                    frame.rotation,
                    center,
                    &d.camera,
                    controls.driver_head_yaw(),
                    pos,
                    |from, to| {
                        let (hit, through) =
                            Self::driver_camera_ray(from, to, passages, collision, |from, to| {
                                Ok(building
                                    .solid_segment(from, to)?
                                    .map(|hit| (hit.distance, hit.normal)))
                            })?;
                        boom = Some((from, through));
                        Ok(hit)
                    },
                )?;
                // The eye carried; `view_camera` turns the look with it.
                let (eye, carry) = match boom {
                    Some((from, through)) => {
                        crate::portal_view::along(&through, from.distance(eye), eye)
                    }
                    None => (eye, None),
                };
                return Ok((eye, yaw, pitch, 0.0, carry));
            }
            Some((_, d, seat, frame)) if d.seat_role(seat) == SeatRole::Actor => {
                Some(mount_camera(d, frame.position, pos))
            }
            // The gunner controls the `TankTurretPlayer` on the Tank's mount2.
            Some((_, d, seat, frame)) if d.seat_role(seat) == SeatRole::Gunner => assets
                .attachment_definition(d)
                .zip(d.attachment_mount.as_ref())
                .map(|(turret, mount)| {
                    let feet = frame.position + frame.rotation * Vec3::from(mount.position);
                    mount_camera(turret, feet, pos)
                }),
            Some((info, _, seat, _)) => vehicles
                .seat(assets, info, seat)
                .map(|(feet, _)| Self::player_camera(assets, &view.archetypes, local, feet, pos)),
            _ if seated.is_none() => Some(Self::player_camera(
                assets,
                &view.archetypes,
                local,
                Vec3::from(local.feet),
                pos,
            )),
            _ => None,
        };
        if let Some((distance, pivot, tilt)) = player_view {
            // `getCameraTransform` composes the tilt onto the eye's pitch, so
            // the chase camera keeps swinging over the head past vertical.
            let (yaw, pitch, roll) =
                crate::portal_view::leaned((yaw, pitch, controls.portal_roll()), tilt);
            // Just out of an opening in a floor or ceiling, the pivot comes
            // round from where the carry turned it (`Controls::portal_tilt`).
            let middle =
                Vec3::from(local.feet) + Vec3::Y * bri_sim::player::nominal_middle(local.scale);
            let pivot = middle + controls.portal_tilt() * (pivot - middle);
            let (eye, boom) = camera_eye(
                controls,
                presented,
                &view.entities,
                drawn_offset,
                building,
                collision,
                pivot,
                look(yaw, pitch),
                Some((middle, distance)),
                passages,
            )?;
            return Ok((eye, yaw, pitch, roll, boom));
        }
        // `cameraTilt` turns the vehicle chase view down without moving the camera.
        let chase = Self::chase_camera(assets, vehicles, view);
        let (eye, boom) = camera_eye(
            controls,
            presented,
            &view.entities,
            drawn_offset,
            building,
            collision,
            chase.map_or(first_person_eye, |(_, pivot, _)| pivot),
            look(yaw, pitch),
            Some((
                chase.map_or(first_person_eye, |(_, pivot, _)| pivot),
                chase.map_or(
                    view.archetypes
                        .resolve(local.archetype)
                        .look
                        .camera_distance,
                    |(distance, ..)| distance,
                ) * pos,
            )),
            passages,
        )?;
        let pitch = chase.map_or(pitch, |(_, _, tilt)| (pitch - tilt).clamp(-1.56, 1.56));
        Ok((eye, yaw, pitch, 0.0, boom))
    }
    /// Seated where v20's `armor::onTrigger` fires the mount's gun instead
    /// of tools: the Tank turret and the pirate cannon.
    pub(super) fn local_weapon_seat(&self) -> bool {
        self.network_view()
            .and_then(|v| {
                let (vehicle, seat) = v.vitals.get(&v.owner)?.mounted?;
                let info = v.vehicles.get(&vehicle)?;
                let d = self.vehicle_assets.definition(&info.definition)?;
                Some(d.seats.get(usize::from(seat))?.weapon)
            })
            .unwrap_or(false)
    }
    /// Steer whatever the server says this client controls. A granted free
    /// camera starts at the player's smoothed eye (`dropCameraAtPlayer`).
    pub(super) fn follow_control(&mut self) {
        let Some(view) = self.network_view() else {
            return;
        };
        let control = view
            .vitals
            .get(&view.owner)
            .map_or_else(Default::default, |v| v.control);
        let eye = self.local_eye().or_else(|| {
            view.poses
                .get(&view.owner)
                .map(|p| view.archetypes.eye(&p.player))
        });
        // A path camera flies its replicated path on the server's clock.
        let path = view
            .vitals
            .get(&view.owner)
            .and_then(|v| v.camera_path.as_ref())
            .zip(self.motion.server_tick())
            .map(|(path, tick)| path.sample(tick));
        let point = view.vitals.get(&view.owner).and_then(|v| v.camera_point);
        let owner = view.owner;
        self.controls.follow(control, owner, eye);
        if let Some(path) = path {
            self.controls.fly_path(path);
        }
        if let Some(point) = point {
            self.controls.orbit_point(point);
        }
    }
    /// The camera in control, as the server's `%client.Camera` transform:
    /// the free camera's position, or where the orbit camera was drawn from.
    pub(super) fn camera_view(&self) -> Option<bri_sim::session::CameraView> {
        let observer = self.controls.observer()?;
        let eye = match observer.mode {
            crate::controls::ObserverMode::Free(position)
            | crate::controls::ObserverMode::Path(position) => position,
            crate::controls::ObserverMode::Orbit(_)
            | crate::controls::ObserverMode::Drive(_)
            | crate::controls::ObserverMode::Point(..) => self.view.observer_eye?,
        };
        let view = bri_sim::session::CameraView {
            eye: eye.to_array(),
            yaw: observer.yaw,
            pitch: observer.pitch,
        };
        view.validate().ok().map(|()| view)
    }
    /// The local player's held weapon as their own game shows it: its aim
    /// zoom and whether it hides the crosshair (`Image::zoom`,
    /// `Image::crosshair`). Purely local.
    pub(super) fn update_held_weapon(&mut self) {
        let view = self
            .net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.view.as_ref());
        let pack = &self.content.weapons.pack;
        let image = view.and_then(|view| {
            if !view.vitals.get(&view.owner).is_some_and(|v| v.alive) {
                return None;
            }
            let mounted = view.weapons.images.get(&view.owner)?;
            let mounted = mounted.iter().find(|m| m.hand == 0)?;
            pack.images.get(&mounted.image)
        });
        self.controls.set_aim(image.and_then(|i| i.zoom.clone()));
        let hidden = image.is_some_and(|i| !i.crosshair) || self.controls.aim_hides_crosshair();
        if hidden != self.view.crosshair_hidden {
            self.view.crosshair_hidden = hidden;
            self.ui.apply(UiUpdate::HideCrosshair(hidden));
        }
        // The trigger goes to the tool only on foot or in a seat that is not
        // a gunner's, and not from a camera. Whether it is held is the UI's
        // to know: it gives the tool the wheel only while it is.
        let wheel = image
            .and_then(|i| i.commands.wheel.clone())
            .filter(|_| self.controls.observer().is_none() && !self.local_weapon_seat());
        claim_wheel(&mut self.ui, &mut self.view.tool_wheel, wheel);
        let keys = image.map_or_else(Default::default, |i| crate::building::ImageKeys {
            shift: i.commands.shift.clone(),
            rotate: i.commands.rotate.clone(),
            plant: i.commands.plant.clone(),
            paint: i.commands.paint.clone(),
            paint_picker: i.paint_picker,
        });
        if let Some(building) = self.build.building.as_mut() {
            building.set_image_keys(keys);
            let takes = building.keeps_tool_for_paint();
            if takes != self.build.tool_takes_paint {
                self.build.tool_takes_paint = takes;
                self.ui.apply(UiUpdate::ToolTakesPaint(takes));
            }
        }
        let zooms = self.controls.orbit_zooms();
        if zooms != self.view.camera_wheel {
            self.view.camera_wheel = zooms;
            self.ui.apply(UiUpdate::CameraWheel(zooms));
        }
        // Aiming a scope with steps, the wheel zooms instead (`Zoom::levels`).
        let aim_wheel = self.controls.aim_takes_wheel();
        if aim_wheel != self.view.aim_wheel {
            self.view.aim_wheel = aim_wheel;
            self.ui.apply(UiUpdate::AimWheel(aim_wheel));
        }
        // A scope's picture while aiming from the eye (`Zoom::overlay`);
        // the weapon itself is not drawn behind it.
        let overlay = image
            .filter(|_| self.controls.scope_overlay().is_some())
            .and_then(|i| self.item_ui.scope_overlay(&i.id));
        if overlay != self.view.scope_overlay {
            self.view.scope_overlay = overlay;
            self.world_items.set_scoped(overlay.is_some());
            self.ui.apply(UiUpdate::ScopeOverlay(overlay));
        }
    }
    /// Dead players watch their corpse from the orbit camera.
    pub(super) fn third_person_view(&self) -> bool {
        draws_third_person(&self.controls, self.local_alive())
    }
}
