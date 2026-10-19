//! A weapons pack's shells: validated, their noise textures listed with the
//! effects' own, a duplicate id refused.
use bri_weapons::{PackEffects, Shell, ShellNoise};

fn shell(id: &str) -> Shell {
    let noise = |texture: &str| ShellNoise {
        texture: texture.into(),
        repeats: 10.0,
        scroll: [2.0, 0.0, 0.0],
        seconds: 2.2,
    };
    Shell {
        id: id.into(),
        facing: [1.0, 0.545, 0.243, 1.0],
        edge: [0.067, 0.067, 1.0, 1.0],
        stand_off: 0.12,
        ease: 3.0,
        noise: [noise("ce-fx/a.png"), noise("ce-fx/b.png")],
    }
}

#[test]
fn a_shell_validates_and_names_its_textures() {
    let effects = PackEffects {
        shells: vec![shell("pack:shell/shield")],
        ..Default::default()
    };
    effects.validate().unwrap();
    assert!(!effects.is_empty());
    let textures: Vec<_> = effects.textures().map(|(_, t)| t).collect();
    assert_eq!(textures, ["ce-fx/a.png", "ce-fx/b.png"]);
}

#[test]
fn a_broken_or_repeated_shell_is_refused() {
    let with = |shells: Vec<Shell>| PackEffects {
        shells,
        ..Default::default()
    };
    let mut slow = shell("pack:shell/a");
    slow.noise[0].seconds = 0.0;
    let mut dark = shell("pack:shell/b");
    dark.facing[0] = f32::NAN;
    let mut far = shell("pack:shell/c");
    far.stand_off = 5.0;
    let mut bad_texture = shell("pack:shell/d");
    bad_texture.noise[1].texture = "../outside.png".into();
    for broken in [slow, dark, far, bad_texture] {
        assert!(
            with(vec![broken.clone()]).validate().is_err(),
            "{}",
            broken.id
        );
    }
    assert!(
        with(vec![shell("pack:shell/same"), shell("PACK:shell/same")])
            .validate()
            .is_err()
    );
}
