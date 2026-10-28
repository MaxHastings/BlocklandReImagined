//! First-party authored sounds in the existing native resource catalog.
//! These are separate from the read-only v20 import and retain their own IDs.
use anyhow::{Result, ensure};
use bri_weapons::{Pack, Resource};
use std::path::Path;

const CLIPS: [&[u8]; 4] = [
    include_bytes!("../../../assets/blockhead-footsteps/step_01.wav"),
    include_bytes!("../../../assets/blockhead-footsteps/step_02.wav"),
    include_bytes!("../../../assets/blockhead-footsteps/step_03.wav"),
    include_bytes!("../../../assets/blockhead-footsteps/step_04.wav"),
];

// Maxwell's -1.5 dB adjustment after the first base-game playtest. Contact
// gain remains +2.5 dB, making the combined change +1 dB over original F.
const PROFILE_VOLUME: f32 = 0.841_395_14;
const LAND_ID: &str = "bri:foley/blockhead/land_01";
const LAND_FILE: &str = "sounds/blockhead/land_01.wav";
const LAND_RESOURCE: &str = "bri/foley/blockhead/land_01.wav";
const LAND_CLIP: &[u8] = include_bytes!("../../../assets/blockhead-footsteps/land_01.wav");
const JET_ID: &str = "bri:foley/blockhead/jet";
const JET_RESOURCE: &str = "bri/foley/blockhead/jet.wav";
const JET_CLIP: &[u8] = include_bytes!("../../../assets/blockhead-jets/jet.wav");
const JET_CREDIT_RESOURCE: &str = "bri/foley/blockhead/jet-attribution.txt";
const JET_CREDITS: &[u8] = include_bytes!("../../../assets/blockhead-jets/ATTRIBUTION.txt");

pub fn install(pack: &mut Pack, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out.join("sounds/blockhead"))?;
    for (index, bytes) in CLIPS.into_iter().enumerate() {
        let id = format!("bri:footsteps/blockhead/step_{:02}", index + 1);
        let file = format!("sounds/blockhead/step_{:02}.wav", index + 1);
        install_sound(
            pack,
            out,
            &id,
            &file,
            &format!("bri/foley/blockhead/step_{:02}.wav", index + 1),
            bytes,
            PROFILE_VOLUME,
        )?;
    }
    install_sound(pack, out, LAND_ID, LAND_FILE, LAND_RESOURCE, LAND_CLIP, 1.0)?;
    install_sound(
        pack,
        out,
        JET_ID,
        "sounds/blockhead/jet.wav",
        JET_RESOURCE,
        JET_CLIP,
        1.0,
    )?;
    let jet = pack.sounds.get_mut(JET_ID).unwrap();
    jet.looping = true;
    jet.sustain = Some(bri_weapons::SustainDef {
        loop_start_seconds: 0.33,
        attack_seconds: 0.05,
        release_seconds: 0.15,
    });
    let credit_file = "jet-audio-attribution.txt";
    std::fs::write(out.join(credit_file), JET_CREDITS)?;
    pack.resources.push(Resource {
        path: JET_CREDIT_RESOURCE.into(),
        sha256: super::hash(JET_CREDITS),
        native_file: Some(credit_file.into()),
        diagnostics: Vec::new(),
        package: None,
    });
    Ok(())
}

fn install_sound(
    pack: &mut Pack,
    out: &Path,
    id: &str,
    file: &str,
    resource: &str,
    bytes: &[u8],
    volume: f32,
) -> Result<()> {
    ensure!(!pack.sounds.contains_key(id), "duplicate base sound {id}");
    std::fs::write(out.join(file), bytes)?;
    pack.sounds.insert(
        id.into(),
        serde_json::from_value(serde_json::json!({"file":file, "volume":volume}))?,
    );
    pack.resources.push(Resource {
        path: resource.into(),
        sha256: super::hash(bytes),
        native_file: Some(file.into()),
        diagnostics: Vec::new(),
        package: None,
    });
    Ok(())
}

/// Refresh only this game's authored Foley in an existing generated catalog.
/// Warm playtests can keep all imported native content; no original install
/// or Torque inputs are read or changed by this operation.
pub fn refresh(out: &Path) -> Result<Pack> {
    let mut pack = Pack::from_json(&std::fs::read(out.join("weapons.json"))?)?;
    for index in 1..=4 {
        pack.sounds
            .remove(&format!("bri:footsteps/blockhead/step_{index:02}"));
        let path = format!("bri/foley/blockhead/step_{index:02}.wav");
        pack.resources.retain(|resource| resource.path != path);
    }
    pack.sounds.remove(LAND_ID);
    pack.sounds.remove(JET_ID);
    pack.resources
        .retain(|resource| resource.path != JET_RESOURCE);
    pack.resources
        .retain(|resource| resource.path != JET_CREDIT_RESOURCE);
    pack.resources
        .retain(|resource| resource.path != LAND_RESOURCE);
    install(&mut pack, out)?;
    pack.validate()?;
    let temporary = out.join("weapons.foley.tmp");
    ensure!(!temporary.exists(), "Foley refresh already staged");
    std::fs::write(&temporary, serde_json::to_vec_pretty(&pack)?)?;
    std::fs::rename(temporary, out.join("weapons.json"))?;
    Ok(pack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_weapons::SoundDef;

    #[test]
    fn base_foley_is_shipped_and_hashed_without_an_add_on() -> Result<()> {
        let out = std::env::temp_dir().join(format!("bri-base-foley-{}", std::process::id()));
        let mut pack = crate::lower(Vec::new())?;
        install(&mut pack, &out)?;
        let policy: serde_json::Value = serde_json::from_str(include_str!(
            "../../../assets/blockhead-footsteps/sounds.json"
        ))?;
        for id in policy["footsteps"]
            .as_array()
            .unwrap()
            .iter()
            .chain(std::iter::once(&policy["land"]))
            .chain(std::iter::once(&policy["jet"]))
        {
            let id = id.as_str().unwrap();
            let sound: &SoundDef = &pack.sounds[id];
            let bytes = std::fs::read(out.join(&sound.file))?;
            let resource = pack
                .resources
                .iter()
                .find(|r| r.native_file.as_deref() == Some(sound.file.as_str()))
                .unwrap();
            assert_eq!(resource.sha256, super::super::hash(&bytes));
            assert_eq!(bytes.get(..4), Some(b"RIFF".as_slice()));
            assert!(sound.package.is_none());
        }
        let jet = &pack.sounds[policy["jet"].as_str().unwrap()];
        let recipe: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/blockhead-jets/recipe.json"))?;
        let held = jet.sustain.unwrap();
        assert!(jet.looping);
        assert!(
            (held.loop_start_seconds - recipe["loop_start_seconds"].as_f64().unwrap() as f32).abs()
                < 1e-6
        );
        assert_eq!(held.attack_seconds, 0.05);
        assert_eq!(held.release_seconds, 0.15);
        let credits = pack
            .resources
            .iter()
            .find(|r| r.path == JET_CREDIT_RESOURCE)
            .unwrap();
        let credit_bytes = std::fs::read(out.join(credits.native_file.as_ref().unwrap()))?;
        assert_eq!(credits.sha256, super::super::hash(&credit_bytes));
        assert_eq!(credit_bytes, JET_CREDITS);
        pack.validate()?;
        std::fs::write(out.join("weapons.json"), serde_json::to_vec(&pack)?)?;
        let refreshed = refresh(&out)?;
        assert_eq!(
            serde_json::to_value(refreshed)?,
            serde_json::to_value(&pack)?
        );
        assert_eq!(
            serde_json::to_value(refresh(&out)?)?,
            serde_json::to_value(&pack)?
        );
        Ok(())
    }
}
