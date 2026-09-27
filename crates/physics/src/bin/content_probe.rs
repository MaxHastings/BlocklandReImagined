//! Headless checks against authored native collision surfaces, with no Torque reader.
use anyhow::{Context, Result, ensure};
use bri_content::collision::{CollisionBody, CollisionLibrary, Part};
use rapier3d::prelude::*;

fn faces(body: &CollisionBody) -> Vec<[Vector; 3]> {
    let mut out = Vec::new();
    for part in &body.parts {
        match part {
            Part::Convex {
                vertices,
                triangles,
                ..
            } => out.extend(
                triangles
                    .iter()
                    .map(|t| t.map(|i| Vector::from_array(vertices[i as usize]))),
            ),
            Part::Box { center, size } => {
                let center = Vector::from_array(*center);
                let half = Vector::from_array(*size) * 0.5;
                let vertices: Vec<_> = (0..8)
                    .map(|i| {
                        center
                            + half
                                * Vector::new(
                                    if i & 1 == 0 { -1.0 } else { 1.0 },
                                    if i & 2 == 0 { -1.0 } else { 1.0 },
                                    if i & 4 == 0 { -1.0 } else { 1.0 },
                                )
                    })
                    .collect();
                for [a, b, c, d] in [
                    [0, 1, 3, 2],
                    [4, 6, 7, 5],
                    [0, 4, 5, 1],
                    [2, 3, 7, 6],
                    [0, 2, 6, 4],
                    [1, 5, 7, 3],
                ] {
                    out.push([vertices[a], vertices[b], vertices[c]]);
                    out.push([vertices[a], vertices[c], vertices[d]]);
                }
            }
        }
    }
    out
}
// Double-sided Moller-Trumbore oracle, separate from the physics hull/raycast code.
fn hit(faces: &[[Vector; 3]], origin: Vector, direction: Vector) -> Option<f32> {
    faces
        .iter()
        .filter_map(|[a, b, c]| {
            let e1 = *b - *a;
            let e2 = *c - *a;
            let h = direction.cross(e2);
            let determinant = e1.dot(h);
            if determinant.abs() < 1e-8 {
                return None;
            }
            let inv = 1.0 / determinant;
            let s = origin - *a;
            let u = inv * s.dot(h);
            if !(-1e-5..=1.00001).contains(&u) {
                return None;
            }
            let q = s.cross(e1);
            let v = inv * direction.dot(q);
            if v < -1e-5 || u + v > 1.00001 {
                return None;
            }
            let time = inv * e2.dot(q);
            (time >= 0.0).then_some(time)
        })
        .min_by(f32::total_cmp)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2 || args.len() == 3,
        "Usage: content_probe <native-collisions.json> <report.json> [expected-catalog.json]"
    );
    let library: CollisionLibrary = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    ensure!(library.schema_version == 1, "Unknown collision schema");
    if let Some(path) = args.get(2) {
        let catalog: bri_content::brick::Catalog = serde_json::from_slice(&std::fs::read(path)?)?;
        let expected: std::collections::BTreeSet<_> =
            catalog.bricks.iter().map(|b| &b.id).collect();
        let actual: std::collections::BTreeSet<_> = library.bodies.iter().map(|b| &b.id).collect();
        ensure!(
            expected == actual
                && expected.len() == catalog.bricks.len()
                && actual.len() == library.bodies.len(),
            "Collision library does not match expected catalog identities"
        );
    } else {
        ensure!(
            library.bodies.len() == 136,
            "Expected complete stock collision library; pass an explicit catalog for add-ons"
        );
    }
    let started = std::time::Instant::now();
    let mut reports = Vec::new();
    let mut total = 0;
    let mut max_error = 0.0_f32;
    let mut hull_coverage_differences = 0;
    let mut max_hull_error = 0.0_f32;
    for body in &library.bodies {
        let collider = bri_physics::content::collider(body)?.build();
        let geometry = faces(body);
        let mut min = Vector::splat(f32::INFINITY);
        let mut max = Vector::splat(f32::NEG_INFINITY);
        for p in geometry.iter().flatten() {
            min = min.min(*p);
            max = max.max(*p);
        }
        let mut hits = 0;
        for axis in 0..3 {
            for a in 0..11 {
                for b in 0..11 {
                    let mut origin = Vector::ZERO;
                    origin[axis] = max[axis] + 2.0;
                    for (dimension, sample, jitter) in
                        [((axis + 1) % 3, a, 0.371), ((axis + 2) % 3, b, 0.371)]
                    {
                        // Include rays just outside the bounds; avoid exact mesh seams.
                        origin[dimension] = min[dimension]
                            + (max[dimension] - min[dimension])
                                * ((sample as f32 + jitter) / 11.0 * 1.2 - 0.1);
                    }
                    let mut direction = Vector::ZERO;
                    direction[axis] = -1.0;
                    let expected = hit(&geometry, origin, direction);
                    let ray = Ray::new(origin, direction);
                    let actual =
                        bri_physics::content::raycast(body, origin, direction, 1000.0).map(|h| h.0);
                    let hull = collider
                        .shape()
                        .cast_ray(&Pose::IDENTITY, &ray, 1000.0, true);
                    match (expected, hull) {
                        (Some(e), Some(h)) => max_hull_error = max_hull_error.max((e - h).abs()),
                        (None, None) => {}
                        _ => hull_coverage_differences += 1,
                    }
                    total += 1;
                    match (expected, actual) {
                        (Some(e), Some(a)) => {
                            hits += 1;
                            max_error = max_error.max((e - a).abs());
                            ensure!(
                                (e - a).abs() < 0.0003,
                                "Ray surface mismatch {}: authored={e}, physics={a}",
                                body.id
                            );
                        }
                        (None, None) => {}
                        values => anyhow::bail!(
                            "Collision coverage mismatch {} at {origin:?} axis {axis}: {values:?}",
                            body.id
                        ),
                    }
                }
            }
        }
        ensure!(hits > 0, "No collision coverage for {}", body.id);
        reports.push(serde_json::json!({"id":body.id,"parts":body.parts.len(),"ray_hits":hits}));
    }
    // Actual solver contact on an imported round brick, not just query geometry.
    let round = library
        .bodies
        .iter()
        .find(|b| b.id.ends_with("/brick1x1rounddata"))
        .context("Missing round brick")?;
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed(),
        bri_physics::content::collider(round)?,
    );
    let (ball, _) = world.insert(
        RigidBodyBuilder::dynamic()
            .translation(Vector::new(0.0, 2.0, 0.0))
            .ccd_enabled(true),
        ColliderBuilder::ball(0.1).restitution(0.0),
    );
    for _ in 0..600 {
        world.step();
    }
    let rest = world.bodies[ball].translation();
    ensure!(
        (rest.y - 0.4).abs() < 0.015 && world.bodies[ball].linvel().length() < 0.01,
        "Body failed to rest on imported brick: {rest:?}"
    );
    let report = serde_json::json!({"status":"passed","stock_bodies":reports,"rays":total,"max_surface_error":max_error,"hull_raycast_coverage_differences":hull_coverage_differences,"hull_raycast_max_error":max_hull_error,"round_brick_resting_ball":rest.to_array(),"elapsed_ms":started.elapsed().as_millis(),"scope":"native convex-plane targeting against authored triangle surfaces and convex solver contact; hull raycast differences reported separately, no movement-feel acceptance"});
    let output = std::path::Path::new(&args[1]);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "{} native brick bodies: {total} precise ray comparisons, max error {max_error:.7}; imported round-brick solver contact passed. Convex GJK ray diagnostics: {hull_coverage_differences} coverage differences, max error {max_hull_error:.7}",
        library.bodies.len()
    );
    Ok(())
}
