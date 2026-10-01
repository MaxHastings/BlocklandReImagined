//! Made-up effects packs for tests that have no converted content. Every
//! value here is invented; none is read from an original installation.
use crate::{
    EffectsPack, Manifest,
    pack::{Composite, TextureImage},
};
use bri_content::effects::{Curve, Emitter, Flare, Library, Light, Particle, ParticleKey};
use std::{collections::BTreeMap, sync::Arc};

/// The one texture every fixture particle and flare uses.
pub const TEXTURE: &str = "original";
/// The emitters of [`showcase_pack`] that take a brick's paint
/// (`use_emitter_colors`), by display name.
pub const PAINTED_EMITTERS: [&str; 2] = ["Painted Puff", "Painted Glow"];

/// One flared light, one particle and one emitter (ids `light`, `particle`,
/// `emitter`) on a 1x1 texture.
pub fn library() -> Library {
    Library {
        schema_version: 1,
        textures: BTreeMap::from([(TEXTURE.into(), "texture.png".into())]),
        lights: vec![Light {
            id: "light".into(),
            name: "Light".into(),
            enabled: true,
            color: [1., 0.5, 0.],
            brightness: 2.,
            radius: 5.,
            color_curves: None,
            brightness_curve: None,
            radius_curve: None,
            flare: Some(Flare {
                texture: TEXTURE.into(),
                color: [1.; 3],
                third_person: true,
                constant_size: Some(1.),
                near_size: 1.,
                far_size: 0.5,
                near_distance: 0.,
                far_distance: 10.,
                fade_seconds: 0.5,
                blend_mode: 0,
                link_color: true,
                link_size: true,
            }),
        }],
        particles: vec![particle(
            "particle",
            vec![
                ParticleKey {
                    time: 0.,
                    color: [1., 0., 0., 1.],
                    size: 1.,
                },
                ParticleKey {
                    time: 1.,
                    color: [0., 0., 1., 0.],
                    size: 3.,
                },
            ],
        )],
        emitters: vec![emitter("emitter", "Emitter", &["particle"])],
    }
}

/// A plain alpha-blended particle with a 90 degree spin.
pub fn particle(id: &str, keys: Vec<ParticleKey>) -> Particle {
    Particle {
        id: id.into(),
        texture: TEXTURE.into(),
        alpha_blend: true,
        lifetime: 2.,
        lifetime_variance: 0.,
        drag: 0.,
        wind: 0.,
        gravity: 0.,
        inherited_velocity: 0.,
        acceleration: 0.,
        spin_degrees: 90.,
        random_spin: [0., 0.],
        keys,
    }
}

/// An emitter shooting `particles` every 0.1 s at speed 2.
pub fn emitter(id: &str, name: &str, particles: &[&str]) -> Emitter {
    Emitter {
        id: id.into(),
        name: name.into(),
        particles: particles.iter().map(|p| p.to_string()).collect(),
        period: 0.1,
        period_variance: 0.,
        speed: 2.,
        speed_variance: 0.,
        offset: 0.,
        offset_variance: 0.,
        theta_degrees: [0., 0.],
        phi_rate_degrees: 0.,
        phi_variance_degrees: 0.,
        lifetime: 0.,
        lifetime_variance: 0.,
        orient: false,
        orient_on_velocity: true,
        override_advance: false,
        use_emitter_colors: false,
        use_emitter_sizes: false,
        use_placement_velocity: false,
        node_time_scale: 1.,
        point_node_time_scale: 1.,
    }
}

/// An empty manifest (no composites or bindings).
pub fn manifest() -> Manifest {
    Manifest {
        schema_version: 1,
        library_sha256: String::new(),
        textures: BTreeMap::new(),
        emitter_alpha: BTreeMap::new(),
        bindings: Vec::new(),
        composites: Vec::new(),
        unresolved: Vec::new(),
    }
}

/// Assembles a pack from `library` and `manifest`, decoding [`TEXTURE`] as
/// one orange pixel.
pub fn pack_from(library: Library, manifest: Manifest) -> Arc<EffectsPack> {
    EffectsPack::from_parts(
        library,
        manifest,
        vec![TextureImage {
            id: TEXTURE.into(),
            width: 1,
            height: 1,
            rgba: vec![120, 80, 20, 255],
        }],
    )
    .unwrap()
}

/// [`library`] after `change`, in a pack with an empty manifest.
pub fn pack(change: impl FnOnce(&mut Library)) -> Arc<EffectsPack> {
    let mut library = library();
    change(&mut library);
    pack_from(library, manifest())
}

/// A pack shaped like a small stock effects pack: plain, additive, falling
/// and painted emitters, flared and curve-driven lights and composites that
/// combine them.
pub fn showcase_pack() -> Arc<EffectsPack> {
    let mut l = library();
    let key = |time, color, size| ParticleKey { time, color, size };
    let mut spark = particle(
        "spark",
        vec![
            key(0., [1., 1., 0.5, 2.], 0.2),
            key(0.5, [1., 0.5, 0., 1.], 0.4),
            key(1., [1., 0., 0., 0.], 0.1),
        ],
    );
    spark.alpha_blend = false;
    spark.gravity = 1.5;
    spark.drag = 0.3;
    spark.lifetime = 0.8;
    spark.lifetime_variance = 0.3;
    let mut puff = particle(
        "puff",
        vec![
            key(0., [1., 1., 1., 0.], 1.5),
            key(0.3, [1., 1., 1., 0.4], 2.),
            key(1., [1., 1., 1., 0.], 1.8),
        ],
    );
    puff.wind = 0.5;
    puff.lifetime = 3.;
    let mut glow = particle(
        "glow",
        vec![
            key(0., [1., 1., 1., 0.6], 0.8),
            key(1., [1., 1., 1., 0.2], 1.),
        ],
    );
    glow.alpha_blend = false;
    l.particles.extend([spark, puff, glow]);

    let mut sparks = emitter("sparks", "Sparks", &["spark", "particle"]);
    sparks.theta_degrees = [0., 90.];
    sparks.speed_variance = 1.;
    sparks.period_variance = 0.05;
    sparks.lifetime = 1.;
    let mut painted_puff = emitter("painted_puff", PAINTED_EMITTERS[0], &["puff"]);
    painted_puff.speed = 0.;
    painted_puff.period = 0.2;
    painted_puff.use_emitter_colors = true;
    let mut painted_glow = emitter("painted_glow", PAINTED_EMITTERS[1], &["glow", "puff"]);
    painted_glow.speed = 0.5;
    painted_glow.use_emitter_colors = true;
    painted_glow.use_emitter_sizes = true;
    let mut orbit = emitter("orbit", "Orbit", &["particle"]);
    orbit.orient = true;
    orbit.orient_on_velocity = false;
    orbit.phi_rate_degrees = 45.;
    l.emitters
        .extend([sparks, painted_puff, painted_glow, orbit]);

    l.lights.push(Light {
        id: "pulse".into(),
        name: "Pulse".into(),
        enabled: true,
        color: [0.2, 0.4, 1.],
        brightness: 1.,
        radius: 8.,
        color_curves: None,
        brightness_curve: Some(Curve {
            period: 0.5,
            linear: true,
            values: vec![0.2, 1., 0.2],
        }),
        radius_curve: None,
        flare: None,
    });

    let mut manifest = manifest();
    manifest.composites = vec![
        Composite {
            id: "explosion".into(),
            lifetime: 0.5,
            emitters: vec!["sparks".into(), "emitter".into()],
            light: Some("light".into()),
            burst: Some(("sparks".into(), 12, 0.5)),
        },
        Composite {
            id: "smoke".into(),
            lifetime: 2.,
            emitters: vec!["painted_puff".into()],
            light: None,
            burst: None,
        },
    ];
    pack_from(l, manifest)
}
