//! `bri_vehicles::testing`'s catalog written as a vehicle pack: box
//! models, the gunners' barrels on a `barrel` node, and a `look.dsq`
//! clip set that swings that node from straight up to straight down, as
//! the converter writes them. The gunners' muzzle tracks are sampled
//! from that clip ([`bri_vehicles::Pack::attach_muzzle_tracks`]). The
//! tank's turret is the player-type turret ([`TANK_TURRET`]), as v20's
//! tank carries its gunner's turret. Wheels are boxes as wide as their
//! radius, the weapons' explosion debris small cubes, and the horse a rig
//! with the clips the avatar's horse loader asks for.
use anyhow::Result;
use bri_content::shape::{Animation, ClipSet, NodeTrack, Shape};
use bri_content::testing::{material, plain, png, rigid_shape, write_file};
use bri_vehicles::{Pack, schema::Asset, testing as vt};
use glam::{Quat, Vec3};
use std::path::Path;

/// Gunners whose barrels follow the `look` clip.
pub const GUNNERS: [&str; 3] = [vt::TANK, vt::CANNON, vt::TURRET];
/// Vehicles with no gun, so no look rig.
pub const UNARMED: [&str; 2] = [vt::CAR, vt::HORSE];
/// The player-type definition the tank carries as its turret.
pub const TANK_TURRET: &str = vt::TURRET;
/// The barrel's swing either side of level across the clip, radians.
pub const LOOK_SWING: f32 = 1.4;

fn asset(dir: &Path, virtual_path: &str, path: &str, kind: &str, bytes: &[u8]) -> Result<Asset> {
    Ok(Asset {
        virtual_path: virtual_path.into(),
        path: path.into(),
        sha256: write_file(dir, path, bytes)?,
        kind: kind.into(),
        source_sha256: "0".repeat(64),
        package: None,
    })
}

/// Write `vehicles.json` and its models, clips and paint into `dir`.
pub fn write_pack(dir: &Path) -> Result<Pack> {
    let tank_turret = vt::tank().attachment_model;
    let mut pack = vt::pack_with(|d| {
        if d.id == TANK_TURRET {
            d.model = tank_turret.clone().unwrap_or_default();
        }
    });
    let native = |source: &str| {
        let stem = source.rsplit('/').next().unwrap_or(source);
        format!("models/{}.json", stem.trim_end_matches(".dts"))
    };
    let mut assets: Vec<Asset> = Vec::new();
    for d in &mut pack.definitions {
        // The gun's model: the tank's turret, the cannon itself.
        let gun = d
            .weapon
            .as_ref()
            .map(|_| d.attachment_model.clone().unwrap_or(d.model.clone()));
        let mut sources = vec![d.model.clone()];
        sources.extend(d.attachment_model.clone());
        for source in sources {
            if assets.iter().any(|a| a.virtual_path == source) {
                continue;
            }
            let shape = if d.id == vt::HORSE {
                horse_shape(&source)
            } else if Some(&source) == gun.as_ref() {
                // A base on the root and a barrel on its hinge, with the
                // muzzle node at the barrel's mouth.
                rigid_shape(
                    &source,
                    &[
                        ("root", None, [0.0; 3]),
                        ("barrel", Some(0), [0.0, 0.6, 0.0]),
                        (d.muzzle_node(), Some(1), [0.0, 0.0, -1.5]),
                    ],
                    &[
                        (0, [0.0, 0.3, 0.0], [0.6, 0.3, 0.6], plain(0)),
                        (1, [0.0, 0.0, -0.75], [0.15, 0.15, 0.75], plain(0)),
                    ],
                    vec![material("blank", "opaque")],
                )
            } else {
                rigid_shape(
                    &source,
                    &[("root", None, [0.0; 3])],
                    &[(0, [0.0, 0.5, 0.0], [1.0, 0.5, 2.0], plain(0))],
                    vec![material("blank", "opaque")],
                )
            };
            shape.validate()?;
            let path = native(&source);
            assets.push(asset(
                dir,
                &source,
                &path,
                "model",
                &serde_json::to_vec(&shape)?,
            )?);
        }
        for wheel in d.wheels.iter_mut().filter(|w| !w.model.is_empty()) {
            if !assets.iter().any(|a| a.virtual_path == wheel.model) {
                let r = wheel.radius;
                let shape = rigid_shape(
                    &wheel.model,
                    &[("root", None, [0.0; 3])],
                    &[(0, [0.0; 3], [r, r, r * 0.4], plain(0))],
                    vec![material("blank", "opaque")],
                );
                let path = native(&wheel.model);
                assets.push(asset(
                    dir,
                    &wheel.model,
                    &path,
                    "model",
                    &serde_json::to_vec(&shape)?,
                )?);
            }
            wheel.model = native(&wheel.model);
        }
        d.model = native(&d.model);
        if let Some(model) = &mut d.attachment_model {
            *model = native(model);
        }
    }
    // The weapons' explosion debris, which the vehicle pack carries.
    for spec in bri_weapons::debris::explosion_debris(&super::items::weapons_pack()).values() {
        if spec.model.is_empty() || assets.iter().any(|a| a.virtual_path == spec.model) {
            continue;
        }
        let shape = rigid_shape(
            &spec.model,
            &[("root", None, [0.0; 3])],
            &[(0, [0.0; 3], [0.15; 3], plain(0))],
            vec![material("blank", "opaque")],
        );
        let path = native(&spec.model);
        assets.push(asset(
            dir,
            &spec.model,
            &path,
            "model",
            &serde_json::to_vec(&shape)?,
        )?);
    }
    // The horse's skin and its clips, one `.dsq` per alias.
    assets.push(asset(
        dir,
        &format!("test/{HORSE_SKIN}.png"),
        &format!("textures/{HORSE_SKIN}.png"),
        "texture",
        &png(4, 4, |x, y| {
            if (x + y) % 2 == 0 {
                [150, 100, 60, 255]
            } else {
                [120, 80, 50, 255]
            }
        })?,
    )?);
    for (alias, clip) in horse_clips() {
        let path = format!("clips/horse_{alias}.json");
        let set = ClipSet {
            schema_version: 1,
            id: format!("test:horse/{alias}"),
            animations: vec![clip],
        };
        assets.push(asset(
            dir,
            &format!("test/horse_{alias}.dsq"),
            &path,
            "animation",
            &serde_json::to_vec(&set)?,
        )?);
        pack.animation_aliases
            .insert(format!("{}::{alias}", vt::HORSE), path);
    }
    let frames = 9;
    let look = Animation {
        name: "look".into(),
        frames,
        duration: 1.0,
        looping: false,
        additive: false,
        priority: 0,
        nodes: vec![NodeTrack {
            node: "barrel".into(),
            rotations: (0..frames)
                .map(|i| {
                    let t = i as f32 / (frames - 1) as f32;
                    Quat::from_rotation_x(LOOK_SWING * (1.0 - 2.0 * t)).to_array()
                })
                .collect(),
            translations: Vec::new(),
            scales: Vec::new(),
            scale_rotations: Vec::new(),
        }],
        objects: Vec::new(),
        ground_translations: Vec::new(),
        ground_rotations: Vec::new(),
        triggers: Vec::new(),
    };
    let clips = ClipSet {
        schema_version: 1,
        id: "test/look.dsq".into(),
        animations: vec![look],
    };
    assets.push(asset(
        dir,
        "test/look.dsq",
        "clips/look.json",
        "animation",
        &serde_json::to_vec(&clips)?,
    )?);
    assets.push(asset(
        dir,
        "test/blank.png",
        "textures/blank.png",
        "texture",
        &png(4, 4, |_, _| [255; 4])?,
    )?);
    pack.assets = assets;
    pack.attach_muzzle_tracks(dir)?;
    pack.validate()?;
    write_file(dir, "vehicles.json", &serde_json::to_vec_pretty(&pack)?)?;
    Ok(pack)
}

/// The horse rig's one material, and its texture's file name.
pub const HORSE_SKIN: &str = "horse_skin";

/// The horse the avatar rides: a body box (`body`, holding the rider's
/// `mount0` on top) and a head box out front, both in one skin.
fn horse_shape(name: &str) -> Shape {
    let (centre, half) = (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.7, 0.5, 1.1));
    let mut shape = rigid_shape(
        name,
        &[
            ("root", None, [0.0; 3]),
            ("body", Some(0), centre.to_array()),
            ("head", Some(1), [0.0, half.y, half.z]),
            ("mount0", Some(0), [0.0, centre.y + half.y, 0.0]),
        ],
        &[
            (1, [0.0; 3], half.to_array(), plain(0)),
            (2, [0.0, 0.2, 0.2], [0.2, 0.25, 0.3], plain(0)),
        ],
        vec![material(HORSE_SKIN, "opaque")],
    );
    shape.objects[0].name = "body".into();
    shape.objects[1].name = "head".into();
    shape
}

/// The clips the engine plays on the horse, by the alias it names each
/// by: a sway of the body for the walks, a tilt of it for crouch and for
/// leaving the ground (jump, fall).
/// The look and headside clips are stand-ins named for neither alias, so
/// the engine poses nothing with them and layers them like the player's
/// own overlays (and the horse, which has no gun, gets no gunner's look
/// rig).
fn horse_clips() -> Vec<(&'static str, Animation)> {
    let clip = |name: &str, node: &str, angles: &[f32], looping: bool| Animation {
        name: name.into(),
        frames: angles.len(),
        duration: 0.8,
        looping,
        additive: false,
        priority: 0,
        nodes: vec![NodeTrack {
            node: node.into(),
            rotations: angles
                .iter()
                .map(|a| Quat::from_rotation_x(*a).to_array())
                .collect(),
            translations: vec![],
            scales: vec![],
            scale_rotations: vec![],
        }],
        objects: vec![],
        ground_translations: vec![],
        ground_rotations: vec![],
        triggers: vec![],
    };
    vec![
        ("root", clip("root", "body", &[0.0], true)),
        ("run", clip("run", "body", &[0.0, 0.08, 0.0, -0.08], true)),
        ("back", clip("back", "body", &[0.0, -0.06, 0.0, 0.06], true)),
        ("side", clip("side", "body", &[0.0, 0.05, 0.0], true)),
        ("crouch", clip("crouch", "body", &[0.15], false)),
        ("jump", clip("jump", "body", &[0.0, -0.2], false)),
        ("fall", clip("fall", "body", &[0.1], false)),
        ("look", clip("horse_look", "head", &[-0.4, 0.0, 0.4], false)),
        (
            "headside",
            clip("horse_headside", "head", &[0.0, 0.2, 0.0], false),
        ),
    ]
}
