//! Torque vehicle datablocks to native vehicle definitions. Offline only.
use anyhow::{Context, Result, ensure};
use bri_content::shape::Shape;
use bri_vehicles::schema::*;
use glam::{Mat4, Quat, Vec3};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[derive(Clone)]
pub struct Block {
    pub package: String,
    pub fields: BTreeMap<String, String>,
    pub evidence: Evidence,
}
pub fn number(b: &Block, key: &str, fallback: f32) -> f32 {
    b.fields
        .get(&key.to_lowercase())
        .and_then(|s| {
            s.split('*')
                .map(|x| x.trim().parse::<f32>().ok())
                .try_fold(1., |a, b| b.map(|b| a * b))
        })
        .unwrap_or(fallback)
}
pub fn field(b: &Block, key: &str) -> String {
    b.fields
        .get(&key.to_lowercase())
        .cloned()
        .unwrap_or_default()
}
pub fn truth(b: &Block, key: &str) -> bool {
    matches!(field(b, key).to_lowercase().as_str(), "true" | "1")
}
/// Torque's `dAtob`: `true`, or any nonzero number.
fn torque_bool(b: &Block, key: &str, default: bool) -> bool {
    let v = field(b, key).trim().to_lowercase();
    if v.is_empty() {
        default
    } else {
        v == "true" || v.parse::<f32>().is_ok_and(|n| n != 0.)
    }
}
/// `WheeledVehicleData::onAdd` in the recovered v20 core scripts: which of
/// `count` wheels steer and which are powered. Past six wheels it sets the
/// first six like six wheels, and later wheels keep Torque's defaults.
pub fn v20_wheel(count: usize, i: usize) -> (f32, bool) {
    let (steer, powered): (usize, std::ops::Range<usize>) = match count {
        1 => (1, 0..1),
        2 => (2, 0..2),
        3 => (1, 1..3),
        4 => (2, 2..4),
        5 => (1, 1..5),
        _ => (2, 2..6),
    };
    if i >= 6 {
        (0., true)
    } else {
        (if i < steer { 1. } else { 0. }, powered.contains(&i))
    }
}
/// Blockland's flying fields on a `WheeledVehicleData`; a datablock that
/// sets none of them nonzero drives as a car.
pub fn wheeled_flight(b: &Block) -> Option<WheeledFlightSettings> {
    let flies = [
        "forwardThrust",
        "reverseThrust",
        "lift",
        "maxForwardVel",
        "maxReverseVel",
        "horizontalSurfaceForce",
        "verticalSurfaceForce",
        "stallSpeed",
    ]
    .iter()
    .any(|k| number(b, k, 0.) != 0.)
        || torque_bool(b, "isSled", false);
    flies.then(|| WheeledFlightSettings {
        max_forward_vel: number(b, "maxForwardVel", 0.),
        max_reverse_vel: number(b, "maxReverseVel", 0.),
        horizontal_surface_force: number(b, "horizontalSurfaceForce", 0.),
        vertical_surface_force: number(b, "verticalSurfaceForce", 0.),
        stall_speed: number(b, "stallSpeed", 0.),
        sled: torque_bool(b, "isSled", false),
    })
}
pub fn virtual_path(package: &str, path: &str) -> String {
    if let Some(relative) = path.strip_prefix("./") {
        format!("Add-Ons/{package}/{relative}")
    } else {
        path.into()
    }
}
/// Authored Torque vector ("x y z") in native X-right, Y-up, -Z-forward axes.
pub fn vector(b: &Block, key: &str) -> Option<Vec3> {
    let v: Vec<f32> = field(b, key)
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    (v.len() == 3).then(|| native(Vec3::new(v[0], v[1], v[2])))
}
pub fn native(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, -v.y)
}
/// Object-space bounds stored in a DTS v24 header, in Torque axes.
pub fn dts_bounds(bytes: &[u8]) -> Result<(Vec3, Vec3)> {
    let word = |i: usize| -> Result<[u8; 4]> {
        Ok(bytes
            .get(i * 4..i * 4 + 4)
            .context("truncated DTS header")?
            .try_into()?)
    };
    ensure!(
        u32::from_le_bytes(word(0)?) & 0xff == 24,
        "unsupported DTS version"
    );
    // Four file words, 17 counts, two smallest-detail words and one guard
    // precede radius, tube radius, center and the bounds box.
    let f = |i: usize| -> Result<f32> { Ok(f32::from_le_bytes(word(4 + 20 + i)?)) };
    let min = Vec3::new(f(5)?, f(6)?, f(7)?);
    let max = Vec3::new(f(8)?, f(9)?, f(10)?);
    ensure!(
        min.is_finite() && max.is_finite() && min.cmplt(max).all(),
        "invalid DTS bounds"
    );
    Ok((min, max))
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn nodes(shape: &Shape) -> Vec<Mat4> {
    fn one(i: usize, s: &Shape, cache: &mut [Option<Mat4>]) -> Mat4 {
        if let Some(m) = cache[i] {
            return m;
        }
        let n = &s.nodes[i];
        let local = Mat4::from_rotation_translation(
            Quat::from_array(n.rotation),
            Vec3::from_array(n.translation),
        );
        let m = n.parent.map_or(local, |p| one(p, s, cache) * local);
        cache[i] = Some(m);
        m
    }
    let mut cache = vec![None; shape.nodes.len()];
    (0..cache.len())
        .map(|i| one(i, shape, &mut cache))
        .collect()
}
pub fn transform(m: Mat4) -> Transform {
    let (_, q, p) = m.to_scale_rotation_translation();
    Transform {
        position: p.to_array(),
        rotation: q.to_array(),
    }
}
pub fn find_node(s: &Shape, n: &[Mat4], name: &str) -> Result<Transform> {
    let i = s
        .nodes
        .iter()
        .position(|x| x.name.eq_ignore_ascii_case(name))
        .with_context(|| format!("missing node {name}"))?;
    Ok(transform(n[i]))
}
pub fn collision(s: &Shape, n: &[Mat4]) -> Vec<Vec<[f32; 3]>> {
    let mut hulls = vec![];
    for detail in &s.details {
        if !detail.collision {
            continue;
        }
        for o in s
            .objects
            .iter()
            .skip(detail.object_start)
            .take(detail.object_count)
        {
            if let Some(Some(mesh)) = o
                .meshes
                .get(detail.mesh_offset)
                .and_then(|i| s.meshes.get(*i))
            {
                let m = o.node.map_or(Mat4::IDENTITY, |i| n[i]);
                hulls.push(
                    mesh.positions
                        .iter()
                        .take(mesh.frame_vertices)
                        .map(|p| m.transform_point3(Vec3::from_array(*p)).to_array())
                        .collect(),
                );
            }
        }
    }
    hulls
}
pub fn bounds(s: &Shape, n: &[Mat4]) -> (Vec3, Vec3) {
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for o in &s.objects {
        for i in &o.meshes {
            if let Some(mesh) = &s.meshes[*i] {
                let m = o.node.map_or(Mat4::IDENTITY, |i| n[i]);
                for p in mesh.positions.iter().take(mesh.frame_vertices) {
                    let p = m.transform_point3(Vec3::from_array(*p));
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
        }
    }
    (lo, hi)
}
pub fn box_hull(lo: Vec3, hi: Vec3) -> Vec<[f32; 3]> {
    [lo.x, hi.x]
        .into_iter()
        .flat_map(|x| {
            [lo.y, hi.y]
                .into_iter()
                .flat_map(move |y| [lo.z, hi.z].into_iter().map(move |z| [x, y, z]))
        })
        .collect()
}
/// Lowers one vehicle datablock into its native definition. `id(kind, name)`
/// names the vehicle and the projectiles it references, so the same lowering
/// serves the base game (`v20`) and a community add-on in its own namespace.
/// `blocks`, `models` and `files` are keyed by lower-case datablock name or
/// virtual path; `blocks` must already carry inherited fields when the
/// caller wants inheritance applied.
pub fn lower(
    name: &str,
    family: Family,
    blocks: &BTreeMap<String, Block>,
    models: &BTreeMap<String, (String, Shape)>,
    files: &BTreeMap<String, (String, Vec<u8>)>,
    id: &dyn Fn(&str, &str) -> String,
) -> Result<Definition> {
    let b = blocks.get(&name.to_lowercase()).context("datablock")?;
    let vp = virtual_path(&b.package, &field(b, "shapefile"));
    let (model, s) = models
        .get(&vp.to_lowercase())
        .with_context(|| format!("unconverted required model {vp}"))?;
    let n = nodes(s);
    let (lo, hi) = bounds(s, &n);
    let (shape_min, shape_max) =
        dts_bounds(&files.get(&vp.to_lowercase()).context("source shape")?.1)?;
    let inertia_box = match vector(b, "massBox").map(Vec3::abs) {
        // FlyingVehicle keeps Vehicle's unit-sphere inertia (0.4 mass on
        // every axis); only WheeledVehicle reads massBox. A cube of side
        // sqrt(2.4) has that inertia.
        _ if family == Family::Flying => Vec3::splat(2.4_f32.sqrt()),
        Some(size) if size.cmpgt(Vec3::ZERO).all() => size,
        _ => native(shape_max - shape_min).abs(),
    };
    let mut hulls = collision(s, &n);
    let mut adaptations = vec![];
    if hulls.is_empty() {
        let (bl, bh) = if matches!(
            family,
            Family::Horse | Family::Cannon | Family::Rowboat | Family::Turret
        ) {
            let dims = match family {
                Family::Horse => Vec3::new(2.5, 2.4, 2.5),
                Family::Cannon => Vec3::new(3., 1., 3.),
                Family::Rowboat => Vec3::new(2.5, 1., 2.5),
                _ => Vec3::new(2.5, 1.7, 2.5),
            };
            adaptations.push("PlayerData boundingBox authoring units divided by four as engine player box convention; actor base at feet".into());
            (
                Vec3::new(-dims.x / 2., 0., -dims.z / 2.),
                Vec3::new(dims.x / 2., dims.y, dims.z / 2.),
            )
        } else {
            adaptations.push(
                "No authored collision detail: convex visual bounds used and requires review"
                    .into(),
            );
            (lo, hi)
        };
        hulls.push(box_hull(bl, bh));
    }
    let mut seats = vec![];
    for i in 0..number(
        b,
        "numMountPoints",
        if matches!(family, Family::Skis | Family::Tumble) {
            1.
        } else {
            0.
        },
    ) as usize
    {
        let node_index = number(b, &format!("mountNode[{i}]"), i as f32) as usize;
        let node = format!("mount{node_index}");
        seats.push(Seat {
            transform: find_node(s, &n, &node)?,
            node,
            pose: if matches!(family, Family::Skis | Family::Tumble) {
                "root".into()
            } else {
                field(b, &format!("mountThread[{i}]"))
            },
            controls: i == 0 && family != Family::Tumble,
            weapon: matches!(family, Family::Cannon | Family::Turret)
                || (name == "TankVehicle" && i == 2),
        });
    }
    let mut wheels = vec![];
    let wheel_count = number(b, "numWheels", if family == Family::Skis { 4. } else { 0. }) as usize;
    for i in 0..wheel_count {
        let tire = blocks
            .get(&if family == Family::Skis {
                "nothingtire".into()
            } else {
                field(b, "defaultTire").to_lowercase()
            })
            .context("tire")?;
        let spring = blocks
            .get(&if family == Family::Skis {
                "skispring".into()
            } else {
                field(b, "defaultSpring").to_lowercase()
            })
            .context("spring")?;
        let wheel_vp = virtual_path(&tire.package, &field(tire, "shapeFile"));
        let wheel_model = &models
            .get(&wheel_vp.to_lowercase())
            .context("wheel model")?
            .0;
        // WheeledVehicleTire::preload replaces the scripted radius with half
        // the tire shape's height; the tire is built with its hub along +Y.
        let (tire_min, tire_max) = dts_bounds(
            &files
                .get(&wheel_vp.to_lowercase())
                .context("wheel source shape")?
                .1,
        )?;
        let position = find_node(s, &n, &format!("hub{i}"))?.position;
        wheels.push(Wheel {
            position,
            radius: (tire_max.z - tire_min.z) / 2.,
            rest_length: number(spring, "length", 0.4),
            spring: number(spring, "force", 6000.),
            damping: number(spring, "damping", 800.),
            friction: number(tire, "staticFriction", 5.),
            // TankVehicle::onAdd steers all four wheels and powers them all;
            // skis have no engine.
            steering: if name == "TankVehicle" {
                if i < 2 { 1. } else { -0.8 }
            } else {
                v20_wheel(wheel_count, i).0
            },
            powered: name == "TankVehicle"
                || (family != Family::Skis && v20_wheel(wheel_count, i).1),
            model: wheel_model.clone(),
            // WheeledVehicle turns each tire a quarter turn about up so its
            // +Y face points outward: clockwise (Torque's positive) on the right.
            model_rotation: Quat::from_rotation_y(if position[0] > 0. {
                -std::f32::consts::FRAC_PI_2
            } else {
                std::f32::consts::FRAC_PI_2
            })
            .to_array(),
        });
    }
    let mut attachment_model = None;
    let mut attachment_collision_hulls = vec![];
    let mut attachment_mount = None;
    let mut attachment_fallback_seat = None;
    let mut pivot = [0.; 3];
    let weapon = if matches!(family, Family::Cannon | Family::Turret) || name == "TankVehicle" {
        let cannon = family == Family::Cannon;
        let muzzle = if name == "TankVehicle" {
            let (tm, ts) = models
                .get("add-ons/vehicle_tank/tank_turret.dts")
                .context("turret")?;
            attachment_model = Some(tm.clone());
            let tn = nodes(ts);
            attachment_collision_hulls = collision(ts, &tn);
            if attachment_collision_hulls.is_empty() {
                attachment_collision_hulls.push(box_hull(
                    Vec3::new(-1.25, 0., -1.25),
                    Vec3::new(1.25, 1.7, 1.25),
                ));
            }
            let parent = find_node(s, &n, "mount2")?;
            let m = Mat4::from_rotation_translation(
                Quat::from_array(parent.rotation),
                Vec3::from_array(parent.position),
            );
            attachment_mount = Some(parent.clone());
            attachment_fallback_seat = Some(seats[2].transform.clone());
            pivot = parent.position;
            let rider = find_node(ts, &tn, "mount0")?;
            seats[2].transform = transform(
                m * Mat4::from_rotation_translation(
                    Quat::from_array(rider.rotation),
                    Vec3::from_array(rider.position),
                ),
            );
            let t = find_node(ts, &tn, "mount1")?;
            transform(
                m * Mat4::from_rotation_translation(
                    Quat::from_array(t.rotation),
                    Vec3::from_array(t.position),
                ),
            )
        } else {
            find_node(s, &n, if cannon { "eye" } else { "mount1" })?
        };
        Some(Weapon {
            projectile: id(
                "projectile",
                if cannon {
                    "cannonballprojectile"
                } else {
                    "tankshellprojectile"
                },
            ),
            muzzle,
            pivot,
            cooldown_ticks: 300,
            speed: if cannon { 5.5 } else { 140. },
            charge_ticks: if cannon { 24 } else { 0 },
            charge_steps: if cannon { 10 } else { 1 },
            sound: "TankshotSound".into(),
            effect: if cannon {
                "CannonSmokeImage"
            } else {
                "TankSmokeImage"
            }
            .into(),
            look_muzzle: Vec::new(),
        })
    } else {
        None
    };
    let projectile = |key: &str| {
        let x = field(b, key);
        (!x.is_empty()).then(|| id("projectile", &x))
    };
    let actor = matches!(
        family,
        Family::Horse | Family::Cannon | Family::Rowboat | Family::Turret
    );
    if family == Family::Rowboat {
        adaptations.push("Inherited PlayerStandardArmor underwater speeds 8.4 forward/7.8 reverse and runForce 4320; no paddle script exists in shipped Rowboat package".into());
    }
    if matches!(family, Family::Skis | Family::Tumble) {
        adaptations.push("Hidden Item_Skis dependency: script-forced mount0; simple dismount and transient lifecycle".into());
    }
    if family == Family::Skis {
        adaptations.push("Powered traction disabled as authored NothingTire friction/longitudinal force are zero".into());
    }
    if !wheels.is_empty() {
        adaptations.push("Torque tire/suspension coefficients mapped to Rapier ray suspension; exact Torque lateral relaxation is retained in evidence, not equivalent in Rapier. Inertia uses the massBox or shape-bounds box".into());
    }
    // Blockland's WheeledVehicle steers with the strafe keys unless the
    // datablock opts out (vehicles with pitch control do).
    // The Tank's gunner aims its TankTurretPlayer, whose look range bounds the barrel.
    let look = if name == "TankVehicle" {
        blocks
            .get("tankturretplayer")
            .context("tank turret player")?
    } else {
        b
    };
    let strafe_steering = !wheels.is_empty()
        && !matches!(
            field(b, "steeringUseStrafeSteering")
                .to_lowercase()
                .as_str(),
            "false" | "0"
        );
    if family == Family::Flying {
        adaptations.push(
            "FlyingVehicle inertia is the engine's unit sphere (0.4 mass), not massBox".into(),
        );
    }
    Ok(Definition {
        id: id("vehicle", name),
        datablock: name.into(),
        name: field(b, "uiName").trim().into(),
        family,
        energy: EnergySettings {
            maximum: number(b, "maxEnergy", 0.),
            minimum_jet: number(b, "minJetEnergy", 1.),
            drain_per_32ms: number(b, "jetEnergyDrain", 0.8),
            recharge_per_32ms: number(b, "rechargeRate", 0.),
            jet_force: number(b, "jetForce", 500.),
        },
        flight: (family == Family::Flying).then(|| FlightSettings {
            hover_height: number(b, "hoverHeight", 2.),
            create_hover_height: number(b, "createHoverHeight", 2.),
            min_drag: number(b, "minDrag", 1.),
            max_auto_speed: number(b, "maxAutoSpeed", 0.),
            auto_linear_force: number(b, "autoLinearForce", 0.),
            auto_angular_force: number(b, "autoAngularForce", 0.),
            auto_input_damping: number(b, "autoInputDamping", 1.),
            horizontal_surface_force: number(b, "horizontalSurfaceForce", 0.),
            vertical_surface_force: number(b, "verticalSurfaceForce", 0.),
            steering_force: number(b, "steeringForce", 1.),
            steering_roll_force: number(b, "steeringRollForce", 1.),
            vertical_thrust_multiple: number(b, "vertThrustMultiple", 1.),
        }),
        model: model.clone(),
        seats,
        wheels,
        weapon,
        attachment_model,
        attachment_collision_hulls,
        attachment_mount,
        attachment_fallback_seat,
        collision_hulls: hulls,
        bounds_min: lo.to_array(),
        bounds_max: hi.to_array(),
        mass: number(b, "mass", 90.),
        mass_center: vector(b, "massCenter").unwrap_or(Vec3::ZERO).to_array(),
        inertia_box: inertia_box.to_array(),
        density: number(b, "density", 1.),
        drag: number(b, "drag", 0.1),
        friction: number(b, "bodyFriction", 0.6),
        restitution: number(b, "bodyRestitution", 0.),
        max_damage: number(b, "maxDamage", 100.),
        burn_ticks: (number(
            b,
            "burnTime",
            if family == Family::Cannon { 3000. } else { 0. },
        ) * 0.12)
            .round() as u64,
        invulnerable_ticks: if actor {
            0
        } else if family == Family::Flying {
            120
        } else {
            12
        },
        initial_explosion: projectile("initialExplosionProjectile").or_else(|| {
            // Vehicle_Pirate_Cannon.cs spawns CannonBaseExplosionProjectile on destruction.
            (family == Family::Cannon).then(|| id("projectile", "cannonbaseexplosionprojectile"))
        }),
        final_explosion: projectile("finalExplosionProjectile"),
        initial_explosion_offset: number(b, "initialExplosionOffset", 0.),
        final_explosion_offset: number(b, "finalExplosionOffset", 0.),
        mount_distance: number(b, "minMountDist", 3.),
        engine_force: number(b, "engineTorque", number(b, "runForce", 4320.)),
        engine_brake: number(b, "engineBrake", 0.),
        brake_force: number(b, "brakeTorque", 0.),
        max_speed: number(b, "maxWheelSpeed", number(b, "maxForwardSpeed", 20.)),
        reverse_speed: number(b, "maxBackwardSpeed", 20.),
        max_steering: number(b, "maxSteeringAngle", 1.),
        // FlyingVehicle's maneuvering jets push both ways and sideways.
        thrust: if family == Family::Flying {
            number(b, "maneuveringForce", 0.)
        } else {
            number(b, "forwardThrust", 0.)
        },
        reverse_thrust: if family == Family::Flying {
            number(b, "maneuveringForce", 0.)
        } else {
            number(b, "reverseThrust", 0.)
        },
        lift: number(b, "lift", 0.),
        yaw_force: number(b, "yawForce", number(b, "steeringForce", 0.)),
        pitch_force: number(b, "pitchForce", 0.),
        roll_force: number(b, "rollForce", 0.),
        angular_drag: number(b, "rotationalDrag", 0.2),
        jump_speed: number(b, "jumpForce", 0.) / number(b, "mass", 90.),
        max_side_speed: number(b, "maxSideSpeed", 0.),
        run_surface_angle: number(b, "runSurfaceAngle", 0.),
        impact_threshold: number(
            b,
            "collDamageThresholdVel",
            number(b, "minImpactSpeed", 250.),
        ),
        impact_damage: number(b, "collDamageMultiplier", 0.),
        strafe_steering,
        steering: SteeringSettings {
            strafe_rate: number(b, "steeringStrafeSteeringRate", 0.1),
            auto_return: torque_bool(b, "steeringUseAutoReturn", true),
            auto_return_rate: number(b, "steeringAutoReturnRate", 0.9),
            auto_return_max_speed: number(b, "steeringAutoReturnMaxSpeed", 10.),
        },
        wheeled_flight: match family {
            Family::Wheeled | Family::Skis => wheeled_flight(b),
            _ => None,
        },
        threads: vec![],
        look_pitch: [
            -number(look, "maxLookAngle", std::f32::consts::FRAC_PI_2),
            -number(look, "minLookAngle", -std::f32::consts::FRAC_PI_2),
        ],
        underwater_speeds: if actor {
            // PlayerStandardArmor's values unless the datablock overrides.
            [
                number(b, "maxUnderwaterForwardSpeed", 8.4),
                number(b, "maxUnderwaterBackwardSpeed", 7.8),
                number(b, "maxUnderwaterSideSpeed", 7.8),
            ]
        } else {
            [0.; 3]
        },
        camera: VehicleCamera {
            max_dist: number(b, "cameraMaxDist", 8.),
            offset: number(
                b,
                if actor {
                    "cameraVerticalOffset"
                } else {
                    "cameraOffset"
                },
                0.,
            ),
            tilt: number(b, "cameraTilt", 0.),
            lag: number(b, "cameraLag", 0.),
            decay: number(b, "cameraDecay", 0.),
        },
        look_limits: [number(b, "lookDownLimit", 0.), number(b, "lookUpLimit", 1.)],
        runover_speed: number(b, "minRunOverSpeed", f32::MAX),
        runover_damage: number(b, "runOverDamageScale", 0.),
        runover_push: number(b, "runOverPushScale", 0.),
        protect_direct: truth(b, "protectPassengersDirect"),
        protect_radius: truth(b, "protectPassengersRadius"),
        protect_burn: truth(b, "protectPassengersBurn"),
        smash: None,
        shove: false,
        authored: b.fields.clone(),
        adaptations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_numeric_expressions_are_literal_only() {
        let b = Block {
            package: "Vehicle_Horse".into(),
            fields: BTreeMap::from([
                ("runforce".into(), "28 * 90".into()),
                ("unsafe".into(), "exec(\"anything\")".into()),
            ]),
            evidence: Evidence {
                source: "synthetic".into(),
                sha256: String::new(),
                line: 1,
                subject: "test".into(),
            },
        };
        assert_eq!(number(&b, "runForce", 0.), 2520.);
        assert_eq!(number(&b, "unsafe", 7.), 7.);
        assert_eq!(
            virtual_path("Vehicle_Jeep", "./jeep.dts"),
            "Add-Ons/Vehicle_Jeep/jeep.dts"
        );
    }
    #[test]
    fn native_parent_chain_is_composed_before_mounting() {
        let shape = Shape {
            schema_version: 1,
            id: "test".into(),
            nodes: vec![
                bri_content::shape::Node {
                    name: "base".into(),
                    parent: None,
                    translation: [10., 2., 0.],
                    rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2).to_array(),
                },
                bri_content::shape::Node {
                    name: "mount0".into(),
                    parent: Some(0),
                    translation: [0., 0., -2.],
                    rotation: Quat::IDENTITY.to_array(),
                },
            ],
            objects: vec![],
            details: vec![],
            meshes: vec![],
            materials: vec![],
            animations: vec![],
        };
        let t = find_node(&shape, &nodes(&shape), "Mount0").unwrap();
        assert!((Vec3::from_array(t.position) - Vec3::new(8., 2., 0.)).length() < 0.001);
        assert!(find_node(&shape, &nodes(&shape), "mount99").is_err());
    }
}
