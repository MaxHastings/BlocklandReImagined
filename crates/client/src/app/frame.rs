//! The per-frame update, in its fixed order (see docs/architecture/client-app-split.md).
use super::*;

impl App {
    pub(super) fn frame(&mut self, elapsed: Duration) -> Result<()> {
        self.perf.frame_stats.push(elapsed);
        if let Some(line) = self
            .perf
            .frame_log
            .as_mut()
            .and_then(|log| log.frame(elapsed))
        {
            // Session log only: players send it, the console stays quiet.
            eprintln!("{line}");
        }
        // Tell the player about a newer release outside a game, not as a
        // dialog over play.
        if !self.ui.core.in_game()
            && let Some(check) = &self.lobby.update_check
        {
            match check.try_recv() {
                Ok(newer) => {
                    self.lobby.update_check = None;
                    self.ui.apply(UiUpdate::NewerVersion {
                        name: newer.name,
                        url: newer.url,
                    });
                }
                Err(mpsc::TryRecvError::Disconnected) => self.lobby.update_check = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut listener = bri_audio::Listener::default();
        // `setTimeScale` slows or speeds the whole game, not the interface:
        // every in-world visual advances by `game_elapsed`; only the UI,
        // camera easing and audio mixing use wall time.
        let scale = self
            .net
            .attempt
            .as_ref()
            .and_then(|a| a.view.as_ref())
            .map_or(1.0, |v| v.time_scale);
        let game_elapsed = elapsed.mul_f32(scale);
        self.avatar.animation_time += game_elapsed.as_secs_f64().min(0.25);
        self.avatar.preview_time += elapsed.as_secs_f64().min(0.25);
        if let Err(error) = self.poll_network() {
            // A fault while following the session (a map's weather, a tool
            // catalog, a closed connection) ends that session with its real
            // reason, never the whole game.
            bri_console::warn(format!("Session ended by a client error: {error:#}"));
            self.ui.apply(UiUpdate::Connection(ConnectionState::Failed {
                reason: format!("{error:#}"),
            }));
            self.disconnect();
        }
        // The server's settings decide some weapon fields: play the pack
        // they make, and the authored one outside a game.
        let values = self.net.attempt.as_ref().and_then(|a| a.view.as_ref());
        let values = values
            .map(|v| v.weapon_settings.clone())
            .unwrap_or_default();
        match self.content.weapons.apply_settings(&values) {
            Ok(false) => {}
            // Items a setting shows or hides: the lists offer what the
            // server does.
            Ok(true) => {
                let choices = self.content.weapons.item_choices.clone();
                let rebuilt = self
                    .build
                    .tool_ui
                    .install_items(choices.clone())
                    .and_then(|()| {
                        self.item_ui = crate::item_ui::ItemUi::new(
                            &self.item_assets,
                            &choices,
                            &self.content.ui_pack,
                        )?;
                        Ok(())
                    });
                if let Err(error) = rebuilt {
                    bri_console::warn(format!("The server's items: {error:#}"));
                }
            }
            Err(error) => bri_console::warn(format!("The server's weapon settings: {error:#}")),
        }
        self.poll_files();
        if let Some((map, name)) = self.files.save_previews.poll() {
            self.ui.apply(UiUpdate::SavePreview {
                map,
                name,
                preview: IconRef::None,
            });
        }
        self.poll_old_saves();
        self.update_package_hud();
        if let Some(a) = self.net.attempt.as_ref().filter(|a| a.entered) {
            let skins = self.addons.item_skins.take_messages();
            for text in self
                .addons
                .client_code
                .take_messages()
                .into_iter()
                .chain(skins)
            {
                self.ui.apply_session(a.id, UiUpdate::Chat { text });
            }
        }
        let alive = self.local_alive();
        self.follow_control();
        self.controls
            .fly(elapsed.as_secs_f32(), &self.motion.passages());
        let prefs = &self.ui.core.prefs;
        self.controls.set_fov_prefs(
            bri_ui::screens::options::default_fov(prefs),
            bri_ui::screens::options::zoom_fov(prefs),
        );
        self.controls.set_invert_prefs(
            prefs.bool_or("$pref::Input::MouseInvert", false),
            prefs.bool_or("$Pref::Input::VehicleMouseInvert", true),
        );
        let steering = steering_prefs(prefs);
        if let Some(a) = self.net.attempt.as_ref().filter(|a| a.entered)
            && self.net.steering_sent != Some((a.id, steering))
        {
            self.net.steering_sent = Some((a.id, steering));
            self.ui.core.request(UiAction::SteeringPrefs {
                strafe: steering.0,
                auto_return: steering.1,
            });
        }
        self.update_held_weapon();
        self.controls.advance_sway(elapsed.as_secs_f32());
        self.controls.advance_zoom(elapsed.as_secs_f32());
        self.controls.ease_roll(elapsed.as_secs_f32());
        let third_person_only = self
            .net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| {
                let view = a.view.as_ref()?;
                let body = self.motion.presented().get(&view.owner)?;
                Some(
                    view.archetypes
                        .resolve(body.archetype)
                        .look
                        .third_person_only,
                )
            });
        self.controls
            .set_third_person_only(third_person_only.unwrap_or(false));
        self.controls.advance_view(elapsed.as_secs_f32());
        self.controls.advance_head(elapsed.as_secs_f32());
        self.advance_local_game(alive, game_elapsed)?;
        self.update_combat_presentation();
        self.update_perf();
        self.update_lag();
        self.poll_background_jobs();
        // Build macro playback: one recorded building action per frame so the
        // server's action budget is never exceeded.
        if let Some(action) = self.build.macro_playback.pop_front() {
            self.ui.core.request(action);
        }
        let third_person = self.third_person_view();
        self.ui.apply(UiUpdate::FirstPerson(!third_person));
        let weapon_checkpoint = self
            .net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| {
                a.view
                    .as_ref()
                    .map(|view| (a.id, view.checkpoint_cue_cursor))
            });
        if let Some((session, cursor)) = weapon_checkpoint {
            self.reset_weapon_effect_session(session, cursor);
        }
        // Bricks are aimed on through portals, as the host's tools are.
        if let Some(building) = &mut self.build.building {
            building.set_passages(&self.motion.passages());
        }
        self.advance_world_presentation(game_elapsed, third_person, &mut listener)?;
        self.audio.tick(elapsed.as_secs_f32(), listener);
        self.view.drawn_controls = Some(self.controls.clone());
        Ok(())
    }

    /// Steps 12-19 of the frame schedule: the local player's motion, then
    /// mounts, vehicles, riders, loose models and music.
    fn advance_local_game(&mut self, alive: bool, game_elapsed: Duration) -> Result<()> {
        if let Some(a) = self.net.attempt.as_ref().filter(|a| a.entered) {
            if let Some(view) = &a.view {
                let vitals = view.vitals.get(&view.owner);
                let mounted = vitals.and_then(|v| v.mounted);
                let ride = vitals.and_then(|v| v.ride);
                let driving = vitals.is_some_and(|v| {
                    matches!(v.control, bri_sim::session::ControlObject::Entity(_))
                });
                self.motion
                    .set_mounted(mounted.is_some() || ride.is_some() || driving);
                let driven = driven_vehicle(mounted, |vehicle, seat| {
                    view.vehicles
                        .get(&vehicle)
                        .and_then(|info| self.vehicle_assets.definition(&info.definition))
                        .and_then(|d| d.seats.get(seat))
                        .is_some_and(|s| s.controls)
                });
                Self::predict_driven(
                    &mut self.motion,
                    &mut self.vehicles,
                    &self.vehicle_assets,
                    &self.ui.core.prefs,
                    &mut self.cosmetic_faults,
                    view,
                    driven,
                );
            }
            let mounted = a
                .view
                .as_ref()
                .and_then(|v| v.vitals.get(&v.owner))
                .is_some_and(|v| v.mounted.is_some() || v.ride.is_some());
            let input = if alive {
                rider_input(self.net.abilities, self.controls.movement(), mounted)
            } else {
                // Corpses ignore controls; keep aim so the server agrees.
                bri_sim::player::MoveInput {
                    yaw: self.controls.yaw,
                    pitch: self.controls.pitch,
                    ..Default::default()
                }
            };
            // A tool in hand with a jet command (v20's `onTrigger` slot 4)
            // takes right click: predict no jet, as the host runs none.
            let tool_jet = a.view.as_ref().is_some_and(|v| {
                v.weapons.images.get(&v.owner).is_some_and(|images| {
                    images.iter().any(|m| {
                        m.hand == 0
                            && self
                                .content
                                .weapons
                                .pack
                                .images
                                .get(&m.image)
                                .is_some_and(|i| i.commands.jet.is_some())
                    })
                })
            });
            self.motion.set_tool_jet(tool_jet);
            if let Some((newest, inputs)) = self.motion.advance(
                game_elapsed.as_secs_f32(),
                input,
                bri_net::protocol::MOVEMENT_REDUNDANCY,
            )? {
                a.worker
                    .movement(newest, inputs, self.camera_view(), self.mounts.seat_report)?;
            }
            // Through an opening: the look turns as the body did.
            if let Some(carry) = self.motion.take_passed() {
                self.controls.carry_look(&carry);
            }
            if let Some((speed, archetype)) = self.motion.take_impact() {
                let min = bri_sim::player_types::PlayerType::from_archetype(archetype)
                    .unwrap_or_default()
                    .min_impact_speed();
                self.fx.actor_effects.ground_impact(
                    speed,
                    min,
                    self.avatar.animation_time.to_bits(),
                );
            }
            if let Some(view) = &a.view {
                Self::view_kick(
                    &mut self.shot_kicks,
                    &mut self.kick_seen,
                    &mut self.fx.actor_effects,
                    &self.content.weapons.pack,
                    &view.weapons,
                    view.owner,
                    |actor, hand| self.world_items.held_muzzle(actor, hand),
                    self.avatar.animation_time.to_bits(),
                );
            }
            if let Some(view) = &a.view {
                let vitals = view.vitals.get(&view.owner);
                let mounted = vitals.and_then(|v| v.mounted);
                // A player riding another player shows the host's pose too.
                let ride = vitals.and_then(|v| v.ride);
                self.controls
                    .set_mounted(mounted.is_some() || ride.is_some());
                let first_person_only = self
                    .presented_local()
                    .is_some_and(|p| view.archetypes.resolve(p.archetype).look.first_person_only);
                self.controls.set_first_person_only(first_person_only);
                let head_yaw = self.controls.movement().head_yaw;
                self.motion.present(
                    view,
                    self.controls.yaw,
                    self.controls.body_pitch(),
                    head_yaw,
                );
                let driven = driven_vehicle(mounted, |vehicle, seat| {
                    view.vehicles
                        .get(&vehicle)
                        .and_then(|info| self.vehicle_assets.definition(&info.definition))
                        .and_then(|d| d.seats.get(seat))
                        .is_some_and(|s| s.controls)
                });
                self.vehicles.update(
                    &view.vehicles,
                    &view.vehicle_poses,
                    self.motion.server_tick(),
                    driven,
                    &self.motion.passages(),
                );
                self.fx.tutorial_targets.update(
                    &view.targets,
                    self.motion.server_tick().unwrap_or(view.tick as f64),
                );
                let new_seat = mounted != self.mounts.seated_on;
                // A new seat starts facing it (`Armor::onMount` resets the
                // transform), even from one passenger seat to another.
                if new_seat {
                    self.mounts.seated_on = mounted;
                    self.mounts.takes_turret = mounted.is_some();
                    self.controls.set_ride(None);
                    // Tell the host the steering prefs again with every seat,
                    // should its copy have been lost (a reconnect).
                    self.net.steering_sent = None;
                }
                // The view rides along: it faces the seat, follows a
                // mouse-steered vehicle, turns with the hull for a gunner, and
                // stays put on a mount facing the look.
                let riding = mounted.and_then(|(vehicle, seat)| {
                    let info = view.vehicles.get(&vehicle)?;
                    let d = self.vehicle_assets.definition(&info.definition)?;
                    let frame = self.vehicles.frame(vehicle)?;
                    let seat_rotation = self
                        .vehicles
                        .seat_transform(&self.vehicle_assets, info, usize::from(seat))
                        .map(|(_, rotation)| rotation);
                    // skiVehicle::onWreck whites the screen out by the crash
                    // speed: clamp(1 + (speed - 10) / 50 * 7, 1, 7) / 7.
                    if d.family == bri_vehicles::Family::Tumble
                        && self.mounts.tumble != Some(vehicle)
                    {
                        self.mounts.tumble = Some(vehicle);
                        let seconds =
                            (1.0 + (frame.velocity.length() - 10.0) / 50.0 * 7.0).clamp(1.0, 7.0);
                        self.ui.apply(UiUpdate::Whiteout(seconds / 7.0));
                    }
                    let forward = frame.rotation * Vec3::NEG_Z;
                    // A driver steers as the host steers them (its copy of
                    // their prefs, in the pose), so view and prediction agree.
                    let pose = view
                        .vehicle_poses
                        .get(&vehicle)
                        .filter(|_| d.control_seat() == Some(usize::from(seat)));
                    let (strafe, _) = steering_in_use(pose, &self.ui.core.prefs);
                    let role = d.seat_role_for(usize::from(seat), strafe);
                    // The first-person view rides the seat on a vehicle and
                    // the hull under a gunner's turret; a player-type mount
                    // stays upright like any player.
                    self.controls.set_ride(match role {
                        // Any passenger, a rowboat's too: the mouse turns the
                        // body on the seat and pitches the head.
                        SeatRole::Passenger => seat_rotation.map(|r| {
                            crate::controls::Ride::Seat(r, crate::controls::SeatLook::Passenger)
                        }),
                        // A player-type mount's rider controls a Player:
                        // upright, and its head never springs back.
                        _ if d.is_actor() => None,
                        SeatRole::StrafeDriver => seat_rotation.map(|r| {
                            crate::controls::Ride::Seat(r, crate::controls::SeatLook::StrafeDriver)
                        }),
                        SeatRole::MouseDriver => seat_rotation.map(|r| {
                            crate::controls::Ride::Seat(r, crate::controls::SeatLook::MouseDriver)
                        }),
                        SeatRole::Gunner => Some(crate::controls::Ride::Hull(frame.rotation)),
                        SeatRole::Actor => None,
                    });
                    Some((
                        role,
                        forward.x.atan2(-forward.z),
                        forward.y.clamp(-1.0, 1.0).asin(),
                        seat_rotation.map(|r| {
                            let forward = r * Vec3::NEG_Z;
                            forward.x.atan2(-forward.z)
                        }),
                    ))
                });
                if riding.is_none() {
                    // A passenger on another player (no control object)
                    // turns on its seat like one on a vehicle.
                    let seat = ride.filter(|r| !r.steers).and_then(|r| {
                        let heading = self.motion.presented().get(&r.mount)?.yaw;
                        Some(crate::controls::Ride::Seat(
                            glam::Quat::from_rotation_y(-heading),
                            crate::controls::SeatLook::Passenger,
                        ))
                    });
                    self.controls.set_ride(seat);
                }
                if !matches!(
                    riding,
                    Some((SeatRole::Passenger | SeatRole::StrafeDriver, ..))
                ) {
                    self.controls.set_seat_yaw(None);
                }
                match riding {
                    Some((SeatRole::MouseDriver, heading, pitch, _)) => {
                        self.controls.set_vehicle_view(Some((heading, pitch)));
                        self.mounts.mount_heading = Some(heading);
                    }
                    Some((SeatRole::Actor, ..)) => {
                        self.controls.set_vehicle_view(None);
                        self.mounts.mount_heading = None;
                    }
                    None => {
                        self.controls.set_vehicle_view(None);
                        self.mounts.mount_heading = None;
                    }
                    // Passengers and the Jeep's driver sit fixed in the seat:
                    // the mouse only tilts their view (`Player::processTick`
                    // takes the mount transform; the driver's yaw goes to the
                    // vehicle, which ignores it when the keys steer).
                    Some((SeatRole::Passenger | SeatRole::StrafeDriver, heading, _, seat_yaw)) => {
                        self.controls.set_vehicle_view(None);
                        self.controls.set_seat_yaw(seat_yaw.or(Some(heading)));
                        self.mounts.mount_heading = Some(heading);
                    }
                    Some((SeatRole::Gunner, heading, ..)) => {
                        self.controls.set_vehicle_view(None);
                        // A new gunner takes control of an attached turret
                        // looking where it points, whichever seat they came
                        // from, once its pose is known (the host holds it
                        // there until they do). Last, so leaving the old
                        // seat's view can't undo it.
                        let turret = mounted.and_then(|(vehicle, _)| {
                            let info = view.vehicles.get(&vehicle)?;
                            let d = self.vehicle_assets.definition(&info.definition)?;
                            (!d.is_actor() && d.attachment_mount.is_some())
                                .then(|| view.vehicle_poses.get(&vehicle))
                        });
                        match (self.mounts.takes_turret, turret) {
                            (true, Some(Some(pose))) => {
                                self.mounts.takes_turret = false;
                                let (yaw, pitch) = crate::vehicles::turret_look(pose);
                                self.controls.take_turret(yaw, pitch);
                            }
                            (true, Some(None)) => {}
                            _ => {
                                self.mounts.takes_turret = false;
                                if let Some(previous) = self.mounts.mount_heading {
                                    let turn = (heading - previous + std::f32::consts::PI)
                                        .rem_euclid(std::f32::consts::TAU)
                                        - std::f32::consts::PI;
                                    self.controls.carry_yaw(turn);
                                }
                            }
                        }
                        self.mounts.mount_heading = Some(heading);
                    }
                }
                // From the next move on, moves are shaped for this seat once
                // its view is in place: the seat's frame is known and a new
                // gunner looks along the turret.
                let shaped = match riding {
                    None => mounted.is_none(),
                    Some((SeatRole::Gunner, ..)) => !self.mounts.takes_turret,
                    Some(_) => true,
                };
                if shaped {
                    self.mounts.seat_report = bri_sim::session::SeatSince::follow(
                        self.mounts.seat_report,
                        mounted,
                        self.motion.next_sequence(),
                    );
                }
                // The local gunner's barrel follows their own look this frame.
                if let Some((vehicle, seat)) = mounted
                    && let Some(info) = view.vehicles.get(&vehicle)
                    && let Some(d) = self.vehicle_assets.definition(&info.definition)
                    && d.seats.get(usize::from(seat)).is_some_and(|s| s.weapon)
                {
                    self.vehicles
                        .aim_locally(vehicle, d, self.controls.yaw, self.controls.pitch);
                }
                // On another player, a passenger faces the seat like one on a
                // vehicle; the first seat of a bot mount turns it instead.
                if let Some(ride) = ride.filter(|r| !r.steers)
                    && let Some(heading) = self.motion.presented().get(&ride.mount).map(|m| m.yaw)
                {
                    self.controls.set_seat_yaw(Some(heading));
                    self.mounts.mount_heading = Some(heading);
                }
                Self::pose_mounts(
                    &mut self.avatar.mount_meshes,
                    &self.avatar.avatar_assets,
                    &self.vehicle_assets,
                    &self.vehicles,
                    self.avatar.animation_time,
                    view,
                )?;
                // Riders sit exactly on their rendered vehicle's seat, tilted
                // with it (`Player::processTick` takes the mount transform).
                self.mounts.rider_rotations.clear();
                self.mounts.rider_eye = None;
                for (owner, vitals) in &view.vitals {
                    let Some((vehicle, seat)) = vitals.mounted else {
                        continue;
                    };
                    if let Some(info) = view.vehicles.get(&vehicle)
                        && let Some((mut feet, mut rotation)) = self.vehicles.seat_transform(
                            &self.vehicle_assets,
                            info,
                            usize::from(seat),
                        )
                    {
                        // A horse's rider rides its animated mount node, rising
                        // and falling with the gait like v20's `mountObject`.
                        if let Some(node) = self
                            .avatar
                            .mount_meshes
                            .get(&vehicle)
                            .zip(
                                self.vehicle_assets
                                    .definition(&info.definition)
                                    .and_then(|d| d.seats.get(usize::from(seat))),
                            )
                            .and_then(|(mesh, s)| {
                                mesh.world_node(&self.avatar.avatar_assets, &s.node)
                            })
                        {
                            let (_, node_rotation, position) = node.to_scale_rotation_translation();
                            feet = position;
                            rotation = node_rotation;
                        }
                        let forward = rotation * Vec3::NEG_Z;
                        let mut yaw = forward.x.atan2(-forward.z);
                        // A passenger's body turns on the seat by its own
                        // `mRot.z` (`Player::setPosition` 0x5a6bc0): the
                        // local one by the mouse, others by the host's yaw.
                        // A tumbling body only rolls with its tumble: its
                        // player watches through the corpse camera.
                        let passenger = self
                            .vehicle_assets
                            .definition(&info.definition)
                            .is_some_and(|d| {
                                d.family != bri_vehicles::Family::Tumble
                                    && d.seat_role(usize::from(seat)) == SeatRole::Passenger
                            });
                        let turn = if !passenger {
                            0.0
                        } else if *owner == view.owner {
                            self.controls.passenger_turn()
                        } else {
                            self.motion.presented().get(owner).map_or(0.0, |p| {
                                (p.yaw - yaw + std::f32::consts::PI)
                                    .rem_euclid(std::f32::consts::TAU)
                                    - std::f32::consts::PI
                            })
                        };
                        rotation *= glam::Quat::from_rotation_y(-turn);
                        yaw += turn;
                        self.mounts.rider_rotations.insert(*owner, rotation);
                        let velocity = self
                            .vehicles
                            .frame(vehicle)
                            .map_or(Vec3::ZERO, |f| f.velocity);
                        // Every rider faces the seat.
                        self.motion.override_presented(
                            *owner,
                            feet,
                            Some(yaw),
                            rotation * Vec3::Y,
                            velocity,
                            *owner == view.owner,
                        );
                    }
                }
                // Players riding players sit on the mount's mount node,
                // rising and falling with its gait (`mountObject`).
                for (owner, vitals) in &view.vitals {
                    let Some(ride) = vitals.ride else {
                        continue;
                    };
                    let Some(mount) = self.motion.presented().get(&ride.mount).cloned() else {
                        continue;
                    };
                    let Some(point) = view
                        .archetypes
                        .resolve(mount.archetype)
                        .mount_points
                        .get(usize::from(ride.seat))
                    else {
                        continue;
                    };
                    let body = glam::Quat::from_rotation_y(-mount.yaw);
                    let (feet, rotation) =
                        match self.avatar.avatars.get(&ride.mount).and_then(|mesh| {
                            mesh.model_node(&self.avatar.avatar_assets, &point.node)
                        }) {
                            Some(node) => {
                                let (_, turn, offset) = node.to_scale_rotation_translation();
                                (
                                    Vec3::from(mount.feet) + body * offset * mount.scale,
                                    body * turn,
                                )
                            }
                            None => (
                                point.seat(Vec3::from(mount.feet), mount.yaw, mount.scale),
                                body,
                            ),
                        };
                    // A passenger's body turns on the seat by its own
                    // `mRot.z`; the rider steering a bot mount faces it.
                    let turn = if ride.steers {
                        0.0
                    } else if *owner == view.owner {
                        self.controls.passenger_turn()
                    } else {
                        self.motion.presented().get(owner).map_or(0.0, |p| {
                            (p.yaw - mount.yaw + std::f32::consts::PI)
                                .rem_euclid(std::f32::consts::TAU)
                                - std::f32::consts::PI
                        })
                    };
                    let rotation = rotation * glam::Quat::from_rotation_y(-turn);
                    self.mounts.rider_rotations.insert(*owner, rotation);
                    self.motion.override_presented(
                        *owner,
                        feet,
                        Some(mount.yaw + turn),
                        rotation * Vec3::Y,
                        Vec3::from(mount.velocity),
                        *owner == view.owner,
                    );
                }
                self.vehicles.set_passages(&self.motion.passages());
                self.vehicles
                    .prepare(&mut self.vehicle_assets, &view.vehicles);
                // Add-On casings, and debris that is not a vehicle's model,
                // draw as loose item models.
                let mut loose: Vec<_> = self.fx.weapon_shells.model_instances().collect();
                for (model, transform, tint) in self.fx.explosion_debris.models() {
                    if !self
                        .vehicle_assets
                        .push_source_model(model, transform, tint)
                    {
                        loose.push((
                            model.replace('\\', "/").to_ascii_lowercase(),
                            transform,
                            tint,
                        ));
                    }
                }
                self.world_items.set_loose(loose);
                let presented = self.motion.presented();
                let mut loops = BTreeMap::new();
                for (owner, images) in &view.weapons.images {
                    let Some(player) = presented.get(owner) else {
                        continue;
                    };
                    for mounted in images {
                        let sound = self
                            .content
                            .weapons
                            .pack
                            .images
                            .get(&mounted.image)
                            .and_then(|image| image.states.iter().find(|s| s.name == mounted.state))
                            .map(|state| state.sound.as_str())
                            .filter(|sound| !sound.is_empty() && self.audio.is_looping(sound));
                        if let Some(sound) = sound {
                            let eye = Vec3::from(player.feet)
                                + Vec3::Y
                                    * view
                                        .archetypes
                                        .tuning(player.archetype, player.scale)
                                        .stand_eye;
                            loops.insert(
                                (*owner, mounted.hand),
                                (sound.to_string(), eye.to_array()),
                            );
                        }
                    }
                }
                self.audio.sync_image_loops(&loops);
                if self
                    .music_world
                    .as_ref()
                    .is_none_or(|old| !Arc::ptr_eq(old, &view.world))
                {
                    self.audio.sync_music(&view.world.bricks);
                    self.music_world = Some(view.world.clone());
                }
            }
        }
        Ok(())
    }

    /// Step 22: the Add-On import, LAN query and firewall fix jobs.
    fn poll_background_jobs(&mut self) {
        self.poll_package_reload();
        if let Some(receiver) = &self.addons.add_on_sync {
            let mut notes = vec![];
            let mut done = false;
            loop {
                match receiver.try_recv() {
                    Ok(note) => {
                        done |= note.finished;
                        notes.push(note);
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
            if done {
                self.addons.add_on_sync = None;
            }
            if !notes.is_empty() || done {
                let mut view = crate::add_ons::view(&self.content.paths.root);
                let last = notes.iter().rev().find(|n| !n.notice.is_empty());
                if let Some(note) = last {
                    view.notice = note.notice.clone();
                }
                if last.is_some_and(|n| n.finished) {
                    // A conversion that was on, replaced or removed.
                    self.add_ons_changed(view);
                } else {
                    self.show_add_ons(view);
                }
            }
        }
        if let Some(receiver) = &self.lobby.lan_query
            && let Some(found) = finished(receiver, "LAN query")
        {
            self.lobby.lan_query = None;
            // A query that died finds nothing rather than spinning forever.
            let found = found.unwrap_or_default();
            self.lobby.lan_hosts.clear();
            let mut servers = Vec::new();
            for (address, beacon) in found.lan {
                if let Ok(certificate) = beacon.certificate_der() {
                    self.lobby
                        .lan_hosts
                        .insert(address.to_string(), certificate);
                    servers.push(ServerInfo {
                        address: address.to_string(),
                        name: plain_chat(&beacon.name),
                        password: false,
                        dedicated: false,
                        ping_ms: None,
                        players: beacon.players,
                        max_players: beacon.max_players,
                        bricks: 0,
                        map: plain_chat(&beacon.map),
                        favorite: false,
                    });
                }
            }
            // Servers joined before or starred, favourites first, with what
            // their game port answered just now.
            let mut saved_rows = Vec::new();
            for (saved, probe) in found.saved {
                let lan = servers
                    .iter_mut()
                    .find(|s| s.address.eq_ignore_ascii_case(&saved.address));
                if let Some(lan) = lan {
                    lan.favorite = saved.favorite;
                    continue;
                }
                let name = if saved.name.is_empty() {
                    saved.address.clone()
                } else {
                    plain_chat(&saved.name)
                };
                let mut row = ServerInfo {
                    address: saved.target().to_string(),
                    name,
                    password: false,
                    dedicated: false,
                    ping_ms: None,
                    players: 0,
                    max_players: 0,
                    bricks: 0,
                    map: String::new(),
                    favorite: saved.favorite,
                };
                match probe {
                    Ok(probe) => {
                        if !probe.listing.name.is_empty() {
                            row.name = plain_chat(&probe.listing.name);
                        }
                        row.map = plain_chat(&probe.listing.map);
                        row.players = probe.listing.players;
                        row.max_players = probe.listing.max_players;
                        row.ping_ms = Some(probe.ping.as_millis().min(9999) as u32);
                    }
                    Err(reason) => row.map = reason,
                }
                saved_rows.push(row);
            }
            // LAN favourites and saved favourites lead the list.
            servers.sort_by_key(|s| !s.favorite);
            saved_rows.sort_by_key(|s| !s.favorite);
            let (favorites, rest): (Vec<_>, Vec<_>) =
                saved_rows.into_iter().partition(|s| s.favorite);
            let mut list = favorites;
            list.extend(servers);
            list.extend(rest);
            self.ui.apply(UiUpdate::LanServers {
                servers: list,
                querying: false,
            });
        }
        if let Some(receiver) = &self.lobby.firewall_fix
            && let Some(result) = finished(receiver, "Firewall fix")
        {
            self.lobby.firewall_fix = None;
            let result = result.unwrap_or_else(|reason| Err(reason.to_string()));
            let (title, text) = match result {
                Ok(()) => (
                    "Windows Firewall",
                    "Blockland ReImagined can now accept friends through Windows Firewall."
                        .to_string(),
                ),
                Err(reason) => ("Windows Firewall", reason),
            };
            self.ui.apply(UiUpdate::MessageBox {
                title: title.into(),
                text,
            });
        }
    }

    /// Step 26, in game only: moving entities, ghosts, liquids, avatar
    /// animation, rider eyes and world effects; sets the audio `listener`.
    fn advance_world_presentation(
        &mut self,
        game_elapsed: Duration,
        third_person: bool,
        listener: &mut bri_audio::Listener,
    ) -> Result<()> {
        if let Some(view) = self
            .net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.view.as_ref())
            && let Some(local) = self.motion.presented().get(&view.owner)
            && let Some(meshes) = &self.scene.meshes
            && let Some(building) = &self.build.building
        {
            let presented = self.motion.presented();
            // Balls, projectiles, dropped items and package entities move at
            // the frame rate between the host's 20 Hz updates.
            let projectiles = &self.content.weapons.pack.projectiles;
            let passages = self.motion.passages();
            self.ghosts.update(
                game_elapsed.as_secs_f32(),
                view.tick,
                &view.weapons,
                &view.entities,
                |id| {
                    let d = projectiles.get(id)?;
                    // The host's fall per tick, as an acceleration.
                    let gravity =
                        bri_weapons::runtime::fall_per_tick(d) * bri_weapons::TICK_HZ as f32;
                    Some(crate::ghosts::Flight {
                        acceleration: Vec3::NEG_Y * gravity,
                        lifetime: d.lifetime_ticks,
                        // Balls and grenades bounce; the rest stop until the
                        // host says what the contact did.
                        bounce: (d.ballistic && d.elasticity > 0.0).then_some(
                            crate::ghosts::Bounce {
                                elasticity: d.elasticity,
                                friction: d.friction,
                                rest_speed: d.rest_speed,
                            },
                        ),
                    })
                },
                // Through portals as the host flies them.
                |from, to| {
                    crate::ghosts::Hit::first(&passages, from, to, |from, to| {
                        let length = (to - from).length();
                        let hit = building.solid_segment(from, to).ok()??;
                        Some(crate::ghosts::Hit {
                            position: hit.position,
                            normal: hit.normal,
                            fraction: hit.distance / length,
                            carry: None,
                        })
                    })
                },
            );
            // Each shot drawn from its shooter's muzzle as this client draws
            // the gun, closing on the host's path where the aim meets the
            // world (`crate::shot_origins`).
            let world_items = &self.world_items;
            let shown = self.shot_origins.shown(
                self.ghosts.weapons(),
                |actor| {
                    world_items
                        .held_muzzle(actor.0, 0)
                        .or_else(|| world_items.held_muzzle(actor.0, 1))
                },
                |from, direction, most| {
                    building
                        .solid_segment(from, from + direction * most)
                        .ok()
                        .flatten()
                        .map_or(most, |hit| hit.distance)
                },
                |p| {
                    projectiles.get(&p.definition).map_or(0.0, |d| {
                        p.velocity.length() * d.lifetime_ticks as f32 / bri_weapons::TICK_HZ as f32
                    })
                },
            );
            let weapons: &bri_sim::session::WeaponView = &shown;
            // Rebuilt only when the liquids or the paint change; they were
            // cloned (textures' names and all) several times every frame.
            let (liquids, waters) = match self.motion.collision() {
                Some(mirror) => {
                    let generation = mirror.water_generation();
                    if self.scene.liquid_cache.as_ref().is_none_or(|c| {
                        c.generation != generation || c.palette != view.world.palette
                    }) {
                        let liquids: Arc<[bri_sim::water::TintedWater]> = mirror
                            .tinted_waters(&view.world.bricks, &view.world.palette)
                            .into();
                        self.scene.liquid_cache = Some(LiquidCache {
                            generation,
                            palette: view.world.palette.clone(),
                            waters: liquids.iter().map(|w| w.water.clone()).collect(),
                            liquids,
                        });
                    }
                    let cache = self.scene.liquid_cache.as_ref().expect("filled above");
                    (cache.liquids.clone(), cache.waters.clone())
                }
                None => (Arc::from(Vec::new()), Arc::from(Vec::new())),
            };
            // Sample every body, including the hidden first-person body, once.
            // Visible geometry and attached items consume these same original nodes.
            Self::update_avatar_animation_inputs(
                &mut self.avatar.avatar_actions,
                &mut self.avatar.avatar_threads,
                &mut self.avatar.avatar_action_images,
                &mut self.fx.weapon_animation_cues,
                &mut self.fx.weapon_animation_drops,
                view,
                game_elapsed.as_secs_f32(),
            );
            self.avatar
                .avatars
                .retain(|owner, _| view.poses.contains_key(owner));
            self.avatar
                .avatar_actions
                .retain(|owner, _| view.poses.contains_key(owner));
            for (owner, player) in presented {
                let appearance = view
                    .avatars
                    .get(owner)
                    .unwrap_or(&self.avatar.avatar_assets.package.defaults);
                // `HorseArmor` players, and archetypes that look like it,
                // draw horse.dts.
                let look = &view.archetypes.resolve(player.archetype).look;
                let model = self
                    .avatar
                    .avatar_assets
                    .has_body(&look.model)
                    .then(|| look.model.clone());
                if self
                    .avatar
                    .avatars
                    .get(owner)
                    .is_none_or(|mesh| &mesh.appearance != appearance || mesh.model != model)
                {
                    let mut mesh = if let Some(model) = &model {
                        self.avatar
                            .avatar_assets
                            .body_mesh(model, appearance.clone())?
                    } else {
                        self.avatar.avatar_assets.mesh(appearance.clone())?
                    };
                    // The drawn mesh is built at render time, and only for
                    // bodies in view (`render_scene`).
                    mesh.defer_mesh = true;
                    mesh.instanced = true;
                    // Outfit changes (spray paint included) keep the running
                    // action thread instead of restarting the clip.
                    if let Some(old) = self
                        .avatar
                        .avatars
                        .get(owner)
                        .filter(|old| old.model == model)
                    {
                        mesh.continue_animation(old);
                    }
                    self.avatar.avatars.insert(*owner, mesh);
                }
                // Death and respawn as of the drawn pose, not the newest vitals.
                let life = view.vitals.get(owner).map(|vitals| {
                    let (tick, spawned) = if *owner == view.owner {
                        view.poses
                            .get(owner)
                            .map_or((u64::MAX, None), |p| (p.tick, Some(p.spawn_tick)))
                    } else {
                        (self.motion.ticked_at(*owner).unwrap_or(u64::MAX), None)
                    };
                    crate::avatar::drawn_life(vitals, tick, spawned)
                });
                // A respawned body starts fresh: no corpse pose, and none of
                // the old body's action or gesture threads.
                let mesh = self.avatar.avatars.get_mut(owner).unwrap();
                if life
                    .and_then(|life| life.body)
                    .is_some_and(|body| mesh.set_body(body))
                {
                    self.avatar.avatar_actions.remove(owner);
                    self.avatar.avatar_threads.remove(owner);
                    self.avatar.avatar_action_images.remove(owner);
                }
                let mut ready_hands = Vec::new();
                let mut hidden_nodes = Vec::new();
                if let Some(images) = view.weapons.images.get(owner) {
                    for mounted in images {
                        let image = self.content.weapons.pack.images.get(&mounted.image);
                        if let Some(image) = image {
                            hidden_nodes.extend(image.hide_nodes.iter().cloned());
                        }
                        if let Some((right, left)) =
                            bri_weapons::scripted_arm_pose(&mounted.image, &mounted.state)
                        {
                            ready_hands.extend([(0, right), (1, left)]);
                        } else if let Some(image) = image {
                            if image.both_arms {
                                ready_hands.extend([(0, true), (1, true)]);
                            } else {
                                ready_hands.push((mounted.hand, image.arm_ready));
                            }
                        }
                    }
                }
                self.avatar
                    .avatars
                    .get_mut(owner)
                    .unwrap()
                    .set_hidden_nodes(hidden_nodes);
                // `Player::startSkiing` shows the LSki/RSki nodes in the
                // skier's paint colour, carried by the ski vehicle.
                let skis = view
                    .vitals
                    .get(owner)
                    .and_then(|v| v.mounted)
                    .and_then(|(vehicle, _)| view.vehicles.get(&vehicle))
                    .filter(|info| {
                        self.vehicle_assets
                            .definition(&info.definition)
                            .is_some_and(|d| d.family == bri_vehicles::Family::Skis)
                    })
                    .map(|info| {
                        info.color
                            .or_else(|| view.world.palette.first().copied())
                            .unwrap_or([1.0; 4])
                    });
                self.avatar.avatars.get_mut(owner).unwrap().set_skis(skis);
                let dead = life.is_some_and(|life| life.dead);
                self.avatar.avatars.get_mut(owner).unwrap().set_dead(dead);
                // `Armor::onMount` applies the mount's look limits; the Tank's
                // gunner rides TankTurretPlayer, so it takes that datablock's.
                let look_limits = view
                    .vitals
                    .get(owner)
                    .and_then(|v| v.mounted)
                    .and_then(|(vehicle, seat)| {
                        let info = view.vehicles.get(&vehicle)?;
                        let d = self.vehicle_assets.definition(&info.definition)?;
                        if d.seat_role(usize::from(seat)) == SeatRole::Gunner
                            && d.attachment_mount.is_some()
                        {
                            return self
                                .vehicle_assets
                                .definition("v20.vehicle.tankturretplayer")
                                .map(|t| t.look_limits);
                        }
                        Some(d.look_limits)
                    })
                    // A rule's `setLookLimits` for the body.
                    .or_else(|| view.vitals.get(owner).and_then(|v| v.look_limits));
                let held = crate::avatar::HeldToolPose::from_mounted_images(ready_hands);
                let threads = self
                    .avatar
                    .avatar_threads
                    .get(owner)
                    .filter(|_| !dead)
                    .cloned()
                    .unwrap_or_default();
                let input = crate::avatar::AvatarAnimationInput {
                    look_limits,
                    mount_rotation: self.mounts.rider_rotations.get(owner).copied(),
                    held_tool_pose: if dead {
                        self.combat.hugging.remove(owner);
                        crate::avatar::HeldToolPose::None
                    } else {
                        self.combat.hug_pose(*owner, held)
                    },
                    action: self
                        .avatar
                        .avatar_actions
                        .get(owner)
                        .cloned()
                        .filter(|_| !dead),
                    gesture: threads[3].clone(),
                    body: [threads[0].clone(), threads[1].clone()],
                    dead,
                    sitting: !dead
                        && (view.vitals.get(owner).is_some_and(|v| v.sitting)
                            || view
                                .vitals
                                .get(owner)
                                .and_then(|v| v.mounted)
                                .and_then(|(vehicle, seat)| {
                                    let info = view.vehicles.get(&vehicle)?;
                                    let d = self.vehicle_assets.definition(&info.definition)?;
                                    Some(d.seats.get(usize::from(seat))?.pose == "sit")
                                })
                                .unwrap_or(false)
                            // A mount point's `mountThread`.
                            || view
                                .vitals
                                .get(owner)
                                .and_then(|v| v.ride)
                                .and_then(|ride| {
                                    let mount = presented.get(&ride.mount)?;
                                    let kind = view.archetypes.resolve(mount.archetype);
                                    Some(kind.mount_points.get(usize::from(ride.seat))?.pose == "sit")
                                })
                                .unwrap_or(false)),
                    // Riders hold `root` (`Armor::onMount` sets the action
                    // thread to root and mountThread on thread 0); they do
                    // not run, jump or fall with their mount's motion.
                    tick_state: if view
                        .vitals
                        .get(owner)
                        .is_some_and(|v| v.mounted.is_some() || v.ride.is_some())
                    {
                        Some(bri_sim::player::PlayerState {
                            velocity: [0.0; 3],
                            grounded: true,
                            jetting: false,
                            crouched: false,
                            ..player.clone()
                        })
                    } else {
                        self.motion.ticked(*owner).cloned()
                    },
                    water_coverage: bri_sim::water::deepest(
                        &waters,
                        player.feet,
                        bri_sim::water::body_height(
                            player,
                            &view.archetypes.tuning(player.archetype, player.scale),
                        ),
                    )
                    .map_or(0.0, |(_, c)| c),
                };
                let avatar = self.avatar.avatars.get_mut(owner).unwrap();
                let posed = avatar.pose_with_animation(
                    &self.avatar.avatar_assets,
                    player,
                    self.avatar.animation_time,
                    &input,
                );
                // Add-On code (`avatar.pose`) may draw the body its own way:
                // a ragdoll, a dance. Only the drawing changes.
                if posed.is_ok()
                    && let Some(nodes) = self.addons.client_code.pose(*owner)
                {
                    avatar.override_nodes(&self.avatar.avatar_assets, nodes);
                }
                self.cosmetic_faults.absorb("avatar pose", posed);
            }
            self.mounts.rider_eye = Self::rider_eye(
                &self.avatar.avatars,
                &self.avatar.avatar_assets,
                &self.vehicle_assets,
                &self.vehicles,
                view,
                local,
            );
            let synced = self.fx.effects.sync(view.world.clone(), meshes);
            self.cosmetic_faults.absorb("world effects", synced);
            self.foliage.advance(game_elapsed);
            // Match the actual view for flare occlusion, including third-person camera collision.
            let (eye, yaw, pitch, roll) = Self::view_camera(
                &self.controls,
                presented,
                building,
                self.motion.collision(),
                &self.vehicle_assets,
                &self.vehicles,
                view,
                local,
                self.local_eye()
                    .unwrap_or_else(|| view.archetypes.eye(local)),
                &self.motion.passages(),
                orbit_drawn_offset(&self.controls, &self.avatar.avatars),
            )?;
            let (forward, view_right, view_up) = rolled_view_basis(yaw, pitch, roll);
            self.view.observer_eye = self.controls.observer().map(|_| eye);
            *listener = bri_audio::Listener {
                position: eye.to_array(),
                forward: forward.to_array(),
                up: view_up.to_array(),
            };
            // v20 tints the screen with the liquid the camera is in, and
            // colours player splashes and froth with the liquid they touch.
            self.ui
                .apply(UiUpdate::Underwater(bri_sim::water::screen_tints(
                    &liquids, eye,
                )));
            self.fx.actor_effects.set_liquids(liquids, waters);
            let (local_view_yaw, local_view_pitch) = self.controls.view_angles();
            self.world_items.set_palette(&view.world.palette);
            self.world_items.set_render_my_items(
                self.ui
                    .core
                    .prefs
                    .bool_or("$pref::Player::renderMyItems", true),
            );
            self.fx.weapon_effects.set_palette(&view.world.palette);
            // Shots' trails, spray, smoke and sparks fly on through portals.
            let passages = self.motion.passages();
            self.fx.weapon_effects.set_passages(&passages);
            self.fx.effects.world.set_passages(&passages);
            self.fx.actor_effects.set_passages(&passages);
            let items = self.world_items.sync(
                weapons,
                crate::world_items::WorldItemFrame {
                    tick: view.tick,
                    seconds: self.avatar.animation_time,
                    eye,
                    local_owner: Some(view.owner),
                    first_person: !third_person,
                    // Mirrors, metal and shadows show the player's own
                    // items as others see them, not at the eye.
                    reflected_self: true,
                },
                |owner| {
                    let avatar = self.avatar.avatars.get(&owner)?;
                    let player = presented.get(&owner)?;
                    // Torque draws a first-person image in the eye's frame,
                    // and the eye is the camera: it pitches, rolls and loops
                    // with any seat, so the image stays where it sits on
                    // screen.
                    let eye = if owner == view.owner && !third_person {
                        crate::controls::view_frame(eye, yaw, pitch, roll)
                    } else {
                        let (yaw, pitch) = if owner == view.owner {
                            (local_view_yaw, local_view_pitch)
                        } else {
                            (player.yaw, player.pitch)
                        };
                        avatar.eye_transform(&self.avatar.avatar_assets, yaw, pitch)
                    }?;
                    Some(crate::world_items::MountPose {
                        eye,
                        // Torque mounts an image whose mount point has no
                        // `mountN` node (the dribbled basketball's Mount8) at
                        // the player's own transform.
                        mounts: (0..32)
                            .map(|n| {
                                let node =
                                    avatar.mount_node(&self.avatar.avatar_assets, n as usize);
                                (n, node.unwrap_or_else(|| avatar.body_transform()))
                            })
                            .collect(),
                        actions: (0..32)
                            .filter_map(|n| {
                                Some((
                                    n,
                                    avatar.mount_action(&self.avatar.avatar_assets, n as usize)?,
                                ))
                            })
                            .collect(),
                        velocity: Vec3::from_array(player.velocity),
                        straddle: body_straddle(&self.vehicles, view, &passages, owner, avatar),
                    })
                },
            );
            self.cosmetic_faults.absorb("held and dropped items", items);
            // Ropes the held image draws (`Image::rope`), from its muzzle
            // to where the rope is tied.
            let ropes: Vec<_> = presented
                .iter()
                .filter_map(|(owner, player)| {
                    let tether = player.tether.as_ref()?;
                    Some(crate::weapon_effects::HeldRope {
                        owner: *owner,
                        image: self.world_items.held_image(*owner, 0)?.to_owned(),
                        from: self.world_items.held_muzzle(*owner, 0)?,
                        to: Vec3::from(tether.anchor),
                    })
                })
                .collect();
            let ropes = self
                .fx
                .weapon_effects
                .sync_ropes(&ropes, game_elapsed.as_secs_f32());
            self.cosmetic_faults.absorb("held ropes", ropes);
            let parts = Self::update_weapon_effect_parts(
                &mut self.fx.weapon_effects,
                &mut self.fx.weapon_cues,
                &self.world_items,
                weapons,
                game_elapsed.as_secs_f32(),
            );
            self.cosmetic_faults.absorb("weapon effects", parts);
            let trails = self
                .fx
                .actor_effects
                .update_debris_trails(&self.fx.explosion_debris.trails());
            self.cosmetic_faults.absorb("explosion debris", trails);
            // Show Jets in First Person (`$pref::Player::renderMyJets`, off
            // in v20): one's own jets stay out of one's own eye in first
            // person, but mirrors and portals still show them.
            let own_jets_hidden = !third_person
                && !self
                    .ui
                    .core
                    .prefs
                    .bool_or("$pref::Player::renderMyJets", false);
            self.fx
                .actor_effects
                .set_own_eye(own_jets_hidden.then_some(view.owner));
            let actors = Self::update_actor_effects(
                &mut self.fx.actor_effects,
                &self.avatar.avatar_assets,
                &self.avatar.avatars,
                &self.vehicles,
                &self.vehicle_assets,
                view,
                presented,
                game_elapsed.as_secs_f32(),
                // `fxLight::TestLOS` casts from the camera to the flare,
                // ignoring the player carrying it.
                |at| {
                    Ok(eye.distance(at) < bri_fx_runtime::FLARE_MAX_DISTANCE
                        && building.effect_visible(bri_world::BrickId::MAX, eye, at)?)
                },
                |from, direction, length| {
                    let hit = building.target(from, direction, length).ok()??;
                    Some((hit.distance, hit.normal))
                },
            );
            self.cosmetic_faults
                .absorb("player and vehicle effects", actors);
            self.fx.explosion_shapes.advance(game_elapsed.as_secs_f32());
            self.fx.beams.advance(game_elapsed.as_secs_f32());
            self.fx
                .explosion_debris
                .advance(game_elapsed.as_secs_f32(), |from, to| {
                    let delta = to - from;
                    let length = delta.length();
                    if length < 1e-5 {
                        return None;
                    }
                    let hit = building.target(from, delta / length, length).ok()??;
                    Some(crate::weapon_debris::DebrisHit {
                        fraction: (hit.distance / length).clamp(0., 1.),
                        normal: hit.normal.normalize_or(Vec3::Y),
                    })
                });
            let mut shells = Vec::new();
            for request in self.fx.weapon_effects.take_host_requests() {
                match request {
                    crate::weapon_effects::HostRequest::Shell(cue) => shells.push(cue),
                    crate::weapon_effects::HostRequest::Animation(cue) => {
                        if let bri_sim::presentation::CueKind::WeaponAnimation {
                            actor,
                            thread: 0,
                            sequence,
                            image_hand: Some(hand),
                        } = &cue.kind
                        {
                            self.world_items
                                .restart_image_sequence(*actor, *hand, sequence);
                        }
                    }
                }
            }
            let world_items = &self.world_items;
            let eject = |actor: u64, image: &str, hand: u8| {
                world_items
                    .mounted_node(actor, hand, image, "ejectPoint")
                    .or_else(|_| world_items.mounted_node(actor, hand, image, "muzzlePoint"))
                    .ok()
            };
            let queued = self.fx.weapon_shells.cues(&shells, eject, |actor| {
                presented
                    .get(&actor)
                    .map_or(Vec3::ZERO, |p| Vec3::from(p.velocity))
            });
            self.cosmetic_faults.absorb("gun casings", queued);
            let moved =
                self.fx
                    .weapon_shells
                    .advance(game_elapsed.as_secs_f32(), eject, |from, to| {
                        let delta = to - from;
                        let length = delta.length();
                        if length < 1e-5 {
                            return None;
                        }
                        let hit = building.target(from, delta / length, length).ok()??;
                        Some(crate::weapon_debris::DebrisHit {
                            fraction: (hit.distance / length).clamp(0., 1.),
                            normal: hit.normal.normalize_or(Vec3::Y),
                        })
                    });
            self.cosmetic_faults.absorb("gun casings", moved);
            self.audio
                .sync_projectiles(&weapons.projectiles, &self.content.weapons.pack);
            // Physics Quality (or the console's maxdebris) picks the limit.
            let limit = bri_ui::screens::options::debris_limit(&self.ui.core.prefs);
            if limit != self.fx.brick_debris.limit() {
                self.fx.brick_debris.set_limit(limit);
            }
            // What debris costs this frame, so a PC it outgrows keeps less.
            let debris_started = std::time::Instant::now();
            let kills = std::mem::take(&mut self.fx.brick_kills);
            let thrown = self.fx.brick_debris.cues(&kills, building);
            // A kill announced after its brick started fading out stops the
            // fade (see the chunk rebuild's `observe`), and a dead brick
            // leaves its drawn chunk this frame.
            for cue in &kills {
                if let bri_sim::presentation::CueKind::BrickKill { brick, .. } = cue.kind {
                    self.fx.brick_fades.settle(brick);
                    if self.fx.brick_debris.is_dead(brick) {
                        self.scene
                            .chunk_hides
                            .entry(brick)
                            .or_insert((crate::world_chunks::chunk_key(cue.position), false));
                    }
                }
            }
            if self
                .cosmetic_faults
                .absorb("brick debris", thrown)
                .unwrap_or(0)
                > 0
            {
                // Newly dead bricks are not hidden bricks to reveal.
                self.gpu.hidden_uploaded = None;
            }
            // Debris and Add-On bodies are local and cosmetic: everyone
            // drawn here shoves them, and nothing about them goes back to
            // the server.
            let bodies = self.addons.client_code.has_bodies();
            let (pushers, shots) = if !self.fx.brick_debris.is_empty() || bodies {
                let pushers = Self::pushers(
                    self.motion.presented(),
                    view,
                    &self.vehicles,
                    &self.vehicle_assets,
                );
                let shots: Vec<_> = view
                    .weapons
                    .fired()
                    .map(|p| crate::local_physics::Shot {
                        id: p.id,
                        position: p.position,
                        velocity: p.velocity,
                    })
                    .collect();
                (pushers, shots)
            } else {
                (Vec::new(), Vec::new())
            };
            if !self.fx.brick_debris.is_empty() {
                self.fx.brick_debris.push(&pushers);
                self.fx.brick_debris.shots(&shots);
            }
            let moved = self
                .fx
                .brick_debris
                .advance(game_elapsed.as_secs_f32().min(0.25), building);
            self.cosmetic_faults.absorb("brick debris", moved);
            self.fx.brick_debris.spent(debris_started.elapsed());
            if bodies {
                // A corpse does not shove bodies: it may be the one lying in
                // them (a ragdoll drawn over it).
                let alive: Vec<_> = pushers
                    .iter()
                    .filter(|p| {
                        p.id & 1 << 63 != 0 || view.vitals.get(&p.id).is_none_or(|v| v.alive)
                    })
                    .copied()
                    .collect();
                let moved = self.addons.client_code.advance_physics(
                    game_elapsed.as_secs_f32().min(0.25),
                    building,
                    &alive,
                    &shots,
                );
                self.cosmetic_faults.absorb("Add-On bodies", moved);
            }
            self.fx
                .brick_fades
                .advance(game_elapsed.as_secs_f32(), &self.scene.chunks_left_out);
            // The avatar/image shell and sequence playback APIs are still a host
            // boundary. Retain requests in the adapter and expose its queue-drop
            // diagnostics; do not claim these have been rendered or played.
            let advanced = self.fx.effects.advance(
                game_elapsed.as_secs_f32(),
                eye,
                Vec3::ZERO,
                |id, from, to| building.effect_visible(id, from, to),
            );
            self.cosmetic_faults.absorb("world effects", advanced);
            let weather = self.weather.advance(
                game_elapsed.as_secs_f32(),
                bri_weather::CameraState {
                    position: eye,
                    forward,
                    right: view_right,
                    up: view_up,
                    velocity: Vec3::from_array(local.velocity),
                },
                building,
            );
            self.cosmetic_faults.absorb("weather", weather);
        }
        Ok(())
    }
}
