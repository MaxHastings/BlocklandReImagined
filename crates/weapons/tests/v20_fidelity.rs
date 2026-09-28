//! v20 fidelity harness for weapons: fire every stock item at a target and
//! compare the cues the runtime emits with the original datablocks' literal
//! fields (kept in the pack's `definitions`), not with the lowered states.
//!
//! For each image state the item passes through, its `stateSound`,
//! `stateEmitter`, `stateSequence` and `stateEjectShell` must be emitted;
//! each projectile fired must be the image's `projectile`; a hit must deal
//! its `directDamage` and push with `impactImpulse`, and its explosion must
//! play and sound (`soundProfile`) and deal `radiusDamage`. The record is
//! written to `$CARGO_TARGET_TMPDIR/v20-fidelity-weapons.json` for
//! docs/audits/v20-fidelity.md.
use bri_weapons::*;
use glam::Vec3;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

struct Target;
impl Query for Target {
    fn sweep(&mut self, start: Vec3, end: Vec3, filter: Filter) -> Option<Hit> {
        // A player standing 4 units in front of the shooter.
        if !filter.players || filter.world_only {
            return None;
        }
        let at = Vec3::new(0.0, 0.0, -4.0);
        let segment = end - start;
        if segment.length_squared() < 1e-4 {
            return None;
        }
        let t = (at - start).dot(segment) / segment.length_squared();
        let closest = start + segment * t.clamp(0.0, 1.0);
        ((0.0..=1.0).contains(&t) && closest.distance(at) < 1.5).then(|| Hit {
            target: TargetId::Actor(ActorId(2)),
            position: closest,
            normal: Vec3::Z,
            fraction: t,
            color: None,
        })
    }
    fn radius(&mut self, center: Vec3, radius: f32, _: usize) -> Vec<Nearby> {
        let at = Vec3::new(0.0, 0.0, -4.0);
        let distance = center.distance(at);
        if distance <= radius {
            vec![Nearby {
                target: TargetId::Actor(ActorId(2)),
                center: at,
                distance,
            }]
        } else {
            vec![]
        }
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        true
    }
}

/// Literal datablock fields with inheritance.
struct Raw<'a>(BTreeMap<String, &'a Definition>);
impl Raw<'_> {
    fn get(&self, name: &str, key: &str) -> Option<String> {
        let mut at = self.0.get(&name.to_ascii_lowercase()).copied();
        for _ in 0..16 {
            let d = at?;
            if let Some(v) = d.fields.get(&key.to_ascii_lowercase()) {
                let v = v.trim().trim_matches('"').trim();
                return (!v.is_empty()).then(|| v.to_owned());
            }
            at = d
                .parent
                .as_ref()
                .and_then(|p| self.0.get(&p.to_ascii_lowercase()).copied());
        }
        None
    }
    fn num(&self, name: &str, key: &str) -> f32 {
        self.get(name, key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    }
    fn state(&self, image: &str, state: &str) -> Option<usize> {
        (0..64).find(|i| {
            self.get(image, &format!("stateName[{i}]"))
                .is_some_and(|n| n.eq_ignore_ascii_case(state))
        })
    }
}

fn load() -> Pack {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-009/weapons.json");
    Pack::from_json(&std::fs::read(path).expect("Run documented importer first")).unwrap()
}

/// Projectiles whose script replaces `ProjectileData::damage`, so the hit
/// deals no `directDamage`: `HorseRayProjectile::Damage` turns the target
/// into a horse instead (Weapon_HorseRay.cs).
const SCRIPTED_DAMAGE: [&str; 1] = ["horserayprojectile"];

/// `color<N>Paint*` spray effects are the palette copies of `bluePaint*`.
fn same_effect(expected: &str, emitted: &str) -> bool {
    let e = expected.to_ascii_lowercase();
    let got = emitted.to_ascii_lowercase();
    match e.strip_prefix("blue") {
        Some(rest) if rest.starts_with("paint") => got.ends_with(rest),
        _ => got == e,
    }
}

#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn every_stock_item_emits_its_v20_datablock_cues() {
    let pack = load();
    let raw = Raw(pack
        .definitions
        .iter()
        .map(|d| (d.name.to_ascii_lowercase(), d))
        .collect());
    let mut record = BTreeMap::new();
    let mut gaps = Vec::new();
    for item in pack.items.values() {
        let image = &pack.images[&item.image];
        let mut w = WeaponsWorld::new(pack.clone()).unwrap();
        w.add_actor(ActorId(1), 5).unwrap();
        let slot = w.give(ActorId(1), &item.id).unwrap();
        w.equip(ActorId(1), Some(slot)).unwrap();
        let mut events = Vec::new();
        let mut q = Target;
        // Equip, hold, release, repeat: covers semi-auto, charge-and-release
        // (spear) and fire-on-release (akimbo) images.
        for (down, ticks) in [(None, 60), (Some(true), 180), (Some(false), 240), (Some(true), 180), (Some(false), 360)] {
            if let Some(down) = down {
                let _ = w.trigger(ActorId(1), down);
            }
            for _ in 0..ticks {
                events.extend(w.step(&mut q));
            }
        }
        let mut states = BTreeSet::new();
        let mut sounds = BTreeSet::new();
        let mut effects = BTreeSet::new();
        let mut sequences = BTreeSet::new();
        let mut spawned = BTreeSet::new();
        let mut damage = Vec::new();
        let mut impulses = 0;
        let mut shells = 0;
        for e in &events {
            match e {
                Event::ImageState { image: i, state, .. } if *i == image.id => {
                    states.insert(state.clone());
                }
                Event::Sound { profile, .. } => {
                    sounds.insert(profile.to_ascii_lowercase());
                }
                Event::Effect { definition, .. } => {
                    effects.insert(definition.clone());
                }
                Event::Animation { sequence, .. } => {
                    sequences.insert(sequence.to_ascii_lowercase());
                }
                Event::Spawned { definition, .. } => {
                    spawned.insert(definition.clone());
                }
                Event::Damage { amount, kind, .. } => damage.push((kind.clone(), *amount)),
                Event::Impulse { .. } => impulses += 1,
                Event::Shell { .. } => shells += 1,
                _ => {}
            }
        }
        let mut gap = |what: String| gaps.push(format!("{}: {what}", item.name));
        for state in &states {
            let Some(i) = raw.state(&image.name, state) else {
                gap(format!("state {state} is not in the datablock"));
                continue;
            };
            if let Some(sound) = raw.get(&image.name, &format!("stateSound[{i}]"))
                && !sounds.contains(&sound.to_ascii_lowercase())
            {
                gap(format!("state {state} sound {sound} not played"));
            }
            if let Some(emitter) = raw.get(&image.name, &format!("stateEmitter[{i}]"))
                && !effects.iter().any(|e| same_effect(&emitter, e))
            {
                gap(format!("state {state} emitter {emitter} not emitted"));
            }
            if let Some(sequence) = raw.get(&image.name, &format!("stateSequence[{i}]"))
                && !sequences.contains(&sequence.to_ascii_lowercase())
            {
                gap(format!("state {state} sequence {sequence} not played"));
            }
            if raw
                .get(&image.name, &format!("stateEjectShell[{i}]"))
                .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true"))
                && raw.get(&image.name, "casing").is_some()
                && shells == 0
            {
                gap(format!("state {state} shell not ejected"));
            }
        }
        let projectile = raw.get(&image.name, "projectile");
        for fired in &spawned {
            if projectile
                .as_ref()
                .is_some_and(|p| native_id("projectile", p) != *fired)
            {
                gap(format!("fired {fired}, datablock says {projectile:?}"));
            }
        }
        for fired in &spawned {
            let Some(p) = pack.projectiles.get(fired) else {
                continue;
            };
            let name = &p.name;
            let direct = raw.num(name, "directDamage");
            if direct > 0.0
                && !SCRIPTED_DAMAGE.contains(&name.to_ascii_lowercase().as_str())
                && !damage.iter().any(|(_, amount)| (amount - direct).abs() < 0.01)
            {
                gap(format!("{name} hit did not deal directDamage {direct}: {damage:?}"));
            }
            if raw.num(name, "impactImpulse") > 0.0 && impulses == 0 {
                gap(format!("{name} hit did not push (impactImpulse)"));
            }
            if let Some(explosion) = raw.get(name, "explosion") {
                if !effects.iter().any(|e| same_effect(&explosion, e)) {
                    gap(format!("{name} explosion {explosion} not shown"));
                }
                if let Some(sound) = raw.get(&explosion, "soundProfile")
                    && !sounds.contains(&sound.to_ascii_lowercase())
                {
                    gap(format!("{explosion} sound {sound} not played"));
                }
                if raw.num(&explosion, "radiusDamage") > 0.0
                    && raw.num(&explosion, "damageRadius") > 0.0
                    && !damage.iter().any(|(kind, _)| {
                        raw.get(name, "radiusDamageType")
                            .is_some_and(|t| t.to_ascii_lowercase().ends_with(&kind.to_ascii_lowercase()))
                    })
                {
                    gap(format!("{explosion} radiusDamage not dealt: {damage:?}"));
                }
            }
        }
        record.insert(
            item.name.clone(),
            json!({
                "image": image.name,
                "states": states,
                "sounds": sounds,
                "effects": effects,
                "sequences": sequences,
                "projectiles": spawned,
                "damage": damage,
                "impulses": impulses,
                "shells": shells,
            }),
        );
    }
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("v20-fidelity-weapons.json");
    std::fs::write(
        &out,
        serde_json::to_vec_pretty(&json!({"items": record, "gaps": gaps})).unwrap(),
    )
    .unwrap();
    assert!(gaps.is_empty(), "{} gaps (record at {}):\n{}", gaps.len(), out.display(), gaps.join("\n"));
}
