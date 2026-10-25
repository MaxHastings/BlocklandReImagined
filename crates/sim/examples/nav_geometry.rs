//! Export the same native fixed collision seen by walking navigation.
//! Output can contain private converted content: keep it outside the engine repo.
use anyhow::{Result, ensure};
use bri_motor::torque::{Box3, Soup};
use glam::Vec3;
use rapier3d::prelude::QueryFilter;
use std::{io::Write, path::Path};
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        a.len() == 3,
        "nav_geometry <map-root> <map-id> <output.obj>"
    );
    let native = bri_sim::map::NativeMap::load(Path::new(&a[0]), &a[1])?;
    let anchors = native.spawn_anchors()?;
    let world = bri_world::World::new(
        "Navigation geometry export".into(),
        a[1].clone(),
        vec![[1.; 4]],
    );
    let mut sim = bri_sim::simulation::Simulation::new(
        world,
        bri_sim::testing::definitions(),
        native.colliders,
    )?;
    sim.attach_terrain(native.terrain, anchors)?;
    let mut session = bri_sim::session::Session::new(sim);
    session.set_map_movers(native.movers)?;
    session.step()?;
    let sim = session.simulation();
    // Simulation safety bounds are not authored map navigation geometry.
    let solid = |h, _: &rapier3d::prelude::Collider| {
        sim.map_collider_index(h).is_some() && !sim.bot_passable().contains(&h)
    };
    let query = sim.physics.query_pipeline_with_filter(QueryFilter {
        predicate: Some(&solid),
        ..QueryFilter::only_fixed().exclude_sensors()
    });
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (h, c) in sim.physics.colliders.iter().filter(|(h, c)| solid(*h, c)) {
        let _ = h;
        let box3 = c.compute_aabb();
        min = min.min(Vec3::from_array(box3.mins.to_array()));
        max = max.max(Vec3::from_array(box3.maxs.to_array()));
    }
    ensure!(min.is_finite() && max.is_finite(), "empty geometry");
    let soup = Soup::gather(
        &query,
        &sim.physics.bodies,
        Box3 { min, max },
        Vec3::ZERO,
        &(),
    );
    let mut out = std::io::BufWriter::new(std::fs::File::create(&a[2])?);
    writeln!(
        out,
        "# native fixed collision; bot-openable doors omitted; Y up"
    )?;
    let mut vertices = 0;
    for poly in &soup.polys {
        let v = soup.verts(poly);
        for i in 1..v.len() - 1 {
            let (a, mut b, mut c) = (v[0], v[i], v[i + 1]);
            if (b - a).cross(c - a).dot(poly.normal) < 0. {
                std::mem::swap(&mut b, &mut c);
            }
            if (b - a).cross(c - a).length_squared() < 1e-12 {
                continue;
            }
            for p in [a, b, c] {
                writeln!(out, "v {} {} {}", p.x, p.y, p.z)?;
            }
            writeln!(out, "f {} {} {}", vertices + 1, vertices + 2, vertices + 3)?;
            vertices += 3;
        }
    }
    out.flush()?;
    eprintln!(
        "Native collision: {} triangles, bounds {:?} .. {:?}",
        vertices / 3,
        min,
        max
    );
    Ok(())
}
