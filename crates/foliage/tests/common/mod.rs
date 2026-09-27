#![allow(dead_code)]
use bri_foliage::*;
use glam::{Mat4, Vec3};
use rapier3d::prelude::*;
pub fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
pub fn pack() -> FoliagePack {
    FoliagePack::load(root().join("content/foliage-pack-001/foliage.json")).unwrap()
}
pub fn floor(ray: PlacementRay) -> Option<SurfaceHit> {
    Some(SurfaceHit {
        position: Vec3::new(ray.start.x, 0., ray.start.z),
        normal: Vec3::Y,
        kind: SurfaceKind::Terrain,
    })
}
pub fn build(d: Definition, budget: u32) -> FoliageField {
    let mut b = PlacementBuilder::new(d).unwrap();
    while !b.stats().completed {
        b.advance(budget, floor).unwrap();
    }
    b.finish().unwrap()
}
pub fn camera(position: Vec3, target: Vec3) -> Camera {
    let direction = (target - position).normalize();
    Camera {
        position,
        right: direction.cross(Vec3::Y).normalize(),
        view_projection: glam::camera::rh::proj::directx::perspective(
            70f32.to_radians(),
            1.,
            0.1,
            200.,
        ) * glam::camera::rh::view::look_at_mat4(position, target, Vec3::Y),
        visible_distance: 200.,
    }
}
pub fn original_world() -> PhysicsWorld {
    let path = root().join("content/map-bundle-016");
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("bundle.json")).unwrap()).unwrap();
    let scene_path = std::fs::read_dir(&path)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| {
            e.file_name().to_string_lossy()
                == "de2b3763cc76daa7fff1df9da8aa75b76c332d45b3f692007505c88b4b8b394e.scene.json"
        })
        .unwrap()
        .path();
    let scene: bri_content::scene::Scene =
        serde_json::from_slice(&std::fs::read(scene_path).unwrap()).unwrap();
    let mut w = bri_physics::new_world();
    let instances = serde_json::from_value(bundle["terrains"][scene.id.as_str()].clone()).unwrap();
    let fields = bri_content::terrain_field::map_fields(&scene, instances, |id| {
        Ok(serde_json::from_slice(&std::fs::read(
            path.join(bundle["assets"][id].as_str().unwrap()),
        )?)?)
    })
    .unwrap();
    for field in fields {
        let mesh = field.mesh([64, 64, 128, 128]).unwrap();
        let c = ColliderBuilder::trimesh_with_flags(
            mesh.positions
                .iter()
                .map(|p| Vector::from_array(*p))
                .collect(),
            mesh.triangles,
            TriMeshFlags::FIX_INTERNAL_EDGES,
        )
        .unwrap()
        .user_data(1);
        w.insert_collider(c, None);
    }
    for n in scene.nodes {
        let t = Mat4::from_cols_array(&n.transform);
        if let Some(asset) = n.asset {
            let data = std::fs::read(path.join(bundle["assets"][asset.as_str()].as_str().unwrap()))
                .unwrap();
            match n.kind {
                bri_content::scene::Kind::Interior => {
                    let interior: bri_content::interior::Interior =
                        serde_json::from_slice(&data).unwrap();
                    let c = bri_physics::content::interior_collider(&interior.details[0], t)
                        .unwrap()
                        .user_data(2);
                    w.insert_collider(c, None);
                }
                bri_content::scene::Kind::StaticModel => {
                    let shape: bri_content::shape::Shape = serde_json::from_slice(&data).unwrap();
                    for c in bri_physics::content::static_shape_colliders(&shape, t).unwrap() {
                        w.insert_collider(c.user_data(3), None);
                    }
                }
                _ => {}
            }
        }
    }
    w.detect_collisions(&(), &());
    w
}
pub fn trace(w: &PhysicsWorld, r: PlacementRay) -> Option<SurfaceHit> {
    let ray = Ray::new(r.start, -Vec3::Y);
    w.query_pipeline()
        .cast_ray_and_get_normal(&ray, 4000., true)
        .map(|(h, i)| SurfaceHit {
            position: ray.point_at(i.time_of_impact),
            normal: i.normal,
            kind: match w.colliders[h].user_data {
                1 => SurfaceKind::Terrain,
                2 => SurfaceKind::Interior,
                _ => SurfaceKind::Static,
            },
        })
}
