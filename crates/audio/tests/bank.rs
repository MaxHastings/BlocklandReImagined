//! Pack loading failure modes (synthetic data, no hardware).

mod common;

use bri_audio::schema::{PACK_SCHEMA_VERSION, PackManifest};
use bri_audio::*;
use common::synthetic;

fn load(
    m: PackManifest,
    files: &std::collections::HashMap<String, Vec<u8>>,
    opts: &BankOptions,
) -> Result<SoundBank, AudioError> {
    SoundBank::from_manifest(m, opts, |c| {
        files.get(&c.file).cloned().ok_or(AudioError::Io {
            path: c.file.clone(),
            message: "missing".into(),
        })
    })
}

#[test]
fn manifest_round_trips_through_json() {
    let (m, _) = synthetic();
    let text = serde_json::to_string_pretty(&m).unwrap();
    let back: PackManifest = serde_json::from_str(&text).unwrap();
    assert_eq!(m, back);
    back.validate().unwrap();
}

#[test]
fn resolves_ids_and_datablock_names_case_insensitively() {
    let (m, files) = synthetic();
    let bank = load(m, &files, &BankOptions::default()).unwrap();
    assert_eq!(
        bank.resolve("test/sound/boom").unwrap().name.as_ref(),
        "BoomSound"
    );
    assert_eq!(
        bank.resolve("boomsound").unwrap().id.as_ref(),
        "test/sound/boom"
    );
    assert!(matches!(
        bank.resolve("MissingSound"),
        Err(AudioError::SoundUnavailable { .. })
    ));
    assert!(matches!(
        bank.resolve("nope"),
        Err(AudioError::UnknownSound(_))
    ));
    assert_eq!(bank.trigger("test.boom"), Some("test/sound/boom"));
    assert_eq!(bank.streamed_clips(), 1);
    assert!(bank.is_music("test/music/loop"));
}

#[test]
fn corrupted_clip_bytes_fail_integrity_check() {
    let (m, mut files) = synthetic();
    let f = files.get_mut("blobs/short.wav").unwrap();
    let last = f.len() - 1;
    f[last] ^= 0x55;
    assert!(matches!(
        load(m.clone(), &files, &BankOptions::default()),
        Err(AudioError::Integrity { .. })
    ));
    // Without verification the (still decodable) clip loads.
    load(
        m,
        &files,
        &BankOptions {
            verify_hashes: false,
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn memory_budget_is_enforced() {
    let (m, files) = synthetic();
    let r = load(
        m,
        &files,
        &BankOptions {
            max_resident_bytes: 10_000,
            ..Default::default()
        },
    );
    assert!(matches!(r, Err(AudioError::MemoryBudget { .. })));
}

#[test]
fn missing_files_and_bad_manifests_are_rejected() {
    let (m, files) = synthetic();
    let mut no_files = files.clone();
    no_files.remove("blobs/tone.wav");
    assert!(matches!(
        load(m.clone(), &no_files, &BankOptions::default()),
        Err(AudioError::Io { .. })
    ));

    let mut bad = m.clone();
    bad.schema_version = PACK_SCHEMA_VERSION + 1;
    assert!(matches!(
        load(bad, &files, &BankOptions::default()),
        Err(AudioError::Manifest { .. })
    ));

    let mut bad = m.clone();
    bad.sounds[0].clip = Some("nope".into());
    assert!(matches!(
        load(bad, &files, &BankOptions::default()),
        Err(AudioError::Manifest { .. })
    ));

    let mut bad = m.clone();
    bad.clips[0].file = "../../etc/passwd".into();
    assert!(matches!(
        load(bad, &files, &BankOptions::default()),
        Err(AudioError::Manifest { .. })
    ));

    let mut bad = m;
    bad.sounds[0].playback.gain = f32::NAN;
    assert!(matches!(
        load(bad, &files, &BankOptions::default()),
        Err(AudioError::Manifest { .. })
    ));
}

#[test]
fn load_from_directory_and_undecodable_clip() {
    let (m, files) = synthetic();
    let dir = std::env::temp_dir().join(format!("bri-audio-bank-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("blobs")).unwrap();
    for (name, bytes) in &files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string(&m).unwrap(),
    )
    .unwrap();
    let bank = SoundBank::load(&dir, &BankOptions::default()).unwrap();
    assert_eq!(bank.ready_count(), 6);

    // Replace a clip with garbage and fix up its hash: decoding must fail cleanly.
    let mut m2 = m;
    let junk = vec![0u8; 64];
    std::fs::write(dir.join("blobs/short.wav"), &junk).unwrap();
    m2.clips[0].sha256 = {
        use sha2::Digest;
        sha2::Sha256::digest(&junk)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string(&m2).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        SoundBank::load(&dir, &BankOptions::default()),
        Err(AudioError::Decode { .. })
    ));
    std::fs::remove_dir_all(&dir).ok();
    assert!(matches!(
        SoundBank::load(&dir, &BankOptions::default()),
        Err(AudioError::Io { .. })
    ));
}

/// An Add-On sound runs at most 30 seconds; a music track (the music bus)
/// may run longer, and streams rather than decoding into memory.
#[test]
fn an_add_on_music_track_may_run_past_a_sounds_thirty_seconds() {
    use bri_audio::bank::{ClipData, SoundAsset};
    let (rate, seconds) = (8000u32, 40u32);
    let data = vec![0u8; (rate * seconds * 2) as usize];
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    let dir = std::env::temp_dir().join(format!("bri-music-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("theme.wav");
    std::fs::write(&path, &wav).unwrap();

    assert!(SoundAsset::decoded_file("theme", &path, 1.0, 2.0).is_err());
    let music = SoundAsset::music_file("theme", &path).unwrap();
    assert_eq!(music.playback.bus, bri_audio::schema::Bus::Music);
    assert!(music.playback.spatial.is_none());
    assert!(matches!(&music.data, ClipData::Stream(s) if s.frames == u64::from(rate * seconds)));
    std::fs::remove_dir_all(&dir).unwrap();
}
