//! Bounded literal AudioEmitter adaptation into ordinary native sound/rule data.
//! No source script execution, runtime Torque dependency, or map-name policy.
use anyhow::{Context, Result, bail, ensure};
use bri_content::scene::{Node, Scene};
use bri_convert::effect_script::Declarations;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Default)]
pub struct Export {
    pub covered: BTreeMap<String, BTreeSet<usize>>,
    pub diagnostics: Vec<String>,
    pub files: usize,
}

fn number(fields: &BTreeMap<String, String>, key: &str, default: f32) -> Result<f32> {
    let n = fields
        .get(key)
        .map(|v| v.trim().parse::<f32>())
        .transpose()?
        .unwrap_or(default);
    ensure!(n.is_finite(), "non-finite {key}");
    Ok(n)
}

fn flag(fields: &BTreeMap<String, String>, key: &str, default: bool) -> Result<bool> {
    match fields
        .get(key)
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        None => Ok(default),
        Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        _ => bail!("nonliteral flag {key}"),
    }
}

struct Emitter {
    profile: String,
    source: String,
    bytes: Vec<u8>,
    volume: f32,
    spatial: Option<bri_weapons::SoundSpatial>,
    exterior: bool,
    position: [f32; 3],
}

fn emitter(
    src: &crate::source::Source,
    declarations: &Declarations,
    map: &str,
    mission: &str,
    node: &Node,
) -> Result<Emitter> {
    let p = &node.properties;
    let supported = [
        "source_class",
        "profile",
        "filename",
        "description",
        "useprofiledescription",
        "islooping",
        "is3d",
        "loopcount",
        "minloopgap",
        "maxloopgap",
        "volume",
        "type",
        "referencedistance",
        "maxdistance",
        "coneinsideangle",
        "coneoutsideangle",
        "coneoutsidevolume",
        "conevector",
        "outsideambient",
        "position",
        "rotation",
        "scale",
        "enablevisualfeedback",
        "locked",
    ];
    for key in p.keys() {
        ensure!(
            supported.contains(&key.as_str()),
            "unsupported emitter field {key}"
        );
    }
    ensure!(
        !node.name.trim().is_empty(),
        "unnamed emitter has no stable authored identity"
    );
    ensure!(
        !flag(p, "useprofiledescription", false)?,
        "profile-description playback not yet adapted"
    );
    ensure!(
        p.get("description").is_none_or(|v| v.trim().is_empty()),
        "explicit description playback not yet adapted"
    );
    ensure!(
        flag(p, "islooping", false)?,
        "nonlooping map emitter not yet adapted"
    );
    ensure!(
        number(p, "loopcount", -1.)? == -1.
            && number(p, "minloopgap", 0.)? == 0.
            && number(p, "maxloopgap", 0.)? == 0.,
        "finite loop counts or scheduled loop gaps not yet adapted"
    );
    ensure!(
        p.contains_key("type") && number(p, "type", 2.)? == 2.,
        "non-effects audio channel not yet adapted"
    );
    let inside = number(p, "coneinsideangle", 360.)?;
    let outside = number(p, "coneoutsideangle", 360.)?;
    let outer_gain = number(p, "coneoutsidevolume", 1.)?;
    ensure!(
        (0.0..=360.0).contains(&inside)
            && (inside..=360.0).contains(&outside)
            && (0.0..=1.0).contains(&outer_gain),
        "invalid sound cone"
    );
    // Full-sphere inner cone, or unity outside gain, is angularly neutral.
    // A non-neutral source needs a real directional consumer, never flattening.
    ensure!(
        inside == 360. || outer_gain == 1.,
        "non-neutral directional cone not yet adapted"
    );
    let volume = number(p, "volume", 1.)?;
    ensure!((0.0..=1.0).contains(&volume), "invalid emitter volume");
    let spatial = if flag(p, "is3d", true)? {
        let reference_distance = number(p, "referencedistance", 1.)?;
        let max_distance = number(p, "maxdistance", 100.)?;
        ensure!(
            reference_distance >= 0.01
                && max_distance >= reference_distance
                && max_distance <= 20000.,
            "unsupported or invalid reference/max distance"
        );
        Some(bri_weapons::SoundSpatial {
            reference_distance,
            max_distance,
        })
    } else {
        None
    };
    let direct = p.get("filename").filter(|v| !v.trim().is_empty());
    let (origin, file) = if let Some(file) = direct {
        (mission, file.as_str())
    } else {
        let profile = p.get("profile").context("no filename or profile")?;
        let d = declarations
            .entries
            .iter()
            .rev()
            .find(|d| {
                d.class.eq_ignore_ascii_case("AudioProfile") && d.name.eq_ignore_ascii_case(profile)
            })
            .context("profile has no literal declaration in this Add-On")?;
        (
            d.source.as_str(),
            d.fields
                .get("filename")
                .context("profile has no filename")?
                .as_str(),
        )
    };
    let source = crate::file_named(src, &BTreeMap::new(), origin, file);
    ensure!(
        !source.starts_with('/') && !source.contains(':') && !source.split('/').any(|s| s == ".."),
        "sound path escapes its virtual content root"
    );
    let extension = Path::new(&source)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    ensure!(
        extension.eq_ignore_ascii_case("wav") || extension.eq_ignore_ascii_case("ogg"),
        "emitter needs a native-supported WAV or Ogg recording"
    );
    let bytes = src
        .get(&source)
        .context("recording is not supplied by this Add-On")?
        .bytes
        .clone();
    ensure!(
        !bytes.is_empty() && bytes.len() <= 8 * 1024 * 1024,
        "recording exceeds native sound resource limit"
    );
    ensure!(
        node.transform.iter().all(|n| n.is_finite()),
        "non-finite emitter transform"
    );
    // Native scene transforms already carry the Torque-to-Y-up conversion,
    // matching the existing map particle-emitter export's placement frame.
    let position = glam::Mat4::from_cols_array(&node.transform)
        .transform_point3(glam::Vec3::ZERO)
        .to_array();
    let profile = format!("{map}/sound/{}", crate::namespace_for(&node.name)?);
    ensure!(
        profile.len() <= 128,
        "authored emitter identity exceeds sound limit"
    );
    Ok(Emitter {
        profile,
        source,
        bytes,
        volume,
        spatial,
        exterior: flag(p, "outsideambient", true)?,
        position,
    })
}

/// Add assets and an actual per-map, start-once rule consumer before returning
/// any supported source-node exemptions to the outer importer report.
pub fn export(
    input: &Path,
    out: &Path,
    namespace: &str,
    maps: &[(String, String, Value)],
    provides: &mut Vec<Value>,
) -> Result<Export> {
    let mut result = Export::default();
    if !maps.iter().any(|(_, _, scene)| {
        scene["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|n| n["properties"]["source_class"] == "audioemitter")
    }) {
        return Ok(result);
    }
    let src = crate::source::read(input)?;
    let mut declarations = Declarations::default();
    for file in src
        .files
        .values()
        .filter(|f| f.path.to_ascii_lowercase().ends_with(".cs"))
    {
        if let Err(error) = declarations.read_classes(
            &String::from_utf8_lossy(&file.bytes),
            &file.path,
            &["AudioProfile"],
        ) {
            result.diagnostics.push(format!(
                "{}: literal audio declarations incomplete: {error:#}",
                file.path
            ));
        }
    }
    result
        .diagnostics
        .extend(declarations.diagnostics.iter().cloned());
    let mut sounds = BTreeMap::new();
    let mut by_map = BTreeMap::<String, Vec<Value>>::new();
    let mut receipts = Vec::new();
    let mut clips = BTreeMap::new();
    for (map, mission, value) in maps {
        let scene: Scene = serde_json::from_value(value.clone())?;
        let mut names = BTreeSet::new();
        for (index, node) in scene.nodes.iter().enumerate().filter(|(_, n)| {
            n.properties
                .get("source_class")
                .is_some_and(|c| c == "audioemitter")
        }) {
            let converted = emitter(&src, &declarations, map, mission, node).and_then(|e| {
                ensure!(
                    names.insert(e.profile.clone()),
                    "duplicate authored emitter identity"
                );
                ensure!(
                    by_map.get(map).map_or(0, Vec::len) < 16,
                    "more than sixteen ambient emitters on one map"
                );
                Ok(e)
            });
            let e = match converted {
                Ok(e) => e,
                Err(error) => {
                    result.diagnostics.push(format!(
                        "Map {map}, audio node {index} '{}': {error:#}",
                        node.name
                    ));
                    continue;
                }
            };
            let hash = crate::source::hash(&e.bytes);
            let extension = Path::new(&e.source)
                .extension()
                .unwrap()
                .to_string_lossy()
                .to_ascii_lowercase();
            let clip = format!("map-audio/{hash}.{extension}");
            clips.insert(clip.clone(), e.bytes);
            let mut sound = json!({"file":clip,"volume":e.volume,"looping":true,"music":true});
            if let Some(spatial) = e.spatial {
                sound["spatial"] = serde_json::to_value(spatial)?;
            }
            if e.exterior {
                sound["environment"] = json!("exterior");
            }
            sounds.insert(e.profile.clone(), sound);
            by_map.entry(map.clone()).or_default().push(json!({
                "key": e.profile, "track":e.profile,
                "at":e.spatial.map(|_| e.position),
            }));
            receipts.push(json!({"map":map,"mission":mission,"node":index,"name":node.name,
                "profile":e.profile,"source":e.source,"sha256":hash,"position":e.position,
                "spatial":e.spatial,"volume":e.volume,"outside_ambient":e.exterior,
                "authored_properties":node.properties,
                "cone":"angularly neutral; no directional approximation",
                "limits":"Exterior shelter uses bounded native physical-geometry exposure, not original interior-audio-manager fidelity."}));
            result.covered.entry(map.clone()).or_default().insert(index);
        }
    }
    if !sounds.is_empty() {
        let assets = out.join("assets");
        ensure!(
            !assets.join("weapons.json").exists(),
            "map audio must not overwrite an existing native sound pack"
        );
        std::fs::create_dir_all(assets.join("map-audio"))?;
        for (file, bytes) in clips {
            std::fs::write(assets.join(file), bytes)?;
            result.files += 1;
        }
        std::fs::write(
            assets.join("weapons.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version":bri_weapons::SCHEMA,"id":namespace,"sounds":sounds,
            }))?,
        )?;
        let mut script = String::from("fn map_beds() {\n");
        for (map, beds) in by_map {
            // JSON object keys need Rhai map syntax; values are literal IDs,
            // native coordinates and null for nonpositional playback only.
            let beds = beds
                .iter()
                .map(|bed| {
                    format!(
                        "#{{key:{},track:{},at:{}}}",
                        bed["key"],
                        bed["track"],
                        if bed["at"].is_null() {
                            "()".into()
                        } else {
                            bed["at"].to_string()
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            script.push_str(&format!(
                "if map_id() == {} {{ return [{beds}]; }}\n",
                serde_json::to_string(&map)?
            ));
        }
        script.push_str("[]\n}\nfn start_beds() {\nif get(\"started\") { return; }\nfor bed in map_beds() {\nlet options = #{fade:0.25};\nif bed.at != () { options.at = bed.at; }\nplay_ambient_all(bed.key, bed.track, options);\n}\nset(\"started\",true);\n}\nfn on_join(p) { start_beds(); }\nfn on_tick() { start_beds(); }\n");
        std::fs::write(out.join("map-audio.rhai"), script)?;
        std::fs::write(
            out.join("map-audio-behaviour.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version":1,"script":"map-audio.rhai","on_join":true,"tick_interval":120,
                "state":{"global":{"started":{"default":false,"persist":false}}},
            }))?,
        )?;
        provides.extend([
            json!({"kind":"weapons","id":format!("{namespace}:weapons/map-audio"),"file":"assets/weapons.json"}),
            json!({"kind":"script","id":format!("{namespace}:script/map-audio"),"file":"map-audio.rhai"}),
            json!({"kind":"behaviour","id":format!("{namespace}:behaviour/map-audio"),"file":"map-audio-behaviour.json"}),
        ]);
        result.files += 3;
    }
    std::fs::write(
        out.join("map-audio-provenance.json"),
        serde_json::to_vec_pretty(&json!({
            "schema_version":1,"source_archive_sha256":src.sha256,"sources":receipts,
            "diagnostics":result.diagnostics,"license":"Original map package provenance; no relicensing of original recordings",
            "evidence":"Source mission/profile literals; cone/outsideAmbient interpretation also supported by related-family OpenMBU legacy code, not original Blockland binary proof",
        }))?,
    )?;
    result.files += 1;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend(164u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(8000u32.to_le_bytes());
        bytes.extend(16000u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(128u32.to_le_bytes());
        for i in 0..64 {
            bytes.extend((if i % 2 == 0 { 8192i16 } else { -8192i16 }).to_le_bytes());
        }
        bytes
    }

    #[test]
    fn literal_profile_and_direct_recordings_get_real_consumers_but_unsupported_nodes_do_not() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("Map_Audio");
        let out = temp.path().join("native");
        std::fs::create_dir_all(&input).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let raw = wav();
        std::fs::write(input.join("tone.wav"), &raw).unwrap();
        std::fs::write(input.join("resources.cs"),
            r#"datablock AudioProfile(RiverSound) { filename = "./tone.wav"; description = AudioDefaultLooping3d; preload = true; };"#).unwrap();
        let node = |name: &str, properties: Value| {
            json!({
                "name":name, "parent":null, "kind":"unadapted", "asset":null,
                "transform":glam::Mat4::from_translation(glam::Vec3::new(2.,3.,-4.)).to_cols_array(),
                "properties":properties,
            })
        };
        let mut p = json!({"source_class":"audioemitter","profile":"RiverSound", "islooping":"1",
            "type":"2","is3d":"1","volume":"0.8","referencedistance":"80","maxdistance":"300",
            "coneinsideangle":"360","coneoutsideangle":"360","coneoutsidevolume":"0.25"});
        let a = node("River", p.clone());
        p["filename"] = json!("./tone.wav");
        p["profile"] = json!("");
        p["referencedistance"] = json!("9521");
        p["maxdistance"] = json!("9521.01");
        p["coneinsideangle"] = json!("90");
        p["coneoutsideangle"] = json!("120");
        p["coneoutsidevolume"] = json!("1");
        p["outsideambient"] = json!("0");
        let b = node("Crickets", p.clone());
        p["coneoutsidevolume"] = json!("0.2");
        let c = node("Directional", p.clone());
        p["coneoutsidevolume"] = json!("1");
        p["maxloopgap"] = json!("10");
        let d = node("Scheduled", p);
        let maps = vec![(
            "audio:map/test".into(),
            "Add-Ons/Map_Audio/test.mis".into(),
            json!({"schema_version":1,"id":"audio:map/test","name":"Audio fixture","nodes":[a,b,c,d]}),
        )];
        let mut provides = Vec::new();
        let result = export(&input, &out, "audio", &maps, &mut provides).unwrap();
        assert_eq!(result.covered["audio:map/test"], BTreeSet::from([0, 1]));
        assert!(result.diagnostics.iter().any(|s| s.contains("non-neutral")));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|s| s.contains("scheduled loop gaps"))
        );
        let pack =
            bri_weapons::Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap())
                .unwrap();
        assert_eq!(pack.sounds.len(), 2);
        assert_eq!(
            pack.sounds["audio:map/test/sound/river"]
                .spatial
                .unwrap()
                .reference_distance,
            80.
        );
        assert_eq!(
            pack.sounds["audio:map/test/sound/crickets"].environment,
            None
        );
        for sound in pack.sounds.values() {
            assert_eq!(
                std::fs::read(out.join("assets").join(&sound.file)).unwrap(),
                raw
            );
        }
        std::fs::write(out.join("package.json"), serde_json::to_vec(&json!({
            "schema_version":1,"id":"audio","version":"1.0.0","api":1,"name":"Audio fixture",
            "authors":["synthetic fixture"],"license":"CC0-1.0","capabilities":["effects"],"provides":provides,
        })).unwrap()).unwrap();
        let check = bri_package_runtime::check::check(&out);
        assert!(check.ok, "{check}");
        let set = serde_json::from_value(json!({"schema_version":1,"packages":[{
            "id":"audio","version":"1.0.0","side":"shared","dir":"native"
        }]}))
        .unwrap();
        let catalog = std::sync::Arc::new(
            bri_package_runtime::Catalog::load(temp.path(), &set, true).unwrap(),
        );
        let session = |map: &str| {
            let simulation = bri_sim::simulation::Simulation::new(
                bri_world::World::new("Audio fixture".into(), map.into(), vec![[1.; 4]]),
                bri_sim::definitions::Definitions::default(),
                vec![],
            )
            .unwrap();
            let mut session = bri_sim::session::Session::new(simulation);
            session.set_map_id(map);
            session.set_weapon_pack(pack.clone()).unwrap();
            session.install_packages(catalog.clone(), None).unwrap();
            session
        };
        let beds = |s: &mut bri_sim::session::Session, owner| {
            s.take_private_notices()
                .into_iter()
                .filter_map(|(who, notice)| match notice {
                    bri_sim::session::Notice::Ambient(play) if who == owner => Some(play),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let mut s = session("audio:map/test");
        let a = s
            .join("First".into(), glam::Vec3::new(0., 4., 0.), true)
            .unwrap();
        s.step().unwrap();
        let first = beds(&mut s, a);
        assert_eq!(first.len(), 2, "{:?}", s.package_diagnostics());
        assert!(first.iter().all(|bed| bed.at == Some([2., 3., -4.])));
        for _ in 0..240 {
            s.step().unwrap();
        }
        assert!(
            beds(&mut s, a).is_empty(),
            "tick callbacks must not restart existing loops"
        );
        let b = s
            .join("Later".into(), glam::Vec3::new(1., 4., 0.), false)
            .unwrap();
        assert_eq!(
            beds(&mut s, b),
            first,
            "late join gets retained beds exactly once"
        );
        let mut other = session("audio:map/other");
        let p = other
            .join("Other map".into(), glam::Vec3::ZERO, true)
            .unwrap();
        other.step().unwrap();
        assert!(
            beds(&mut other, p).is_empty(),
            "map package must not play on another map"
        );
        let receipt: Value =
            serde_json::from_slice(&std::fs::read(out.join("map-audio-provenance.json")).unwrap())
                .unwrap();
        assert_eq!(receipt["sources"][0]["position"], json!([2., 3., -4.]));
        assert_eq!(receipt["sources"][0]["sha256"], crate::source::hash(&raw));
    }
}
