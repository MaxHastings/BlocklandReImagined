//! v20 fidelity harness, client half: every sound and effect a stock weapon,
//! projectile, explosion or vehicle can cue must resolve to something the
//! client plays or draws. The server half (crates/weapons/tests/v20_fidelity.rs)
//! proves the cues are emitted; a cue the client cannot resolve is the
//! "missing sound or particle" a player notices.
use anyhow::Result;
use bri_audio::{BankOptions, SoundBank};
use bri_client::weapon_effects::WeaponEffects;
use std::{path::Path, sync::Arc};

fn content(dir: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content").join(dir)
}

#[test]
#[ignore = "requires the converted weapons, effects, audio and vehicle packs"]
fn every_stock_cue_resolves_to_a_sound_or_effect() -> Result<()> {
    let weapons = Arc::new(bri_weapons::Pack::from_json(&std::fs::read(
        content("weapons-pack-009").join("weapons.json"),
    )?)?);
    let effects = bri_fx_runtime::EffectsPack::load(content("effects-runtime-pack-004"))?;
    let mut fx = WeaponEffects::new(effects, weapons.clone(), Default::default())?;
    fx.set_palette(&[[1.0, 0.0, 0.0, 1.0]]);
    let bank = SoundBank::load(content("audio-pack-001"), &BankOptions::default())?;
    let mut gaps = Vec::new();
    let mut sound = |owner: &str, name: &str| {
        if !name.is_empty() && bank.resolve(name).is_err() {
            gaps.push(format!("{owner}: sound {name} does not resolve"));
        }
    };
    for image in weapons.images.values() {
        for state in &image.states {
            sound(&format!("{} {}", image.name, state.name), &state.sound);
        }
    }
    for p in weapons.projectiles.values() {
        sound(&p.name, &p.sound);
    }
    for e in weapons.explosions.values() {
        sound(&e.name, &e.sound);
    }
    let vehicles = bri_vehicles::Pack::load(content("vehicles-pack-011").join("vehicles.json"))?;
    for d in &vehicles.definitions {
        if let Some(w) = &d.weapon {
            sound(&d.id, &w.sound);
        }
    }
    // Keys the client's cue audio maps to (crates/client/src/audio.rs).
    for key in [
        "player.jump",
        "brick.plant",
        "brick.break",
        "tool.hammer.hit",
        "tool.wrench.hit",
        "player.pain_cry",
        "player.death_cry",
        "player.water.impact_hard",
        "player.water.impact_medium",
        "player.water.impact_easy",
        "player.water.exit",
        "player.mount",
    ] {
        if bank.trigger(key).is_none() {
            gaps.push(format!("audio trigger {key} is unbound"));
        }
    }
    let mut effect = |owner: &str, name: &str| {
        if !name.is_empty() && !fx.resolves(name) {
            gaps.push(format!("{owner}: effect {name} does not resolve"));
        }
    };
    for image in weapons.images.values() {
        for state in &image.states {
            // Spray cans draw their palette copies (`color<N>Paint*`).
            let emitter = state.emitter.replacen("bluePaint", "color0Paint", 1);
            effect(&format!("{} {}", image.name, state.name), &emitter);
        }
    }
    for p in weapons.projectiles.values() {
        let paint = |e: &str| e.replacen("bluePaint", "color0Paint", 1);
        effect(&p.name, &paint(&p.trail));
        effect(&p.name, &paint(&p.explosion.effect));
        effect(&p.name, &p.bounce_effect);
        effect(&p.name, &p.stick_effect);
        effect(&p.name, &p.blood_effect);
    }
    for (explosion, debris) in bri_weapons::debris::explosion_debris(&weapons) {
        for trail in &debris.emitters {
            effect(&format!("{explosion} debris"), trail);
        }
    }
    for d in &vehicles.definitions {
        if let Some(w) = &d.weapon
            && !weapons
                .images
                .contains_key(&bri_weapons::native_id("image", &w.effect))
        {
            gaps.push(format!("{}: firing image {} is not in the weapons pack", d.id, w.effect));
        }
    }
    assert!(gaps.is_empty(), "{} gaps:\n{}", gaps.len(), gaps.join("\n"));
    Ok(())
}
