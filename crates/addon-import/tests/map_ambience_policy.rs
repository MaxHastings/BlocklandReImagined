//! Actual package rule consumers with synthetic media, never a gameplay driver.
use bri_sim::{
    definitions::Definitions,
    presentation::CueKind,
    session::{Notice, Session},
    simulation::Simulation,
};
use glam::Vec3;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};

struct Fixture {
    _dir: tempfile::TempDir,
    catalog: Arc<bri_package_runtime::Catalog>,
    pack: bri_weapons::Pack,
    maps: Vec<Value>,
    originals: Vec<(String, String, [f32; 3])>,
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
                "if map_id() == {} {{ return #{{beds:{},details:{},anchors:{},locals:[{}]}}; }}\n",
                m["map"],
                m["beds"],
                m["details"],
                m["anchors"],
                m["locals"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| format!("#{{clip:{},at:{}}}", c["clip"], c["at"]))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            for (kind, looping) in [("beds", true), ("details", false)] {
                for key in m[kind].as_array().unwrap() {
                    let key = key.as_str().unwrap();
                    let mut sound = json!({"file":"tone.wav","looping":looping,"music":looping});
                    sound["environment"] = json!(if key == "indoor-base" {
                        "interior"
                    } else {
                        "exterior"
                    });
                    if !looping {
                        sound["spatial"] = json!({"reference_distance":12.0,"max_distance":160.0});
                    }
                    sounds.insert(format!("map_ambience:{key}"), sound);
                }
            }
            for local in m["locals"].as_array().unwrap() {
                sounds.insert(
                    format!("map_ambience:{}", local["clip"].as_str().unwrap()),
                    json!({
                        "file":"tone.wav","looping":true,"music":true,
                        "spatial":{"reference_distance":45.0,"max_distance":270.0}
                    }),
                );
            }
        }
        script.push_str("#{beds:[],details:[],anchors:[],locals:[]}\n}\n");
        std::fs::write(rules.join("palette.rhai"), script).unwrap();
        // Execute the actual authored overlay template against an available
        // full original palette, so exclusion is policy, not missing assets.
        let allowed: Value = serde_json::from_slice(
            &std::fs::read(here.join("original-emitter-policy.json")).unwrap(),
        )
        .unwrap();
        let overlay = dir.path().join("original_overlay");
        std::fs::create_dir_all(&overlay).unwrap();
        let mut original_script = String::from("fn map_beds() {\n");
        let mut originals = Vec::new();
        for (i, profile) in [
            "map_creek:map/map_creek/sound/waterfallloop",
            "map_creek:map/map_creek/sound/shoreloop",
            "map_creek:map/map_creek/sound/critterloop",
            "map_forest:map/forestnight/sound/crickets",
        ]
        .into_iter()
        .enumerate()
        {
            sounds.insert(
                profile.into(),
                json!({"file":"tone.wav","looping":true,"music":true}),
            );
            if !allowed["enabled_profiles"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str() == Some(profile))
            {
                continue;
            }
            let map = profile.split("/sound/").next().unwrap();
            originals.push((map.to_string(), profile.to_string(), [i as f32, 3., -4.]));
        }
        for map in ["map_creek:map/map_creek", "map_forest:map/forestnight"] {
            let entries = originals
                .iter()
                .filter(|(m, _, _)| m == map)
                .map(|(_, profile, at)| {
                    let digest = format!("{:x}", Sha256::digest(profile.as_bytes()));
                    format!(
                        "#{{key:{},track:{},at:{}}}",
                        json!(format!("map_audio/{}", &digest[..20])),
                        json!(profile),
                        json!(at)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            original_script.push_str(&format!(
                "if map_id() == {} {{return [{entries}];}}\n",
                json!(map)
            ));
        }
        original_script.push_str("[]\n}\n");
        original_script
            .push_str(&std::fs::read_to_string(here.join("original-overlay.rhai")).unwrap());
        std::fs::write(overlay.join("policy.rhai"), original_script).unwrap();
        std::fs::write(overlay.join("behaviour.json"),serde_json::to_vec(&json!({"schema_version":1,"script":"policy.rhai","tick_interval":120,"state":{"global":{"started":{"default":false,"persist":false}}}})).unwrap()).unwrap();
        std::fs::write(overlay.join("package.json"),serde_json::to_vec(&json!({
            "schema_version":1,"id":"original_overlay","version":"1.0.0","api":1,"name":"Original policy fixture","authors":["fixture"],"license":"CC0-1.0",
            "dependencies":{"map_ambience":"^1.0.0"},"capabilities":["effects"],
            "provides":[{"kind":"script","id":"original_overlay:script/policy","file":"policy.rhai"},{"kind":"behaviour","id":"original_overlay:behaviour/policy","file":"behaviour.json"}]
        })).unwrap()).unwrap();
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
        },{"id":"map_ambience_rules","version":"1.0.0","side":"server","dir":"map_ambience_rules"},
        {"id":"original_overlay","version":"1.0.0","side":"server","dir":"original_overlay"}]}))
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
            originals,
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
        assert!(
            first.iter().all(|bed| bed.key.len() <= 32),
            "ambient keys must fit the actual operation contract"
        );
        let mut expected = m["beds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| format!("map_ambience:{}", v.as_str().unwrap()))
            .collect::<Vec<_>>();
        expected.extend(
            m["locals"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| format!("map_ambience:{}", c["clip"].as_str().unwrap())),
        );
        expected.extend(
            f.originals
                .iter()
                .filter(|(map, _, _)| map == m["map"].as_str().unwrap())
                .map(|(_, profile, _)| profile.clone()),
        );
        expected.sort();
        let mut actual_tracks = first
            .iter()
            .filter_map(|bed| bed.track.clone())
            .collect::<Vec<_>>();
        actual_tracks.sort();
        assert_eq!(actual_tracks, expected, "{:?}", s.package_diagnostics());
        for (_, profile, at) in f
            .originals
            .iter()
            .filter(|(map, _, _)| map == m["map"].as_str().unwrap())
        {
            assert_eq!(
                first
                    .iter()
                    .find(|bed| bed.track.as_ref() == Some(profile))
                    .unwrap()
                    .at,
                Some(*at)
            );
        }
        for local in m["locals"].as_array().unwrap() {
            let track = format!("map_ambience:{}", local["clip"].as_str().unwrap());
            let emitted = first
                .iter()
                .find(|b| b.track.as_ref() == Some(&track))
                .unwrap();
            let at: [f32; 3] = serde_json::from_value(local["at"].clone()).unwrap();
            assert_eq!(
                emitted.at,
                Some(at),
                "fridge loop must retain authored world position"
            );
        }
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
        let mut replay = beds(&mut s, b);
        let mut retained = first.clone();
        replay.sort_by(|a, b| a.key.cmp(&b.key));
        retained.sort_by(|a, b| a.key.cmp(&b.key));
        assert_eq!(
            replay, retained,
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
fn universal_bases_do_not_emit_random_detail_events_over_time() {
    let f = Fixture::new();
    for name in ["Forest", "Creek", "Bedroom", "Slate Desert"] {
        let m = f.maps.iter().find(|m| m["name"] == name).unwrap();
        let mut s = f.session(m["map"].as_str().unwrap());
        s.join("Listener".into(), Vec3::new(0., 84., 0.), true)
            .unwrap();
        for _ in 0..11_000 {
            s.step().unwrap();
        }
        assert!(!s.take_cues().iter().any(|cue|matches!(&cue.kind,
            CueKind::WeaponSound{profile}|CueKind::SoundShaped{profile,..} if profile.starts_with("map_ambience:"))),"unexpected extra sound event on{name}");
        assert!(
            s.package_diagnostics().is_empty(),
            "{:?}",
            s.package_diagnostics()
        );
    }
}
