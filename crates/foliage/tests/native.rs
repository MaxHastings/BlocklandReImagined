mod common;
use bri_foliage::*;
use common::*;
use glam::Vec3;
#[test]
#[ignore = "requires private converted foliage/map packs; offscreen only"]
fn original_inventory_and_embedded_alpha() {
    let p = pack();
    assert_eq!(p.definitions.len(), 2);
    assert_eq!(p.definitions.iter().map(|d| d.count).sum::<u32>(), 41000);
    let images = p.images(root().join("content/foliage-pack-001")).unwrap();
    for image in images {
        assert!(image.rgba.chunks_exact(4).any(|p| p[3] == 0));
        assert!(image.rgba.chunks_exact(4).any(|p| p[3] > 200));
    }
    assert_eq!(p.definitions[1].height, [1., 10.]);
    assert!(p.definitions[1].fixed_aspect);
}
#[test]
#[ignore = "requires private converted foliage/map packs; offscreen only"]
fn bounded_retries_completion_and_cancellation() {
    let mut d = pack().definitions[0].clone();
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
#[test]
#[ignore = "requires private converted foliage/map packs; offscreen only"]
fn deterministic_chunking_original_aspect_and_range() {
    let mut d = pack().definitions[1].clone();
    d.count = 1000;
    let a = build(d.clone(), 1);
    let b = build(d, 4096);
    for (a, b) in a.plants().iter().zip(b.plants()) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.sway_phase, b.sway_phase);
        assert_eq!(a.light_phase, b.light_phase);
        assert_eq!(a.width, a.height);
        assert!((1. ..=10.).contains(&a.height));
    }
    let p = &a.plants()[0];
    let dx = (p.position.x - a.definition().origin[0]) / 600.;
    let dz = (p.position.z - a.definition().origin[2]) / 600.;
    assert!(dx * dx + dz * dz <= 1.);
}
#[test]
#[ignore = "requires private converted foliage/map packs; offscreen only"]
fn surface_occlusion_slope_and_resumable_water_second_ray() {
    let mut d = pack().definitions[1].clone();
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
#[test]
#[ignore = "requires private converted foliage/map packs; offscreen only"]
fn culling_is_bounded_and_keeps_plant_extent() {
    let field = build(pack().definitions[0].clone(), 4096);
    let center = Vec3::from_array(field.definition().origin) * Vec3::new(1., 0., 1.);
    let c = camera(
        center + Vec3::new(0., 6., 30.),
        center + Vec3::new(0., 3., 0.),
    );
    let mut visible = vec![];
    let stats = field.visible(&c, &mut visible).unwrap();
    assert!(stats.instances_visible > 0 && stats.instances_visible < 10000);
    assert!(stats.instances_tested < 15000);
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
#[test]
#[ignore = "requires private converted foliage/map packs; offscreen only"]
fn authored_fade_sway_light_and_invalid_inputs() {
    let field = build(pack().definitions[1].clone(), 4096);
    let d = field.definition();
    assert_eq!(fade(d, 0.), 0.);
    assert_eq!(fade(d, 1.), 1.);
    assert_eq!(fade(d, 70.), 1.);
    assert_eq!(fade(d, 80.), 0.5);
    assert_eq!(fade(d, 90.), 0.);
    let p = &field.plants()[0];
    for time in [0., 1., 10., 100.] {
        let (s, l) = motion(d, p, time);
        assert!(s[0].abs() <= 0.1 && s[1].abs() <= 0.2);
        assert!((0.7..=1.).contains(&l));
    }
    let mut d = d.clone();
    d.count = u32::MAX;
    assert!(PlacementBuilder::new(d).is_err());
}
#[test]
#[ignore = "requires private converted foliage/map packs; offscreen only"]
fn real_bedroom_terrain_architecture_and_static_collision_probe() {
    let p = pack();
    let w = original_world();
    let start = std::time::Instant::now();
    let mut report = vec![];
    for d in p.definitions {
        let mut b = PlacementBuilder::new(d).unwrap();
        while !b.stats().completed {
            b.advance(2048, |r| trace(&w, r)).unwrap();
        }
        let field = b.finish().unwrap();
        assert!(!field.plants().is_empty());
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
            assert!((plant.position.y - hit.position.y).abs() < 0.01);
        }
        let c = camera(
            field.plants()[0].position + Vec3::new(10., 5., 10.),
            field.plants()[0].position,
        );
        let mut indices = vec![];
        let cull = field.visible(&c, &mut indices).unwrap();
        report.push(serde_json::json!({"definition":field.definition().id,"placement":field.placement,"culling":cull}));
    }
    let report = serde_json::json!({"schema_version":1,"native_map":"map-bundle-014","total_milliseconds":start.elapsed().as_secs_f64()*1000.,"reports":report,"subjective_parity":false});
    std::fs::write(
        root().join("artifacts/native-foliage/placement-probe.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
