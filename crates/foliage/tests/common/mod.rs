#![allow(dead_code, unused_macros)]
use bri_foliage::*;
use glam::{Mat4, Vec3};
use rapier3d::prelude::*;
pub fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
/// The foliage pack a test runs on, with the directory its textures are
/// read from and the world its placement probes trace against.
pub struct Fixture {
    pub pack: FoliagePack,
    pub dir: std::path::PathBuf,
    synthetic: bool,
}
impl Fixture {
    /// `bri_foliage::testing::pack()` written to a fresh scratch directory,
    /// traced against [`synthetic_world`].
    pub fn synthetic() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "foliage-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let pack = testing::write_pack(&dir).unwrap();
        assert_eq!(
            FoliagePack::load(dir.join("foliage.json"))
                .unwrap()
                .definitions
                .len(),
            pack.definitions.len()
        );
        Self {
            pack,
            dir,
            synthetic: true,
        }
    }
    /// The converted `foliage-pack-003`, traced against map-bundle-017.
    pub fn content() -> Self {
        let dir = root().join("content/foliage-pack-003");
        Self {
            pack: FoliagePack::load(dir.join("foliage.json")).unwrap(),
            dir,
            synthetic: false,
        }
    }
    /// The dense swaying grass (`definitions[0]` in both packs).
    pub fn grass(&self) -> Definition {
        self.pack.definitions[testing::GRASS].clone()
    }
    /// The fixed-aspect shrub (`definitions[1]` in both packs).
    pub fn shrub(&self) -> Definition {
        self.pack.definitions[testing::SHRUB].clone()
    }
    pub fn images(&self) -> Vec<Image> {
        self.pack.images(&self.dir).unwrap()
    }
    pub fn world(&self) -> PhysicsWorld {
        if self.synthetic {
            synthetic_world()
        } else {
            original_world()
        }
    }
}
/// Emits a synthetic `#[test]` and an ignored content one, both running
/// `$body(&Fixture)`.
macro_rules! on_both {
    ($synthetic:ident, $content:ident, $body:ident) => {
        #[test]
        fn $synthetic() {
            $body(&Fixture::synthetic());
        }
        #[test]
        #[ignore = "requires generated v20 content"]
        fn $content() {
            $body(&Fixture::content());
        }
    };
}
/// Rolling terrain (collider data 1) under both fixture definitions, with an
/// interior box (2) and a static pillar (3) standing on it.
pub fn synthetic_world() -> PhysicsWorld {
    let mut w = bri_physics::new_world();
    let (n, half) = (48u32, 320f32);
    let height = |x: f32, z: f32| 3. * (x * 0.02).sin() + 2. * (z * 0.03).cos();
    let mut positions = vec![];
    for j in 0..=n {
        for i in 0..=n {
            let x = -half + 2. * half * i as f32 / n as f32;
            let z = -half + 2. * half * j as f32 / n as f32;
            positions.push(Vector::new(x, height(x, z), z));
        }
    }
    let mut triangles = vec![];
    for j in 0..n {
        for i in 0..n {
            let a = j * (n + 1) + i;
            let (b, c, d) = (a + 1, a + n + 1, a + n + 2);
            triangles.extend([[a, c, b], [b, c, d]]);
        }
    }
    let terrain =
        ColliderBuilder::trimesh_with_flags(positions, triangles, TriMeshFlags::FIX_INTERNAL_EDGES)
            .unwrap()
            .user_data(1);
    w.insert_collider(terrain, None);
    w.insert_collider(
        ColliderBuilder::cuboid(8., 4., 6.)
            .translation(Vector::new(20., 6., 10.))
            .user_data(2),
        None,
    );
    w.insert_collider(
        ColliderBuilder::cuboid(1., 10., 1.)
            .translation(Vector::new(-30., 10., 40.))
            .user_data(3),
        None,
    );
    w.detect_collisions(&(), &());
    w
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
        view_projection: bri_render::scene::perspective(70f32.to_radians(), 1., 0.1, 200.)
            * glam::camera::rh::view::look_at_mat4(position, target, Vec3::Y),
        visible_distance: 200.,
    }
}
pub fn original_world() -> PhysicsWorld {
    let path = root().join("content/map-bundle-017");
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
