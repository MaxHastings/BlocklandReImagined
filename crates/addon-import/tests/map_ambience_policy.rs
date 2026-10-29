//! Actual package rule consumers with synthetic media, never a gameplay driver.
use bri_sim::{
    definitions::Definitions,
    presentation::CueKind,
    session::{Notice, Session},
    simulation::Simulation,
};
use glam::Vec3;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

struct Fixture {
    _dir: tempfile::TempDir,
    catalog: Arc<bri_package_runtime::Catalog>,
    pack: bri_weapons::Pack,
    maps: Vec<Value>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("map_ambience");
        let rules = dir.path().join("map_ambience_rules");
        std::fs::create_dir_all(out.join("assets")).unwrap();
        std::fs::create_dir_all(&rules).unwrap();
        let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/map_ambience");
        std::fs::copy(here.join("package.json"), out.join("package.json")).unwrap();
        std::fs::copy(here.join("rules-package.json"), rules.join("package.json")).unwrap();
        for file in ["behaviour.json", "ambience.rhai"] {
            std::fs::copy(here.join(file), rules.join(file)).unwrap();
        }
        let palette: Value =
            serde_json::from_slice(&std::fs::read(here.join("palette.json")).unwrap()).unwrap();
        let maps = palette["maps"].as_array().unwrap().clone();
        let mut script = String::from("fn palette() {\n");
        let mut sounds = serde_json::Map::new();
        for m in &maps {
            script.push_str(&format!(
                "if map_id() == {} {{ return #{{beds:{},details:{},anchors:{}}}; }}\n",
                m["map"], m["beds"], m["details"], m["anchors"]
            ));
            for (kind, looping) in [("beds", true), ("details", false)] {
                for key in m[kind].as_array().unwrap() {
                    let key = key.as_str().unwrap();
                    let mut sound = json!({"file":"tone.wav","looping":looping,"music":looping});
                    if key != "room" {
                        sound["environment"] = json!("exterior");
                    }
                    if !looping {
                        sound["spatial"] = json!({"reference_distance":12.0,"max_distance":160.0});
                    }
                    sounds.insert(format!("map_ambience:{key}"), sound);
                }
            }
        }
        script.push_str("#{beds:[],details:[],anchors:[]}\n}\n");
        std::fs::write(rules.join("palette.rhai"), script).unwrap();
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend(164u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(8000u32.to_le_bytes());
        wav.extend(16000u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(128u32.to_le_bytes());
        for i in 0..64 {
            wav.extend((if i % 2 == 0 { 8192i16 } else { -8192i16 }).to_le_bytes());
        }
        std::fs::write(out.join("assets/tone.wav"), wav).unwrap();
        let data = serde_json::to_vec(
            &json!({"schema_version":bri_weapons::SCHEMA,"id":"map_ambience","sounds":sounds}),
        )
        .unwrap();
        std::fs::write(out.join("assets/weapons.json"), &data).unwrap();
        let check = bri_package_runtime::check::check(&rules);
        assert!(check.ok, "{check}");
        let set = serde_json::from_value(json!({"schema_version":1,"packages":[{
            "id":"map_ambience","version":"1.0.0","side":"shared","dir":"map_ambience"
        },{"id":"map_ambience_rules","version":"1.0.0","side":"server","dir":"map_ambience_rules"}]}))
        .unwrap();
        let environment = bri_package::environment::Environment::load(dir.path(), &set).unwrap();
        assert_eq!(
            environment
                .client_packages()
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            vec!["map_ambience"],
            "join payload includes shared audio only, never server policy"
        );
        let catalog = Arc::new(bri_package_runtime::Catalog::load(dir.path(), &set, true).unwrap());
        Self {
            _dir: dir,
            catalog,
            pack: bri_weapons::Pack::from_json(&data).unwrap(),
            maps,
        }
    }

    fn session(&self, map: &str) -> Session {
        let floor = rapier3d::prelude::ColliderBuilder::cuboid(2000., 1., 2000.)
            .translation(Vec3::new(0., 80., 0.));
        let sim = Simulation::new(
            bri_world::World::new("Ambience fixture".into(), map.into(), vec![[1.; 4]]),
            Definitions::default(),
            vec![floor],
        )
        .unwrap();
        let mut s = Session::new(sim);
        s.set_map_id(map);
        s.set_weapon_pack(self.pack.clone()).unwrap();
        s.install_packages(self.catalog.clone(), None).unwrap();
        s
    }
}

fn beds(s: &mut Session, owner: u64) -> Vec<bri_sim::session::AmbientPlay> {
    s.take_private_notices()
        .into_iter()
        .filter_map(|(who, n)| match n {
            Notice::Ambient(play) if who == owner => Some(play),
            _ => None,
        })
        .collect()
}

#[test]
fn fixed_inventory_beds_join_late_join_and_unknown_map_have_actual_consumers() {
    let f = Fixture::new();
    assert_eq!(f.maps.len(), 21);
    for m in &f.maps {
        let mut s = f.session(m["map"].as_str().unwrap());
        let a = s
            .join("First".into(), Vec3::new(0., 84., 0.), true)
            .unwrap();
        for _ in 0..121 {
            s.step().unwrap();
        }
        let first = beds(&mut s, a);
        let expected = m["beds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| format!("map_ambience:{}", v.as_str().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            first
                .iter()
                .filter_map(|b| b.track.clone())
                .collect::<Vec<_>>(),
            expected,
            "{:?}",
            s.package_diagnostics()
        );
        for _ in 0..240 {
            s.step().unwrap();
        }
        assert!(
            beds(&mut s, a).is_empty(),
            "routine ticks must not restart beds"
        );
        let b = s
            .join("Later".into(), Vec3::new(2., 84., 0.), false)
            .unwrap();
        assert_eq!(
            beds(&mut s, b),
            first,
            "late join must get each retained bed once"
        );
        assert!(
            s.package_diagnostics().is_empty(),
            "{:?}",
            s.package_diagnostics()
        );
    }
    let mut unknown = f.session("unrelated:map/not-in-palette");
    let a = unknown
        .join("Unknown".into(), Vec3::new(0., 84., 0.), true)
        .unwrap();
    for _ in 0..240 {
        unknown.step().unwrap();
    }
    assert!(beds(&mut unknown, a).is_empty());
}

#[test]
fn sparse_details_use_evidenced_sources_quiet_gaps_and_no_duplicate_player_broadcast() {
    let f = Fixture::new();
    let forest = f.maps.iter().find(|m| m["name"] == "Forest").unwrap();
    let anchors: Vec<[f32; 3]> = serde_json::from_value(forest["anchors"].clone()).unwrap();
    let mut s = f.session(forest["map"].as_str().unwrap());
    for (i, at) in anchors.iter().take(2).enumerate() {
        s.join(
            format!("Listener{i}"),
            Vec3::from(*at) + Vec3::Y * 2.,
            i == 0,
        )
        .unwrap();
    }
    let mut events = Vec::new();
    for tick in 1..=32_000 {
        s.step().unwrap();
        for cue in s.take_cues() {
            let profile = match cue.kind {
                CueKind::WeaponSound { profile } | CueKind::SoundShaped { profile, .. } => profile,
                _ => continue,
            };
            if profile.starts_with("map_ambience:bird-") {
                assert!(
                    anchors.contains(&cue.position),
                    "detail invented a source position"
                );
                events.push((tick, profile, cue.position));
            }
        }
    }
    assert!(
        events.len() >= 3,
        "{events:?}; {:?}",
        s.package_diagnostics()
    );
    assert!(events[0].0 >= 4200, "initial quiet gap missing");
    for pair in events.windows(2) {
        assert!(
            (4200..=10320).contains(&(pair[1].0 - pair[0].0)),
            "quiet gap or duplicate broadcast: {pair:?}"
        );
        assert_ne!(pair[0].1, pair[1].1, "immediate recording repeat");
        assert_ne!(pair[0].2, pair[1].2, "immediate source repeat");
    }
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
    let mut empty = f.session(forest["map"].as_str().unwrap());
    for _ in 0..11_000 {
        empty.step().unwrap();
    }
    assert!(!empty.take_cues().iter().any(|cue|matches!(&cue.kind,
        CueKind::WeaponSound{profile}|CueKind::SoundShaped{profile,..} if profile.starts_with("map_ambience:bird-"))),"no listeners must not emit detail events");
}
