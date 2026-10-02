//! Map lighting recovery (`bri_render::map_lighting`) against the stock maps.
use anyhow::{Context, Result};
use bri_render::{
    map_lighting::{Bake, MapLight},
    scene::{DECOMPOSED_LIGHTMAP, Material, MeshBatch, SceneData, SceneImage, SceneVertex},
    scene_loader::load_map_bundle,
};
use glam::Vec3;
use std::{path::PathBuf, sync::Arc};

/// A closed room from -10 to 10 on every axis, faces pointing in, each face
/// with its own 64x64 lightmap holding what `light` casts on it (the map
/// compiler's model: every facing surface lit, linear falloff).
fn lit_room(light: MapLight) -> SceneData {
    const SIZE: u32 = 64;
    let mut scene = SceneData {
        // Sun straight down: the closed room keeps it out.
        sun_direction: [0.0, -1.0, 0.0],
        ..Default::default()
    };
    for axis in 0..3 {
        for side in [-1.0f32, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let corner = |a: f32, b: f32| {
                let mut p = Vec3::ZERO;
                p[axis] = 10.0 * side;
                p[u] = 10.0 * a;
                p[v] = 10.0 * b;
                p
            };
            let mut normal = Vec3::ZERO;
            normal[axis] = -side;
            let mut rgba = Vec::new();
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let a = (x as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                    let b = (y as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                    let c = light.shade(corner(a, b), normal).min(Vec3::ONE) * 255.0 + 0.5;
                    rgba.extend([c.x as u8, c.y as u8, c.z as u8, 255]);
                }
            }
            let image = scene.images.len();
            let base = SceneImage {
                label: format!("face {axis} {side}"),
                width: SIZE,
                height: SIZE,
                rgba,
                srgb: false,
            };
            scene.images.push(base.clone());
            scene.lightmap_bases.push((image, Arc::new(base)));
            let mut material = Material::surface(format!("face {axis} {side}"), 0, image);
            material.parameters = Some(DECOMPOSED_LIGHTMAP);
            let first = scene.vertices.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                scene.vertices.push(SceneVertex {
                    position: corner(a, b).to_array(),
                    normal: normal.to_array(),
                    uv: [0.0; 2],
                    lightmap_uv: [(a + 1.0) * 0.5, (b + 1.0) * 0.5],
                    color: [1.0; 4],
                    fx: [0.0; 4],
                });
            }
            let start = scene.indices.len() as u32;
            scene.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
            scene.batches.push(MeshBatch {
                indices: start..start + 6,
                material: scene.materials.len(),
                center: [0.0; 3],
            });
            scene.materials.push(material);
        }
    }
    scene
}

#[test]
fn a_baked_point_light_is_recovered_from_its_lightmaps() {
    let truth = MapLight {
        position: [3.0, 4.0, -2.0],
        color: [0.9, 0.7, 0.5],
        inner: 5.0,
        outer: 25.0,
        channel: None,
    };
    let lit = Bake::new(&lit_room(truth))
        .expect("lightmapped room")
        .bake(1.0, 50_000, 1.0, 50_000);
    let light = lit.lights.first().copied().expect("a light");
    // The falloff grid has 5 inner and 5 + 20 outer among its choices.
    assert!(
        Vec3::from(light.position).distance(Vec3::from(truth.position)) < 1.0,
        "{light:?}"
    );
    assert!(
        (Vec3::from(light.color) - Vec3::from(truth.color))
            .abs()
            .max_element()
            < 0.08,
        "{light:?}"
    );
    assert_eq!((light.inner, light.outer), (truth.inner, truth.outer));
    // Position steps stop at half a unit: a level or so off.
    assert!(
        lit.report.mean < 2.0 && lit.report.unlit_mean > 20.0,
        "{:?}",
        lit.report
    );
    let channel = light.channel.expect("the only light gets a channel");
    // Its visibility: seen across the room, the sun kept out by the ceiling.
    let v = &lit.visibility;
    let cell = |p: Vec3| {
        let i = ((p - Vec3::from(v.origin)) / v.cell).as_uvec3();
        v.texels[((i.z * v.dims[1] + i.y) * v.dims[0] + i.x) as usize]
    };
    let near = cell(Vec3::new(-5.0, -5.0, 5.0));
    assert_eq!(near[0], 0, "sun inside a closed room");
    assert_eq!(near[1 + channel as usize], 255);
}

fn content() -> PathBuf {
    std::env::var_os("BRI_CONTENT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

/// The client's bake settings (`MapLightingState` in the client app).
const MIN_CELL: f32 = 2.0;
const MAX_CELLS: usize = 1_000_000;
const VIS_CELL: f32 = 2.0;
const VIS_CELLS: usize = 2_000_000;

/// Fits lights to every map in `bundle` (or those `BRI_MAP` names), checks
/// the stored bake round-trips and never fits worse than no lights, and
/// hands each fit to `check`. Returns how many maps had lightmaps.
fn maps_fit_lights_that_explain_their_lightmaps(
    bundle: &std::path::Path,
    check: impl Fn(&str, &bri_render::map_lighting::MapLighting),
) -> Result<usize> {
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("bundle.json"))?)?;
    let only = std::env::var("BRI_MAP").ok();
    let mut seen = std::collections::BTreeSet::new();
    for record in index["maps"].as_array().context("maps")? {
        let id = record["id"].as_str().context("id")?;
        if only.as_deref().is_some_and(|m| !id.contains(m)) {
            continue;
        }
        let map = load_map_bundle(bundle, id)?;
        let Some(bake) = Bake::new(&map.scene) else {
            eprintln!("{id}: no interior lightmaps");
            continue;
        };
        let key = bake.key();
        let lit = bake.bake(MIN_CELL, MAX_CELLS, VIS_CELL, VIS_CELLS);
        let r = &lit.report;
        eprintln!(
            "{id}: {} lights ({} with channels) in {:.1} s; {} lexels ({} lit): mean error {:.2} (unlit {:.2}), rms {:.2} (unlit {:.2}), lit mean {:.2}, channel-only mean {:.2}; visibility {:?} cells of {:.2}",
            lit.lights.len(),
            lit.lights.iter().filter(|l| l.channel.is_some()).count(),
            r.seconds,
            r.lexels,
            r.lit_lexels,
            r.mean,
            r.unlit_mean,
            r.rms,
            r.unlit_rms,
            r.lit_mean,
            r.channel_mean,
            lit.visibility.dims,
            lit.visibility.cell,
        );
        for l in &lit.lights {
            eprintln!("  {l:?}");
        }
        // Stored bakes read back only under their own key.
        let bytes = lit.to_bytes(key);
        let stored = bri_render::map_lighting::MapLighting::from_bytes(&bytes, key)
            .context("stored bake")?;
        assert!(
            stored.lights == lit.lights
                && stored.report == lit.report
                && stored.visibility == lit.visibility
        );
        assert_eq!(stored.residual.texels, lit.residual.texels);
        assert!(bri_render::map_lighting::MapLighting::from_bytes(&bytes, [0; 32]).is_none());
        // Lights never make the fit worse than no lights at all.
        assert!(r.mean <= r.unlit_mean + 1e-3, "{id}");
        check(id, &lit);
        seen.insert(id.to_string());
    }
    Ok(seen.len())
}

#[test]
fn fixture_maps_fit_lights_that_explain_their_lightmaps() -> Result<()> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("map-lighting-{}", std::process::id()));
    let maps = bri_render::testing::rooms();
    bri_render::testing::write_bundle(&dir, &maps)?;
    let fitted = maps_fit_lights_that_explain_their_lightmaps(&dir, |id, lit| {
        // The one lamp baked into the room's lightmaps is found within two
        // lightmap texels of where it was, in about its colour.
        let room = maps.iter().find(|m| m.id == id).unwrap();
        let (lamp, texel) = (
            room.lamp,
            2.0 * room.half / bri_render::testing::LIGHTMAP_SIZE as f32,
        );
        assert!(
            lit.lights.iter().any(|l| {
                Vec3::from(l.position).distance(Vec3::from(lamp.position)) < 2.0 * texel
                    && (Vec3::from(l.color) - Vec3::from(lamp.color))
                        .abs()
                        .max_element()
                        < 0.15
            }),
            "{id}: {:?} vs {lamp:?}",
            lit.lights
        );
        assert!(
            lit.report.mean < lit.report.unlit_mean,
            "{id}: {:?}",
            lit.report
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
    if std::env::var_os("BRI_MAP").is_none() {
        assert_eq!(fitted?, maps.len());
    }
    Ok(())
}

#[test]
#[ignore = "requires generated v20 content"]
fn stock_maps_fit_lights_that_explain_their_lightmaps() -> Result<()> {
    maps_fit_lights_that_explain_their_lightmaps(&content().join("map-bundle-017"), |id, lit| {
        // Kitchen's stove glows orange, dimmer than the white lights round
        // it; the fit finds it (and its error then drops below 9 levels).
        if id.ends_with("/kitchen.mis") {
            assert!(
                lit.lights.iter().any(|l| l.color[0] > 0.4
                    && l.color[2] < 0.02
                    && l.color[0] > 1.8 * l.color[1]),
                "{:?}",
                lit.lights
            );
            assert!(lit.report.mean < 9.0, "{}", lit.report.mean);
        }
    })?;
    Ok(())
}

/// Adds a lightmapped quad facing +Y at height `y` over `-half..half` in x
/// and z, lit by `texel(x, y)` of a 64x64 lightmap, with its decomposition
/// (no baked sun). Returns the drawn lightmap's image index.
fn add_floor(scene: &mut SceneData, y: f32, half: f32, texel: impl Fn(u32, u32) -> u8) -> usize {
    const SIZE: u32 = 64;
    let mut rgba = Vec::new();
    for ty in 0..SIZE {
        for tx in 0..SIZE {
            let c = texel(tx, ty);
            rgba.extend([c, c, c, 255]);
        }
    }
    let base = SceneImage {
        label: format!("floor {y}"),
        width: SIZE,
        height: SIZE,
        rgba,
        srgb: false,
    };
    let image = scene.images.len();
    scene.images.push(base.clone());
    let mut parts = base.clone();
    parts.rgba.chunks_exact_mut(4).for_each(|t| t[3] = 0);
    scene.images.push(parts);
    scene.lightmap_bases.push((image, Arc::new(base)));
    let mut material = Material::surface(format!("floor {y}"), 0, image);
    material.images[9] = image + 1;
    material.parameters = Some(DECOMPOSED_LIGHTMAP);
    let first = scene.vertices.len() as u32;
    for (a, b) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, 1.0), (1.0, -1.0)] {
        scene.vertices.push(SceneVertex {
            position: [a * half, y, b * half],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
            lightmap_uv: [(a + 1.0) * 0.5, (b + 1.0) * 0.5],
            color: [1.0; 4],
            fx: [0.0; 4],
        });
    }
    let start = scene.indices.len() as u32;
    scene.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
    scene.batches.push(MeshBatch {
        indices: start..start + 6,
        material: scene.materials.len(),
        center: [0.0; 3],
    });
    scene.materials.push(material);
    image
}

/// The map compiler's light leaking through a sealed floor: a thin bright
/// strip on a floor under a closed, lit room, which the room's light cannot
/// reach past the room's own floor. The cleanup takes the strip away, and
/// keeps a thin line of light the room's light really reaches and thin
/// lines inside a lit patch.
#[test]
fn thin_light_leaks_through_sealed_walls_are_cleaned_up() {
    let light = MapLight {
        position: [3.0, 4.0, -2.0],
        color: [0.9, 0.7, 0.5],
        inner: 5.0,
        outer: 25.0,
        channel: None,
    };
    let mut scene = lit_room(light);
    // Under the room's floor (y = -10): dark but for a two-texel strip.
    let under = add_floor(&mut scene, -12.0, 10.0, |_, y| {
        if (31..33).contains(&y) { 60 } else { 0 }
    });
    // Inside the room, just over its floor, lit as the light lights it with a
    // thin line brighter still (a real bright trim).
    let inside = add_floor(&mut scene, -9.9, 8.0, |x, y| {
        let p = Vec3::new(
            ((x as f32 + 0.5) / 64.0 * 2.0 - 1.0) * 8.0,
            -9.9,
            ((y as f32 + 0.5) / 64.0 * 2.0 - 1.0) * 8.0,
        );
        let lit = light.shade(p, Vec3::Y).x * 255.0;
        (lit + if (31..33).contains(&y) { 40.0 } else { 0.0 }).min(255.0) as u8
    });
    // Farther under, a broad lit patch (as the fit may leave unexplained: a
    // window's sun patch, a stove's glow) crossed by brighter thin lines.
    let patch = add_floor(&mut scene, -14.0, 10.0, |x, y| match (x, y) {
        (_, 31..33) | (31..33, _) if (16..48).contains(&x) && (16..48).contains(&y) => 110,
        (16..48, 16..48) => 80,
        _ => 0,
    });
    let lit = Bake::new(&scene)
        .expect("lightmapped room")
        .bake(1.0, 50_000, 1.0, 50_000);
    let fixed = |image: usize| {
        lit.leaks
            .iter()
            .filter(|f| f.image as usize == image)
            .collect::<Vec<_>>()
    };
    // The patch and its lines are light too, whatever the walls say.
    assert!(fixed(patch).is_empty(), "{}", fixed(patch).len());
    // Every strip texel under the floor goes dark, in the drawn lightmap and
    // its decomposition, and nothing else changes there.
    let under_fixes = fixed(under);
    assert_eq!(under_fixes.len(), 2 * 64, "{}", under_fixes.len());
    assert!(
        under_fixes
            .iter()
            .all(|f| f.rgba[..3] == [0, 0, 0] && (31..33).contains(&(f.index / 64)))
    );
    assert_eq!(fixed(under + 1).len(), 2 * 64);
    // The room's light reaches the trim: it stays.
    assert!(fixed(inside).is_empty(), "{:?}", fixed(inside).len());
    assert_eq!(lit.report.leak_texels, 2 * 64);
    // Applying them to the scene changes exactly those images.
    let mut images = scene.images.clone();
    assert_eq!(
        bri_render::map_lighting::TexelFix::apply(&lit.leaks, &mut images),
        vec![under, under + 1]
    );
}

/// Baked sun through a seam: a thin strip of sun share on a floor the
/// room above hides from the (overhead) sun loses it, and the drawn
/// lightmap loses that sun.
#[test]
fn thin_sun_leaks_under_a_closed_room_are_cleaned_up() {
    let light = MapLight {
        position: [3.0, 4.0, -2.0],
        color: [0.9, 0.7, 0.5],
        inner: 5.0,
        outer: 25.0,
        channel: None,
    };
    let mut scene = lit_room(light);
    let under = add_floor(&mut scene, -12.0, 10.0, |_, _| 20);
    // The strip: static 20 plus sun (drawn 100, sun share 1).
    for y in 31..33u32 {
        for x in 0..64u32 {
            let i = ((y * 64 + x) * 4) as usize;
            scene.images[under].rgba[i..i + 3].copy_from_slice(&[100; 3]);
            scene.images[under + 1].rgba[i + 3] = 255;
        }
    }
    let lit = Bake::new(&scene)
        .expect("lightmapped room")
        .bake(1.0, 50_000, 1.0, 50_000);
    let drawn: Vec<_> = lit
        .leaks
        .iter()
        .filter(|f| f.image as usize == under)
        .collect();
    let parts: Vec<_> = lit
        .leaks
        .iter()
        .filter(|f| f.image as usize == under + 1)
        .collect();
    assert_eq!((drawn.len(), parts.len()), (2 * 64, 2 * 64));
    assert!(
        drawn.iter().all(|f| f.rgba[..3] == [20; 3]),
        "{:?}",
        drawn[0]
    );
    assert!(
        parts.iter().all(|f| f.rgba == [20, 20, 20, 0]),
        "{:?}",
        parts[0]
    );
}

/// The Dynamic mode's sheets: each decomposed sheet less every recovered
/// light, so the light alone leaves nothing (a level or so of fit error),
/// and the leftover (here 0.1 of ambient on every texel) stays; per texel,
/// the share of each light that reaches it. Texels just past a surface's
/// edge (which bilinear filtering blends into it) are lit as the edge is,
/// so no seam shows; texels farther out stay as they were. The sheets equip
/// the map's materials and survive a stored bake.
#[test]
fn dynamic_sheets_keep_only_the_light_no_recovered_light_explains() {
    let truth = MapLight {
        position: [3.0, 4.0, -2.0],
        color: [0.6, 0.5, 0.4],
        inner: 5.0,
        outer: 25.0,
        channel: None,
    };
    let mut scene = lit_room(truth);
    // The first wall's lightmap covers only the middle of its sheet
    // (texels 16..48 each way).
    for v in &mut scene.vertices[..4] {
        v.lightmap_uv = v.lightmap_uv.map(|c| 0.25 + c * 0.5);
    }
    // Its light, texels past the edge carrying on (x = -10, facing +x).
    let image = scene.materials[0].images[8];
    let base = &mut scene.images[image];
    for y in 0..64 {
        for x in 0..64 {
            let at = |t: usize| ((t as f32 + 0.5) / 64.0 - 0.25) * 4.0 - 1.0;
            let c = truth
                .shade(Vec3::new(-10.0, 10.0 * at(x), 10.0 * at(y)), Vec3::X)
                .min(Vec3::ONE)
                * 255.0
                + 0.5;
            base.rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3]
                .copy_from_slice(&[c.x as u8, c.y as u8, c.z as u8]);
        }
    }
    scene.lightmap_bases[0].1 = Arc::new(scene.images[image].clone());
    for m in 0..scene.materials.len() {
        let lightmap = scene.materials[m].images[8];
        let mut parts = scene.images[lightmap].clone();
        for t in parts.rgba.chunks_exact_mut(4) {
            for c in &mut t[..3] {
                *c = c.saturating_add(26);
            }
            t[3] = 0;
        }
        scene.images.push(parts);
        scene.materials[m].images[9] = scene.images.len() - 1;
    }
    let bake = Bake::new(&scene).expect("lightmapped room");
    let key = bake.key();
    let lit = bake.bake(1.0, 50_000, 1.0, 50_000);
    assert_eq!(lit.lights.len(), 1);
    assert_eq!(lit.dynamic.len(), 6);
    for (m, sheet) in scene.materials.iter().zip(&lit.dynamic) {
        assert_eq!(sheet.parts_image as usize, m.images[9]);
        assert_eq!(
            (sheet.lights.as_slice(), sheet.visibility.len()),
            ([0u8].as_slice(), 1)
        );
        let reached: Vec<usize> = (0..sheet.left.len() / 4)
            .filter(|&i| sheet.visibility[0][i * 4] > 0)
            .collect();
        assert!(
            reached.len() >= 32 * 32,
            "sheet {}: {}",
            sheet.parts_image,
            reached.len()
        );
        let mean = reached
            .iter()
            .map(|&i| sheet.left[i * 4] as f32)
            .sum::<f32>()
            / reached.len() as f32;
        assert!(
            (mean - 26.0).abs() < 3.0,
            "sheet {}: {mean}",
            sheet.parts_image
        );
        assert!(sheet.left.chunks_exact(4).all(|t| t[3] == 0));
    }
    // The first wall: inside its lightmap the light arrives whole; half a
    // texel past its edge it is lit too; two and a half texels out it is
    // not, and keeps its decomposition.
    let first = &lit.dynamic[0];
    let texel = |x: usize| 32 * 64 + x;
    assert!(
        first.share(first.lights[0], texel(24)) > 0.94,
        "{}",
        first.share(first.lights[0], texel(24))
    );
    assert!(first.visibility[0][texel(15) * 4] > 0 && first.visibility[0][texel(48) * 4] > 0);
    assert_eq!(first.visibility[0][texel(13) * 4], 0);
    let parts = &scene.images[scene.materials[0].images[9]].rgba;
    assert_eq!(
        first.left[texel(13) * 4..texel(13) * 4 + 4],
        parts[texel(13) * 4..texel(13) * 4 + 4]
    );
    // The only light has a channel, so objects' residual is the same.
    assert_eq!(lit.residual_all, lit.residual);
    // Equipped, each material draws its sheet's leftover light and
    // visibility, and names its light.
    let mut equipped = scene.clone();
    assert!(bri_render::map_lighting::DynamicSheet::equip(
        &lit.dynamic,
        &mut equipped
    ));
    assert_eq!(equipped.images.len(), scene.images.len() + 12);
    for (m, sheet) in equipped.materials.iter().zip(&lit.dynamic) {
        assert_eq!(equipped.images[m.images[10]].rgba, sheet.left);
        assert_eq!(equipped.images[m.images[1]].rgba, sheet.visibility[0]);
        let p = m.parameters.expect("decomposed");
        assert_eq!((p[0][0], p[0][1], p[1][0]), (1.0, 1.0, 0.0));
    }
    let stored = bri_render::map_lighting::MapLighting::from_bytes(&lit.to_bytes(key), key)
        .expect("stored bake");
    // (A loaded volume casts no rays.)
    assert_eq!(
        (&stored.lights, &stored.visibility, &stored.dynamic),
        (&lit.lights, &lit.visibility, &lit.dynamic)
    );
    assert_eq!(stored.residual_all.texels, lit.residual_all.texels);
    assert_eq!(stored.residual.texels, lit.residual.texels);
}

/// Switching a light off in the Dynamic mode takes away exactly the light it
/// baked, even where the bake's rays disagree with the map compiler: a slab
/// the compiler never saw (a shape, like the Bedroom lamp's shade) hides the
/// light from part of the floor that its lightmap shows lit, and part of a
/// wall is dark in its lightmap where nothing blocks the rays. With the light
/// off, every texel keeps only the compiler's ambient (20 levels, which no
/// light takes) and the sun's ambient (6), so the light's baked shadow does
/// not turn darker than the room and nothing it lit stays lit.
#[test]
fn a_switched_off_light_leaves_only_ambient_where_rays_and_lightmap_disagree() {
    let truth = MapLight {
        position: [3.0, 4.0, -2.0],
        color: [0.6, 0.5, 0.4],
        inner: 5.0,
        outer: 25.0,
        channel: None,
    };
    let mut scene = lit_room(truth);
    // The compiler's ambient everywhere, and its shadow on the +x wall.
    for m in 0..scene.materials.len() {
        let image = scene.materials[m].images[8];
        for (i, t) in scene.images[image].rgba.chunks_exact_mut(4).enumerate() {
            let shadowed = m == 1 && (20..30).contains(&(i % 64));
            for c in &mut t[..3] {
                *c = if shadowed { 20 } else { c.saturating_add(20) };
            }
        }
        scene.lightmap_bases[m].1 = Arc::new(scene.images[image].clone());
    }
    for m in 0..scene.materials.len() {
        let mut parts = scene.images[scene.materials[m].images[8]].clone();
        for t in parts.rgba.chunks_exact_mut(4) {
            for c in &mut t[..3] {
                *c = c.saturating_add(6);
            }
            t[3] = 0;
        }
        scene.images.push(parts);
        scene.materials[m].images[9] = scene.images.len() - 1;
    }
    // Two plain slabs (no lightmap) over the floor (y = -10): one the
    // compiler never saw, high up; one it did, a low table the floor under
    // which holds only its ambient.
    let floor_image = scene.materials[2].images[8];
    let parts_image = scene.materials[2].images[9];
    for (row, column) in (0..64).flat_map(|row| (0..64).map(move |column| (row, column))) {
        // A floor texel's column follows z, its row x.
        let at = |t: usize| (t as f32 + 0.5) / 64.0 * 20.0 - 10.0;
        if (4.0..8.0).contains(&at(row)) && (-8.0..-4.0).contains(&at(column)) {
            let i = (row * 64 + column) * 4;
            scene.images[floor_image].rgba[i..i + 3].copy_from_slice(&[20; 3]);
            scene.images[parts_image].rgba[i..i + 3].copy_from_slice(&[26; 3]);
        }
    }
    scene.lightmap_bases[2].1 = Arc::new(scene.images[floor_image].clone());
    for (y, corners) in [
        (-5.0, [(-8.0, 2.0), (-2.0, 2.0), (-2.0, 8.0), (-8.0, 8.0)]),
        (-9.9, [(4.0, -8.0), (8.0, -8.0), (8.0, -4.0), (4.0, -4.0)]),
    ] {
        let first = scene.vertices.len() as u32;
        for (x, z) in corners {
            scene.vertices.push(SceneVertex {
                position: [x, y, z],
                normal: [0.0, -1.0, 0.0],
                uv: [0.0; 2],
                lightmap_uv: [0.0; 2],
                color: [1.0; 4],
                fx: [0.0; 4],
            });
        }
        let start = scene.indices.len() as u32;
        scene.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
        scene.batches.push(MeshBatch {
            indices: start..start + 6,
            material: scene.materials.len(),
            center: [0.0; 3],
        });
        scene.materials.push(Material::surface("slab", 0, 0));
    }
    let lit = Bake::new(&scene)
        .expect("lightmapped room")
        .bake(1.0, 50_000, 1.0, 50_000);
    assert!(!lit.lights.is_empty());
    // With the lights off each texel draws its leftover alone: all within a
    // few levels of the ambient (the fit is not exact where it had to
    // explain light the rays cannot see).
    let (mut close, mut total, mut worst) = (0, 0, (0, 0, 0));
    for (sheet_index, sheet) in lit.dynamic.iter().enumerate() {
        for (i, t) in sheet.left.chunks_exact(4).enumerate() {
            let off = t[..3]
                .iter()
                .map(|&c| (i32::from(c) - 26).abs())
                .max()
                .unwrap_or(0);
            total += 1;
            close += usize::from(off <= 6);
            if off > worst.0 {
                worst = (off, sheet_index, i);
            }
        }
    }
    assert!(
        worst.0 <= 12,
        "left {} levels off ambient on sheet {} texel {}",
        worst.0,
        worst.1,
        worst.2
    );
    assert!(
        close * 100 >= total * 95,
        "{close} of {total} within 6 levels"
    );
    // Under the high slab the main light takes part of the floor's light
    // (the fit put a small light there for the rest), though the rays say it
    // cannot arrive.
    let floor = &lit.dynamic[2];
    let under = |x: usize, y: usize| y * 64 + x;
    // Floor (axis 1): lightmap u from z, v from x; slab x -8..-2, z 2..8.
    let texel = under(
        ((5.0 + 10.0) / 20.0 * 64.0) as usize,
        ((-5.0 + 10.0) / 20.0 * 64.0) as usize,
    );
    assert!(floor.lights.contains(&0), "the light reaches the floor");
    let share = floor.share(0, texel);
    assert!(share > 0.16, "{share}");
}

/// A slab's shadow on the floor, its edge texels half lit as the map
/// compiler filtered them while a ray from each texel's centre says lit or
/// not. Switching the light off leaves no line of light along the edge.
#[test]
fn a_switched_off_light_leaves_no_line_along_its_shadows_edges() {
    let truth = MapLight {
        position: [3.0, 4.0, -2.0],
        color: [0.6, 0.5, 0.4],
        inner: 5.0,
        outer: 25.0,
        channel: None,
    };
    let mut scene = lit_room(truth);
    let light = Vec3::from(truth.position);
    // The slab at y = 0, 4 units square turned 0.3 radians about y, so its
    // shadow's edges cut across the floor's texels at every angle; the floor
    // (material 2, y = -10) lit by the share of each texel's 4x4 samples
    // that pass it.
    let turn = 0.3f32;
    let floor_image = scene.materials[2].images[8];
    let mut coverage = vec![0u32; 64 * 64];
    for ty in 0..64u32 {
        for tx in 0..64u32 {
            let mut open = 0;
            for s in 0..16u32 {
                let z = ((tx as f32 + (s % 4) as f32 / 4.0 + 0.125) / 64.0 * 2.0 - 1.0) * 10.0;
                let x = ((ty as f32 + (s / 4) as f32 / 4.0 + 0.125) / 64.0 * 2.0 - 1.0) * 10.0;
                let p = Vec3::new(x, -10.0, z);
                let hit = p + (light - p) * (10.0 / (light.y + 10.0));
                let (x, z) = (
                    hit.x * turn.cos() + hit.z * turn.sin(),
                    hit.z * turn.cos() - hit.x * turn.sin(),
                );
                open += u32::from(x.abs() > 2.0 || z.abs() > 2.0);
            }
            coverage[(ty * 64 + tx) as usize] = open;
            let i = ((ty * 64 + tx) * 4) as usize;
            for c in &mut scene.images[floor_image].rgba[i..i + 3] {
                *c = (f32::from(*c) * open as f32 / 16.0 + 0.5) as u8;
            }
        }
    }
    scene.lightmap_bases[2].1 = Arc::new(scene.images[floor_image].clone());
    for m in 0..scene.materials.len() {
        let mut parts = scene.images[scene.materials[m].images[8]].clone();
        parts.rgba.chunks_exact_mut(4).for_each(|t| t[3] = 0);
        scene.images.push(parts);
        scene.materials[m].images[9] = scene.images.len() - 1;
    }
    let first = scene.vertices.len() as u32;
    for (x, z) in [(-2.0f32, -2.0f32), (2.0, -2.0), (2.0, 2.0), (-2.0, 2.0)] {
        scene.vertices.push(SceneVertex {
            position: [
                x * turn.cos() - z * turn.sin(),
                0.0,
                z * turn.cos() + x * turn.sin(),
            ],
            normal: [0.0, -1.0, 0.0],
            uv: [0.0; 2],
            lightmap_uv: [0.0; 2],
            color: [1.0; 4],
            fx: [0.0; 4],
        });
    }
    let start = scene.indices.len() as u32;
    scene.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
    scene.batches.push(MeshBatch {
        indices: start..start + 6,
        material: scene.materials.len(),
        center: [0.0; 3],
    });
    scene.materials.push(Material::surface("slab", 0, 0));
    let lit = Bake::new(&scene)
        .expect("lightmapped room")
        .bake(1.0, 50_000, 1.0, 50_000);
    let floor = lit
        .dynamic
        .iter()
        .find(|s| s.parts_image as usize == scene.materials[2].images[9])
        .expect("the floor's sheet");
    // Where the light all but missed a texel (a quarter of it or less),
    // nothing of it stays; elsewhere the fit's error, a few levels.
    let mut edge = (0, 0);
    let mut worst = (0, 0);
    for (i, t) in floor.left.chunks_exact(4).enumerate() {
        let left = t[..3].iter().copied().max().unwrap_or(0);
        if (1..=4).contains(&coverage[i]) {
            edge = edge.max((left, i));
        }
        worst = worst.max((left, i));
    }
    assert!(
        edge.0 <= 3,
        "a shadow's edge keeps {} levels at texel {} with the light off",
        edge.0,
        edge.1
    );
    assert!(
        worst.0 <= 8,
        "the floor keeps {} levels at texel {} with the light off",
        worst.0,
        worst.1
    );
}
