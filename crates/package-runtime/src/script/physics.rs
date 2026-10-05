//! Native object queries and physics operation registration.
use super::*;

/// Movable objects: reading them, and the `physics` operations.
pub(super) fn register(engine: &mut Engine) {
    engine.register_fn("object", |object: Dynamic| {
        with(|i| {
            let object = object_ref(&object)?;
            let snapshot = i.snapshot.clone();
            match snapshot.object(object) {
                Some(o) => view_in(i, None, || object_map(&o)),
                None => Ok(Dynamic::UNIT),
            }
        })
    });
    engine.register_fn("objects", || {
        with(|i| {
            let snapshot = i.snapshot.clone();
            snapshot
                .objects
                .iter()
                .map(|o| view_in(i, None, || object_map(o)))
                .collect::<Fallible<Array>>()
        })
    });
    engine.register_fn(
        "objects_near",
        |x: Dynamic, y: Dynamic, z: Dynamic, radius: Dynamic| {
            with(|i| {
                let centre = [float(&x)?, float(&y)?, float(&z)?];
                let radius = float(&radius)?;
                let near = |p: [f32; 3]| {
                    (p[0] - centre[0]).powi(2)
                        + (p[1] - centre[1]).powi(2)
                        + (p[2] - centre[2]).powi(2)
                        <= radius * radius
                };
                let players = i
                    .snapshot
                    .players
                    .iter()
                    .filter(|p| p.alive)
                    .map(|p| ObjectRef::Player(p.id));
                let entities = i.snapshot.entities.iter().map(|e| ObjectRef::Entity(e.id));
                let vehicles = i.snapshot.objects.iter().map(|o| o.object);
                let near: Vec<_> = players
                    .chain(entities)
                    .chain(vehicles)
                    .filter_map(|o| i.snapshot.object(o))
                    .filter(|o| near(o.position))
                    .collect();
                near.iter()
                    .map(|o| view_in(i, None, || object_map(o)))
                    .collect::<Fallible<Array>>()
            })
        },
    );
    engine.register_fn("held", |player: Dynamic| {
        with(|i| {
            let player = id(&player)?;
            Ok(i.snapshot
                .holds
                .iter()
                .find(|h| h.player == player)
                .map_or(Dynamic::UNIT, |h| Dynamic::from(h.object.to_string())))
        })
    });
    engine.register_fn(
        "push",
        |target: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic| {
            push_op(target, x, y, z, Dynamic::UNIT)
        },
    );
    engine.register_fn("push", push_op);
    engine.register_fn(
        "tumble",
        |player: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic| {
            tumble_op(player, x, y, z, Dynamic::UNIT)
        },
    );
    engine.register_fn("tumble", tumble_op);
    engine.register_fn("tumble", tumble_for);
    engine.register_fn(
        "hold",
        |player: Dynamic, target: Dynamic, distance: Dynamic| {
            push(Op::Hold(ops::Hold {
                player: id(&player)?,
                target: object_ref(&target)?,
                distance: float(&distance)?,
                at: None,
                force: None,
                turn: false,
            }))
        },
    );
    // `hold(player, ref, distance, #{ at: [x, y, z], force: f, turn: true })`:
    // every option may be left out.
    engine.register_fn(
        "hold",
        |player: Dynamic, target: Dynamic, distance: Dynamic, options: rhai::Map| {
            for key in options.keys() {
                if !matches!(key.as_str(), "at" | "force" | "turn") {
                    return fail(format!("hold has no option `{key}` (at, force, turn)"));
                }
            }
            let at = match options.get("at") {
                None => None,
                Some(value) if value.is_unit() => None,
                Some(value) => {
                    let Some(a) = value.clone().try_cast::<Array>() else {
                        return fail("hold's `at` is [x, y, z]");
                    };
                    let v = a.iter().map(float).collect::<Fallible<Vec<f32>>>()?;
                    let [x, y, z] = v[..] else {
                        return fail("hold's `at` is [x, y, z]");
                    };
                    Some([x, y, z])
                }
            };
            let force = options.get("force").map(float).transpose()?;
            let turn = match options.get("turn") {
                None => false,
                Some(value) => match value.as_bool() {
                    Ok(b) => b,
                    Err(_) => return fail("hold's `turn` is true or false"),
                },
            };
            push(Op::Hold(ops::Hold {
                player: id(&player)?,
                target: object_ref(&target)?,
                distance: float(&distance)?,
                at,
                force,
                turn,
            }))
        },
    );
    engine.register_fn("hold_distance", |player: Dynamic, distance: Dynamic| {
        push(Op::HoldDistance(ops::HoldDistance {
            player: id(&player)?,
            distance: float(&distance)?,
        }))
    });
    // `reach(player, distance, #{ near: d, force: f, turn: true })`: hold
    // the first thing that comes where they look within `distance`
    // (`Op::Reach`); every option may be left out.
    engine.register_fn(
        "reach",
        |player: Dynamic, distance: Dynamic, options: rhai::Map| {
            for key in options.keys() {
                if !matches!(key.as_str(), "near" | "force" | "turn") {
                    return fail(format!("reach has no option `{key}` (near, force, turn)"));
                }
            }
            let near = options.get("near").map(float).transpose()?.unwrap_or(0.5);
            let force = options.get("force").map(float).transpose()?;
            let turn = match options.get("turn") {
                None => false,
                Some(value) => match value.as_bool() {
                    Ok(b) => b,
                    Err(_) => return fail("reach's `turn` is true or false"),
                },
            };
            push(Op::Reach(ops::Reach {
                player: id(&player)?,
                distance: float(&distance)?,
                near,
                force,
                turn,
            }))
        },
    );
    // How far off what `player` holds is carried, or () when nothing is held.
    engine.register_fn("held_distance", |player: Dynamic| {
        with(|i| {
            let player = id(&player)?;
            Ok(i.snapshot
                .holds
                .iter()
                .find(|h| h.player == player)
                .map_or(Dynamic::UNIT, |h| {
                    Dynamic::from_float(f64::from(h.distance))
                }))
        })
    });
    engine.register_fn("let_go", |player: Dynamic| {
        push(Op::LetGo(ops::LetGo {
            player: id(&player)?,
        }))
    });
    engine.register_fn("tethered", |player: Dynamic| {
        with(|i| {
            let player = id(&player)?;
            Ok(i.snapshot
                .tethers
                .iter()
                .find(|t| t.player == player)
                .map_or(Dynamic::UNIT, |t| {
                    let mut map = rhai::Map::new();
                    let [x, y, z] = t.anchor;
                    map.insert("x".into(), Dynamic::from_float(x.into()));
                    map.insert("y".into(), Dynamic::from_float(y.into()));
                    map.insert("z".into(), Dynamic::from_float(z.into()));
                    map.insert("length".into(), Dynamic::from_float(t.length.into()));
                    map.insert("target".into(), Dynamic::from_float(t.target.into()));
                    map.insert(
                        "brick".into(),
                        t.brick
                            .map_or(Dynamic::UNIT, |b| Dynamic::from_int(b as i64)),
                    );
                    map.insert(
                        "object".into(),
                        t.object
                            .map_or(Dynamic::UNIT, |o| Dynamic::from(o.to_string())),
                    );
                    Dynamic::from_map(map)
                }))
        })
    });
    engine.register_fn(
        "tether",
        |player: Dynamic, anchor: Array, length: Dynamic| {
            tether_op(player, anchor, length, rhai::Map::new())
        },
    );
    // `tether(player, [x, y, z], length, #{ brick: id, object: ref, reel: r,
    // swing: s, keys: [shortest, longest], straight: true })`:
    // every option may be left out; a length of `()` is as long as the
    // rope spans now.
    engine.register_fn("tether", tether_op);
    engine.register_fn("tether_length", |player: Dynamic, length: Dynamic| {
        push(Op::TetherLength(ops::TetherLength {
            player: id(&player)?,
            length: float(&length)?,
        }))
    });
    engine.register_fn("untether", |player: Dynamic| {
        push(Op::Untether(ops::Untether {
            player: id(&player)?,
            keep: None,
        }))
    });
    // `untether(player, #{ keep: k })`: let go, keeping only `k` (0 to 1) of
    // their speed relative to what the rope was tied to.
    engine.register_fn("untether", |player: Dynamic, options: rhai::Map| {
        let keep = match options.get("keep") {
            Some(k) => {
                let k = float(k)?;
                if !(0.0..=1.0).contains(&k) {
                    return Err("untether keep must be between 0 and 1".into());
                }
                Some(k)
            }
            None => None,
        };
        push(Op::Untether(ops::Untether {
            player: id(&player)?,
            keep,
        }))
    });
    fn mount_object(
        mount: Dynamic,
        rider: Dynamic,
        node: i64,
        can_dismount: bool,
        turn: Dynamic,
    ) -> Fallible<()> {
        push(Op::MountObject(ops::MountObject {
            mount: id(&mount)?,
            rider: id(&rider)?,
            node: u8::try_from(node)
                .ok()
                .filter(|n| usize::from(*n) < crate::ops::MAX_MOUNT_POINTS)
                .ok_or("a mount point is 0 to 7")?,
            can_dismount,
            turn: float(&turn)?.to_radians(),
        }))
    }
    engine.register_fn(
        "mount_object",
        |mount: Dynamic, rider: Dynamic, node: i64, can_dismount: bool| {
            mount_object(mount, rider, node, can_dismount, Dynamic::from_float(0.0))
        },
    );
    engine.register_fn("mount_object", mount_object);
    engine.register_fn("unmount_object", |rider: Dynamic| {
        push(Op::UnmountObject(ops::UnmountObject { rider: id(&rider)? }))
    });
    engine.register_fn(
        "spawn_vehicle",
        |definition: &str,
         x: Dynamic,
         y: Dynamic,
         z: Dynamic,
         yaw: Dynamic,
         velocity: Array,
         owner: Dynamic| {
            let v = velocity.iter().map(float).collect::<Fallible<Vec<f32>>>()?;
            let [vx, vy, vz] = v[..] else {
                return fail("velocity is [x, y, z]");
            };
            push(Op::SpawnVehicle(ops::SpawnVehicle {
                definition: definition.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                yaw: float(&yaw)?,
                velocity: [vx, vy, vz],
                owner: credit(&owner)?,
            }))
        },
    );
    engine.register_fn("remove_vehicle", |vehicle: Dynamic| {
        let vehicle = match object_ref(&vehicle) {
            Ok(ObjectRef::Vehicle(v)) => v,
            Ok(other) => return fail(format!("{other} is not a vehicle")),
            Err(_) => id(&vehicle)?,
        };
        push(Op::RemoveVehicle(ops::RemoveVehicle { vehicle }))
    });
}
