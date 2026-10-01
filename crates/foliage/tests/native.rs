#[macro_use]
mod common;
use bri_foliage::*;
use common::*;
use glam::Vec3;

fn textures_decode_with_embedded_alpha(f: &Fixture) {
    let images = f.images();
    assert_eq!(images.len(), f.pack.textures.len());
    for image in images {
        assert!(image.rgba.chunks_exact(4).any(|p| p[3] == 0));
        assert!(image.rgba.chunks_exact(4).any(|p| p[3] > 200));
    }
}
on_both!(
    textures_decode_with_embedded_alpha_synthetic,
    textures_decode_with_embedded_alpha_content,
    textures_decode_with_embedded_alpha
);
#[test]
#[ignore = "requires generated v20 content"]
fn original_inventory() {
    let p = Fixture::content().pack;
    assert_eq!(p.definitions.len(), 2);
    assert_eq!(p.definitions.iter().map(|d| d.count).sum::<u32>(), 41000);
    assert_eq!(p.definitions[1].height, [1., 10.]);
    assert!(p.definitions[1].fixed_aspect);
}

fn bounded_retries_completion_and_cancellation(f: &Fixture) {
    let mut d = f.grass();
    d.count = 4;
    d.retries = 3;
    let mut b = PlacementBuilder::new(d).unwrap();
    assert!(!b.advance(5, |_| None).unwrap().completed);
    assert_eq!(b.stats().queries, 5);
    assert!(b.advance(65537, |_| None).is_err());
    assert!(b.advance(7, |_| None).unwrap().completed);
    assert_eq!(b.stats().rejected, 4);
    assert_eq!(b.finish().unwrap().plants().len(), 0);
}
on_both!(
    bounded_retries_completion_and_cancellation_synthetic,
    bounded_retries_completion_and_cancellation_content,
    bounded_retries_completion_and_cancellation
);

fn deterministic_chunking_aspect_and_range(f: &Fixture) {
    let mut d = f.shrub();
    assert!(d.fixed_aspect && !d.square);
    d.count = 1000;
    let a = build(d.clone(), 1);
    let b = build(d.clone(), 4096);
    assert_eq!(a.plants().len(), b.plants().len());
    for (a, b) in a.plants().iter().zip(b.plants()) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.sway_phase, b.sway_phase);
        assert_eq!(a.light_phase, b.light_phase);
        assert_eq!(a.width, a.height);
        assert!((d.height[0]..=d.height[1]).contains(&a.height));
    }
    let p = &a.plants()[0];
    let dx = (p.position.x - a.definition().origin[0]) / d.outer[0];
    let dz = (p.position.z - a.definition().origin[2]) / d.outer[1];
    assert!(dx * dx + dz * dz <= 1.);
}
on_both!(
    deterministic_chunking_aspect_and_range_synthetic,
    deterministic_chunking_original_aspect_and_range,
    deterministic_chunking_aspect_and_range
);

fn surface_occlusion_slope_and_resumable_water_second_ray(f: &Fixture) {
    let mut d = f.shrub();
    assert!(d.allow_terrain && !d.allow_interior && !d.water_surface);
    d.count = 1;
    d.retries = 2;
    let mut b = PlacementBuilder::new(d.clone()).unwrap();
    b.advance(1, |r| {
        Some(SurfaceHit {
            kind: SurfaceKind::Interior,
            ..floor(r).unwrap()
        })
    })
    .unwrap();
    assert_eq!(b.stats().placed, 0);
    b.advance(1, floor).unwrap();
    assert_eq!(b.stats().placed, 1);
    d.allow_water = true;
    let mut b = PlacementBuilder::new(d.clone()).unwrap();
    b.advance(1, |r| {
        Some(SurfaceHit {
            kind: SurfaceKind::Water,
            ..floor(r).unwrap()
        })
    })
    .unwrap();
    assert!(!b.stats().completed);
    b.advance(1, |r| {
        assert!(!r.include_water);
        floor(r)
    })
    .unwrap();
    assert_eq!(b.stats().queries, 2);
    assert_eq!(b.stats().placed, 1);
    d.allowed_slope = 20.;
    let mut b = PlacementBuilder::new(d).unwrap();
    b.advance(2, |r| {
        Some(SurfaceHit {
            normal: Vec3::X,
            ..floor(r).unwrap()
        })
    })
    .unwrap();
    assert_eq!(b.stats().rejected, 1);
}
on_both!(
    surface_occlusion_slope_and_resumable_water_second_ray_synthetic,
    surface_occlusion_slope_and_resumable_water_second_ray_content,
    surface_occlusion_slope_and_resumable_water_second_ray
);

fn culling_is_bounded_and_keeps_plant_extent(f: &Fixture) {
    let field = build(f.grass(), 4096);
    let center = Vec3::from_array(field.definition().origin) * Vec3::new(1., 0., 1.);
    let c = camera(
        center + Vec3::new(0., 6., 30.),
        center + Vec3::new(0., 3., 0.),
    );
    let mut visible = vec![];
    let stats = field.visible(&c, &mut visible).unwrap();
    let total = field.plants().len();
    assert!(
        stats.instances_visible > 0 && stats.instances_visible < total,
        "{stats:?}"
    );
    assert!(stats.instances_tested < total, "{stats:?}");
    assert!(stats.cells_tested < stats.source_cells);
    let ids: std::collections::BTreeSet<_> = visible.iter().collect();
    assert_eq!(ids.len(), visible.len());
    let far = camera(
        center + Vec3::new(5000., 10., 0.),
        center + Vec3::new(5000., 0., -10.),
    );
    assert_eq!(
        field.visible(&far, &mut visible).unwrap().instances_visible,
        0
    );
}
on_both!(
    culling_is_bounded_and_keeps_plant_extent_synthetic,
    culling_is_bounded_and_keeps_plant_extent_content,
    culling_is_bounded_and_keeps_plant_extent
);

fn authored_fade_sway_light_and_invalid_inputs(f: &Fixture) {
    let field = build(f.shrub(), 4096);
    let d = field.definition();
    // At the eye a plant is invisible and it fades in no later than
    // `closest` (at once with no near fade, over `fade_near` otherwise);
    // inside the range it is opaque; the far fade is linear over
    // `fade_far`.
    assert!(d.closest > 0. && d.fade_far > 0.);
    assert_eq!(fade(d, 0.), 0.);
    let mut last = 0.;
    for i in 0..=16 {
        let f = fade(d, d.closest * i as f32 / 16.);
        assert!(f >= last, "fading in: {f} after {last}");
        last = f;
    }
    assert_eq!(fade(d, d.closest), 1.);
    assert_eq!(fade(d, d.distance), 1.);
    assert!((fade(d, d.distance + d.fade_far * 0.5) - 0.5).abs() < 1e-5);
    assert_eq!(fade(d, d.distance + d.fade_far), 0.);
    assert!(d.sway && d.light);
    let p = &field.plants()[0];
    for time in [0., 1., 10., 100.] {
        let (s, l) = motion(d, p, time);
        assert!(s[0].abs() <= d.sway_magnitude[0] + 1e-6);
        assert!(s[1].abs() <= d.sway_magnitude[1] + 1e-6);
        assert!((d.luminance[0] - 1e-6..=d.luminance[1] + 1e-6).contains(&l));
    }
    let mut d = d.clone();
    d.count = u32::MAX;
    assert!(PlacementBuilder::new(d).is_err());
}
on_both!(
    authored_fade_sway_light_and_invalid_inputs_synthetic,
    authored_fade_sway_light_and_invalid_inputs_content,
    authored_fade_sway_light_and_invalid_inputs
);

/// Places every definition against the fixture's world and checks each
/// sampled plant stands on terrain; returns the placement/culling report.
fn terrain_architecture_and_static_collision_probe(f: &Fixture) -> Vec<serde_json::Value> {
    let w = f.world();
    let mut report = vec![];
    for d in f.pack.definitions.clone() {
        let mut b = PlacementBuilder::new(d).unwrap();
        while !b.stats().completed {
            b.advance(2048, |r| trace(&w, r)).unwrap();
        }
        let field = b.finish().unwrap();
        assert!(!field.plants().is_empty());
        let offset = field.definition().offset;
        for plant in field.plants().iter().step_by(200) {
            let hit = trace(
                &w,
                PlacementRay {
                    start: Vec3::new(plant.position.x, 2000., plant.position.z),
                    end: Vec3::new(plant.position.x, -2000., plant.position.z),
                    include_water: true,
                },
            )
            .unwrap();
            assert_eq!(hit.kind, SurfaceKind::Terrain);
            assert!((plant.position.y - offset - hit.position.y).abs() < 0.01);
        }
        let c = camera(
            field.plants()[0].position + Vec3::new(10., 5., 10.),
            field.plants()[0].position,
        );
        let mut indices = vec![];
        let cull = field.visible(&c, &mut indices).unwrap();
        report.push(serde_json::json!({"definition":field.definition().id,"placement":field.placement,"culling":cull}));
    }
    report
}
#[test]
fn terrain_architecture_and_static_collision_probe_synthetic() {
    let f = Fixture::synthetic();
    let report = terrain_architecture_and_static_collision_probe(&f);
    assert_eq!(report.len(), f.pack.definitions.len());
    // The interior and the pillar stand inside both ellipses: some rays hit
    // them and are retried or rejected.
    assert!(
        report
            .iter()
            .any(|r| r["placement"]["queries"].as_u64().unwrap()
                > r["placement"]["placed"].as_u64().unwrap()),
        "{report:?}"
    );
}
#[test]
#[ignore = "requires generated v20 content"]
fn real_bedroom_terrain_architecture_and_static_collision_probe() {
    let start = std::time::Instant::now();
    let report = terrain_architecture_and_static_collision_probe(&Fixture::content());
    let report = serde_json::json!({"schema_version":1,"native_map":"map-bundle-014","total_milliseconds":start.elapsed().as_secs_f64()*1000.,"reports":report,"subjective_parity":false});
    std::fs::write(
        root().join("artifacts/native-foliage/placement-probe.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
