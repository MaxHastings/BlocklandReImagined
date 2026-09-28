//! The showcase Add-Ons' sounds decode as the game plays Add-On sounds.
use std::path::Path;

#[test]
fn every_showcase_sound_decodes_as_a_short_world_sound() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase");
    let mut found = 0;
    for addon in ["gravity-gun-fx", "steel-ball-fx"] {
        let dir = root.join(addon).join("client/sounds");
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            let asset = bri_audio::SoundAsset::decoded("test", &bytes, "wav", 10.0, 90.0)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let bri_audio::ClipData::Pcm(clip) = &asset.data else {
                panic!("decoded fully");
            };
            let seconds = clip.duration_seconds();
            assert!(
                seconds > 0.2 && seconds < 1.5,
                "{}: {seconds} s",
                path.display()
            );
            let peak = clip.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.4 && peak <= 1.0, "{}: peak {peak}", path.display());
            found += 1;
        }
    }
    assert_eq!(found, 7);
}
