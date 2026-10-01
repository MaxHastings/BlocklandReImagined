//! A made-up item presentation pack (`ItemAssets::load`) over
//! `bri_weapons::testing`'s weapons pack: a model for every item and
//! image, textures and icons drawn in code, and the item physics boxes.
//!
//! - The hammer and wrench are held tools with a first-person detail
//!   ([`HELD_DETAIL`]) that their `fire` sequence swings and a detail
//!   everyone else sees ([`WORLD_DETAIL`]) that it leaves still.
//! - The printer's model is presented under the stock path Add-Ons name
//!   it by ([`PRINTER_MODEL`]); its and the hammer's icons can be drawn
//!   from their models ([`write_with_icons`]), as stock icons are
//!   pictures of them.
//! - The FX cans hold a can whose body ([`CLEAR_CAN_BODY`]) is
//!   translucent under solid trim ([`CLEAR_CAN_TRIM`]).
//! - Sports balls have no icon of their own ([`LETTER_ITEMS`]).
//! - Every authored box is a little larger than the mesh it bounds.
use anyhow::Result;
use bri_content::shape::{Animation, Detail, NodeTrack, Shape};
use bri_content::testing::{Paint, material, plain, png, rigid_shape, write_file};
use bri_weapons::testing as weapons;
use glam::{Mat4, Quat, Vec3};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

/// The detail a holder sees, and the one everyone else sees.
pub const HELD_DETAIL: &str = "held";
pub const WORLD_DETAIL: &str = "world";
/// Images whose `fire` sequence swings only the held detail.
pub const SWUNG_IMAGES: [&str; 2] = [weapons::HAMMER_IMAGE, weapons::WRENCH_IMAGE];
/// The printer's model key: the stock path an Add-On names it by.
pub const PRINTER_MODEL: &str = "base/data/shapes/printgun.dts";
/// The gun's and the wand's models and icons, under the stock paths
/// Add-Ons name them by (the Bubble Blaster, the Duplicator).
pub const GUN_MODEL: &str = "add-ons/weapon_gun/pistol.dts";
pub const GUN_ICON: &str = "add-ons/weapon_gun/icon_gun.png";
pub const WAND_MODEL: &str = "base/data/shapes/wand.dts";
pub const WAND_ICON: &str = "base/client/ui/itemicons/wand.png";
/// The FX cans' model, its translucent body material and its trim.
pub const CLEAR_CAN_MODEL: &str = "test/shapes/clearcan.dts";
pub const CLEAR_CAN_BODY: &str = "blank";
pub const CLEAR_CAN_TRIM: [&str; 2] = ["label", "cap"];
/// Items without an icon, which show their name's first letter.
pub const LETTER_ITEMS: [&str; 4] = weapons::SPORT_ITEMS;
/// Icon edge length in pixels.
pub const ICON_SIZE: u32 = 64;
/// The detail size Torque gives a first-person detail (a detail named
/// `...9999`), which the client draws only for the holder.
pub const FIRST_PERSON_DETAIL_SIZE: f32 = 9999.0;
/// Models whose icons the client draws from the model itself, as stock
/// icons are pictures of their models; the rest get a flat badge.
pub const PICTURED_MODELS: [&str; 2] = [PRINTER_MODEL, "test/shapes/hammer.dts"];

/// Draws a [`PICTURED_MODELS`] icon from the model key, its shape and its
/// textures (texture key, PNG bytes), or `None` for the flat badge.
pub type IconPainter<'a> =
    &'a dyn Fn(&str, &Shape, &[(String, Vec<u8>)]) -> Result<Option<Vec<u8>>>;

/// The model key of a weapons-pack item or image.
fn model_of(id: &str) -> String {
    match id {
        _ if id == weapons::HAMMER || id == weapons::HAMMER_IMAGE => {
            "test/shapes/hammer.dts".into()
        }
        _ if id == weapons::WRENCH || id == weapons::WRENCH_IMAGE => {
            "test/shapes/wrench.dts".into()
        }
        _ if id == weapons::PRINTER || id == weapons::PRINTER_IMAGE => PRINTER_MODEL.into(),
        _ if [
            weapons::GUN_ITEM,
            weapons::GUN_IMAGE,
            weapons::BUBBLE_BLASTER_ITEM,
        ]
        .contains(&id)
            || id == "sample-bubble-blaster:image/bubble_blaster" =>
        {
            GUN_MODEL.into()
        }
        _ if id == weapons::WAND || id == weapons::WAND_IMAGE => WAND_MODEL.into(),
        _ if weapons::FX_CANS.iter().any(|(image, _)| *image == id) => CLEAR_CAN_MODEL.into(),
        _ if id == weapons::SPRAY_CAN_IMAGE => "test/shapes/can.dts".into(),
        _ => {
            let name = id.rsplit(['/', '.', ':']).next().unwrap_or(id);
            format!(
                "test/shapes/{}.dts",
                name.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
            )
        }
    }
}

/// `bri_weapons::testing::pack()` with a model for every item and image.
pub fn weapons_pack() -> bri_weapons::Pack {
    let mut pack = weapons::pack();
    for (id, item) in &mut pack.items {
        item.model = model_of(id);
    }
    for (id, image) in &mut pack.images {
        image.model = model_of(id);
    }
    pack
}

/// A held tool: a handle up from the grip (`mountPoint`, at the origin)
/// and a head across its top, front to back. Its held detail hangs from
/// `swing`, which its `fire` sequence turns.
fn held_tool(id: &str, handle: f32, head: [f32; 3]) -> Shape {
    let parts = |node: usize| {
        [
            (
                node,
                [0.0, handle * 0.5 - 0.1, 0.0],
                [0.03, handle * 0.5, 0.03],
                plain(0),
            ),
            (node, [0.0, handle - 0.1 + head[1], 0.0], head, plain(1)),
        ]
    };
    let mut shape = rigid_shape(
        id,
        &[
            ("root", None, [0.0; 3]),
            ("mountPoint", Some(0), [0.0; 3]),
            ("swing", Some(0), [0.0; 3]),
        ],
        &[parts(0), parts(2)].concat(),
        vec![material("handle", "opaque"), material("head", "opaque")],
    );
    shape.details = vec![
        Detail {
            name: WORLD_DETAIL.into(),
            pixel_threshold: 48.0,
            object_start: 0,
            object_count: 2,
            mesh_offset: 0,
            collision: false,
        },
        Detail {
            name: HELD_DETAIL.into(),
            pixel_threshold: FIRST_PERSON_DETAIL_SIZE,
            object_start: 2,
            object_count: 2,
            mesh_offset: 0,
            collision: false,
        },
    ];
    let turn = |a: f32| Quat::from_rotation_x(a).to_array();
    shape.animations.push(Animation {
        name: "fire".into(),
        frames: 3,
        duration: 0.3,
        looping: false,
        additive: false,
        priority: 0,
        nodes: vec![NodeTrack {
            node: "swing".into(),
            rotations: vec![turn(0.0), turn(-0.9), turn(0.0)],
            translations: Vec::new(),
            scales: Vec::new(),
            scale_rotations: Vec::new(),
        }],
        objects: Vec::new(),
        ground_translations: Vec::new(),
        ground_rotations: Vec::new(),
        triggers: Vec::new(),
    });
    shape
}

/// Every model: key, shape. Unlisted keys get a plain stick.
fn shapes(pack: &bri_weapons::Pack) -> BTreeMap<String, Shape> {
    let mut out = BTreeMap::new();
    let keys = pack
        .items
        .values()
        .map(|i| i.model.to_ascii_lowercase())
        .chain(pack.images.values().map(|i| i.model.to_ascii_lowercase()))
        .chain(
            pack.projectiles
                .values()
                .map(|p| p.model.to_ascii_lowercase())
                .filter(|m| !m.is_empty()),
        );
    for key in keys {
        let shape = match key.as_str() {
            "test/shapes/hammer.dts" => held_tool(&key, 1.0, [0.06, 0.08, 0.16]),
            "test/shapes/wrench.dts" => held_tool(&key, 0.8, [0.05, 0.05, 0.1]),
            PRINTER_MODEL => rigid_shape(
                &key,
                &[("root", None, [0.0; 3]), ("mountPoint", Some(0), [0.0; 3])],
                &[
                    (0, [0.0, 0.0, 0.05], [0.04, 0.12, 0.05], plain(0)),
                    (0, [0.0, 0.13, -0.12], [0.06, 0.06, 0.22], plain(1)),
                    (0, [0.0, 0.2, -0.3], [0.025, 0.02, 0.06], plain(1)),
                ],
                vec![material("grip", "opaque"), material("body", "opaque")],
            ),
            // A blaster: a long grip down from the hand and a barrel
            // out front, its box taller than a brick, so a player who
            // walks into it beside a brick touches it. Shots leave the
            // barrel's mouth (`muzzlePoint`) and casings its top
            // (`ejectPoint`).
            GUN_MODEL => rigid_shape(
                &key,
                &[
                    ("root", None, [0.0; 3]),
                    ("mountPoint", Some(0), [0.0; 3]),
                    ("muzzlePoint", Some(0), [0.0, 0.08, -0.36]),
                    ("ejectPoint", Some(0), [0.0, 0.15, -0.08]),
                ],
                &[
                    (0, [0.0, -0.3, 0.04], [0.04, 0.35, 0.05], plain(0)),
                    (0, [0.0, 0.08, -0.12], [0.05, 0.07, 0.24], plain(0)),
                ],
                vec![material("surface", "opaque")],
            ),
            CLEAR_CAN_MODEL | "test/shapes/can.dts" => {
                let body = if key == CLEAR_CAN_MODEL {
                    "alpha"
                } else {
                    "opaque"
                };
                rigid_shape(
                    &key,
                    &[("root", None, [0.0; 3])],
                    &[
                        (
                            0,
                            [0.0, 0.0, 0.0],
                            [0.06, 0.15, 0.06],
                            Paint { front: 1, rest: 0 },
                        ),
                        (0, [0.0, 0.18, 0.0], [0.03, 0.03, 0.03], plain(2)),
                    ],
                    vec![
                        material(CLEAR_CAN_BODY, body),
                        material(CLEAR_CAN_TRIM[0], "opaque"),
                        material(CLEAR_CAN_TRIM[1], "opaque"),
                    ],
                )
            }
            _ => {
                // A stick a little different for each key, its tip the
                // `muzzlePoint` (where a gun on it fires from and, having
                // no `ejectPoint`, throws its casings).
                let n = key
                    .bytes()
                    .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
                let long = 0.15 + (n % 7) as f32 * 0.03;
                rigid_shape(
                    &key,
                    &[
                        ("root", None, [0.0; 3]),
                        ("mountPoint", Some(0), [0.0, -0.05, 0.0]),
                        ("muzzlePoint", Some(0), [0.0, long * 1.5, -0.02]),
                    ],
                    &[(0, [0.0, long * 0.5, -0.02], [0.03, long, 0.04], plain(0))],
                    vec![material("surface", "opaque")],
                )
            }
        };
        out.insert(key, shape);
    }
    out
}

/// The box round every mesh vertex at rest, grown a little: the authored
/// box is never the mesh's fit.
fn authored_bounds(shape: &Shape) -> Result<([f32; 3], [f32; 3])> {
    let pose = bri_content::animation::sample(shape, None, 0.0)?;
    let (mut lo, mut hi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for object in &shape.objects {
        let node = object.node.map_or(Mat4::IDENTITY, |n| pose.nodes[n]);
        for mesh in object
            .meshes
            .iter()
            .filter_map(|m| shape.meshes[*m].as_ref())
        {
            for p in &mesh.positions {
                let p = node.transform_point3(Vec3::from(*p));
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    Ok(((lo - 0.01).to_array(), (hi + 0.01).to_array()))
}

/// Material `index`'s texture of model `key`: a colour from the two
/// names with a lighter stripe; the translucent body is see-through.
fn texture(key: &str, name: &str, index: usize) -> Result<Vec<u8>> {
    let n = format!("{key}/{name}")
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(131).wrapping_add(u32::from(b)));
    let rgb = [
        (n & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        ((n >> 16) & 0xff) as u8,
    ];
    let alpha = if name == CLEAR_CAN_BODY { 60 } else { 255 };
    png(8, 8, |x, _| {
        let stripe = x == 3 + index as u32 % 3;
        let c = |v: u8| if stripe { v / 2 + 128 } else { v };
        [c(rgb[0]), c(rgb[1]), c(rgb[2]), alpha]
    })
}

fn evidence() -> serde_json::Value {
    json!({"path": "testing", "sha256": "", "line": 0})
}

/// Write `weapons.json` into `weapons_dir` and the presentation pack
/// (models, textures, flat icons, item physics) into `dir`.
pub fn write(dir: &Path, weapons_dir: &Path) -> Result<()> {
    write_with_icons(dir, weapons_dir, &|_, _, _| Ok(None))
}

/// [`write`], drawing the [`PICTURED_MODELS`]' icons with `paint`.
pub fn write_with_icons(dir: &Path, weapons_dir: &Path, paint: IconPainter) -> Result<()> {
    let pack = weapons_pack();
    let weapons_sha = write_file(
        weapons_dir,
        "weapons.json",
        &serde_json::to_vec_pretty(&pack)?,
    )?;
    let mut textures = serde_json::Map::new();
    let mut models = serde_json::Map::new();
    let mut bounds = BTreeMap::new();
    let mut icons = BTreeMap::new();
    for (key, shape) in shapes(&pack) {
        let stem = key.trim_end_matches(".dts").replace('/', "_");
        let mut keys = Vec::new();
        let mut images = Vec::new();
        for (i, m) in shape.materials.iter().enumerate() {
            let texture_key = format!(
                "{}/{}.png",
                key.rsplit_once('/').map_or("", |(d, _)| d),
                m.name
            );
            let bytes = texture(&key, &m.name, i)?;
            let file = format!("textures/{stem}_{}.png", m.name);
            let sha = write_file(dir, &file, &bytes)?;
            textures.insert(
                texture_key.clone(),
                json!({"file": file, "sha256": sha, "width": 8, "height": 8, "source": "made up"}),
            );
            images.push((texture_key.clone(), bytes));
            keys.push(texture_key);
        }
        if PICTURED_MODELS.contains(&key.as_str())
            && let Some(icon) = paint(&key, &shape, &images)?
        {
            icons.insert(key.clone(), icon);
        }
        let file = format!("models/{stem}.shape.json");
        let sha = write_file(dir, &file, &serde_json::to_vec(&shape)?)?;
        let (min, max) = authored_bounds(&shape)?;
        bounds.insert(key.clone(), (min, max));
        models.insert(
            key,
            json!({"file": file, "sha256": sha, "source": "made up", "source_sha256": "0".repeat(64),
                "textures": keys, "bounds_min": min, "bounds_max": max}),
        );
    }
    let mut items = serde_json::Map::new();
    let mut physics = serde_json::Map::new();
    for (id, item) in &pack.items {
        let model = item.model.to_ascii_lowercase();
        let icon = if LETTER_ITEMS.contains(&id.as_str()) {
            None
        } else {
            let bytes = match icons.get(&model) {
                Some(bytes) => bytes.clone(),
                None => {
                    let n = id.len() as u8;
                    png(ICON_SIZE, ICON_SIZE, |x, y| {
                        let inside = (8..56).contains(&x) && (20..44).contains(&y);
                        if inside {
                            [
                                n.wrapping_mul(9),
                                200u8.wrapping_sub(n.wrapping_mul(3)),
                                90,
                                255,
                            ]
                        } else {
                            [0; 4]
                        }
                    })?
                }
            };
            let name = id.rsplit(['/', '.', ':']).next().unwrap_or(id);
            let key = match model.as_str() {
                GUN_MODEL => GUN_ICON.to_string(),
                WAND_MODEL => WAND_ICON.to_string(),
                _ => format!("test/icons/{name}.png"),
            };
            let file = format!("icons/{}", key.replace('/', "_"));
            let sha = write_file(dir, &file, &bytes)?;
            textures.insert(
                key.clone(),
                json!({"file": file, "sha256": sha, "width": ICON_SIZE, "height": ICON_SIZE, "source": "made up"}),
            );
            Some(key)
        };
        items.insert(
            id.clone(),
            json!({"model": model, "image": item.image, "tint": [1.0, 1.0, 1.0, 1.0],
                "icon": icon, "evidence": evidence()}),
        );
        let (min, max) = bounds[&model];
        physics.insert(id.clone(), json!({"min": min, "max": max}));
    }
    let images: serde_json::Map<_, _> = pack
        .images
        .iter()
        .map(|(id, image)| {
            (
                id.clone(),
                json!({"model": image.model.to_ascii_lowercase(), "mount_point": image.mount_point,
                    "offset": image.offset, "eye_offset": image.eye_offset,
                    "source_rotation_degrees": image.source_rotation_degrees,
                    "eye_rotation_degrees": [0.0, 0.0, 0.0], "tint": [1.0, 1.0, 1.0, 1.0],
                    "evidence": evidence()}),
            )
        })
        .collect();
    let projectiles: serde_json::Map<_, _> = pack
        .projectiles
        .iter()
        .map(|(id, p)| {
            let model = (!p.model.is_empty()).then(|| p.model.to_ascii_lowercase());
            (
                id.clone(),
                json!({"model": model, "tint": [1.0, 1.0, 1.0, 1.0]}),
            )
        })
        .collect();
    let physics_sha = write_file(
        dir,
        "item-physics.json",
        &serde_json::to_vec_pretty(&json!({"schema_version": 1, "items": physics}))?,
    )?;
    let presentation = json!({
        "schema_version": 2, "id": "test:item-presentation",
        "weapons_sha256": weapons_sha, "item_physics_sha256": physics_sha,
        "models": models, "textures": textures, "items": items, "images": images,
        "projectiles": projectiles, "diagnostics": [],
    });
    write_file(
        dir,
        "presentation.json",
        &serde_json::to_vec_pretty(&presentation)?,
    )?;
    Ok(())
}
