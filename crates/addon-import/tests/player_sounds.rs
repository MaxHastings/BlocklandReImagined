//! A player type's own sounds: an Add-On's `AudioProfile` named by a
//! `PlayerData`'s `jumpSound` (a Mario body's "wahoo") goes into its
//! weapons pack, and the archetype plays it by that id. Everything here is
//! written by the test; all of it is ours (CC0).
use bri_addon_import::{Options, import};
use std::path::{Path, PathBuf};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bri-player-sounds-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A short 16-bit mono WAV.
fn wav() -> Vec<u8> {
    let samples: Vec<i16> = (0..2205)
        .map(|i| ((i as f32 * 0.2).sin() * 8000.0) as i16)
        .collect();
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&22050u32.to_le_bytes());
    out.extend_from_slice(&44100u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

#[test]
fn a_player_type_jumps_and_cries_with_the_add_ons_own_sounds() {
    let dir = temp("addon").join("Player_Test_Mario");
    write(&dir.join("server.cs"), b"exec(\"./mario.cs\");\n");
    write(
        &dir.join("description.txt"),
        b"Title: Mario\nAuthor: Tester\nA test.\n",
    );
    write(&dir.join("wahoo.wav"), &wav());
    write(&dir.join("mamamia.wav"), &wav());
    write(
        &dir.join("mario.cs"),
        br#"datablock AudioProfile(MarioWahooSound) { filename = "./wahoo.wav"; description = AudioClose3d; preload = true; };
datablock AudioProfile(MarioHurtSound) { filename = "./mamamia.wav"; description = AudioClose3d; preload = true; };
datablock AudioProfile(MarioGoneSound) { filename = "./notThere.wav"; description = AudioClose3d; };
datablock PlayerData(PlayerMarioArmor : PlayerStandardArmor)
{
   uiName = "Mario";
   jumpSound = MarioWahooSound;
   useCustomPainEffects = true;
   painSound = MarioHurtSound;
   deathSound = MarioGoneSound;
};
"#,
    );
    let out = temp("out").join("out");
    import(&Options {
        input: dir,
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let archetype: serde_json::Value = serde_json::from_slice(
        &std::fs::read(out.join("assets/archetypes/playermarioarmor.json")).unwrap(),
    )
    .unwrap();
    let wahoo = "player_test_mario:sound/mariowahoosound";
    assert_eq!(
        archetype["sounds"],
        serde_json::json!({
            "jump": wahoo,
            "pain": "player_test_mario:sound/mariohurtsound",
            // Its file is not in the Add-On: v20 played nothing.
            "death": "",
        })
    );
    // The weapons pack carries the file, so joining players download it.
    let pack =
        bri_weapons::Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap())
            .unwrap();
    let sound = pack.sound(wahoo).expect("the wahoo is packed");
    assert!(!sound.stock, "{sound:?}");
    assert!(
        out.join("assets").join(&sound.file).is_file(),
        "{}",
        sound.file
    );
    let report = std::fs::read_to_string(out.join("IMPORT-REPORT.md")).unwrap();
    assert!(!report.contains("jumpsound"), "{report}");
}
