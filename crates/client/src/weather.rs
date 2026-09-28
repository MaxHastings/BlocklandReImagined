//! Map-authored precipitation using the same native solid/water geometry as gameplay.
use crate::building::Building;
use anyhow::Result;
use bri_content::water::Water;
use bri_ui::{api::Settings, prefs::Prefs};
use bri_weather::*;
use glam::Vec3;
use std::{collections::BTreeMap, path::Path};

pub struct ClientWeather {
    pub world: WeatherWorld,
    waters: Vec<Water>,
}
impl ClientWeather {
    pub fn load(path: &Path, settings: &mut Settings) -> Result<Self> {
        let key = "$pref::precipitationOn";
        if !settings.prefs.keys().any(|k| k.eq_ignore_ascii_case(key)) {
            settings.prefs.insert(key.into(), "1".into());
        }
        let mut value = Self {
            world: WeatherWorld::new(WeatherPack::load(path)?, WeatherLimits::default(), 0x425249)?,
            waters: vec![],
        };
        value.apply_settings(settings)?;
        Ok(value)
    }
    pub fn apply_settings(&mut self, settings: &Settings) -> Result<()> {
        let prefs = Prefs::new(&BTreeMap::new(), &settings.prefs);
        self.world
            .set_density(if prefs.bool_or("$pref::precipitationOn", true) {
                1.
            } else {
                0.
            })
    }
    pub fn set_map(&mut self, map: &str, waters: Vec<Water>) -> Result<()> {
        for water in &waters {
            water.validate()?;
        }
        self.world.set_map(map)?;
        let wind = self
            .world
            .pack()
            .placements_for(map)
            .next()
            .map_or(Vec3::ZERO, |p| Vec3::from_array(p.reference_wind_velocity));
        self.world.set_environment(WeatherEnvironment {
            wind_velocity: wind,
        })?;
        self.waters = waters;
        Ok(())
    }
    pub fn clear(&mut self) {
        self.world.clear();
        self.waters.clear();
    }
    pub fn advance(&mut self, dt: f32, camera: CameraState, building: &Building) -> Result<()> {
        let mut error = None;
        self.world.advance(
            f64::from(dt),
            camera,
            building.query_generation(),
            &mut |ray| match closest_hit(building, &self.waters, ray) {
                Ok(hit) => hit,
                Err(e) => {
                    error = Some(e);
                    None
                }
            },
        )?;
        if let Some(error) = error {
            self.world.invalidate_collision();
            return Err(error);
        }
        Ok(())
    }
}

pub fn closest_hit(
    building: &Building,
    waters: &[Water],
    ray: CollisionRay,
) -> Result<Option<WeatherHit>> {
    let solid = building.solid_segment(ray.start, ray.end)?;
    let mut nearest = solid.map(|hit| WeatherHit {
        position: hit.position,
        normal: hit.normal.normalize_or_zero(),
        surface: WeatherSurface::Solid,
    });
    let delta = ray.end - ray.start;
    // Weather hits the physical still-water surface. Visual waves do not alter
    // the authoritative volume; repetition preserves gaps in authored tiles.
    if delta.y.abs() > 0.00001 {
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
                nearest = Some(WeatherHit {
                    position,
                    normal: Vec3::Y,
                    surface: WeatherSurface::Water,
                });
            }
        }
    }
    Ok(nearest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::prelude::{ColliderBuilder, Vector};
    #[test]
    #[ignore = "reads converted Slate Sea water; no original assets or devices"]
    fn repeated_native_water_and_roofs_choose_the_nearest_surface() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/map-bundle-017");
        let map = bri_sim::map::NativeMap::load(
            &root,
            "v20/add-ons/map_slate_sea_revised/slatesearevised.mis",
        )?;
        let water = &map.waters[0];
        let center = (Vec3::from_array(water.min) + Vec3::from_array(water.max)) * 0.5;
        let center = Vec3::new(
            center.x + water.repeat_period.unwrap_or(0.),
            water.max[1],
            center.z,
        );
        let ray = CollisionRay {
            start: center + Vec3::Y * 20.,
            end: center - Vec3::Y * 20.,
        };
        let empty = || bri_sim::definitions::Definitions {
            entries: BTreeMap::new(),
        };
        let building = Building::new(empty(), vec![])?;
        let hit = closest_hit(&building, &map.waters, ray)?.unwrap();
        assert_eq!(hit.surface, WeatherSurface::Water);
        assert!((hit.position.y - water.max[1]).abs() < 0.001);
        let roof = ColliderBuilder::cuboid(2., 0.5, 2.).translation(Vector::new(
            center.x,
            center.y + 3.,
            center.z,
        ));
        let building = Building::new(empty(), vec![roof])?;
        let hit = closest_hit(&building, &map.waters, ray)?.unwrap();
        assert_eq!(hit.surface, WeatherSurface::Solid);
        assert!((hit.position.y - (center.y + 3.5)).abs() < 0.001);
        assert!(hit.normal.dot(Vec3::Y) > 0.99);
        Ok(())
    }
}
