//! Made-up content for tests that have no generated v20 content: packs
//! written in code with invented values and read back through the same
//! loaders (and checks) the real packs go through.
//!
//! Nothing here is read from, measured on, or copied out of the original
//! game's files. Names follow what the engine itself looks up (the avatar
//! slots, `Mount0`, `Eye`, the sequence aliases `pose_with_animation`
//! plays); every number is invented.
//!
//! - [`avatar`]: a small Blockhead-like avatar package (rig, outfit parts,
//!   animations, textures) that `AvatarAssets::load` accepts.
pub use bri_content::testing::{
    BoxPart, Paint, ScratchDir, cuboid, material, plain, png, rigid_shape, sha256, write_file,
};

pub mod avatar {
    //! The made-up avatar package of [`bri_content::testing::avatar`]
    //! (rig, outfit parts, animations, textures), plus [`assets`]: the
    //! same package loaded through the client's `AvatarAssets`.
    //!
    //! ```no_run
    //! let assets = bri_client::testing::avatar::assets()?;
    //! # anyhow::Ok(())
    //! ```
    pub use bri_content::testing::avatar::*;

    use crate::avatar::AvatarAssets;
    use anyhow::Result;

    /// The pack loaded through `AvatarAssets::load` (written to a scratch
    /// folder that is removed once loaded: loading reads everything).
    pub fn assets() -> Result<AvatarAssets> {
        let dir = super::ScratchDir::new("avatar")?;
        write(dir.path())?;
        AvatarAssets::load(dir.path())
    }
}

pub mod weapon_debris {
    //! A made-up weapon-debris pack (`WeaponDebrisAssets::load`): the gun
    //! casing, an explosion sphere and the casing's two textures, with
    //! invented motion values. The pack id, model ids, texture ids and the
    //! casing's material names are the ones the engine binds.
    use super::{material, plain, png, rigid_shape, write_file};
    use anyhow::Result;
    use serde_json::json;
    use std::path::Path;

    pub const PACK_ID: &str = "v20.weapon-debris.001";
    pub const SHELL_MODEL: &str = "v20.weapon_debris.gun_shell";
    pub const SPHERE_MODEL: &str = "v20.weapon_debris.rocket_explosion_sphere";
    /// The shell's bounces before it settles.
    pub const BOUNCES: u32 = 2;

    /// Write `pack.json`, its models and textures into `dir`.
    pub fn write(dir: &Path) -> Result<()> {
        let shell = rigid_shape(
            "test:debris/shell",
            &[("root", None, [0.0; 3])],
            &[
                (0, [0.0, 0.0, 0.0], [0.02, 0.05, 0.02], plain(0)),
                (0, [0.0, 0.055, 0.0], [0.021, 0.005, 0.021], plain(1)),
            ],
            vec![material("black50", "opaque"), material("yellow", "opaque")],
        );
        let sphere = rigid_shape(
            "test:debris/sphere",
            &[("root", None, [0.0; 3])],
            &[(0, [0.0; 3], [1.0; 3], plain(0))],
            vec![material("yellow", "opaque")],
        );
        let shell_sha = write_file(dir, "models/shell.json", &serde_json::to_vec(&shell)?)?;
        let sphere_sha = write_file(dir, "models/sphere.json", &serde_json::to_vec(&sphere)?)?;
        let dark = write_file(
            dir,
            "textures/dark.png",
            &png(4, 4, |_, _| [30, 30, 30, 255])?,
        )?;
        let gold = write_file(
            dir,
            "textures/gold.png",
            &png(4, 4, |x, _| [200, 160 + x as u8 * 8, 40, 255])?,
        )?;
        let pack = json!({
            "schema_version": 1,
            "id": PACK_ID,
            "weapons_sha256": "1".repeat(64),
            "models": [
                {"id": SHELL_MODEL, "file": "models/shell.json", "native_sha256": shell_sha},
                {"id": SPHERE_MODEL, "file": "models/sphere.json", "native_sha256": sphere_sha},
            ],
            "textures": [
                {"id": "gun_black50", "file": "textures/dark.png", "source_sha256": dark, "width": 4, "height": 4},
                {"id": "gun_yellow", "file": "textures/gold.png", "source_sha256": gold, "width": 4, "height": 4},
            ],
            "shell": {
                "model": SHELL_MODEL,
                "lifetime_seconds": 1.5,
                "min_spin_degrees_per_second": -300.0,
                "max_spin_degrees_per_second": 250.0,
                "elasticity": 0.4,
                "friction": 0.3,
                "bounces": BOUNCES,
                "static_on_max_bounce": true,
                "snap_on_max_bounce": false,
                "fade": true,
                "gravity_multiplier": 1.5,
                "exit_direction": [1.0, 0.8, 1.1],
                "exit_offset": [0.0, 0.0, 0.0],
                "exit_variance_degrees": 10.0,
                "velocity": 6.0,
            },
        });
        write_file(dir, "pack.json", &serde_json::to_vec_pretty(&pack)?)?;
        Ok(())
    }

    /// The pack loaded through `WeaponDebrisAssets::load`.
    pub fn assets() -> Result<crate::weapon_debris::WeaponDebrisAssets> {
        let dir = super::ScratchDir::new("weapon-debris")?;
        write(dir.path())?;
        crate::weapon_debris::WeaponDebrisAssets::load(dir.path())
    }
}

pub mod tutorial {
    //! A made-up tutorial pack's target models (`TutorialTargets::load`):
    //! the four target shapes the engine names, each a made-up board on a
    //! post, the marked ones skinnable (`base.` materials), with textures
    //! drawn in code. The index names the brick layouts but they are not
    //! written.
    use super::{Paint, material, plain, png, rigid_shape, sha256, write_file};
    use anyhow::Result;
    use bri_content::tutorial::{
        PACK_INDEX, PACK_SCHEMA, PackIndex, TARGET_HIT_SHAPE, TARGET_M_HIT_SHAPE, TARGET_M_SHAPE,
        TARGET_SHAPE, TARGET_SKINS,
    };
    use std::{collections::BTreeMap, path::Path};

    /// Write the index, the target shapes and their textures into `dir`.
    pub fn write_targets(dir: &Path) -> Result<PackIndex> {
        let mut textures = BTreeMap::new();
        let mut texture = |name: &str, rgb: [u8; 3]| -> Result<()> {
            let bytes = png(4, 4, |x, y| {
                let ring = (x + y) % 2 == 0;
                [
                    rgb[0],
                    if ring { rgb[1] } else { 255 - rgb[1] },
                    rgb[2],
                    255,
                ]
            })?;
            let file = format!("{}.png", sha256(&bytes));
            write_file(dir, &file, &bytes)?;
            textures.insert(name.to_string(), file);
            Ok(())
        };
        texture("post.png", [90, 70, 50])?;
        texture("board.png", [220, 30, 30])?;
        texture("burnt.png", [40, 40, 40])?;
        for (i, skin) in TARGET_SKINS.iter().enumerate() {
            texture(&format!("{skin}.mark.png"), [20, 60 * i as u8, 200])?;
        }
        let mut shapes = BTreeMap::new();
        for id in [
            TARGET_SHAPE,
            TARGET_HIT_SHAPE,
            TARGET_M_SHAPE,
            TARGET_M_HIT_SHAPE,
        ] {
            let hit = id == TARGET_HIT_SHAPE || id == TARGET_M_HIT_SHAPE;
            let marked = id == TARGET_M_SHAPE || id == TARGET_M_HIT_SHAPE;
            // A hit target's post is burnt; a marked one's board takes
            // the skin.
            let post = if hit { "burnt.png" } else { "post.png" };
            let face = if marked { "base.mark.png" } else { "board.png" };
            let shape = rigid_shape(
                id,
                &[
                    ("root", None, [0.0; 3]),
                    ("board", Some(0), [0.0, 1.2, 0.0]),
                ],
                &[
                    (0, [0.0, 0.6, 0.0], [0.05, 0.6, 0.05], plain(0)),
                    (
                        1,
                        [0.0, 0.0, 0.0],
                        [0.5, 0.5, 0.05],
                        Paint { front: 1, rest: 0 },
                    ),
                ],
                vec![material(post, "opaque"), material(face, "opaque")],
            );
            let bytes = serde_json::to_vec(&shape)?;
            let file = format!("{}.shape.json", sha256(&bytes));
            write_file(dir, &file, &bytes)?;
            shapes.insert(id.to_string(), file);
        }
        let index = PackIndex {
            schema_version: PACK_SCHEMA,
            part1: "part1.world.json".into(),
            part2: "part2.world.json".into(),
            targets: Vec::new(),
            targets_end_ms: 60_000,
            shapes,
            textures,
        };
        index.validate()?;
        write_file(dir, PACK_INDEX, &serde_json::to_vec_pretty(&index)?)?;
        Ok(index)
    }
}

pub mod items {
    //! `bri_net::testing::items`' made-up weapons and item presentation
    //! packs (`ItemAssets::load`), the printer's and the hammer's icons
    //! pictured from their models by the client's icon renderer, as stock
    //! icons are pictures of their models.
    use super::ScratchDir;
    use anyhow::{Context, Result};
    use bri_content::shape::Shape;
    use glam::{Mat4, Quat, Vec2};
    use std::path::{Path, PathBuf};

    pub use bri_net::testing::items::{
        CLEAR_CAN_BODY, CLEAR_CAN_MODEL, CLEAR_CAN_TRIM, GUN_ICON, GUN_MODEL, HELD_DETAIL,
        ICON_SIZE, LETTER_ITEMS, PICTURED_MODELS, PRINTER_MODEL, SWUNG_IMAGES, WAND_ICON,
        WAND_MODEL, WORLD_DETAIL, weapons_pack,
    };

    /// The written packs, removed when dropped.
    pub struct ItemPacks {
        /// The folder holding `presentation.json` and `item-physics.json`.
        pub presentation: PathBuf,
        /// The folder holding `weapons.json`.
        pub weapons: PathBuf,
        _scratch: ScratchDir,
    }

    /// Write both packs into a fresh scratch folder.
    pub fn write_packs() -> Result<ItemPacks> {
        let scratch = ScratchDir::new("items")?;
        let presentation = scratch.path().join("items");
        let weapons = scratch.path().join("weapons");
        write(&presentation, &weapons)?;
        Ok(ItemPacks {
            presentation,
            weapons,
            _scratch: scratch,
        })
    }

    /// Write `weapons.json` into `weapons_dir` and the presentation pack
    /// (models, textures, icons, item physics) into `dir`.
    pub fn write(dir: &Path, weapons_dir: &Path) -> Result<()> {
        bri_net::testing::items::write_with_icons(dir, weapons_dir, &|key, shape, textures| {
            let images = textures
                .iter()
                .map(|(label, bytes)| {
                    let rgba = image::load_from_memory(bytes)?.to_rgba8();
                    Ok(bri_render::scene::SceneImage {
                        label: label.clone(),
                        width: rgba.width(),
                        height: rgba.height(),
                        rgba: rgba.into_raw(),
                        srgb: false,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            rendered_icon(key, shape, &images).map(Some)
        })
    }

    /// A picture of `shape` turned side on, filling the icon but for a
    /// clear border, as stock icons picture their models.
    fn rendered_icon(
        key: &str,
        shape: &Shape,
        textures: &[bri_render::scene::SceneImage],
    ) -> Result<Vec<u8>> {
        use crate::item_icon_render::{Look, Mesh, frame, render};
        let pose = bri_content::animation::sample(shape, None, 0.0)?;
        let refs: Vec<_> = textures.iter().collect();
        let scene = crate::items::native_shape_scene(
            key,
            shape,
            &refs,
            [1.0; 4],
            true,
            Mat4::IDENTITY,
            &pose,
        )?;
        let mesh = Mesh::from_scene(&scene);
        let turn = Quat::from_rotation_z(-0.7)
            * Quat::from_rotation_x(0.3)
            * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        let size = ICON_SIZE as f32;
        let pose = frame(
            &mesh,
            turn,
            0.0,
            (Vec2::splat(size * 0.1), Vec2::splat(size * 0.9)),
            [ICON_SIZE; 2],
        )
        .context("an icon pose")?;
        let look = Look {
            base: [1.0; 3],
            textured: true,
            skin: None,
        };
        let image = render(&mesh, &pose, &look, key);
        let mut bytes = Vec::new();
        image::RgbaImage::from_raw(image.width, image.height, image.rgba)
            .context("icon pixels")?
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )?;
        Ok(bytes)
    }

    #[cfg(test)]
    mod tests {
        /// The made-up held detail is the one the client draws only for
        /// the holder.
        #[test]
        fn the_held_detail_is_the_first_person_detail() {
            assert_eq!(
                bri_net::testing::items::FIRST_PERSON_DETAIL_SIZE,
                crate::items::FIRST_PERSON_DETAIL
            );
        }
    }
}

pub mod explosions {
    //! `bri_weapons::testing`'s weapons pack with an `explosionShape` on its
    //! rocket explosion ([`EXPLOSION`]): a made-up sphere whose `ambient`
    //! sequence grows it and fades it out, written beside `weapons.json` as
    //! the converter writes converted resources.
    use super::{material, plain, png, rigid_shape, write_file};
    use anyhow::{Context, Result};
    use bri_content::shape::{Animation, NodeTrack, ObjectTrack};
    use std::path::Path;

    /// The explosion that has a shape.
    pub const EXPLOSION: &str = bri_weapons::testing::ROCKET_EXPLOSION;
    /// Its shape's source path.
    pub const SHAPE: &str = "test/shapes/blast.dts";

    /// Write `weapons.json` and the shape's resources into `dir`.
    pub fn write_pack(dir: &Path) -> Result<bri_weapons::Pack> {
        let mut shape = rigid_shape(
            SHAPE,
            &[("root", None, [0.0; 3]), ("ball", Some(0), [0.0; 3])],
            &[(1, [0.0; 3], [1.0; 3], plain(0))],
            vec![material("blastglow", "additive")],
        );
        let frames = 4;
        shape.animations.push(Animation {
            name: "ambient".into(),
            frames,
            duration: 0.35,
            looping: false,
            additive: false,
            priority: 0,
            nodes: vec![NodeTrack {
                node: "ball".into(),
                rotations: Vec::new(),
                translations: Vec::new(),
                scales: (0..frames).map(|i| [0.3 + i as f32 * 0.5; 3]).collect(),
                scale_rotations: Vec::new(),
            }],
            objects: vec![ObjectTrack {
                object: 0,
                visibility: (0..frames)
                    .map(|i| 1.0 - i as f32 / frames as f32)
                    .collect(),
                frames: Vec::new(),
                material_frames: Vec::new(),
            }],
            ground_translations: Vec::new(),
            ground_rotations: Vec::new(),
            triggers: Vec::new(),
        });
        shape.validate()?;
        let model_sha = write_file(dir, "shapes/blast.json", &serde_json::to_vec(&shape)?)?;
        let texture_sha = write_file(
            dir,
            "shapes/blastglow.png",
            &png(4, 4, |x, y| {
                [255, 120 + (x * 20) as u8, 40 + (y * 30) as u8, 200]
            })?,
        )?;
        let mut pack = bri_weapons::testing::pack();
        let explosion = pack
            .explosions
            .get_mut(&EXPLOSION.to_ascii_lowercase())
            .context("the rocket explosion")?;
        explosion.shape = SHAPE.into();
        // Short enough to end within a few clamped frames.
        explosion.play_speed = 1.0;
        explosion.seconds = 0.3;
        let resource = |path: &str, sha256: String, file: &str| bri_weapons::Resource {
            path: path.into(),
            sha256,
            native_file: Some(file.into()),
            diagnostics: Vec::new(),
            package: None,
        };
        pack.resources
            .push(resource(SHAPE, model_sha, "shapes/blast.json"));
        pack.resources.push(resource(
            "test/shapes/blastglow.png",
            texture_sha,
            "shapes/blastglow.png",
        ));
        write_file(dir, "weapons.json", &serde_json::to_vec_pretty(&pack)?)?;
        Ok(pack)
    }
}

pub mod vehicles {
    //! `bri_net::testing::vehicles`' made-up vehicle pack.
    pub use bri_net::testing::vehicles::*;
}

pub mod content_root;

pub mod audio {
    //! A made-up audio pack (`manifest.json` and one WAV clip, a tone) as
    //! `bri_audio::SoundBank::load` reads it: a spatial brick plant and
    //! break sound bound to the engine's `brick.plant` and `brick.break`
    //! triggers, an interface sound named [`NOTE`], a music loop
    //! ([`MUSIC_SOUND`]) and a sound events may play ([`EVENT_SOUND`]).
    use super::write_file;
    use anyhow::Result;
    use bri_audio::schema::*;
    use std::path::Path;

    /// The interface sound's datablock name, played as a profile.
    pub const NOTE: &str = "TestNoteSound";
    /// The triggers the pack binds.
    pub const TRIGGERS: [&str; 2] = ["brick.plant", "brick.break"];
    /// The music loop music bricks offer (`music-brick:` +
    /// `bri_net::testing::MUSIC`), and the sound events may play: the
    /// ids the server's own reading of the pack names.
    pub const MUSIC_SOUND: &str = "fixture/music/tune";
    pub const EVENT_SOUND: &str = "fixture/sound/event";

    fn evidence() -> Evidence {
        Evidence {
            file: "synthetic".into(),
            line: 1,
        }
    }

    fn sound(
        id: &str,
        name: &str,
        spatial: Option<(f32, f32)>,
        channel: u8,
        bus: Bus,
    ) -> SoundEntry {
        SoundEntry {
            id: id.into(),
            name: name.into(),
            clip: Some(CLIP.into()),
            description: None,
            playback: Playback {
                gain: 0.9,
                pitch: 1.0,
                looping: false,
                spatial: spatial.map(|(reference_distance, max_distance)| Spatial {
                    reference_distance,
                    max_distance,
                }),
                channel,
                bus,
            },
            preload: true,
            ui_name: None,
            family: "test".into(),
            package: "base".into(),
            default_enabled: true,
            layer: "test".into(),
            lists: Vec::new(),
            status: SoundStatus::Ready,
            defined_at: evidence(),
        }
    }

    const CLIP: &str = "test/clip/tone";

    /// Write the pack into `dir`.
    pub fn write_pack(dir: &Path) -> Result<PackManifest> {
        let rate = 22_050;
        let frames = rate / 4;
        let pcm: Vec<f32> = (0..frames)
            .map(|i| (i as f32 / rate as f32 * 330.0 * std::f32::consts::TAU).sin() * 0.4)
            .collect();
        let bytes = bri_audio::wav::encode_pcm16(rate, 1, &pcm);
        let file = "clips/tone.wav";
        let sha256 = write_file(dir, file, &bytes)?;
        let clip = ClipEntry {
            id: CLIP.into(),
            file: file.into(),
            sha256,
            bytes: bytes.len() as u64,
            format: ClipFormat::Wav,
            channels: 1,
            sample_rate: rate,
            bits_per_sample: Some(16),
            frames: u64::from(frames),
            duration_seconds: f64::from(frames) / f64::from(rate),
            peak: 0.4,
            rms: 0.28,
            stream: false,
            sources: Vec::new(),
        };
        let mut music = sound(MUSIC_SOUND, "musicData_Fixture_Tune", None, 2, Bus::Music);
        music.playback.looping = true;
        music.ui_name = Some(bri_net::testing::MUSIC.replace('_', " "));
        let mut event = sound(
            EVENT_SOUND,
            "FixtureEventSound",
            Some((8.0, 40.0)),
            2,
            Bus::Effects,
        );
        event.lists = vec!["event-param:Sound".into()];
        let sounds = vec![
            music,
            event,
            sound(
                "test/sound/plant",
                "TestPlantSound",
                Some((8.0, 40.0)),
                2,
                Bus::Effects,
            ),
            sound(
                "test/sound/break",
                "TestBreakSound",
                Some((8.0, 40.0)),
                2,
                Bus::Effects,
            ),
            sound("test/sound/note", NOTE, None, 1, Bus::Interface),
        ];
        let triggers = TRIGGERS
            .iter()
            .zip(["test/sound/plant", "test/sound/break"])
            .map(|(key, sound)| TriggerEntry {
                key: (*key).into(),
                label: (*key).into(),
                sound: sound.into(),
                placement: PlacementKind::World,
                source: "rule".into(),
                package: "base".into(),
                evidence: vec![evidence()],
                note: None,
            })
            .chain([TriggerEntry {
                key: format!("music-brick:{}", bri_net::testing::MUSIC),
                label: bri_net::testing::MUSIC.into(),
                sound: MUSIC_SOUND.into(),
                placement: PlacementKind::World,
                source: "rule".into(),
                package: "base".into(),
                evidence: vec![evidence()],
                note: None,
            }])
            .collect();
        let manifest = PackManifest {
            schema: PACK_SCHEMA.into(),
            schema_version: PACK_SCHEMA_VERSION,
            pack_id: "test-audio".into(),
            generator: Generator {
                name: "bri_client::testing".into(),
                version: "0".into(),
                arguments: Vec::new(),
            },
            source: SourceSummary {
                reference_label: "synthetic".into(),
                executable_sha256: None,
                audio_files_found: 1,
                wav_files_found: 1,
                ogg_files_found: 0,
                scripts_scanned: 0,
            },
            defaults: MixDefaults {
                master_volume: 0.7,
                ..MixDefaults::default()
            },
            channels: Vec::new(),
            clips: vec![clip],
            descriptions: Vec::new(),
            sounds,
            triggers,
            diagnostics: Vec::new(),
        };
        manifest.validate().map_err(anyhow::Error::msg)?;
        write_file(dir, "manifest.json", &serde_json::to_vec_pretty(&manifest)?)?;
        Ok(manifest)
    }
}

/// `synthetic_and_content!(Fixture: body, ...)` emits, for each `body`
/// (`fn body(&Fixture) -> anyhow::Result<()>`), a module `body` holding the
/// tests `synthetic` (on `Fixture::synthetic()`) and `content` (on
/// `Fixture::content()`, the generated v20 content only the local push gate
/// has, so ignored).
#[cfg(test)]
macro_rules! synthetic_and_content {
    ($fixture:ident: $($body:ident),+ $(,)?) => {$(
        mod $body {
            #[test]
            fn synthetic() -> anyhow::Result<()> {
                super::$body(&super::$fixture::synthetic()?)
            }
            #[test]
            #[ignore = "requires generated v20 content"]
            fn content() -> anyhow::Result<()> {
                super::$body(&super::$fixture::content()?)
            }
        }
    )+};
}
#[cfg(test)]
pub(crate) use synthetic_and_content;
