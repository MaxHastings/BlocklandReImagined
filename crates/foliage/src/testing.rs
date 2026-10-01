//! A made-up foliage pack for tests that have no converted content: two
//! definitions (a dense swaying grass and a sparse fixed-aspect shrub) and
//! two cut-out textures drawn in code. No value here comes from an original
//! installation.
use crate::{Definition, Evidence, FoliagePack, Image, Texture};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

/// Index of the dense, swaying, flickering grass in [`pack`].
pub const GRASS: usize = 0;
/// Index of the sparse fixed-aspect shrub in [`pack`].
pub const SHRUB: usize = 1;
/// Texture edge length in pixels.
pub const TEXTURE_SIZE: u32 = 32;

fn evidence() -> Evidence {
    Evidence {
        source: "fixture".into(),
        sha256: String::new(),
        line: 1,
        fields: BTreeMap::new(),
        adaptations: Vec::new(),
    }
}

/// The dense grass: random sizes, swaying and flickering, faded out
/// between 50 and 66 units.
pub fn grass() -> Definition {
    Definition {
        id: "fixture.grass".into(),
        scene: "fixture.scene".into(),
        node: 3,
        texture: 0,
        origin: [12., 0., -20.],
        seed: 4242,
        count: 12000,
        retries: 3,
        inner: [0., 0.],
        outer: [120., 120.],
        square: false,
        offset: 0.,
        allowed_slope: 40.,
        allow_terrain: true,
        allow_interior: false,
        allow_static: false,
        allow_water: false,
        water_surface: false,
        width: [0.8, 1.6],
        height: [1., 2.5],
        fixed_size: false,
        fixed_aspect: false,
        flip: true,
        billboard: false,
        random_rotation: true,
        rotation: 0.,
        sway: true,
        sway_sync: false,
        sway_magnitude: [0.15, 0.05],
        sway_seconds: [2., 5.],
        light: true,
        light_sync: false,
        light_seconds: 3.,
        luminance: [0.6, 1.],
        color_top: [1.; 4],
        color_bottom: [0.4, 0.5, 0.4, 1.],
        ground_alpha: 1.,
        alpha_cutoff: 0.3,
        closest: 0.,
        distance: 50.,
        fade_far: 16.,
        fade_near: 0.,
        cull_size: 16.,
        culling: true,
        hidden: false,
        evidence: evidence(),
    }
}

/// The sparse shrub: square sprites (`fixed_aspect`) from 0.75 to 6 units
/// tall, never on interiors or water, invisible closer than 2 units.
pub fn shrub() -> Definition {
    Definition {
        id: "fixture.shrub".into(),
        node: 4,
        texture: 1,
        origin: [-15., 0., 25.],
        seed: 777,
        count: 400,
        retries: 4,
        outer: [80., 90.],
        width: [0.75, 6.],
        height: [0.75, 6.],
        fixed_aspect: true,
        flip: false,
        billboard: true,
        random_rotation: false,
        sway_magnitude: [0.3, 0.2],
        sway_seconds: [1., 3.],
        light_seconds: 5.,
        luminance: [0.5, 0.9],
        closest: 2.,
        distance: 60.,
        fade_far: 24.,
        cull_size: 32.,
        ..grass()
    }
}

/// The two definitions, at [`GRASS`] and [`SHRUB`].
pub fn definitions() -> Vec<Definition> {
    vec![grass(), shrub()]
}

/// Draws texture `index`: a transparent background with an opaque cut-out
/// (blades for 0, a round bush otherwise).
pub fn image(index: usize) -> Image {
    let n = TEXTURE_SIZE;
    let mut rgba = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32 - 0.5, y as f32 / n as f32);
            let inside = if index == 0 {
                // Three blades narrowing towards the top (v = 0).
                [-0.25f32, 0., 0.25]
                    .iter()
                    .any(|c| (u - c).abs() < 0.08 * v)
            } else {
                u * u + (v - 0.55) * (v - 0.55) < 0.16
            };
            let shade = (60. + 150. * v) as u8;
            rgba.extend_from_slice(&if inside {
                [shade / 3, shade, shade / 4, 255]
            } else {
                [0, 0, 0, 0]
            });
        }
    }
    Image {
        width: n,
        height: n,
        rgba,
    }
}

/// [`image`] for every texture of [`pack`].
pub fn images() -> Vec<Image> {
    (0..2).map(image).collect()
}

/// [`image`] `index` encoded as PNG.
pub fn png(index: usize) -> Vec<u8> {
    let i = image(index);
    let mut bytes = Vec::new();
    image::write_buffer_with_format(
        &mut std::io::Cursor::new(&mut bytes),
        &i.rgba,
        i.width,
        i.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .unwrap();
    bytes
}

/// The pack, with texture records naming `textures/<n>.png` and the hash of
/// [`png`]`(n)`.
pub fn pack() -> FoliagePack {
    let p = FoliagePack {
        schema_version: 1,
        definitions: definitions(),
        textures: (0..2)
            .map(|i| Texture {
                id: format!("fixture.texture.{i}"),
                path: format!("textures/{i}.png"),
                sha256: format!("{:x}", Sha256::digest(png(i))),
                source: "fixture".into(),
            })
            .collect(),
        source_bundle: "fixture-bundle".into(),
    };
    p.validate().unwrap();
    p
}

/// Writes [`pack`] as `dir/foliage.json` with its textures beside it, the
/// layout of a converted foliage pack, and returns it.
pub fn write_pack(dir: &Path) -> Result<FoliagePack> {
    let p = pack();
    std::fs::create_dir_all(dir.join("textures"))?;
    for (i, t) in p.textures.iter().enumerate() {
        std::fs::write(dir.join(&t.path), png(i))?;
    }
    std::fs::write(dir.join("foliage.json"), serde_json::to_vec_pretty(&p)?)?;
    Ok(p)
}
