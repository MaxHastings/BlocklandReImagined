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
    /// The map-authored pack, which a Steam sky's rain or snow joins.
    base: std::sync::Arc<WeatherPack>,
    /// The sky whose rain or snow falls now, if any.
    sky: Option<String>,
    density: f32,
    /// Changes whenever `world`'s pack does, so its GPU textures follow.
    pub revision: u64,
}

/// The rain or snow a Steam sky brings (v21's `$Rain::` values), with its
/// pictures decoded.
pub struct SkyRain<'a> {
    pub sky: &'a str,
    pub precipitation: &'a bri_content::environment_pack::Precipitation,
    pub drop: (&'a bri_content::environment::Image, WeatherTexture),
    pub splash: Option<(&'a bri_content::environment::Image, WeatherTexture)>,
}
const SEED: u64 = 0x425249;
const SKY_RAIN: &str = "environment-sky";
impl ClientWeather {
    pub fn load(path: &Path, settings: &mut Settings) -> Result<Self> {
        let key = "$pref::precipitationOn";
        if !settings.prefs.keys().any(|k| k.eq_ignore_ascii_case(key)) {
            settings.prefs.insert(key.into(), "1".into());
        }
        let base = WeatherPack::load(path)?;
        let mut value = Self {
            world: WeatherWorld::new(base.clone(), WeatherLimits::default(), SEED)?,
            waters: vec![],
            base,
            sky: None,
            density: 1.0,
            revision: 0,
        };
        value.apply_settings(settings)?;
        Ok(value)
    }
    pub fn apply_settings(&mut self, settings: &Settings) -> Result<()> {
        let prefs = Prefs::new(&BTreeMap::new(), &settings.prefs);
        self.density = if prefs.bool_or("$pref::precipitationOn", true) {
            1.
        } else {
            0.
        };
        self.world.set_density(self.density)
    }
    pub fn set_map(&mut self, map: &str, waters: Vec<Water>) -> Result<()> {
        for water in &waters {
            water.validate()?;
        }
        if self.sky.take().is_some() {
            self.replace(self.base.clone())?;
        }
        self.start(map)?;
        self.waters = waters;
        Ok(())
    }
    /// A Steam sky's rain or snow in place of the map's own, as v21's
    /// `setSkyBox` replaces its `Rain`; None gives the map's own back.
    /// Whether anything changed.
    pub fn set_sky(&mut self, rain: Option<SkyRain>) -> Result<bool> {
        if rain.as_ref().map(|r| r.sky) == self.sky.as_deref() {
            return Ok(false);
        }
        let Some(map) = self.world.map_id().map(str::to_owned) else {
            return Ok(false);
        };
        self.sky = rain.as_ref().map(|r| r.sky.to_owned());
        let pack = match rain {
            Some(rain) => with_sky_rain(&self.base, &map, rain)?,
            None => self.base.clone(),
        };
        self.replace(pack)?;
        self.start(&map)?;
        Ok(true)
    }
    fn replace(&mut self, pack: std::sync::Arc<WeatherPack>) -> Result<()> {
        self.world = WeatherWorld::new(pack, WeatherLimits::default(), SEED)?;
        self.world.set_density(self.density)?;
        self.revision += 1;
        Ok(())
    }
    fn start(&mut self, map: &str) -> Result<()> {
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
        Ok(())
    }
    /// The liquids rain and snow stop at (the Environment window's water).
    pub fn set_waters(&mut self, waters: Vec<Water>) {
        self.waters = waters;
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

/// `base` with `rain` falling on `map` in place of the map's own.
fn with_sky_rain(
    base: &WeatherPack,
    map: &str,
    rain: SkyRain,
) -> Result<std::sync::Arc<WeatherPack>> {
    let p = rain.precipitation;
    let mut manifest = base.manifest.clone();
    manifest
        .placements
        .retain(|placement| placement.map_id != map);
    let mut textures: Vec<WeatherTexture> = base
        .textures
        .iter()
        .map(|t| WeatherTexture {
            id: t.id.clone(),
            width: t.width,
            height: t.height,
            rgba: t.rgba.clone(),
        })
        .collect();
    // A picture the map packs already use is cut into the cells they cut it
    // into (v20's snow is four flakes); any other into Torque's defaults,
    // four drops and two splashes a side.
    let cells = |image: &bri_content::environment::Image, default: u32| {
        manifest
            .definitions
            .iter()
            .find_map(|d| {
                let drop = manifest.textures.get(&d.drop_texture)?;
                if drop.sha256 == image.sha256 {
                    return Some(d.drops_per_side);
                }
                let splash = manifest.textures.get(d.splash_texture.as_ref()?)?;
                (splash.sha256 == image.sha256).then_some(d.splashes_per_side)
            })
            .unwrap_or(default)
    };
    let drops_per_side = cells(rain.drop.0, 4);
    let splashes_per_side = rain.splash.as_ref().map_or(2, |(image, _)| cells(image, 2));
    let mut add =
        |name: &str, (image, texture): (&bri_content::environment::Image, WeatherTexture)| {
            let id = format!("{SKY_RAIN}-{name}");
            manifest.textures.insert(
                id.clone(),
                TextureRecord {
                    file: image.file.clone(),
                    sha256: image.sha256.clone(),
                    rgba_sha256: sha256(&texture.rgba),
                    width: texture.width,
                    height: texture.height,
                    source_paths: vec![image.source.clone()],
                    alpha_policy: "straight".into(),
                },
            );
            textures.push(WeatherTexture {
                id: id.clone(),
                ..texture
            });
            id
        };
    let drop_texture = add("drop", rain.drop);
    let splash_texture = rain.splash.map(|splash| add("splash", splash));
    manifest.definitions.push(Definition {
        id: SKY_RAIN.into(),
        drop_texture,
        splash_texture,
        drop_radius: p.drop_size.max(0.01),
        splash_radius: p.splash_size,
        true_billboards: p.true_billboards,
        splash_seconds: p.splash_ms as f32 / 1000.0,
        drop_animation_seconds: 0.0,
        animate_splashes: true,
        drops_per_side,
        splashes_per_side,
    });
    manifest.placements.push(Placement {
        id: SKY_RAIN.into(),
        map_id: map.into(),
        definition: SKY_RAIN.into(),
        position: [0.0; 3],
        drops: p.drops,
        width: p.box_width.max(0.01),
        height: p.box_height.max(0.01),
        speed_per_tick: [p.speed[0].max(0.01), p.speed[1].max(p.speed[0]).max(0.01)],
        mass: [p.mass[0].max(0.01), p.mass[1].max(p.mass[0]).max(0.01)],
        turbulence_amplitude: p.max_turbulence,
        turbulence_radians_per_tick: p.turbulence_speed,
        use_turbulence: p.turbulence,
        rotate_with_camera_velocity: p.rotate_with_camera,
        collision: p.collision,
        follow_camera: true,
        use_wind: false,
        authored_sky_wind: [0.0; 3],
        reference_wind_velocity: [0.0; 3],
        original_fields: BTreeMap::new(),
    });
    WeatherPack::from_parts(manifest, textures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_steam_skys_snow_replaces_the_maps_rain_until_unpicked() {
        let base = bri_weather::testing::showcase_pack();
        let mut snow_record = base.manifest.textures["fixture/snow"].clone();
        snow_record.sha256 = "a".repeat(64);
        let mut manifest = base.manifest.clone();
        manifest.textures.insert("fixture/snow".into(), snow_record);
        let textures = base
            .textures
            .iter()
            .map(|t| WeatherTexture {
                id: t.id.clone(),
                width: t.width,
                height: t.height,
                rgba: t.rgba.clone(),
            })
            .collect();
        let base = WeatherPack::from_parts(manifest, textures).unwrap();
        let mut weather = ClientWeather {
            world: WeatherWorld::new(base.clone(), WeatherLimits::default(), SEED).unwrap(),
            waters: vec![],
            base,
            sky: None,
            density: 1.0,
            revision: 0,
        };
        weather.set_map("fixture/storm", vec![]).unwrap();
        // v20's snow picture (the same file): cut into the same four flakes.
        let image = bri_content::environment::Image {
            file: "snow.png".into(),
            source: "base/data/specialfx/snow.png".into(),
            sha256: "a".repeat(64),
            width: 8,
            height: 8,
        };
        let precipitation = bri_content::environment_pack::Precipitation {
            drop: image.clone(),
            splash: None,
            drop_size: 0.35,
            splash_size: 0.0,
            splash_ms: 0,
            true_billboards: true,
            speed: [0.3, 0.5],
            mass: [0.75, 0.85],
            max_turbulence: 0.3,
            turbulence_speed: 0.1,
            rotate_with_camera: true,
            turbulence: true,
            drops: 800,
            box_width: 100.0,
            box_height: 100.0,
            collision: true,
        };
        let rain = || SkyRain {
            sky: "add-ons/sky_spooky3/spooky3snow.dml",
            precipitation: &precipitation,
            drop: (&image, bri_weather::testing::texture("", 8, 8, 255)),
            splash: None,
        };
        assert!(weather.set_sky(Some(rain())).unwrap());
        assert!(!weather.set_sky(Some(rain())).unwrap(), "already falling");
        assert_eq!(weather.revision, 1);
        let pack = weather.world.pack().clone();
        let placements: Vec<_> = pack.placements_for("fixture/storm").collect();
        assert_eq!(placements.len(), 1, "the map's own rain is replaced");
        assert_eq!(
            (placements[0].id.as_str(), placements[0].drops),
            (SKY_RAIN, 800)
        );
        let definition = pack
            .manifest
            .definitions
            .iter()
            .find(|d| d.id == SKY_RAIN)
            .unwrap();
        assert_eq!(definition.drops_per_side, 2);
        // The other map keeps its own.
        assert_eq!(pack.placements_for("fixture/winter").count(), 1);
        assert!(weather.set_sky(None).unwrap());
        assert_eq!(
            weather
                .world
                .pack()
                .placements_for("fixture/storm")
                .next()
                .unwrap()
                .definition,
            "fixture/rain"
        );
    }
    use rapier3d::prelude::{ColliderBuilder, Vector};
    /// A map's waters, the first repeating: a made-up repeating sea, or
    /// the converted Slate Sea's.
    struct Sea {
        waters: Vec<Water>,
    }
    impl Sea {
        fn synthetic() -> Result<Self> {
            let mut sea = Water::volume([-40.0, -12.0, -40.0], [40.0, 3.5, 40.0]);
            sea.repeat_period = Some(96.0);
            sea.validate()?;
            Ok(Self { waters: vec![sea] })
        }
        fn content() -> Result<Self> {
            let root = bri_package::testing::pack_dir(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
                "map_bundle",
            );
            let map = bri_sim::map::NativeMap::load(
                &root,
                "v20/add-ons/map_slate_sea_revised/slatesearevised.mis",
            )?;
            Ok(Self { waters: map.waters })
        }
    }
    crate::testing::synthetic_and_content!(
        Sea: repeated_native_water_and_roofs_choose_the_nearest_surface
    );
    fn repeated_native_water_and_roofs_choose_the_nearest_surface(fx: &Sea) -> Result<()> {
        let map = fx;
        let water = &map.waters[0];
        assert!(water.repeat_period.is_some(), "the sea repeats");
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
