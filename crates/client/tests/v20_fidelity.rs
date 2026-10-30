//! v20 fidelity harness, client half: every sound and effect a stock weapon,
//! projectile, explosion or vehicle can cue must resolve to something the
//! client plays or draws. The server half (crates/weapons/tests/v20_fidelity.rs)
//! proves the cues are emitted; a cue the client cannot resolve is the
//! "missing sound or particle" a player notices.
use anyhow::Result;
use bri_audio::{BankOptions, OutputKind, SoundBank};
use bri_client::{audio::ClientAudio, weapon_effects::WeaponEffects};
use std::{path::Path, sync::Arc};

fn content(role: &str) -> std::path::PathBuf {
    bri_package::testing::pack_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        role,
    )
}

#[test]
#[ignore = "requires the converted weapons, effects, audio and vehicle packs"]
fn every_stock_cue_resolves_to_a_sound_or_effect() -> Result<()> {
    let weapons = Arc::new(bri_weapons::Pack::from_json(&std::fs::read(
        content("weapons").join("weapons.json"),
    )?)?);
    let effects = bri_fx_runtime::EffectsPack::load(content("effects_runtime"))?;
    let mut fx = WeaponEffects::new(effects, weapons.clone(), Default::default())?;
    fx.set_palette(&[[1.0, 0.0, 0.0, 1.0]]);
    let bank = SoundBank::load(content("audio"), &BankOptions::default())?;
    // Native package sounds (including weighted tool swings) are decoded by
    // the same adapter as gameplay, then fall back to the legacy sound bank.
    let mut audio = ClientAudio::load(
        &content("audio"),
        &mut bri_ui::api::Settings::default(),
        OutputKind::Offline,
    )?;
    audio.set_pack_sounds(&weapons, &content("weapons"));
    let mut gaps = Vec::new();
    let mut sound = |owner: &str, name: &str| {
        if !name.is_empty() && !audio.has_sound(name) {
            gaps.push(format!("{owner}: sound {name} does not resolve"));
        }
    };
    // A playable first choice must not conceal a missing alternative.
    for (profile, definition) in &weapons.sounds {
        for variant in &definition.variants {
            sound(profile, &variant.profile);
        }
    }
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
    let vehicles = bri_vehicles::Pack::load(content("vehicles").join("vehicles.json"))?;
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
            gaps.push(format!(
                "{}: firing image {} is not in the weapons pack",
                d.id, w.effect
            ));
        }
    }
    assert!(gaps.is_empty(), "{} gaps:\n{}", gaps.len(), gaps.join("\n"));
    Ok(())
}

#[test]
#[ignore = "requires the converted audio pack"]
fn cue_sounds_keep_their_v20_descriptions() -> Result<()> {
    // Profile, then its AudioDescription: 3D reference and max distance
    // (None is 2D) and volume. The brick sounds are client profiles on
    // AudioClientClose3d (allClientScripts.cs:143).
    let bank = SoundBank::load(content("audio"), &BankOptions::default())?;
    for (profile, spatial, gain) in [
        ("BrickBreak", Some((10.0, 60.0)), 1.0),
        ("BrickPlant", Some((10.0, 60.0)), 1.0),
        ("rocketExplodeSound", Some((20.0, 100.0)), 1.0),
        ("vehicleExplosionSound", Some((20.0, 100.0)), 1.0),
        ("gunShot1Sound", Some((10.0, 60.0)), 1.0),
        ("bulletHitSound", Some((10.0, 60.0)), 1.0),
        ("JumpSound", Some((5.0, 30.0)), 1.0),
        ("weaponSwitchSound", Some((5.0, 30.0)), 1.0),
    ] {
        let playback = &bank.resolve(profile)?.playback;
        let got = playback
            .spatial
            .as_ref()
            .map(|s| (s.reference_distance, s.max_distance));
        assert_eq!((got, playback.gain), (spatial, gain), "{profile}");
    }
    Ok(())
}
