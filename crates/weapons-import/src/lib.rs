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
    } else {
        p
    }
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
        schema_version: SCHEMA,
        id: "v20.weapons.001".into(),
        items: BTreeMap::new(),
        images: BTreeMap::new(),
        projectiles: BTreeMap::new(),
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
        .filter(|d| d.class.eq_ignore_ascii_case("ProjectileData"))
    {
        let e = resolved.get(&field(d, "explosion").to_ascii_lowercase());
        let explosion = Explosion {
            effect: field(d, "explosion"),
            damage: e.map_or(0.0, |e| num(e, "radiusDamage", 0.0)),
            radius: e.map_or(0.0, |e| num(e, "damageRadius", 0.0)),
            impulse: e.map_or(0.0, |e| num(e, "impulseForce", 0.0)),
            impulse_radius: e.map_or(0.0, |e| num(e, "impulseRadius", 0.0)),
            burn_seconds: e.map_or(0.0, |e| num(e, "playerBurnTime", 0.0)),
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
        let rotation = field(d, "rotation");
        let rot = rotation.split('"').nth(1).unwrap_or("");
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
                source_rotation_degrees: vec(rot, [0.0; 3]),
                correct_muzzle: flag(d, "correctMuzzleVector", false),
                melee: flag(d, "melee", false),
                color: vec(&field(d, "colorShiftColor"), [1.0; 4]),
                color_shift: flag(d, "doColorShift", false),
                arm_ready: flag(d, "armReady", false),
                casing: field(d, "casing"),
                min_shot_ticks: ticks(num(d, "minShotTime", 0.0) / 1000.0),
                states: native,
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
        let id = native_id("weapon", &d.name);
        pack.items.insert(
            id.clone(),
            Item {
                id,
                name: d.name.clone(),
                ui_name: field(d, "uiName"),
                image,
                model: resource(d, "shapeFile"),
                icon: resource(d, "iconName"),
                can_drop: flag(d, "canDrop", true),
                sport: flag(d, "isSportBall", false),
            },
        );
    }
    pack.validate()?;
    Ok(pack)
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
/// Core datablocks imported by name, with everything they reference. The
/// building tools, both wands and the spray cans are ordinary v20 images: the
/// base colour can (`blueSprayCanImage`) is the template `setSprayCanColor`
/// derives every palette can from, and the nine FX cans are literal.
const CORE_ROOTS: [&str; 16] = [
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
];
/// `setSprayCanColor` swaps a translucent palette colour's can to this shape.
/// No datablock names it literally, so it is imported explicitly.
pub const EXTRA_MODELS: [&str; 1] = ["base/data/shapes/transspraycan.dts"];
pub fn convert(root: &Path, core: &Path, out: &Path) -> Result<Pack> {
    check_output(root, out)?;
    ensure!(
        !out.exists(),
        "Output already exists; choose fresh pack directory"
    );
    let mut assets = BTreeMap::new();
    let mut imported_bytes = 0usize;
    let mut defs = vec![];
    let mut paths = std::fs::read_dir(root.join("Add-Ons"))?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if ![
            "Weapon_",
            "Item_",
            "Projectile_",
            "Vehicle_Tank",
            "Vehicle_Pirate_Cannon",
        ]
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
                defs.extend(parse(&String::from_utf8_lossy(&data), &virtual_path)?);
            }
            assets.insert(virtual_path.to_ascii_lowercase(), (virtual_path, data));
        }
    }
    // Core dependencies are imported only when explicitly referenced from this closure.
    let core_defs = parse(
        &std::fs::read_to_string(core)?,
        "base/server/scripts/allGameScripts.cs (recovered)",
    )?;
    defs.extend(
        core_defs
            .iter()
            .filter(|d| {
                CORE_ROOTS
                    .iter()
                    .any(|root| d.name.eq_ignore_ascii_case(root))
            })
            .cloned(),
    );
    let mut names: BTreeSet<_> = defs.iter().map(|d| d.name.to_ascii_lowercase()).collect();
    for _ in 0..8 {
        let refs = defs
            .iter()
            .flat_map(|d| d.fields.values())
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
    let mut pack = lower(defs)?;
    std::fs::create_dir_all(out.join("shapes"))?;
    std::fs::create_dir_all(out.join("textures"))?;
    let paths: BTreeSet<_> = pack
        .images
        .values()
        .map(|i| i.model.clone())
        .chain(pack.projectiles.values().map(|p| p.model.clone()))
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
