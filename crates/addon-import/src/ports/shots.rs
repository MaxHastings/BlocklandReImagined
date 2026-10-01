//! How a port's guns fire, read from the Add-On itself: [`Shots`] from each
//! image's `onFire` written with v20's spread code, [`Hitscans`] from the
//! image fields of a raycasting support script. Both follow each copy's own
//! numbers, so a copy with other guns gets its own, and a gun the readers
//! cannot place stops the port with its name.
use super::datablocks::{Datablocks, id_of, set};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Shots read from each image's `onFire` written with v20's spread code
/// (`%projectile = ...; %spread = ...; %shellcount = ...;` then the loop
/// that fires them). Each such block is a set of projectiles; one with a
/// `setVelocity` recoil starts a shot, and the blocks after it until the
/// next recoil are its volleys (a shotgun's slug after its pellets).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shots {
    /// For an `onFire` that fires one of several shots (a branch per
    /// magazine count), which one to port, by image datablock name: 0 for
    /// the first. An image with several and no pick stops the port.
    #[serde(default)]
    pub pick: BTreeMap<String, usize>,
    /// For an `onFire` whose magazine's last few rounds fire another of
    /// its shots (a two-barrel gun's single barrel), which shot and from
    /// how many rounds left, by image datablock name.
    #[serde(default)]
    pub last: BTreeMap<String, Last>,
}

/// [`Shots::last`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Last {
    /// The shot, 0 for the first.
    pub shot: usize,
    /// With this many rounds or fewer left.
    pub rounds: u32,
}

/// Hitscan guns read from their images' fields, by the names a raycasting
/// support script gave them. Each gun whose `when` field is set gets
/// `shot.hitscan`, and a projectile of its own (`<image>Ray`) that carries
/// the image's damage, so hit rules and `on_damage` see it by id.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hitscans {
    /// The field that makes an image hitscan (`raycastEnabled`; or its
    /// range when the script has no switch).
    pub when: String,
    /// The field holding its range in units.
    pub range: String,
    /// The field that casts from the eye (`raycastFromEye`).
    #[serde(default)]
    pub from_eye: Option<String>,
    /// The field that casts from the muzzle when set, from the eye
    /// otherwise (`raycastFromMuzzle`).
    #[serde(default)]
    pub from_muzzle: Option<String>,
    /// The field holding a hit's damage, and the most it may be (the
    /// script's own clamp), when it has one.
    pub damage: String,
    #[serde(default)]
    pub damage_limit: Option<f32>,
    /// The field holding its damage type (`$DamageType::Gun`).
    pub damage_type: String,
    /// The field naming the projectile whose look and blast a hit shows;
    /// the image's own projectile without it.
    #[serde(default)]
    pub hit_projectile: Option<String>,
    /// The fields holding the shove along the shot and straight up.
    #[serde(default)]
    pub impulse: Option<String>,
    #[serde(default)]
    pub vertical: Option<String>,
    /// The fields holding rays per shot and their spread (v20's `%spread`
    /// units, or degrees across with `spread_degrees`).
    #[serde(default)]
    pub count: Option<String>,
    #[serde(default)]
    pub spread: Option<String>,
    #[serde(default)]
    pub spread_degrees: bool,
    /// A streak for each image whose `field` is set, drawn as `look`
    /// (the engine's tracer: `color`, `width`, `seconds`).
    #[serde(default)]
    pub tracer: Option<TracerField>,
}

/// [`Hitscans::tracer`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TracerField {
    pub field: String,
    pub look: Value,
}

/// What the readers add to `weapons.json`: a merge patch, and definitions
/// for the projectiles they made, so rule tables find them.
#[derive(Debug, Default)]
pub struct Reading {
    pub patch: Value,
    pub definitions: Vec<Value>,
}

/// One block of v20's spread code.
#[derive(Debug, PartialEq)]
struct Block {
    /// The datablock fired, `None` for the image's own.
    projectile: Option<String>,
    spread: f32,
    count: u32,
    /// `setVelocity` recoil along the aim, when the block has one.
    recoil: Option<f32>,
    /// The `scale` the script gave the projectiles it made.
    scale: f32,
}

/// A body without its `//` comments.
fn uncommented(body: &str) -> String {
    body.lines()
        .map(|l| l.find("//").map_or(l, |i| &l[..i]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `scale = "x y z"` the first projectile made in `text` was given:
/// its height, as v20 read a projectile's scale.
fn scale_of(text: &str) -> f32 {
    let scale =
        regex::RegexBuilder::new(r#"\bscale\s*=\s*"\s*[0-9.]+\s+[0-9.]+\s+([0-9]*\.?[0-9]+)\s*""#)
            .case_insensitive(true)
            .build()
            .expect("pattern");
    scale
        .captures(text)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(1.0)
}

/// The first `setVelocity` kick back along the aim in `text`.
fn recoil_of(text: &str) -> Option<f32> {
    let recoil = regex::RegexBuilder::new(
        r#"setvelocity\s*\([^;]*vectorscale\s*\([^;]*?,\s*"?\s*-\s*([0-9]*\.?[0-9]+)\s*"?\s*\)"#,
    )
    .case_insensitive(true)
    .build()
    .expect("pattern");
    recoil.captures(text).and_then(|r| r[1].parse().ok())
}

fn blocks(body: &str) -> Vec<Block> {
    let body = uncommented(body);
    let set = regex::RegexBuilder::new(
        r"%projectile\s*=\s*([^;]+?)\s*;\s*%spread\s*=\s*([0-9]*\.?[0-9]+)\s*;\s*%shellcount\s*=\s*(\d+)\s*;",
    )
    .case_insensitive(true)
    .build()
    .expect("pattern");
    let found: Vec<_> = set.captures_iter(&body).collect();
    found
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let whole = c.get(0).expect("match");
            let end = found
                .get(i + 1)
                .map_or(body.len(), |n| n.get(0).expect("match").start());
            let expr = c[1].trim();
            let projectile = (!expr.eq_ignore_ascii_case("%this.projectile"))
                .then(|| crate::literal(expr).to_owned());
            Block {
                projectile,
                spread: c[2].parse().unwrap_or(0.0),
                count: c[3].parse().unwrap_or(1),
                recoil: recoil_of(&body[whole.end()..end]),
                scale: scale_of(&body[whole.end()..end]),
            }
        })
        .collect()
}

/// The images' shots and volleys from their `onFire` bodies.
pub fn shots(s: &Shots, weapons: &Value, bodies: &super::Bodies) -> Result<Value> {
    let mut images = serde_json::Map::new();
    for (id, image) in weapons["images"].as_object().into_iter().flatten() {
        let name = image["name"].as_str().unwrap_or_default();
        let Some(body) = bodies.get(&format!("{}::onfire", name.to_ascii_lowercase())) else {
            continue;
        };
        let blocks = blocks(body);
        if blocks.is_empty() {
            // No spread code, only a kick before the shot it hands on
            // (`Parent::onFire`, a raycast): one projectile straight.
            if let Some(recoil) = recoil_of(&uncommented(body)) {
                images.insert(
                    id.clone(),
                    json!({ "shot": { "projectiles": 1, "spread": 0.0, "recoil": recoil } }),
                );
            }
            continue;
        }
        // Each recoil starts a shot; the blocks after it are its volleys.
        let mut shots: Vec<Vec<&Block>> = vec![];
        for b in &blocks {
            match shots.last_mut() {
                Some(shot) if b.recoil.is_none() => shot.push(b),
                _ => shots.push(vec![b]),
            }
        }
        let pick = s
            .pick
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v);
        let shot = match (shots.len(), pick) {
            (1, None) => &shots[0],
            (_, Some(n)) => shots
                .get(n)
                .with_context(|| format!("{name}: pick {n}, but it fires {} ways", shots.len()))?,
            (n, None) => bail!("{name}: its onFire fires {n} ways; the port picks none"),
        };
        let projectile = |b: &Block| -> Result<Option<String>> {
            b.projectile
                .as_deref()
                .map(|n| {
                    id_of(weapons, "ProjectileData", n)
                        .with_context(|| format!("{name}: it fires {n}, which did not import"))
                })
                .transpose()
        };
        // A shot and its volleys, and the projectile it fires when that
        // is not the image's own.
        let read = |shot: &[&Block]| -> Result<(Value, Vec<Value>, Option<String>)> {
            let main = shot[0];
            let mut volleys = vec![];
            for b in &shot[1..] {
                let p = match projectile(b)? {
                    Some(p) => p,
                    None => image["projectile"]
                        .as_str()
                        .with_context(|| format!("{name}: no projectile of its own"))?
                        .to_owned(),
                };
                volleys
                    .push(json!({ "projectile": p, "projectiles": b.count, "spread": b.spread }));
            }
            let mut fired = json!({
                "projectiles": main.count,
                "spread": main.spread,
                "recoil": main.recoil.unwrap_or(0.0),
            });
            if main.scale != 1.0 {
                fired["scale"] = json!(main.scale);
            }
            Ok((fired, volleys, projectile(main)?))
        };
        let (fired, volleys, own) = read(shot)?;
        let mut patch = json!({ "shot": fired });
        if let Some(p) = own {
            patch["projectile"] = json!(p);
        }
        if !volleys.is_empty() {
            patch["volleys"] = Value::Array(volleys);
        }
        if let Some((_, last)) = s.last.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)) {
            let group = shots.get(last.shot).with_context(|| {
                format!(
                    "{name}: last shot {}, but it fires {} ways",
                    last.shot,
                    shots.len()
                )
            })?;
            let (fired, volleys, own) = read(group)?;
            ensure!(
                own.is_none(),
                "{name}: its last shot fires another projectile than the image's"
            );
            patch["last_shot"] = json!({ "shot": fired, "volleys": volleys });
            patch["magazine"] = json!({ "last_rounds": last.rounds });
        }
        images.insert(id.clone(), patch);
    }
    // Fire states with scripts of their own (`onFire2`), each one shot of
    // the image's own projectile.
    for (id, image) in weapons["images"].as_object().into_iter().flatten() {
        let name = image["name"].as_str().unwrap_or_default();
        let mut state_shots = serde_json::Map::new();
        for state in image["states"].as_array().into_iter().flatten() {
            let script = state["script"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if script.is_empty() || script == "onfire" || state_shots.contains_key(&script) {
                continue;
            }
            let Some(body) = bodies.get(&format!("{}::{script}", name.to_ascii_lowercase())) else {
                continue;
            };
            let blocks = blocks(body);
            let [b] = blocks.as_slice() else {
                ensure!(
                    blocks.is_empty(),
                    "{name}: its {script} fires {} sets; a fire state's own shot is one",
                    blocks.len()
                );
                continue;
            };
            ensure!(
                b.projectile.is_none(),
                "{name}: its {script} fires another projectile than the image's"
            );
            let mut shot = json!({
                "projectiles": b.count,
                "spread": b.spread,
                "recoil": b.recoil.unwrap_or(0.0),
            });
            if b.scale != 1.0 {
                shot["scale"] = json!(b.scale);
            }
            state_shots.insert(script, shot);
        }
        if !state_shots.is_empty() {
            let entry = images.entry(id.clone()).or_insert_with(|| json!({}));
            entry["state_shots"] = Value::Object(state_shots);
        }
    }
    // Projectiles whose own `damage` dealt `directDamage` as it was, where
    // v20's default scaled it with the projectile.
    let mut projectiles = serde_json::Map::new();
    for (id, p) in weapons["projectiles"].as_object().into_iter().flatten() {
        let name = p["name"].as_str().unwrap_or_default().to_ascii_lowercase();
        if let Some(body) = bodies.get(&format!("{name}::damage"))
            && !uncommented(body).to_ascii_lowercase().contains("getscale")
        {
            projectiles.insert(id.clone(), json!({ "fixed_damage": true }));
        }
    }
    for (id, image) in weapons["images"].as_object().into_iter().flatten() {
        let patch = scripted(image, weapons, bodies);
        if patch.as_object().is_some_and(|p| !p.is_empty()) {
            let entry = images.entry(id.clone()).or_insert_with(|| json!({}));
            super::merge(entry, &patch);
            // A kick on a gun with no shot of its own: v20's one straight.
            if entry["shot"].is_object()
                && entry["shot"]["projectiles"].is_null()
                && image["shot"].is_null()
            {
                entry["shot"]["projectiles"] = json!(1);
            }
        }
    }
    let mut out = json!({ "images": images });
    if !projectiles.is_empty() {
        out["projectiles"] = Value::Object(projectiles);
    }
    Ok(out)
}

/// What an image's state scripts did by hand that its states can say
/// themselves: the sound each played (`serverPlay3d`), the arm move
/// (`playThread(2, ...)`) and gesture (`playThread(3, ...)`), each where
/// the state names none; and the kick of the recoil blast `onFire` set off
/// at the shooter (`spawnExplosion` of a projectile whose explosion shakes
/// the camera), as the shot's `kick`.
fn scripted(image: &Value, weapons: &Value, bodies: &super::Bodies) -> Value {
    let name = image["name"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let Some(states) = image["states"].as_array() else {
        return json!({});
    };
    let call = |what: &str| {
        regex::RegexBuilder::new(what)
            .case_insensitive(true)
            .build()
            .expect("pattern")
    };
    let sound_re = call(r"serverplay3d\s*\(\s*([A-Za-z_]\w*)\s*,");
    let arm_re = call(r"playthread\s*\(\s*2\s*,\s*([A-Za-z_]\w*)\s*\)");
    let gesture_re = call(r"playthread\s*\(\s*3\s*,\s*([A-Za-z_]\w*)\s*\)");
    let blast_re = call(r"spawnexplosion\s*\(\s*([A-Za-z_]\w*)\s*,");
    let sound_id = |n: &str| {
        let suffix = format!(":sound/{}", n.to_ascii_lowercase());
        weapons["sounds"]
            .as_object()?
            .keys()
            .find(|k| k.ends_with(&suffix))
            .cloned()
    };
    let mut patch = json!({});
    let mut changed = states.clone();
    let mut any = false;
    for state in changed.iter_mut() {
        let script = state["script"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if script.is_empty() {
            continue;
        }
        let Some(body) = bodies.get(&format!("{name}::{script}")) else {
            continue;
        };
        let body = uncommented(body);
        if state["sound"].as_str().unwrap_or_default().is_empty()
            && let Some(sound) = sound_re.captures(&body).and_then(|c| sound_id(&c[1]))
        {
            state["sound"] = json!(sound);
            any = true;
        }
        if state["arm"].as_str().unwrap_or_default().is_empty()
            && let Some(arm) = arm_re.captures(&body)
        {
            state["arm"] = json!(arm[1].to_ascii_lowercase());
            any = true;
        }
        if state["gesture"].as_str().unwrap_or_default().is_empty()
            && let Some(gesture) = gesture_re.captures(&body)
        {
            state["gesture"] = json!(gesture[1].to_ascii_lowercase());
            any = true;
        }
        if let Some(kick) = blast_re
            .captures(&body)
            .and_then(|c| kick_of(weapons, &c[1]))
        {
            if script == "onfire" {
                patch["shot"] = json!({ "kick": kick });
            } else if blocks(&body).len() == 1 {
                patch["state_shots"][&script] = json!({ "kick": kick });
            }
        }
    }
    if any {
        patch["states"] = Value::Array(changed);
    }
    patch
}

/// The camera shake of a recoil blast's explosion, as a shot's `kick`.
fn kick_of(weapons: &Value, projectile: &str) -> Option<Value> {
    let id = id_of(weapons, "ProjectileData", projectile)?;
    let effect = weapons["projectiles"][&id]["explosion"]["effect"].as_str()?;
    let shake = &weapons["explosions"][&effect.to_ascii_lowercase()]["shake"];
    let numbers = |v: &Value| -> Vec<f32> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n.as_f64().map(|n| n as f32))
            .collect()
    };
    let amplitude = numbers(&shake["amplitude"])
        .into_iter()
        .fold(0.0f32, f32::max);
    let frequency = numbers(&shake["frequency"]);
    let seconds = shake["seconds"].as_f64()? as f32;
    if amplitude <= 0.0 || frequency.is_empty() || seconds <= 0.0 {
        return None;
    }
    let frequency = frequency.iter().sum::<f32>() / frequency.len() as f32;
    Some(json!({
        "amplitude": amplitude.min(1.0),
        "frequency": frequency.clamp(0.1, 30.0),
        "seconds": seconds.clamp(0.05, 2.0),
    }))
}

/// The hitscan guns' shots and projectiles from their image fields.
pub fn hitscans(h: &Hitscans, weapons: &Value) -> Result<Reading> {
    let blocks = Datablocks::new(weapons);
    let number = |image: &str, field: &Option<String>| -> Result<Option<f32>> {
        let Some(field) = field else { return Ok(None) };
        blocks
            .field(image, field)
            .map(|v| {
                v.trim()
                    .parse::<f32>()
                    .with_context(|| format!("{image}: {field} `{v}` is not a number"))
            })
            .transpose()
    };
    let mut reading = Reading {
        patch: json!({ "images": {}, "projectiles": {} }),
        definitions: vec![],
    };
    for (id, image) in weapons["images"].as_object().into_iter().flatten() {
        let name = image["name"].as_str().unwrap_or_default();
        if !set(blocks.field(name, &h.when)) {
            continue;
        }
        let range = number(name, &Some(h.range.clone()))?
            .with_context(|| format!("{name}: no {}", h.range))?;
        ensure!(
            (1.0..=2000.0).contains(&range),
            "{name}: range {range} is outside 1 to 2000"
        );
        let from_eye = match (&h.from_eye, &h.from_muzzle) {
            (Some(f), _) => set(blocks.field(name, f)),
            (None, Some(f)) => !set(blocks.field(name, f)),
            (None, None) => false,
        };
        let mut hitscan = json!({ "range": range, "from_eye": from_eye });
        if let Some(t) = &h.tracer
            && set(blocks.field(name, &t.field))
        {
            hitscan["tracer"] = t.look.clone();
        }
        let mut damage = number(name, &Some(h.damage.clone()))?.unwrap_or(0.0);
        if let Some(limit) = h.damage_limit {
            damage = damage.clamp(-limit, limit);
        }
        let damage_type = blocks.field(name, &h.damage_type).unwrap_or_default();
        // The look and blast of a hit: the named projectile's, or the
        // image's own.
        let base = match h
            .hit_projectile
            .as_ref()
            .and_then(|f| blocks.field(name, f))
            .filter(|v| !v.trim().is_empty())
        {
            Some(n) => id_of(weapons, "ProjectileData", n)
                .with_context(|| format!("{name}: its hit shows {n}, which did not import"))?,
            None => image["projectile"]
                .as_str()
                .with_context(|| format!("{name}: no projectile for its hits"))?
                .to_owned(),
        };
        let mut ray = weapons["projectiles"][&base].clone();
        ensure!(ray.is_object(), "{name}: {base} did not import");
        let namespace = id.split(':').next().unwrap_or_default();
        let ray_name = format!("{name}Ray");
        let ray_id = format!("{namespace}:projectile/{}", ray_name.to_ascii_lowercase());
        ray["id"] = json!(ray_id);
        ray["name"] = json!(ray_name);
        ray["damage"] = json!(damage);
        ray["damage_type"] = json!(damage_type);
        ray["impulse"] = json!(number(name, &h.impulse)?.unwrap_or(0.0));
        ray["vertical"] = json!(number(name, &h.vertical)?.unwrap_or(0.0));
        ray["collide_players"] = json!(true);
        // A ray hurts players and vehicles; it leaves bricks be.
        ray["brick"]["direct"] = json!(false);
        let count = number(name, &h.count)?.map_or(1, |c| c.max(1.0) as u32);
        let mut spread = number(name, &h.spread)?.unwrap_or(0.0).max(0.0);
        if h.spread_degrees {
            // Degrees across, each axis turned by up to half: as v20's
            // `%spread`, whose turn is up to 5π·spread.
            spread = (spread / 2.0).to_radians() / (5.0 * std::f32::consts::PI);
        }
        reading.patch["images"][id] = json!({
            "projectile": ray_id,
            "shot": { "projectiles": count, "spread": spread, "hitscan": hitscan },
        });
        reading.patch["projectiles"][&ray_id] = ray;
        let source = weapons["definitions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| {
                d["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .map(|d| d["source"].clone())
            .unwrap_or(Value::Null);
        // Its fields are the image's, so a table of projectile fields
        // (a headshot multiplier) reads them.
        reading.definitions.push(json!({
            "name": ray_name,
            "class": "ProjectileData",
            "parent": name,
            "source": source,
            "fields": {},
        }));
    }
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_blocks_split_into_shots_by_recoil() {
        let body = r#"
            %projectile = %this.projectile; %spread = 0.004; %shellcount = 15;
            %obj.setVelocity(VectorAdd(%obj.getVelocity(),VectorScale(%aimVec,"-2")));
            for(...) { %p = new Projectile() { scale = "1.5 1.5 1.5"; }; }
            %projectile = slugProjectile;
            %spread = 0.0005;
            %shellcount = 1;
            // %obj.setVelocity(VectorAdd(%obj.getVelocity(),VectorScale(%aimVec,"-9")));
            for(...) {}"#;
        assert_eq!(
            blocks(body),
            [
                Block {
                    projectile: None,
                    spread: 0.004,
                    count: 15,
                    recoil: Some(2.0),
                    scale: 1.5,
                },
                Block {
                    projectile: Some("slugProjectile".into()),
                    spread: 0.0005,
                    count: 1,
                    recoil: None,
                    scale: 1.0,
                },
            ]
        );
    }

    #[test]
    fn spread_degrees_become_v20_units() {
        // 10 degrees across: each axis up to 5 degrees, 5π·spread radians.
        let spread = (10.0f32 / 2.0).to_radians() / (5.0 * std::f32::consts::PI);
        assert!((5.0 * std::f32::consts::PI * spread - 5f32.to_radians()).abs() < 1e-6);
    }
}
