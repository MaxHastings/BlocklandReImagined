//! Authored static foliage is placed during background map loading, never per frame.
use crate::building::Building;
use anyhow::{Result, ensure};
use bri_content::water::Water;
use bri_foliage::*;
use glam::Vec3;
use std::{path::Path, time::Instant};

#[derive(Default)]
pub struct PreparedFoliage {
    pub fields: Vec<FoliageField>,
    pub placement: Vec<PlacementStats>,
    pub elapsed_ms: f64,
}
impl PreparedFoliage {
    pub fn load(root: &Path, map: &str, building: &Building, waters: &[Water]) -> Result<Self> {
        let start = Instant::now();
        let pack = FoliagePack::load(root.join("foliage.json"))?;
        let mut prepared = Self::default();
        let mut total_queries = 0;
        for definition in pack.definitions.into_iter().filter(|d| d.scene == map) {
            let mut builder = PlacementBuilder::new(definition)?;
            while !builder.stats().completed {
                let mut error = None;
                let previous = builder.stats().queries;
                builder.advance(1024, |ray| match static_hit(building, waters, ray) {
                    Ok(hit) => hit,
                    Err(e) => {
                        error = Some(e);
                        None
                    }
                })?;
                if let Some(error) = error {
                    return Err(error);
                }
                total_queries += builder.stats().queries - previous;
                ensure!(
                    total_queries <= 16_000_000,
                    "Native foliage placement exceeds total load query budget"
                );
            }
            prepared.placement.push(builder.stats());
            prepared.fields.push(builder.finish()?);
        }
        prepared.elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
        Ok(prepared)
    }
}

pub fn static_hit(
    building: &Building,
    waters: &[Water],
    ray: PlacementRay,
) -> Result<Option<SurfaceHit>> {
    use bri_sim::map::MapSurface;
    let mut nearest = building
        .static_surface(ray.start, ray.end)?
        .map(|(hit, surface)| SurfaceHit {
            position: hit.position,
            normal: hit.normal.normalize_or_zero(),
            kind: match surface {
                MapSurface::Terrain => SurfaceKind::Terrain,
                MapSurface::Interior => SurfaceKind::Interior,
                MapSurface::Static => SurfaceKind::Static,
            },
        });
    let delta = ray.end - ray.start;
    if ray.include_water && delta.y.abs() > 0.00001 {
        for water in waters {
            let t = (water.max[1] - ray.start.y) / delta.y;
            if !(0.0..=1.0).contains(&t) {
                continue;
            }
            let position = ray.start + delta * t;
            if water.footprint(position.x, position.z).is_some()
                && nearest.as_ref().is_none_or(|hit| {
                    ray.start.distance_squared(position) < ray.start.distance_squared(hit.position)
                })
            {
                nearest = Some(SurfaceHit {
                    position,
                    normal: Vec3::Y,
                    kind: SurfaceKind::Water,
                });
            }
        }
    }
    Ok(nearest)
}

pub struct ClientFoliage {
    pack: FoliagePack,
    images: Vec<Image>,
    pub prepared: PreparedFoliage,
    renderer: Option<FoliageRenderer>,
    /// Samples per pixel of the world pass the foliage draws into.
    samples: u32,
    seconds: f64,
    pub stats: RenderStats,
}
impl ClientFoliage {
    pub fn load(root: &Path) -> Result<Self> {
        let pack = FoliagePack::load(root.join("foliage.json"))?;
        let images = pack.images(root)?;
        Ok(Self {
            pack,
            images,
            prepared: PreparedFoliage::default(),
            renderer: None,
            samples: 1,
            seconds: 0.,
            stats: RenderStats::default(),
        })
    }
    pub fn set_map(&mut self, prepared: PreparedFoliage) {
        self.clear();
        self.prepared = prepared;
    }
    pub fn clear(&mut self) {
        self.prepared = PreparedFoliage::default();
        self.renderer = None;
        self.seconds = 0.;
        self.stats = RenderStats::default();
    }
    pub fn gpu_stopped(&mut self) {
        self.renderer = None;
    }
    /// Match the world pass's samples per pixel; the renderer is rebuilt.
    pub fn set_samples(&mut self, samples: u32) {
        if samples != self.samples {
            self.samples = samples;
            self.renderer = None;
        }
    }
    pub fn advance(&mut self, elapsed: std::time::Duration) {
        self.seconds += elapsed.as_secs_f64();
    }
    pub fn prepare(
        &mut self,
        frame: &crate::platform::RenderContext<'_>,
        camera: &Camera,
        fog_start: f32,
        fog_end: f32,
    ) -> Result<()> {
        if self.prepared.fields.is_empty() {
            return Ok(());
        }
        if self.renderer.is_none() {
            self.renderer = Some(FoliageRenderer::new(
                frame.device,
                frame.queue,
                &self.pack,
                &self.images,
                self.prepared.fields.clone(),
                RenderConfig {
                    target: frame.format,
                    depth: bri_render::scene::DEPTH_FORMAT,
                    samples: self.samples.max(1),
                },
            )?);
        }
        self.stats = self.renderer.as_mut().unwrap().prepare_elapsed(
            frame.queue,
            camera,
            self.seconds,
            fog_start,
            fog_end,
        )?;
        Ok(())
    }
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(renderer) = &self.renderer {
            renderer.render(pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_sim::{definitions::Definitions, map::MapSurface};
    use rapier3d::prelude::{ColliderBuilder, Vector};
    use std::collections::BTreeMap;
    #[test]
    fn forbidden_roof_blocks_terrain_instead_of_being_skipped() -> Result<()> {
        let floor = ColliderBuilder::cuboid(10., 0.5, 10.)
            .translation(Vector::new(0., -0.5, 0.))
            .user_data(MapSurface::Terrain as u128);
        let roof = ColliderBuilder::cuboid(2., 0.5, 2.)
            .translation(Vector::new(0., 3., 0.))
            .user_data(MapSurface::Interior as u128);
        let building = Building::new(
            Definitions {
                entries: BTreeMap::new(),
            },
            vec![floor, roof],
        )?;
        let hit = static_hit(
            &building,
            &[],
            PlacementRay {
                start: Vec3::Y * 2000.,
                end: Vec3::Y * -2000.,
                include_water: true,
            },
        )?
        .unwrap();
        assert_eq!(hit.kind, SurfaceKind::Interior);
        assert!((hit.position.y - 3.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    #[ignore = "reads native Bedroom and foliage packs, CPU only"]
    fn actual_client_collision_places_original_foliage_on_allowed_surfaces() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let map_id = "v20/add-ons/map_bedroom/bedroom.mis";
        let map = bri_sim::map::NativeMap::load(&root.join("map-bundle-017"), map_id)?;
        let mut building = Building::new(
            Definitions {
                entries: BTreeMap::new(),
            },
            map.colliders,
        )?;
        building.attach_terrain(map.terrain);
        let prepared = PreparedFoliage::load(
            &root.join("foliage-pack-001"),
            map_id,
            &building,
            &map.waters,
        )?;
        assert_eq!(
            prepared.placement.iter().map(|p| p.placed).sum::<u32>(),
            41000
        );
        assert!(
            prepared
                .placement
                .iter()
                .all(|p| p.completed && p.rejected == 0)
        );
        for field in &prepared.fields {
            for plant in field.plants().iter().step_by(200) {
                let hit = static_hit(
                    &building,
                    &map.waters,
                    PlacementRay {
                        start: Vec3::new(plant.position.x, 2000., plant.position.z),
                        end: Vec3::new(plant.position.x, -2000., plant.position.z),
                        include_water: true,
                    },
                )?
                .unwrap();
                assert_eq!(hit.kind, SurfaceKind::Terrain);
                assert!(
                    (plant.position.y - hit.position.y - field.definition().offset).abs() < 0.001
                );
            }
        }
        Ok(())
    }
}
