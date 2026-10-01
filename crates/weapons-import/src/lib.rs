//! Read-only, bounded literal add-on importer. This does not execute TorqueScript.
use anyhow::{Context, Result, ensure};
use bri_weapons::*;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn clean(s: &str) -> String {
    s.trim().trim_matches('"').to_owned()
}
fn field(d: &Definition, k: &str) -> String {
    d.fields
        .get(&k.to_ascii_lowercase())
        .map(|s| clean(s))
        .unwrap_or_default()
}
fn num(d: &Definition, k: &str, default: f32) -> f32 {
    field(d, k).parse().unwrap_or(default)
}
fn flag(d: &Definition, k: &str, default: bool) -> bool {
    match field(d, k).to_ascii_lowercase().as_str() {
        "true" | "1" => true,
        "false" | "0" => false,
        _ => default,
    }
}
fn ticks(seconds: f32) -> u32 {
    (seconds * 120.0 - 0.00001).ceil().max(0.0) as u32
}
fn vec<const N: usize>(s: &str, default: [f32; N]) -> [f32; N] {
    let v: Vec<_> = s.split_whitespace().map(str::parse::<f32>).collect();
    if v.len() != N || v.iter().any(Result::is_err) {
        return default;
    }
    std::array::from_fn(|i| v[i].as_ref().copied().unwrap_or(0.0))
}
fn axis(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[2], -v[1]]
}
/// A Torque (x, y, z) scale on native (x, up, forward) axes.
fn axis_scale(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[2], v[1]]
}
fn resource(d: &Definition, key: &str) -> String {
    let p = field(d, key);
    if let Some(rest) = p.strip_prefix("~/") {
        // Core datablocks resolve `~/` against the base game directory.
        format!("base/{rest}")
    } else if p.starts_with("./") {
        format!(
            "{}/{}",
            d.source
                .path
                .split('/')
                .take(2)
                .collect::<Vec<_>>()
                .join("/"),
            p.trim_start_matches("./")
        )
    } else if let Some(rest) = p.strip_prefix("~/") {
        format!("base/{rest}")
    } else {
        p
    }
}
/// Image `rotation` as Euler degrees: `eulerToMatrix("x y z")`, or a literal
/// `"ax ay az degrees"` about one principal axis, whose Torque matrix equals
/// the Euler rotation of that angle on that axis. Empty is no rotation.
fn source_rotation(value: &str) -> Option<[f32; 3]> {
    if value.is_empty() {
        return Some([0.0; 3]);
    }
    if let Some(euler) = value.split('"').nth(1) {
        return Some(vec(euler, [0.0; 3]));
    }
    let v: Vec<f32> = value
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [x, y, z, degrees] = v[..] else {
        return None;
    };
    let axis = [x, y, z];
    if let Some(principal) = axis.iter().position(|a| (a.abs() - 1.0).abs() < 1e-6)
        && axis.iter().filter(|a| **a != 0.0).count() == 1
    {
        let mut euler = [0.0; 3];
        euler[principal] = degrees * axis[principal].signum();
        return Some(euler);
    }
    // Any other axis (`"0 1 1 45"`), normalized as `TypeMatrixRotation` does.
    bri_weapons::rotation::axis_angle(axis, degrees)
}
/// Where an image sits on its mount node, from its literal `offset` and
/// `rotation` fields (lower-case keys, source expressions): the native
/// offset and the rotation in the pack's Euler degrees, `eulerToMatrix`
/// corrected as [`rotation::correct_image_rotations`] does. `None` when the
/// rotation is not a literal the pack can hold.
pub fn image_placement(fields: &BTreeMap<String, String>) -> Option<([f32; 3], [f32; 3])> {
    let get = |k: &str| fields.get(k).map(|v| clean(v)).unwrap_or_default();
    let rotation = get("rotation");
    let degrees = source_rotation(&rotation)?;
    let degrees = if rotation.to_ascii_lowercase().contains("eulertomatrix") {
        rotation::euler_to_matrix(degrees)
    } else {
        degrees
    };
    Some((axis(vec(&get("offset"), [0.0; 3])), degrees))
}
/// Removes comments while respecting quoted strings; retains newlines for evidence.
fn uncomment(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    let mut quoted = false;
    while let Some(c) = it.next() {
        if c == '"' {
            quoted = !quoted;
            out.push(c);
        } else if c == '\\' && quoted {
            out.push(c);
            if let Some(n) = it.next() {
                out.push(n);
            }
        } else if c == '/' && !quoted && it.peek() == Some(&'/') {
            it.next();
            for n in it.by_ref() {
                if n == '\n' {
                    out.push(n);
                    break;
                }
            }
        } else if c == '/' && !quoted && it.peek() == Some(&'*') {
            it.next();
            while let Some(n) = it.next() {
                if n == '\n' {
                    out.push(n);
                }
                if n == '*' && it.peek() == Some(&'/') {
                    it.next();
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
pub fn parse(text: &str, path: &str) -> Result<Vec<Definition>> {
    ensure!(text.len() <= 8 * 1024 * 1024, "Script too large");
    let text_clean = uncomment(text);
    let re = Regex::new(
        r"(?is)datablock\s+(\w+)\s*\(\s*(\w+)\s*(?::\s*(\w+)\s*)?\)\s*\{([^{}]*)\}\s*;",
    )?;
    let assignment = Regex::new(r"(?is)([A-Za-z_]\w*(?:\s*\[\s*\d+\s*\])?)\s*=\s*([^;]*);")?;
    let mut defs = Vec::new();
    for c in re.captures_iter(&text_clean) {
        let fields = assignment
            .captures_iter(&c[4])
            .map(|a| {
                (
                    a[1].chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>()
                        .to_ascii_lowercase(),
                    a[2].trim().to_owned(),
                )
            })
            .collect();
        defs.push(Definition {
            name: c[2].to_owned(),
            class: c[1].to_owned(),
            parent: c.get(3).map(|m| m.as_str().to_owned()),
            source: Evidence {
                path: path.to_owned(),
                sha256: hash(text.as_bytes()),
                line: text_clean[..c.get(0).unwrap().start()]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1,
            },
            fields,
        });
    }
    ensure!(defs.len() <= 4096, "Definition budget exceeded");
    Ok(defs)
}
/// Literal `AddDamageType(name, suicide, murder, vehicleScale, direct)` calls
/// in source order. Guards are not evaluated: a later call replaces an earlier
/// one of the same name, as `AddDamageType` reuses an existing index.
pub fn damage_types(text: &str) -> Result<Vec<DamageType>> {
    ensure!(text.len() <= 8 * 1024 * 1024, "Script too large");
    let call = Regex::new(
        r#"(?i)AddDamageType\s*\(\s*"(\w+)"\s*,\s*(?:'([^']*)'|"([^"]*)")\s*,\s*(?:'([^']*)'|"([^"]*)")\s*,\s*([^,()]*),\s*([^,()]*)\)"#,
    )?;
    let text = uncomment(text);
    Ok(call
        .captures_iter(&text)
        .map(|c| {
            let text = |a: usize, b: usize| {
                c.get(a)
                    .or_else(|| c.get(b))
                    .map_or(String::new(), |m| m.as_str().to_owned())
            };
            DamageType {
                name: c[1].to_owned(),
                suicide_message: text(2, 3),
                murder_message: text(4, 5),
                vehicle_scale: c[6].trim().parse().unwrap_or(1.0),
                direct: matches!(c[7].trim(), "1" | "true"),
            }
        })
        .collect())
}
fn resolve(
    key: &str,
    all: &BTreeMap<String, Definition>,
    stack: &mut BTreeSet<String>,
) -> Result<Definition> {
    ensure!(stack.len() < 64, "Inheritance depth budget");
    ensure!(stack.insert(key.to_owned()), "Inheritance cycle {key}");
    let mut d = all.get(key).context("Missing definition")?.clone();
    if let Some(p) = &d.parent
        && all.contains_key(&p.to_ascii_lowercase())
    {
        let parent = resolve(&p.to_ascii_lowercase(), all, stack)?;
        let mut merged = parent.fields;
        merged.extend(d.fields);
        d.fields = merged;
    }
    stack.remove(key);
    Ok(d)
}
pub fn lower(definitions: Vec<Definition>) -> Result<Pack> {
    let all: BTreeMap<_, _> = definitions
        .iter()
        .map(|d| (d.name.to_ascii_lowercase(), d.clone()))
        .collect();
    let mut resolved = BTreeMap::new();
    for key in all.keys() {
        resolved.insert(key.clone(), resolve(key, &all, &mut BTreeSet::new())?);
    }
    // Only literal scalar references; arbitrary expressions stay visible in original fields.
    for _ in 0..4 {
        let old = resolved.clone();
        for d in resolved.values_mut() {
            for v in d.fields.values_mut() {
                let s = clean(v);
                if let Some((obj, key)) = s.split_once('.')
                    && let Some(value) = old
                        .get(&obj.to_ascii_lowercase())
                        .and_then(|d| d.fields.get(&key.to_ascii_lowercase()))
                {
                    *v = value.clone();
                }
            }
        }
    }
    let mut pack = Pack {
        effects: Default::default(),
        schema_version: SCHEMA,
        id: "v20.weapons.001".into(),
        items: BTreeMap::new(),
        images: BTreeMap::new(),
        projectiles: BTreeMap::new(),
        damage_types: BTreeMap::new(),
        explosions: BTreeMap::new(),
        sounds: BTreeMap::new(),
        definitions,
        resources: vec![],
        diagnostics: vec![],
    };
    for d in resolved.values_mut() {
        if d.name.eq_ignore_ascii_case("pushBroomItem")
            || d.name.eq_ignore_ascii_case("pushBroomImage")
        {
            d.fields
                .insert("colorshiftcolor".into(), "0.4 0.19607843 0 1".into());
        }
    }
    for d in resolved
        .values()
        .filter(|d| d.class.eq_ignore_ascii_case("ExplosionData"))
    {
        pack.explosions.insert(
            d.name.to_ascii_lowercase(),
            ExplosionInfo {
                name: d.name.clone(),
                sound: field(d, "soundProfile"),
                // Engine defaults: freq 10, amp 1, duration 1.5 s, radius 10, falloff 10.
                shake: flag(d, "shakeCamera", false).then(|| CameraShake {
                    frequency: vec(&field(d, "camShakeFreq"), [10.0; 3]),
                    amplitude: vec(&field(d, "camShakeAmp"), [1.0; 3]),
                    seconds: num(d, "camShakeDuration", 1.5),
                    radius: num(d, "camShakeRadius", 10.0),
                    falloff: num(d, "camShakeFalloff", 10.0),
                }),
                shape: resource(d, "explosionShape"),
                seconds: num(d, "lifetimeMS", 1000.0) / 1000.0,
                play_speed: num(d, "playSpeed", 1.0),
                face_viewer: flag(d, "faceViewer", false),
                scale: axis_scale(vec(&field(d, "explosionScale"), [1.0; 3])),
                // Engine defaults: sizes 1, times 0 then 1.
                sizes: (0..4)
                    .filter_map(|i| {
                        let size = field(d, &format!("sizes[{i}]"));
                        let time = field(d, &format!("times[{i}]"));
                        (!size.is_empty() || !time.is_empty()).then(|| {
                            (
                                axis_scale(vec(&size, [1.0; 3])),
                                time.parse().unwrap_or(if i == 0 { 0.0 } else { 1.0 }),
                            )
                        })
                    })
                    .collect(),
            },
        );
    }
    for d in resolved
        .values()
        .filter(|d| d.class.eq_ignore_ascii_case("ProjectileData"))
    {
        let e = resolved.get(&field(d, "explosion").to_ascii_lowercase());
        let explosion = Explosion {
            effect: field(d, "explosion"),
            damage: e.map_or(0.0, |e| num(e, "radiusDamage", 0.0)),
            radius: e.map_or(0.0, |e| num(e, "damageRadius", 0.0)),
            impulse: e.map_or(0.0, |e| num(e, "impulseForce", 0.0)),
            impulse_radius: e.map_or(0.0, |e| num(e, "impulseRadius", 0.0)),
            impulse_vertical: e.map_or(0.0, |e| num(e, "impulseVertical", 0.0)),
            burn_seconds: e.map_or(0.0, |e| num(e, "playerBurnTime", 0.0) / 1000.0),
        };
        let sport = field(d, "sportBallImage");
        let id = native_id("projectile", &d.name);
        let p = ProjectileDef {
            id: id.clone(),
            name: d.name.clone(),
            model: resource(d, "projectileShapeName"),
            speed: num(d, "muzzleVelocity", 50.0),
            inherit: num(d, "velInheritFactor", 1.0),
            gravity: num(d, "gravityMod", 1.0),
            lifetime_ticks: ticks(num(d, "lifetime", 1000.0) / 1000.0).max(1),
            fade_ticks: ticks(num(d, "fadeDelay", 1000.0) / 1000.0),
            arm_ticks: ticks(num(d, "armingDelay", 0.0) / 1000.0),
            ballistic: flag(d, "isBallistic", false),
            elasticity: num(d, "bounceElasticity", 1.0),
            friction: num(d, "bounceFriction", 0.0),
            damage: num(d, "directDamage", 0.0),
            damage_type: field(d, "directDamageType"),
            radius_damage_type: field(d, "radiusDamageType"),
            impulse: num(d, "impactImpulse", 0.0),
            vertical: num(d, "verticalImpulse", 0.0),
            explode_player: flag(d, "explodeOnPlayerImpact", false),
            explode_death: flag(d, "explodeOnDeath", false),
            collide_players: flag(d, "collideWithPlayers", true),
            explosion,
            brick: BrickImpact {
                radius: num(d, "brickExplosionRadius", 0.0),
                direct: flag(d, "brickExplosionImpact", false),
                force: num(d, "brickExplosionForce", 0.0),
                max_volume: num(d, "brickExplosionMaxVolume", 0.0),
                max_floating_volume: num(d, "brickExplosionMaxVolumeFloating", 0.0),
            },
            bounce_effect: field(d, "bounceExplosion"),
            stick_effect: field(d, "stickExplosion"),
            blood_effect: field(d, "bloodExplosion"),
            bounce_angle: num(d, "bounceAngle", 0.0),
            min_stick_speed: num(d, "minStickVelocity", 0.0),
            trail: field(d, "particleEmitter"),
            sound: field(d, "sound"),
            light_radius: if flag(d, "hasLight", false) {
                num(d, "lightRadius", 0.0)
            } else {
                0.0
            },
            light_color: vec(&field(d, "lightColor"), [1.0; 3]),
            sport_image: (!sport.is_empty()).then(|| native_id("image", &sport)),
            rest_speed: num(d, "restVelocity", 0.0),
            max_bounces: 0,
            children: None,
            aura: None,
        };
        pack.projectiles.insert(id, p);
    }
    for d in resolved
        .values()
        .filter(|d| d.class.eq_ignore_ascii_case("ShapeBaseImageData"))
    {
        let mut states = vec![];
        let mut indices = BTreeMap::new();
        for n in 0..64 {
            let name = field(d, &format!("stateName[{n}]"));
            if !name.is_empty() {
                indices.insert(name.to_ascii_lowercase(), states.len());
                states.push((n, name));
            }
        }
        let mut native = vec![];
        for (n, name) in states {
            let f = |key: &str| field(d, &format!("{key}[{n}]"));
            let target = |key: &str| indices.get(&f(key).to_ascii_lowercase()).copied();
            for key in [
                "stateTransitionOnTimeout",
                "stateTransitionOnTriggerDown",
                "stateTransitionOnTriggerUp",
                "stateTransitionOnAmmo",
                "stateTransitionOnNoAmmo",
            ] {
                if !f(key).is_empty() && target(key).is_none() {
                    pack.diagnostics
                        .push(format!("{} unresolved state target {}", d.name, f(key)));
                }
            }
            native.push(State {
                name,
                ticks: ticks(num(d, &format!("stateTimeoutValue[{n}]"), 0.0)),
                wait: flag(d, &format!("stateWaitForTimeout[{n}]"), true),
                allow_change: flag(d, &format!("stateAllowImageChange[{n}]"), true),
                timeout: target("stateTransitionOnTimeout"),
                down: target("stateTransitionOnTriggerDown"),
                up: target("stateTransitionOnTriggerUp"),
                ammo: target("stateTransitionOnAmmo"),
                no_ammo: target("stateTransitionOnNoAmmo"),
                script: f("stateScript"),
                sequence: f("stateSequence"),
                sound: f("stateSound"),
                emitter: f("stateEmitter"),
                emitter_node: f("stateEmitterNode"),
                emitter_seconds: num(d, &format!("stateEmitterTime[{n}]"), 0.0),
                eject_shell: flag(d, &format!("stateEjectShell[{n}]"), false),
                // v20 swings arms from script by image name, never from state data.
                arm: String::new(),
            });
        }
        let p = field(d, "projectile");
        let projectile = (!p.trim().is_empty()).then(|| native_id("projectile", &p));
        if let Some(p) = &projectile
            && !pack.projectiles.contains_key(p)
        {
            pack.diagnostics
                .push(format!("{} missing {p}; image excluded", d.name));
            continue;
        }
        let rotation = source_rotation(&field(d, "rotation"));
        if rotation.is_none() {
            pack.diagnostics.push(format!(
                "{} rotation is not a literal Euler or axis rotation",
                d.name
            ));
        }
        let eye_rotation = source_rotation(&field(d, "eyeRotation"));
        if eye_rotation.is_none() {
            pack.diagnostics.push(format!(
                "{} eyeRotation is not a literal Euler or axis rotation",
                d.name
            ));
        }
        let id = native_id("image", &d.name);
        pack.images.insert(
            id.clone(),
            Image {
                id,
                name: d.name.clone(),
                model: resource(d, "shapeFile"),
                projectile,
                mount_point: num(d, "mountPoint", 0.0) as u32,
                offset: axis(vec(&field(d, "offset"), [0.0; 3])),
                eye_offset: axis(vec(&field(d, "eyeOffset"), [0.0; 3])),
                source_rotation_degrees: rotation.unwrap_or([0.0; 3]),
                correct_muzzle: flag(d, "correctMuzzleVector", false),
                melee: flag(d, "melee", false),
                color: vec(&field(d, "colorShiftColor"), [1.0; 4]),
                color_shift: flag(d, "doColorShift", false),
                arm_ready: flag(d, "armReady", false),
                casing: field(d, "casing"),
                min_shot_ticks: ticks(num(d, "minShotTime", 0.0) / 1000.0),
                states: native,
                command: None,
                commands: Default::default(),
                shot: None,
                eye_rotation: eye_rotation.unwrap_or([0.0; 3]),
                zoom: None,
                crosshair: true,
                follow_arm: false,
                paint_tint: false,
                left_image: None,
                magazine: None,
                volleys: vec![],
                last_shot: None,
            },
        );
    }
    for d in resolved
        .values()
        .filter(|d| d.class.eq_ignore_ascii_case("ItemData"))
    {
        let image = native_id("image", &field(d, "image"));
        if !pack.images.contains_key(&image) {
            continue;
        }
        // v20 lists only named items; one without a `uiName` is hidden and
        // only scripts mount its image, which is kept.
        if field(d, "uiName").trim().is_empty() {
            pack.diagnostics.push(format!(
                "item {} has no uiName: left out, its image kept",
                d.name
            ));
            continue;
        }
        let item = item(d, image);
        pack.items.insert(item.id.clone(), item);
    }
    pack.validate()?;
    Ok(pack)
}
fn item(d: &Definition, image: String) -> Item {
    Item {
        id: native_id("weapon", &d.name),
        name: d.name.clone(),
        ui_name: field(d, "uiName"),
        image,
        model: resource(d, "shapeFile"),
        icon: resource(d, "iconName"),
        can_drop: flag(d, "canDrop", true),
        sport: flag(d, "isSportBall", false),
    }
}
/// An `ItemData` with a `uiName` but no `image`: picked up, held by nobody
/// (an ammo box). `None` for any other item.
pub fn pickup_item(d: &Definition) -> Option<Item> {
    (d.class.eq_ignore_ascii_case("ItemData")
        && field(d, "image").is_empty()
        && !field(d, "uiName").trim().is_empty())
    .then(|| item(d, String::new()))
}
fn check_output(root: &Path, out: &Path) -> Result<()> {
    let reference = root.canonicalize()?;
    let absolute = if out.is_absolute() {
        out.to_path_buf()
    } else {
        std::env::current_dir()?.join(out)
    };
    let parent = absolute
        .parent()
        .context("Output has no parent")?
        .canonicalize()?;
    ensure!(
        !parent.starts_with(&reference),
        "Output must stay outside the read-only reference installation"
    );
    Ok(())
}
/// Core datablocks presented by native hosts without an add-on referencing
/// them: emote/pain/burn images, the spawn/death projectiles and the player's
/// water splash (converted to an effect by the weapon effects importer).
const CORE_PRESENTATION: [&str; 8] = [
    "PlayerSplash",
    "clockProjectile",
    "PainLowImage",
    "PainMidImage",
    "PainHighImage",
    "PlayerBurnImage",
    "spawnProjectile",
    "deathProjectile",
];
/// Core datablocks imported by name, with everything they reference. The
/// building tools, both wands and the spray cans are ordinary v20 images: the
/// base colour can (`blueSprayCanImage`) is the template `setSprayCanColor`
/// derives every palette can from, and the nine FX cans are literal.
/// `brickImage` is the grey 2x2 `fxDTSBrickData::onUse` mounts in the right
/// hand while bricks are in hand (and `horseBrickImage`'s parent).
const CORE_ROOTS: [&str; 17] = [
    "clockProjectile",
    "hammerItem",
    "wrenchItem",
    "printGun",
    "WandItem",
    "AdminWandImage",
    "blueSprayCanImage",
    "flatSprayCanImage",
    "pearlSprayCanImage",
    "chromeSprayCanImage",
    "glowSprayCanImage",
    "blinkSprayCanImage",
    "swirlSprayCanImage",
    "rainbowSprayCanImage",
    "stableSprayCanImage",
    "jelloSprayCanImage",
    "brickImage",
];
/// `setSprayCanColor` swaps a translucent palette colour's can to this shape.
/// No datablock names it literally, so it is imported explicitly.
pub const EXTRA_MODELS: [&str; 1] = ["base/data/shapes/transspraycan.dts"];
/// `core` is the recovered `allGameScripts.cs`; `core_damage_types` the
/// recovered `DamageTypes.cs` holding `initDefaultDamageTypes`.
pub fn convert(root: &Path, core: &Path, core_damage_types: &Path, out: &Path) -> Result<Pack> {
    check_output(root, out)?;
    ensure!(
        !out.exists(),
        "Output already exists; choose fresh pack directory"
    );
    let mut assets = BTreeMap::new();
    let mut imported_bytes = 0usize;
    let mut defs = vec![];
    // Load order: base defaults, base script, then add-ons alphabetically.
    let core_text = std::fs::read_to_string(core)?;
    let mut damage = damage_types(&std::fs::read_to_string(core_damage_types)?)?;
    damage.extend(damage_types(&core_text)?);
    let mut paths = std::fs::read_dir(root.join("Add-Ons"))?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !["Weapon_", "Item_", "Projectile_", "Vehicle_", "Emote_"]
            .iter()
            .any(|p| name.starts_with(p))
            || path.extension().is_none_or(|s| s != "zip")
        {
            continue;
        }
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&path)?)?;
        ensure!(archive.len() <= 4096, "Archive member budget");
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if file.is_dir() {
                continue;
            }
            ensure!(file.size() <= 32 * 1024 * 1024, "Oversized archive member");
            let virtual_path = format!("Add-Ons/{name}/{}", file.name().replace('\\', "/"));
            let mut data = vec![];
            file.by_ref()
                .take(32 * 1024 * 1024 + 1)
                .read_to_end(&mut data)?;
            ensure!(
                data.len() <= 32 * 1024 * 1024,
                "Oversized decompressed member"
            );
            imported_bytes = imported_bytes
                .checked_add(data.len())
                .context("Import size overflow")?;
            ensure!(
                imported_bytes <= 256 * 1024 * 1024,
                "Aggregate archive byte budget"
            );
            if virtual_path.ends_with(".cs") {
                let text = String::from_utf8_lossy(&data);
                defs.extend(parse(&text, &virtual_path)?);
                damage.extend(damage_types(&text)?);
            }
            assets.insert(virtual_path.to_ascii_lowercase(), (virtual_path, data));
        }
    }
    // Core dependencies are imported only when explicitly referenced from this closure.
    let core_defs = parse(
        &core_text,
        "base/server/scripts/allGameScripts.cs (recovered)",
    )?;
    defs.extend(
        core_defs
            .iter()
            .filter(|d| {
                CORE_PRESENTATION
                    .iter()
                    .any(|n| d.name.eq_ignore_ascii_case(n))
                    || CORE_ROOTS
                        .iter()
                        .any(|root| d.name.eq_ignore_ascii_case(root))
            })
            .cloned(),
    );
    let mut names: BTreeSet<_> = defs.iter().map(|d| d.name.to_ascii_lowercase()).collect();
    for _ in 0..8 {
        let refs = defs
            .iter()
            .flat_map(|d| {
                // Projectile inheritance (`jeepExplosionProjectile : vehicleExplosionProjectile`).
                let parent = d
                    .parent
                    .as_ref()
                    .filter(|_| d.class.eq_ignore_ascii_case("ProjectileData"));
                d.fields.values().chain(parent)
            })
            .map(|s| clean(s).to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let add: Vec<_> = core_defs
            .iter()
            .filter(|d| {
                !names.contains(&d.name.to_ascii_lowercase())
                    && refs.contains(&d.name.to_ascii_lowercase())
            })
            .cloned()
            .collect();
        if add.is_empty() {
            break;
        }
        for d in add {
            names.insert(d.name.to_ascii_lowercase());
            defs.push(d);
        }
    }
    // Numeric core globals such as `$HeadSlot = 5;` used as literal field values.
    let global = Regex::new(r"(?m)^\$(\w+)\s*=\s*(-?\d+(?:\.\d+)?)\s*;")?;
    let globals: BTreeMap<String, String> = global
        .captures_iter(&core_text)
        .map(|c| (format!("${}", c[1].to_ascii_lowercase()), c[2].to_owned()))
        .collect();
    for d in &mut defs {
        for v in d.fields.values_mut() {
            if let Some(n) = globals.get(&clean(v).to_ascii_lowercase()) {
                *v = n.clone();
            }
        }
    }
    let mut pack = lower(defs)?;
    for t in damage {
        // `AddDamageType` refuses a type whose icon file is missing.
        let missing: Vec<_> = t
            .icons()
            .filter(|id| {
                let file = format!("{id}.png");
                !assets.contains_key(&file) && !root.join(&file).is_file()
            })
            .collect();
        if missing.is_empty() {
            pack.damage_types.insert(t.name.to_ascii_lowercase(), t);
        } else {
            pack.diagnostics.push(format!(
                "Damage type {} icon missing: {}",
                t.name,
                missing.join(", ")
            ));
        }
    }
    std::fs::create_dir_all(out.join("shapes"))?;
    std::fs::create_dir_all(out.join("textures"))?;
    let paths: BTreeSet<_> = pack
        .images
        .values()
        .map(|i| i.model.clone())
        .chain(pack.projectiles.values().map(|p| p.model.clone()))
        .chain(pack.explosions.values().map(|e| e.shape.clone()))
        .chain(EXTRA_MODELS.iter().map(|p| (*p).to_owned()))
        .filter(|p| !p.is_empty())
        .collect();
    let mut texture_references: Vec<(String, String)> = pack
        .items
        .values()
        .filter(|i| !i.icon.is_empty())
        .map(|i| (i.icon.clone(), i.name.clone()))
        .collect();
    for path in paths {
        let data = assets
            .get(&path.to_ascii_lowercase())
            .map(|(_, v)| v.clone())
            .or_else(|| std::fs::read(root.join(&path)).ok());
        let Some(data) = data else {
            pack.diagnostics.push(format!("Missing model {path}"));
            continue;
        };
        let mut r = Resource {
            path: path.clone(),
            sha256: hash(&data),
            native_file: None,
            diagnostics: vec![],
            package: None,
        };
        match bri_convert::shape::read_dts(
            &data,
            format!("v20.shape.{}", path.to_ascii_lowercase()),
        ) {
            Ok((shape, provenance)) => {
                for material in &shape.materials {
                    let parent = path.rsplit_once('/').map_or("", |(p, _)| p);
                    texture_references.push((format!("{parent}/{}", material.name), path.clone()));
                }
                let file = format!("shapes/{}.json", &r.sha256[..24]);
                std::fs::write(out.join(&file), serde_json::to_vec(&shape)?)?;
                r.native_file = Some(file);
                r.diagnostics = provenance.warnings;
            }
            Err(e) => r.diagnostics.push(format!("DTS conversion: {e:#}")),
        }
        pack.resources.push(r);
    }
    for (reference, owner) in texture_references {
        let mut found = false;
        for extension in ["", ".png", ".jpg"] {
            let path = format!("{reference}{extension}");
            if assets.contains_key(&path.to_ascii_lowercase()) {
                found = true;
                break;
            }
            if let Ok(bytes) = std::fs::read(root.join(&path)) {
                ensure!(bytes.len() <= 32 * 1024 * 1024, "Texture byte budget");
                assets.insert(path.to_ascii_lowercase(), (path, bytes));
                found = true;
                break;
            }
        }
        if !found {
            pack.diagnostics.push(format!(
                "Unresolved texture/icon {reference} referenced by {owner}"
            ));
        }
    }
    for (_, (path, data)) in assets {
        if [".png", ".jpg"]
            .iter()
            .any(|e| path.to_ascii_lowercase().ends_with(e))
        {
            let digest = hash(&data);
            let ext = path.rsplit('.').next().unwrap();
            let file = format!("textures/{}.{}", &digest[..24], ext);
            std::fs::write(out.join(&file), &data)?;
            pack.resources.push(Resource {
                path,
                sha256: digest,
                native_file: Some(file),
                diagnostics: vec![],
                package: None,
            });
        }
    }
    pack.validate()?;
    std::fs::write(out.join("weapons.json"), serde_json::to_vec_pretty(&pack)?)?;
    Ok(pack)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments_inheritance_and_timing() {
        let d=parse("// no\ndatablock ItemData(x) {a=1; text=\"http://ok\";}; datablock ItemData(y:x){a=2;};","test").unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].source.line, 2);
        assert_eq!(field(&d[0], "text"), "http://ok");
        let a = d.iter().map(|d| (d.name.clone(), d.clone())).collect();
        assert_eq!(
            field(&resolve("y", &a, &mut BTreeSet::new()).unwrap(), "a"),
            "2"
        );
        assert_eq!(ticks(0.14), 17);
    }
    #[test]
    fn damage_types_keep_literal_messages_in_order() {
        let t = damage_types(
            "// AddDamageType(\"Old\", 'x', 'y', 1, 1);
if(!$DamageType::Gun)
  AddDamageType(\"Gun\",   '<bitmap:add-ons/Weapon_Gun/CI_gun> %1',    '%2 <bitmap:add-ons/Weapon_Gun/CI_gun> %1',0.2,1);
AddDamageType(\"Radius\", '<bitmap:base/client/ui/ci/bomb> %1', '%2 <bitmap:base/client/ui/ci/splat> %1', 1, 0);",
        )
        .unwrap();
        assert_eq!(t.len(), 2);
        assert!(t[0].direct && !t[1].direct);
        assert_eq!(
            t[0].icons().collect::<Vec<_>>(),
            ["add-ons/weapon_gun/ci_gun"; 2]
        );
        assert_eq!(
            t[1].message("Victim", None),
            "<bitmap:base/client/ui/ci/bomb> Victim"
        );
        assert_eq!(
            t[1].message("%2", Some("Killer")),
            "Killer <bitmap:base/client/ui/ci/splat> %2"
        );
    }
    #[test]
    fn rotations_accept_euler_and_principal_axis_literals() {
        assert_eq!(source_rotation(""), Some([0.0; 3]));
        assert_eq!(
            source_rotation("eulerToMatrix( \"0 35 90\" )"),
            Some([0.0, 35.0, 90.0])
        );
        assert_eq!(source_rotation("1 0 0 -90"), Some([-90.0, 0.0, 0.0]));
        assert_eq!(source_rotation("0 0 -1 180"), Some([0.0, 0.0, -180.0]));
        // Any axis (TypeMatrixRotation normalizes it).
        assert_eq!(
            source_rotation("1 1 0 45"),
            bri_weapons::rotation::axis_angle([1.0, 1.0, 0.0], 45.0)
        );
        assert_eq!(source_rotation("0 0 0 45"), None);
    }
    #[test]
    fn cycles_reject() {
        let d = parse(
            "datablock ItemData(a:b){};datablock ItemData(b:a){};",
            "test",
        )
        .unwrap();
        assert!(lower(d).is_err());
    }
    #[test]
    fn reference_output_is_rejected_before_writes() {
        let root = std::env::current_dir().unwrap();
        assert!(check_output(&root, &root.join("never-created-pack")).is_err());
    }
}
