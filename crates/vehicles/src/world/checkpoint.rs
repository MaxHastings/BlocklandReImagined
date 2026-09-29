use super::*;
use std::collections::BTreeSet;

/// A durable native checkpoint. Host persists this together with actors, terrain,
/// buildings and other shared-world systems at the same fixed-tick boundary.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub schema_version: u32,
    pub content_fingerprint: String,
    pub tick: u64,
    pub vehicles: Vec<VehicleSave>,
    pub pending_respawns: Vec<RespawnSave>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RespawnSave {
    pub due_tick: u64,
    pub spawn: Spawn,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelSave {
    pub rotation: f32,
    pub steering: f32,
    pub engine_force: f32,
    pub brake: f32,
    pub forward_impulse: f32,
    pub side_impulse: f32,
    pub suspension_force: f32,
    pub suspension_length: f32,
    pub in_contact: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleSave {
    pub spawn: Spawn,
    pub transform: Transform,
    pub velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub sleeping: bool,
    pub seats: Vec<Option<Occupant>>,
    pub controls: Vec<Controls>,
    pub wheels: Vec<WheelSave>,
    pub damage: f32,
    pub born_tick: u64,
    pub destroyed_tick: Option<u64>,
    pub last_damage_owner: OwnerId,
    pub last_shot_tick: Option<u64>,
    pub charge_started_tick: Option<u64>,
    pub charge: u8,
    pub turret_damage: Option<f32>,
    pub steering: f32,
    pub animation: String,
    pub water: bool,
    pub water_coverage: f32,
    pub energy: f32,
    pub jetting: bool,
    pub energy_phase: u8,
    pub previous_velocity: [f32; 3],
    pub jump_held: bool,
    pub fire_held: bool,
    pub mounted_once: bool,
    pub mouse_steering: [f32; 2],
    pub grounded: bool,
    /// A player-type mount's whole motor state (its Torque tick phase and
    /// the last tick's feet included); older checkpoints restore from the
    /// transform, velocity and `grounded` above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<PlayerState>,
}
impl Checkpoint {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 16 * 1024 * 1024, "checkpoint exceeds 16MiB");
        Ok(serde_json::from_slice(bytes)?)
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self)?;
        ensure!(bytes.len() <= 16 * 1024 * 1024, "checkpoint exceeds 16MiB");
        Ok(bytes)
    }
}
impl VehiclesWorld {
    /// No unconsumed intents or half-stepped state may be silently discarded.
    pub fn checkpoint(&self, world: &PhysicsWorld) -> Result<Checkpoint> {
        ensure!(
            !self.step_pending,
            "checkpoint requires completed physics tick"
        );
        ensure!(
            self.intents.is_empty(),
            "drain and apply vehicle intents before checkpoint"
        );
        let mut vehicles = vec![];
        for v in self.instances.values() {
            let b = world.bodies.get(v.body).context("shared body missing")?;
            let d = &self.catalog[&v.spawn.definition];
            vehicles.push(VehicleSave {
                spawn: v.spawn.clone(),
                transform: transform(b.position()),
                velocity: v.velocity(d, b).to_array(),
                angular_velocity: b.angvel().to_array(),
                sleeping: b.is_sleeping(),
                seats: v.seats.clone(),
                controls: v.controls.clone(),
                wheels: v.controller.as_ref().map_or_else(Vec::new, |c| {
                    c.wheels()
                        .iter()
                        .enumerate()
                        .map(|(i, w)| WheelSave {
                            rotation: w.rotation,
                            steering: w.steering,
                            engine_force: w.engine_force,
                            brake: w.brake,
                            forward_impulse: w.forward_impulse,
                            side_impulse: w.side_impulse,
                            suspension_force: w.wheel_suspension_force,
                            suspension_length: v
                                .restored_suspension
                                .as_ref()
                                .map_or(w.raycast_info().suspension_length, |s| s[i]),
                            in_contact: v
                                .restored_contacts
                                .as_ref()
                                .map_or(w.raycast_info().is_in_contact, |s| s[i]),
                        })
                        .collect()
                }),
                damage: v.damage,
                born_tick: v.born,
                destroyed_tick: v.dead_at,
                last_damage_owner: v.last_damage,
                last_shot_tick: v.last_shot,
                charge_started_tick: v.charge_started,
                charge: v.charge,
                turret_damage: v.turret_damage,
                steering: v.steering,
                animation: v.animation.clone(),
                water: v.water,
                water_coverage: v.water_coverage,
                energy: v.energy,
                jetting: v.jetting,
                energy_phase: v.energy_phase,
                previous_velocity: v.previous_velocity.to_array(),
                jump_held: v.jump_held,
                fire_held: v.fire_held,
                mounted_once: v.mounted_once,
                mouse_steering: v.mouse_steering,
                grounded: v.actor.as_ref().is_some_and(|a| a.state().grounded),
                actor: v.actor.as_ref().map(|a| a.state().clone()),
            });
        }
        Ok(Checkpoint {
            schema_version: 2,
            content_fingerprint: self.catalog_fingerprint.clone(),
            tick: self.tick,
            vehicles,
            pending_respawns: self
                .respawns
                .iter()
                .map(|p| RespawnSave {
                    due_tick: p.tick,
                    spawn: p.spawn.clone(),
                })
                .collect(),
        })
    }
    /// Replaces only this component's vehicles. The actor callback must resolve
    /// real host identities and permissions; serialized owner IDs never authorize.
    /// All returned validation errors leave this component AND physics untouched.
    /// On success use the returned snapshot to reconcile mounted player adapters.
    pub fn restore_checkpoint(
        &mut self,
        world: &mut PhysicsWorld,
        checkpoint: Checkpoint,
        mut actor_allowed: impl FnMut(Occupant, &Spawn, usize) -> bool,
    ) -> Result<Snapshot> {
        ensure!(
            !self.step_pending && self.intents.is_empty(),
            "restore requires idle tick boundary and drained intents"
        );
        ensure!(
            (world.integration_parameters.dt - FIXED_DT).abs() < 1e-6,
            "restore requires shared 120Hz world"
        );
        self.validate_checkpoint(&checkpoint, &mut actor_allowed)?;
        // Preconstruct *all* body/collider builders before publishing any state.
        // They contain no handles, and no second physics world is used.
        let mut prepared = Vec::with_capacity(checkpoint.vehicles.len());
        for v in &checkpoint.vehicles {
            let mut spawn = v.spawn.clone();
            spawn.transform = v.transform.clone();
            let d = &self.catalog[&spawn.definition];
            prepared.push(prepare_spawn(&spawn, d, world.gravity.length())?);
        }
        // No fallible operation remains below this point. Insertion/removal is
        // synchronous and no callbacks or physics step observe the intermediate set.
        let mut next = Self {
            catalog: self.catalog.clone(),
            catalog_fingerprint: self.catalog_fingerprint.clone(),
            instances: BTreeMap::new(),
            occupied: BTreeMap::new(),
            tick: checkpoint.tick,
            intents: vec![],
            respawns: checkpoint
                .pending_respawns
                .iter()
                .map(|p| PendingRespawn {
                    tick: p.due_tick,
                    spawn: p.spawn.clone(),
                })
                .collect(),
            step_pending: false,
            prediction: self.prediction,
        };
        for (saved, prepared) in checkpoint.vehicles.into_iter().zip(prepared) {
            let id = saved.spawn.id;
            let d = &next.catalog[&saved.spawn.definition];
            let (builder, collider, turret) = prepared;
            let (body, collider) = world.insert(builder, collider);
            let turret_collider = if saved.turret_damage.is_none_or(|damage| damage < 250.) {
                turret.map(|c| world.insert_collider(c, Some(body)))
            } else {
                None
            };
            if let (Some(handle), Some(mount)) = (turret_collider, &d.attachment_mount) {
                let mut pose = local_pose(mount, saved.spawn.scale);
                pose.rotation *= Quat::from_rotation_y(
                    saved.controls.get(2).copied().unwrap_or_default().aim_yaw,
                );
                world.colliders[handle].set_position_wrt_parent(pose);
            }
            let mut controller = if saved.destroyed_tick.is_some() || d.wheels.is_empty() {
                None
            } else {
                Some(build_controller(body, d, saved.spawn.scale))
            };
            if let Some(c) = &mut controller {
                for (w, s) in c.wheels_mut().iter_mut().zip(&saved.wheels) {
                    w.rotation = s.rotation;
                    w.steering = s.steering;
                    w.engine_force = s.engine_force;
                    w.brake = s.brake;
                    w.forward_impulse = s.forward_impulse;
                    w.side_impulse = s.side_impulse;
                    w.wheel_suspension_force = s.suspension_force;
                }
            }
            let actor = if d.is_actor() {
                let (feet, yaw) = super::feet_and_yaw(&saved.transform);
                let mut actor = Player::adopt(
                    body,
                    collider,
                    feet,
                    yaw,
                    super::actor_tuning(d, saved.spawn.scale),
                )
                .expect("validated mount");
                actor.set_motion(Vec3::from_array(saved.velocity), saved.grounded);
                if let Some(state) = &saved.actor {
                    let state = PlayerState {
                        owner: actor.state().owner,
                        ..state.clone()
                    };
                    let tuning = actor.tuning().clone();
                    actor
                        .restore(world, state, tuning)
                        .expect("validated mount state");
                }
                Some(actor)
            } else {
                None
            };
            let b = &mut world.bodies[body];
            b.set_linvel(Vec3::from_array(saved.velocity), true);
            b.set_angvel(Vec3::from_array(saved.angular_velocity), true);
            if saved.sleeping {
                b.sleep();
            }
            for (index, occupant) in saved.seats.iter().enumerate() {
                if let Some(o) = occupant {
                    next.occupied.insert(o.id, (id, index));
                }
            }
            next.instances.insert(
                id,
                Instance {
                    restored_suspension: Some(
                        saved.wheels.iter().map(|w| w.suspension_length).collect(),
                    ),
                    restored_contacts: Some(saved.wheels.iter().map(|w| w.in_contact).collect()),
                    spawn: saved.spawn,
                    body,
                    collider,
                    turret_collider,
                    controller,
                    seats: saved.seats,
                    controls: saved.controls,
                    damage: saved.damage,
                    born: saved.born_tick,
                    dead_at: saved.destroyed_tick,
                    last_damage: saved.last_damage_owner,
                    last_shot: saved.last_shot_tick,
                    charge_started: saved.charge_started_tick,
                    turret_damage: saved.turret_damage,
                    charge: saved.charge,
                    steering: saved.steering,
                    animation: saved.animation,
                    water: saved.water,
                    previous_velocity: Vec3::from_array(saved.previous_velocity),
                    jump_held: saved.jump_held,
                    fire_held: saved.fire_held,
                    mounted_once: saved.mounted_once,
                    water_coverage: saved.water_coverage,
                    energy: saved.energy,
                    jetting: saved.jetting,
                    energy_phase: saved.energy_phase,
                    mouse_steering: saved.mouse_steering,
                    actor,
                },
            );
        }
        for old in self.instances.values() {
            world.remove_body(old.body);
        }
        *self = next;
        // Rebuild collider poses/broadphase against the existing host geometry;
        // this does not advance simulation time or execute game callbacks.
        bri_physics::detect_collisions(world);
        Ok(self.snapshot(world))
    }
    fn validate_checkpoint(
        &self,
        c: &Checkpoint,
        allowed: &mut impl FnMut(Occupant, &Spawn, usize) -> bool,
    ) -> Result<()> {
        ensure!(c.schema_version == 2, "unsupported checkpoint schema");
        ensure!(
            c.content_fingerprint == self.catalog_fingerprint,
            "checkpoint content fingerprint mismatch"
        );
        ensure!(
            c.tick <= u64::MAX - 120 * 86400 * 2,
            "checkpoint tick too close to overflow"
        );
        ensure!(
            c.vehicles.len() <= 4096 && c.pending_respawns.len() <= 4096,
            "too many saved vehicles"
        );
        let mut ids = BTreeSet::new();
        let mut occupants = BTreeSet::new();
        let mut spawns = BTreeSet::new();
        let validate_spawn = |s: &Spawn| -> Result<()> {
            ensure!(
                self.catalog.contains_key(&s.definition),
                "unknown saved vehicle content"
            );
            ensure!(
                s.scale.is_finite() && (0.2..=5.).contains(&s.scale),
                "invalid saved scale"
            );
            valid_transform(&s.transform)?;
            ensure!(
                s.respawn_ticks.is_none_or(|n| n <= 120 * 86400),
                "invalid saved respawn duration"
            );
            Ok(())
        };
        for v in &c.vehicles {
            let s = &v.spawn;
            validate_spawn(s)?;
            ensure!(ids.insert(s.id), "duplicate saved vehicle identity");
            if let Some(id) = s.spawn_id {
                ensure!(spawns.insert(id), "duplicate saved spawn brick");
            }
            let d = &self.catalog[&s.definition];
            valid_transform(&v.transform)?;
            ensure!(
                v.velocity
                    .iter()
                    .chain(v.angular_velocity.iter())
                    .chain(v.previous_velocity.iter())
                    .all(|x| x.is_finite() && x.abs() < 1e6),
                "invalid body motion"
            );
            ensure!(
                !v.sleeping
                    || v.velocity
                        .iter()
                        .chain(v.angular_velocity.iter())
                        .all(|x| *x == 0.),
                "sleeping body has motion"
            );
            if let Some(a) = &v.actor {
                ensure!(
                    d.is_actor()
                        && a.feet
                            .iter()
                            .chain(&a.velocity)
                            .chain(&a.tick.feet)
                            .chain(&a.tick.from)
                            .all(|x| x.is_finite() && x.abs() < 1e6)
                        && a.velocity.iter().all(|x| x.abs() <= 1000.)
                        && a.yaw.is_finite()
                        && a.pitch.is_finite()
                        && a.tick.phase < 96,
                    "invalid saved mount state"
                );
            }
            ensure!(
                v.seats.len() == d.seats.len() && v.controls.len() == d.seats.len(),
                "invalid saved seat/control layout"
            );
            for (index, occupant) in v.seats.iter().enumerate() {
                v.controls[index].validate()?;
                if let Some(o) = occupant {
                    ensure!(occupants.insert(o.id), "duplicate occupant identity");
                    ensure!(allowed(*o, s, index), "saved actor absent or unauthorized");
                } else {
                    let control = v.controls[index];
                    ensure!(
                        !control.fire
                            && !control.jet
                            && !control.brake
                            && !control.jump
                            && control.throttle == 0.
                            && control.steer == 0.
                            && control.pitch == 0.
                            && control.roll == 0.
                            && control.vertical == 0.
                            && control.strafe == 0.,
                        "empty seat has active controls"
                    );
                }
            }
            ensure!(
                v.damage.is_finite() && v.damage >= 0. && v.damage <= d.max_damage,
                "invalid saved damage"
            );
            ensure!(
                v.born_tick <= c.tick
                    && v.destroyed_tick
                        .is_none_or(|n| n >= v.born_tick && n <= c.tick)
                    && v.last_shot_tick
                        .is_none_or(|n| n >= v.born_tick && n <= c.tick)
                    && v.charge_started_tick
                        .is_none_or(|n| n >= v.born_tick && n <= c.tick),
                "invalid saved timer"
            );
            ensure!(
                v.destroyed_tick.is_some() == (v.damage >= d.max_damage),
                "damage/death timer disagree"
            );
            if let Some(dead) = v.destroyed_tick {
                ensure!(dead + d.burn_ticks >= c.tick, "expired wreck retained");
            }
            ensure!(
                v.turret_damage.is_some() == d.attachment_model.is_some()
                    && v.turret_damage
                        .is_none_or(|n| n.is_finite() && (0. ..=250.).contains(&n)),
                "invalid turret state"
            );
            ensure!(
                v.steering.is_finite()
                    && v.steering.abs() <= d.max_steering + 0.001
                    && v.mouse_steering
                        .iter()
                        .all(|x| x.is_finite() && x.abs() <= d.max_steering.max(0.01) + 0.001)
                    && v.water_coverage.is_finite()
                    && (0. ..=1.).contains(&v.water_coverage),
                "invalid steering/water state"
            );
            ensure!(
                v.energy.is_finite()
                    && (0. ..=d.energy.maximum).contains(&v.energy)
                    && v.energy_phase < 96
                    && (!v.jetting || d.energy.maximum > 0.),
                "invalid saved energy state"
            );
            ensure!(
                v.animation.len() <= 64 && !v.animation.is_empty(),
                "invalid animation state"
            );
            ensure!(
                v.mounted_once || v.seats.iter().all(Option::is_none),
                "mounted lifecycle inconsistent"
            );
            let expected_wheels = if v.destroyed_tick.is_some() {
                0
            } else {
                d.wheels.len()
            };
            ensure!(
                v.wheels.len() == expected_wheels,
                "invalid saved wheel count"
            );
            for w in &v.wheels {
                ensure!(
                    [
                        w.rotation,
                        w.steering,
                        w.engine_force,
                        w.brake,
                        w.forward_impulse,
                        w.side_impulse,
                        w.suspension_force,
                        w.suspension_length
                    ]
                    .iter()
                    .all(|x| x.is_finite() && x.abs() < 1e9)
                        && w.brake >= 0.
                        && w.suspension_length >= 0.,
                    "invalid saved wheel state"
                );
            }
            if let Some(start) = v.charge_started_tick {
                let weapon = d.weapon.as_ref().context("charge without weapon")?;
                let seat = d
                    .seats
                    .iter()
                    .position(|s| s.weapon)
                    .context("charge without weapon seat")?;
                ensure!(
                    weapon.charge_ticks > 0
                        && v.seats[seat].is_some()
                        && v.controls[seat].fire
                        && v.fire_held
                        && v.destroyed_tick.is_none(),
                    "invalid active charge"
                );
                let expected = (1 + c.tick.saturating_sub(1).saturating_sub(start)
                    / weapon.charge_ticks)
                    .min(u64::from(weapon.charge_steps)) as u8;
                ensure!(v.charge == expected, "charge timer/count mismatch");
            } else {
                ensure!(v.charge == 0, "charge count without timer");
            }
            if d.family == Family::Tumble {
                ensure!(c.tick - v.born_tick < 5400, "expired tumble retained");
            }
        }
        for p in &c.pending_respawns {
            validate_spawn(&p.spawn)?;
            ensure!(ids.insert(p.spawn.id), "duplicate pending vehicle identity");
            let spawn = p
                .spawn
                .spawn_id
                .context("pending respawn lacks brick identity")?;
            ensure!(spawns.insert(spawn), "duplicate pending spawn brick");
            ensure!(
                p.spawn.respawn_ticks.is_some()
                    && p.due_tick > c.tick
                    && p.due_tick - c.tick <= 120 * 86400,
                "invalid respawn deadline"
            );
        }
        Ok(())
    }
}
fn valid_transform(t: &Transform) -> Result<()> {
    ensure!(
        t.position.iter().all(|x| x.is_finite() && x.abs() < 1e7)
            && t.rotation.iter().all(|x| x.is_finite())
            && (Quat::from_array(t.rotation).length_squared() - 1.).abs() < 0.001,
        "invalid saved transform"
    );
    Ok(())
}
