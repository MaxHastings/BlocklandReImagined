//! The ground and water the Environment window lays over a flat map's plate
//! (v21's `groundPlane` and `WaterPlane`): flat quads over the whole plate,
//! textured from the Steam environment pack. They live in the map scene
//! beside its own surfaces, so they take the sun, shadows and fog as the
//! map does; a change strips the old planes and appends the new.
use crate::scene::*;
use bri_content::{
    atmosphere::{NO_WATER, Settings},
    environment_pack::{Ground, Pack, SEA_MARGIN, Water},
    scene::Plate,
};

/// Names of the planes' materials, which [`strip`] finds them by.
const GROUND: &str = "environment/ground";
const SEA_TOP: &str = "environment/water-top";
const SEA_BOTTOM: &str = "environment/water-bottom";

fn ours(material: &Material) -> bool {
    material.name.starts_with("environment/")
}

/// What the planes need from the settings: the picked ground and its
/// colour, the picked water (None: the map's own), each by the pack's.
pub struct Planes<'a> {
    pub ground: Option<(&'a Ground, [f32; 4])>,
    /// `Some(None)` takes the map's own water away ([`NO_WATER`]).
    pub water: Option<Option<(&'a Water, [f32; 4], f32)>>,
}
impl<'a> Planes<'a> {
    /// The planes `settings` pick from `pack`. A name the pack lacks keeps
    /// the map's own, as the host does.
    pub fn of(pack: &'a Pack, settings: &Settings) -> Self {
        let ground = settings
            .ground
            .as_deref()
            .and_then(|id| pack.ground(id))
            .map(|g| (g, settings.ground_color.unwrap_or(g.color)));
        let water = match settings.water.as_deref() {
            Some(NO_WATER) => Some(None),
            Some(id) => pack.water(id).map(|w| {
                Some((
                    w,
                    settings.water_color.unwrap_or(w.color),
                    settings.water_height.unwrap_or(w.height),
                ))
            }),
            None => None,
        };
        Self { ground, water }
    }
    /// Whether a ground that hides what lies under the plate is drawn: the
    /// sky box then needs no floor (v21's `renderBottomTexture`).
    pub fn ground_shows(&self) -> bool {
        self.ground.is_some_and(|(_, color)| color[3] > 0.0)
    }
}

/// Stop drawing the planes; the pictures only they drew shrink to one
/// pixel, so changing them again and again does not pile up textures.
pub fn strip(scene: &mut SceneData) {
    let materials = &scene.materials;
    scene.batches.retain(|b| !ours(&materials[b.material]));
    let mut used = vec![false; scene.images.len()];
    let mut planes = vec![false; scene.images.len()];
    for material in &scene.materials {
        let marks = if ours(material) {
            &mut planes
        } else {
            &mut used
        };
        for &image in &material.images {
            if let Some(mark) = marks.get_mut(image) {
                *mark = true;
            }
        }
    }
    for (image, (plane, used)) in scene.images.iter_mut().zip(planes.into_iter().zip(used)) {
        if plane && !used && image.width * image.height > 1 {
            *image = SceneImage {
                label: image.label.clone(),
                ..SceneImage::white()
            };
        }
    }
}

/// Lay `planes` over `plate`, with `textures` the pictures each names
/// (ground; water top, water bottom), already decoded. The map's own water
/// batches are the caller's: it keeps or drops them by `planes.water`.
pub fn append(
    scene: &mut SceneData,
    plate: &Plate,
    planes: &Planes,
    ground: Option<SceneImage>,
    water: Option<(SceneImage, SceneImage)>,
) {
    let white = scene.images.len();
    scene.images.push(SceneImage::white());
    if let (Some((g, color)), Some(texture)) = (planes.ground, ground)
        && color[3] > 0.0
    {
        let image = push(scene, texture);
        let mut material = if g.color_multiply {
            Material::vertex_lit(GROUND, image)
        } else {
            // The texture's alpha lays it over the colour (Plate, White).
            Material::brick_overlay(GROUND, image)
        };
        fill(&mut material, white, image);
        material.double_sided = true;
        if color[3] < 1.0 {
            material.alpha = AlphaMode::Blend;
        }
        let m = push_material(scene, material);
        quad(
            scene,
            m,
            plate.min,
            plate.max,
            plate.top,
            g.loops_per_unit,
            color,
            true,
        );
    }
    if let (Some(Some((w, color, height))), Some((top, bottom))) = (planes.water, water) {
        let surface = plate.top + height;
        let (min, max) = (
            [plate.min[0] - SEA_MARGIN, plate.min[1] - SEA_MARGIN],
            [plate.max[0] + SEA_MARGIN, plate.max[1] + SEA_MARGIN],
        );
        for (name, texture, up) in [(SEA_TOP, top, true), (SEA_BOTTOM, bottom, false)] {
            let image = push(scene, texture);
            let mut material = Material::surface(name, image, white);
            material.kind = if w.color_multiply {
                MaterialKind::Unlit
            } else {
                MaterialKind::UnlitOverlay
            };
            fill(&mut material, white, image);
            material.alpha = AlphaMode::Blend;
            let m = push_material(scene, material);
            quad(scene, m, min, max, surface, w.loops_per_unit, color, up);
        }
    }
}

fn push(scene: &mut SceneData, image: SceneImage) -> usize {
    scene.images.push(image);
    scene.images.len() - 1
}
fn push_material(scene: &mut SceneData, material: Material) -> usize {
    scene.materials.push(material);
    scene.materials.len() - 1
}
/// Every slot but the diffuse one on the white picture.
fn fill(material: &mut Material, white: usize, diffuse: usize) {
    material.images = [white; 13];
    material.images[0] = diffuse;
}

/// One quad at height `y` over `min..max` (X, Z), tiled `loops` times a
/// unit, facing up or down.
#[allow(clippy::too_many_arguments)]
fn quad(
    scene: &mut SceneData,
    material: usize,
    min: [f32; 2],
    max: [f32; 2],
    y: f32,
    loops: f32,
    color: [f32; 4],
    up: bool,
) {
    let base = scene.vertices.len() as u32;
    let start = scene.indices.len() as u32;
    // Counter-clockwise seen from above.
    let mut corners = [
        [min[0], min[1]],
        [min[0], max[1]],
        [max[0], max[1]],
        [max[0], min[1]],
    ];
    if !up {
        corners.reverse();
    }
    for [x, z] in corners {
        scene.vertices.push(SceneVertex {
            position: [x, y, z],
            normal: [0.0, if up { 1.0 } else { -1.0 }, 0.0],
            uv: [x * loops, z * loops],
            lightmap_uv: [0.0; 2],
            color,
            fx: [0.0; 4],
        });
    }
    scene
        .indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    scene.batches.push(MeshBatch {
        indices: start..start + 6,
        material,
        center: [(min[0] + max[0]) * 0.5, y, (min[1] + max[1]) * 0.5],
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::environment::Image;

    fn image(name: &str) -> Image {
        Image {
            file: format!("{name}.png"),
            source: name.into(),
            sha256: "0".repeat(64),
            width: 1,
            height: 1,
        }
    }
    fn pack() -> Pack {
        Pack {
            schema_version: bri_content::environment_pack::SCHEMA,
            source: Some("test".into()),
            grounds: vec![Ground {
                id: "grass".into(),
                label: "Grass".into(),
                thumbnail: image("t"),
                texture: image("grass"),
                loops_per_unit: 0.125,
                color: [1.0; 4],
                color_multiply: true,
                scroll: [0.0; 2],
            }],
            waters: vec![Water {
                id: "lava".into(),
                label: "Lava".into(),
                thumbnail: image("t"),
                top: image("top"),
                bottom: image("bottom"),
                loops_per_unit: 0.15,
                color: [1.0; 4],
                overlay_color: [1.0, 0.4, 0.1, 0.7],
                color_multiply: true,
                height: 5.0,
                kill: true,
                scroll: [0.0; 2],
            }],
            ..Default::default()
        }
    }
    fn picture(label: &str) -> SceneImage {
        SceneImage {
            label: label.into(),
            width: 2,
            height: 2,
            rgba: vec![128; 16],
            srgb: true,
        }
    }

    #[test]
    fn planes_cover_the_plate_and_strip_cleanly() {
        let pack = pack();
        let plate = Plate {
            top: 0.0,
            min: [-10.0, -10.0],
            max: [10.0, 10.0],
        };
        let mut scene = SceneData::default();
        let settings = Settings {
            ground: Some("grass".into()),
            water: Some("lava".into()),
            water_height: Some(2.0),
            ..Default::default()
        };
        let planes = Planes::of(&pack, &settings);
        assert!(planes.ground_shows());
        append(
            &mut scene,
            &plate,
            &planes,
            Some(picture("grass")),
            Some((picture("top"), picture("bottom"))),
        );
        scene.validate().unwrap();
        assert_eq!(scene.batches.len(), 3);
        // The ground lies on the plate, tiled by world position; the sea's
        // surface is the height above it and reaches past the plate.
        let ground = &scene.vertices[0..4];
        assert!(ground.iter().all(|v| v.position[1] == 0.0));
        assert_eq!(ground[2].uv, [1.25, 1.25]);
        let sea = &scene.vertices[4..8];
        assert!(sea.iter().all(|v| v.position[1] == 2.0));
        assert_eq!(sea[0].position[0], -10.0 - SEA_MARGIN);
        // Its top faces up, its underside down.
        assert_eq!((sea[0].normal[1], scene.vertices[8].normal[1]), (1.0, -1.0));
        strip(&mut scene);
        assert!(scene.batches.is_empty());
        assert!(scene.images.iter().all(|i| i.width * i.height == 1));
        // A ground of no colour draws nothing; None is no water at all.
        let planes = Planes::of(
            &pack,
            &Settings {
                ground: Some("grass".into()),
                ground_color: Some([1.0, 1.0, 1.0, 0.0]),
                water: Some(NO_WATER.into()),
                ..Default::default()
            },
        );
        assert!(!planes.ground_shows());
        assert_eq!(planes.water.map(|w| w.is_none()), Some(true));
        append(&mut scene, &plate, &planes, Some(picture("grass")), None);
        assert!(scene.batches.is_empty());
    }
}
