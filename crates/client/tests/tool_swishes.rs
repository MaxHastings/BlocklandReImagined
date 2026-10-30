//! Actual converted tools -> authoritative cues -> two silent/offline listeners.
//! No window, device, gameplay input automation or GPU is involved.
use anyhow::{Context, Result};
use bri_audio::{Listener, OutputKind, Placement};
use bri_client::audio::ClientAudio;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    presentation::{Cue, CueKind},
    session::{Command, Session},
    simulation::Simulation,
};
use bri_ui::api::Settings;
use glam::Vec3;
use rapier3d::prelude::*;
use std::path::PathBuf;

const SWING: &str = "bri:foley/tools/swing";

fn sounds(cues: &[Cue], profile: &str) -> usize {
    cues.iter()
        .filter(|c| matches!(&c.kind, CueKind::WeaponSound { profile: p } if p == profile))
        .count()
}

fn advance(s: &mut Session, owner: u64, pitch: f32, ticks: usize) -> Result<()> {
    for _ in 0..ticks {
        s.movement(
            owner,
            1_000_000 + s.simulation().state().tick,
            MoveInput {
                pitch,
                ..Default::default()
            },
        )?;
        s.step()?;
    }
    Ok(())
}

fn ready(s: &mut Session, owner: u64, pitch: f32) -> Result<()> {
    for _ in 0..240 {
        if s.weapon_view()
            .images
            .get(&owner)
            .is_some_and(|images| images.iter().all(|image| image.state == "Ready"))
        {
            return Ok(());
        }
        advance(s, owner, pitch, 1)?;
    }
    anyhow::bail!("Tool never became ready")
}

fn energy(pcm: &[f32]) -> f32 {
    assert!(!pcm.is_empty());
    assert!(pcm.iter().all(|v| v.is_finite() && v.abs() < 1.0));
    pcm.iter().map(|v| v * v).sum::<f32>() / pcm.len() as f32
}

#[test]
#[ignore = "requires regenerated native weapons/audio packs with the approved tool augmentation"]
fn actual_tools_sound_on_misses_keep_contacts_and_mix_for_both_observers() -> Result<()> {
    let root = std::env::var_os("BRI_CONTENT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    let weapons_root = std::env::var_os("BRI_TOOL_SWISH_WEAPONS")
        .map(PathBuf::from)
        .unwrap_or_else(|| bri_package::testing::pack_dir(&root, "weapons"));
    let pack = bri_weapons::Pack::from_json(&std::fs::read(weapons_root.join("weapons.json"))?)?;
    assert_eq!(pack.sounds[SWING].variants.len(), 3);
    let mut session = Session::new(Simulation::new(
        bri_world::World::new("Tool audio".into(), "test".into(), vec![[1.; 4]]),
        Definitions::default(),
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vec3::new(0., -0.5, 0.))],
    )?);
    session.set_weapon_pack(pack.clone())?;
    let owner = session.join("Builder".into(), Vec3::new(0., 0.05, 0.), false)?;
    advance(&mut session, owner, 0.8, 240)?;
    session.take_cues();
    let mut sequence = 1;
    let mut miss_cues = Vec::new();
    for (slot, contact, repeats_while_held) in
        [(0, "hammerHitSound", true), (1, "wrenchMissSound", false)]
    {
        session.equip_tool(owner, Some(slot))?;
        ready(&mut session, owner, 0.8)?;
        assert_eq!(
            sounds(&session.take_cues(), SWING),
            0,
            "equip sounded a swing"
        );
        session.command(owner, sequence, Command::WeaponTrigger { down: true })?;
        sequence += 1;
        advance(&mut session, owner, 0.8, 8)?;
        session.release_trigger(owner)?;
        ready(&mut session, owner, 0.8)?;
        let cues = session.take_cues();
        assert_eq!(sounds(&cues, SWING), 1, "one click into empty air");
        assert_eq!(sounds(&cues, contact), 0, "a miss invented a contact");
        miss_cues.extend(
            cues.into_iter().filter(
                |c| matches!(&c.kind, CueKind::WeaponSound { profile } if profile == SWING),
            ),
        );
        // Refresh the actual look before a press, then hit the fixed ground.
        advance(&mut session, owner, -1.2, 1)?;
        session.command(owner, sequence, Command::WeaponTrigger { down: true })?;
        sequence += 1;
        advance(&mut session, owner, -1.2, 8)?;
        session.release_trigger(owner)?;
        ready(&mut session, owner, -1.2)?;
        let cues = session.take_cues();
        assert_eq!(sounds(&cues, SWING), 1, "hit duplicated the swing");
        assert_eq!(
            sounds(&cues, contact),
            1,
            "original contact was lost/duplicated"
        );
        advance(&mut session, owner, 0.8, 1)?;
        session.command(owner, sequence, Command::WeaponTrigger { down: true })?;
        sequence += 1;
        advance(&mut session, owner, 0.8, 240)?;
        session.release_trigger(owner)?;
        ready(&mut session, owner, 0.8)?;
        let held = session.take_cues();
        let ticks: Vec<_> = held
            .iter()
            .filter_map(|c| {
                matches!(&c.kind, CueKind::WeaponSound { profile } if profile == SWING)
                    .then_some(c.tick)
            })
            .collect();
        // Native hammer CheckFire loops down into Fire; native wrench has no
        // down transition and waits for release. Audio must preserve both.
        if repeats_while_held {
            assert!(ticks.len() >= 3, "held hammer use didn't repeat: {ticks:?}");
        } else {
            assert_eq!(ticks.len(), 1, "held wrench invented repeated swings: {ticks:?}");
        }
        assert!(
            ticks.windows(2).all(|w| w[1] > w[0] + 1),
            "duplicate state cues"
        );
        advance(&mut session, owner, 0.8, 120)?;
        assert_eq!(
            sounds(&session.take_cues(), SWING),
            0,
            "release kept sounding"
        );
    }
    assert!(
        session
            .command(u64::MAX, 1, Command::WeaponTrigger { down: true })
            .is_err()
    );
    assert_eq!(sounds(&session.take_cues(), SWING), 0, "denied use sounded");

    let audio_root = bri_package::testing::pack_dir(&root, "audio");
    let mut local = ClientAudio::load(&audio_root, &mut Settings::default(), OutputKind::Offline)?;
    let mut observer =
        ClientAudio::load(&audio_root, &mut Settings::default(), OutputKind::Offline)?;
    for audio in [&mut local, &mut observer] {
        audio.set_pack_sounds(&pack, &weapons_root);
        for cue in &miss_cues {
            audio.cue(cue);
        }
        audio.tick(0.3, Listener::default());
        assert_eq!(audio.requested.get(SWING), Some(&2));
        assert_eq!(audio.stats().started, 2, "not one voice per accepted cue");
        assert!(
            energy(&audio.take_capture()) > 1e-9,
            "actual recordings mixed silence"
        );
        assert!(audio.warnings.is_empty(), "{:?}", audio.warnings);
        // Each approved variant decodes and produces a finite signal.
        for variant in &pack.sounds[SWING].variants {
            audio.profile(&variant.profile, Placement::Listener);
            audio.tick(0.3, Listener::default());
            assert!(
                energy(&audio.take_capture()) > 1e-9,
                "{} silent",
                variant.profile
            );
        }
        audio.clear();
        audio.tick(0.3, Listener::default());
        audio.take_capture();
        audio.tick(0.3, Listener::default());
        assert_eq!(energy(&audio.take_capture()), 0.0, "teardown left a voice");
    }
    let leaf = &pack.sounds[SWING]
        .variants
        .first()
        .context("swing choices")?
        .profile;
    local.profile(leaf, Placement::World([0., 0., -2.]));
    local.tick(0.3, Listener::default());
    let near = energy(&local.take_capture());
    local.profile(leaf, Placement::World([0., 0., -45.]));
    local.tick(0.3, Listener::default());
    assert!(
        energy(&local.take_capture()) < near * 0.5,
        "distance didn't reduce the swing"
    );
    local.set_volume("effects", 0.)?;
    local.tick(0.02, Listener::default());
    local.take_capture();
    local.profile(leaf, Placement::Listener);
    local.tick(0.3, Listener::default());
    assert_eq!(
        energy(&local.take_capture()),
        0.0,
        "effects mute didn't gate swings"
    );
    Ok(())
}
