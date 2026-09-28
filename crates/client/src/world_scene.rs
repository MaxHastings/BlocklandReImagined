//! Derive render geometry from a replicated public world, never physics handles
//! or UI guesses. Native brick overlays and prints retain authored UVs and paint.
//! The material-free development path explicitly reports its missing resources.
use anyhow::{Context, Result, bail, ensure};
use bri_content::brick::Brick as BrickMesh;
use bri_net::protocol::PublicWorld;
use bri_render::scene::{BrickFx, Material, SceneData};
use bri_world::ContentRef;
use std::collections::BTreeMap;

/// Meshes are keyed by stable *definition* ID, since several definitions may
/// share the same native mesh. The budget rejects the entire replacement before
/// allocating its geometry; the caller must report failure, never hide excess
/// bricks or continue displaying a stale snapshot as current authoritative state.
pub fn build_world_scene(
    world: &PublicWorld,
    meshes: &BTreeMap<String, BrickMesh>,
    max_triangles: usize,
) -> Result<SceneData> {
    build_world_scene_materials(world, meshes, max_triangles, None)
}

pub fn build_world_scene_materials(
    world: &PublicWorld,
    meshes: &BTreeMap<String, BrickMesh>,
    max_triangles: usize,
    materials: Option<&crate::materials::BrickMaterials>,
) -> Result<SceneData> {
    ensure!(
        !world.palette.is_empty()
            && world.palette.len() <= 256
            && world
                .palette
                .iter()
                .flatten()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
        "Invalid replicated world paint palette"
    );
    let mut count = 0usize;
    for (id, brick) in &world.bricks {
        brick
            .validate(world.palette.len())
            .with_context(|| format!("Invalid replicated brick {id}"))?;
        if !brick.visible {
            continue;
        }
        let ContentRef::Resolved(definition) = &brick.definition else {
            bail!(
                "Visible brick {id} has an unresolved definition: {:?}",
                brick.definition
            );
        };
        let mesh = meshes.get(definition).with_context(|| {
            format!("Visible brick {id} definition {definition} has no native render mesh")
        })?;
        count = count
            .checked_add(
                mesh.quads
                    .len()
                    .checked_mul(2)
                    .context("Brick triangle count overflow")?,
            )
            .context("World triangle count overflow")?;
        ensure!(
            count <= max_triangles,
            "World requires {count} or more brick triangles, exceeding configured render budget {max_triangles}; no bricks were omitted"
        );
    }
    let mut scene = SceneData {
        id: format!("{}/replicated-bricks", world.map_id),
        name: format!("{} bricks", world.name),
        ..Default::default()
    };
    let surface_materials = if let Some(materials) = materials {
        materials.surface_materials(&mut scene)
    } else {
        scene.materials.push(Material::vertex_lit(
            "Development brick color; original surface images not yet bound",
            0,
        ));
        scene.omissions.push("Original brick TOP/SIDE/BOTTOMEDGE/BOTTOMLOOP/RAMP texture resources are not yet bound; this pass draws native geometry and paint with a white diffuse material".into());
        scene
            .omissions
            .push("Brick print images are not bound to this development scene".into());
        [0; 6]
    };
    scene.omissions.push("Attached brick lights/emitters are bound by the host effects adapter, outside this geometry pass".into());
    scene.vertices.reserve(count.saturating_mul(2));
    scene.indices.reserve(count.saturating_mul(3));
    for (id, brick) in world.bricks.iter().filter(|(_, b)| b.visible) {
        append_world_brick(
            &mut scene,
            *id,
            brick,
            &world.palette,
            meshes,
            surface_materials,
            materials,
            false,
        )?;
    }
    scene.coalesce_opaque_batches()?;
    scene.omissions.sort();
    scene.omissions.dedup();
    Ok(scene)
}

/// Append one validated visible brick with its paint, print and FX.
#[allow(clippy::too_many_arguments)] // one brick plus the shared chunk/palette context
pub(crate) fn append_world_brick(
    scene: &mut SceneData,
    id: u64,
    brick: &bri_world::Brick,
    palette: &[[f32; 4]],
    meshes: &BTreeMap<String, BrickMesh>,
    surface_materials: [usize; 6],
    materials: Option<&crate::materials::BrickMaterials>,
    mesh_validated: bool,
) -> Result<()> {
    let ContentRef::Resolved(definition) = &brick.definition else {
        bail!(
            "Visible brick {id} has an unresolved definition: {:?}",
            brick.definition
        );
    };
    let mesh = meshes.get(definition).with_context(|| {
        format!("Visible brick {id} definition {definition} has no native render mesh")
    })?;
    let mut surfaces = surface_materials;
    if let (Some(materials), Some(print)) = (materials, &brick.print) {
        let name = match print {
            ContentRef::Resolved(id) => id,
            ContentRef::Unresolved { namespace, name }
                if namespace.eq_ignore_ascii_case("print") =>
            {
                name
            }
            _ => bail!("Brick {id} has unsupported print namespace"),
        };
        surfaces[5] = materials.print_material(scene, name)?;
    }
    if !mesh_validated {
        mesh.validate()?;
    }
    scene
        .append_validated_brick_with_fx(
            mesh,
            brick.transform().to_cols_array(),
            palette[brick.color as usize],
            surfaces,
            BrickFx::new(brick.color_effect, brick.shape_effect)?,
        )
        .with_context(|| format!("Building native geometry for brick {id}"))
}

/// The player's temp brick options (Options > Advanced,
/// `$pref::HUD::tempBrick*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempBrickLook {
    /// The outside colour, or `None` for the paint colour times 1.5.
    pub outside: Option<[f32; 3]>,
    /// The inside colour, or `None` for the paint colour.
    pub inside: Option<[f32; 3]>,
    /// Flash period (ms), and the opacity's range and offset.
    pub flash_ms: f32,
    pub flash_range: f32,
    pub flash_offset: f32,
}
impl Default for TempBrickLook {
    /// v20's defaults: paint outside, black inside, 800 ms between 0.3 and 0.6.
    fn default() -> Self {
        Self {
            outside: None,
            inside: Some([0.0; 3]),
            flash_ms: 800.0,
            flash_range: 0.3,
            flash_offset: 0.3,
        }
    }
}
impl TempBrickLook {
    pub fn from_prefs(p: &bri_ui::prefs::Prefs) -> Self {
        let d = Self::default();
        let rgb = |side: &str| {
            ["Red", "Green", "Blue"].map(|c| {
                p.f32_or(&format!("$pref::HUD::tempBrick{side}{c}"), 0.0)
                    .clamp(0.0, 1.0)
            })
        };
        Self {
            outside: (!p.bool_or("$pref::HUD::tempBrickOutsideUsePaintColor", true))
                .then(|| rgb("Outside")),
            inside: (!p.bool_or("$pref::HUD::tempBrickInsideUsePaintColor", false))
                .then(|| rgb("Inside")),
            flash_ms: p
                .f32_or("$pref::HUD::tempBrickFlashTime", d.flash_ms)
                .clamp(100.0, 10_000.0),
            flash_range: p
                .f32_or("$pref::HUD::tempBrickFlashRange", d.flash_range)
                .clamp(0.0, 1.0),
            flash_offset: p
                .f32_or("$pref::HUD::tempBrickFlashoffset", d.flash_offset)
                .clamp(0.0, 1.0),
        }
    }
}

/// v20 temp (ghost) brick look, from `blocklandv20.exe` 0x52e370/0x52e860:
/// every quad is pushed 0.02 units out along its normals and drawn twice in
/// the translucent brick pass. The reversed-winding copy shows the far inner
/// walls in the inside colour (default black); the forward copy is the paint
/// colour times 1.5 (or the outside colour). Both flash via
/// `Material::temp_brick_flash` and keep the normal surface overlays.
pub fn v20_temp_brick(scene: &mut SceneData, look: &TempBrickLook) {
    const INFLATE: f32 = 0.02;
    for vertex in &mut scene.vertices {
        let n = glam::Vec3::from(vertex.normal).normalize_or_zero();
        vertex.position = (glam::Vec3::from(vertex.position) + n * INFLATE).to_array();
    }
    let inside_base = scene.vertices.len() as u32;
    let inside: Vec<_> = scene
        .vertices
        .iter()
        .map(|v| bri_render::scene::SceneVertex {
            color: match look.inside {
                Some([r, g, b]) => [r, g, b, 1.0],
                None => [v.color[0], v.color[1], v.color[2], 1.0],
            },
            ..*v
        })
        .collect();
    for vertex in &mut scene.vertices {
        let [r, g, b, _] = vertex.color;
        vertex.color = match look.outside {
            Some([r, g, b]) => [r, g, b, 1.0],
            None => [r * 1.5, g * 1.5, b * 1.5, 1.0],
        };
    }
    scene.vertices.extend(inside);
    let mut indices = Vec::with_capacity(scene.indices.len() * 2);
    for batch in &mut scene.batches {
        let start = indices.len() as u32;
        let original = &scene.indices[batch.indices.start as usize..batch.indices.end as usize];
        for triangle in original.chunks_exact(3) {
            indices.extend([triangle[0], triangle[2], triangle[1]].map(|i| i + inside_base));
        }
        indices.extend_from_slice(original);
        batch.indices = start..indices.len() as u32;
    }
    scene.indices = indices;
    for material in &mut scene.materials {
        material.alpha = bri_render::scene::AlphaMode::Blend;
        material.temp_brick_flash = true;
        material.double_sided = false;
        material.parameters = Some([
            [
                look.flash_ms / 1000.0,
                look.flash_range,
                look.flash_offset,
                0.0,
            ],
            [0.0; 4],
            [0.0; 4],
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::brick::{Face, Quad, Surface, Vertex};
    use bri_render::scene::AlphaMode;

    fn mesh() -> BrickMesh {
        BrickMesh {
            schema_version: 1,
            id: "mesh/shared".into(),
            footprint_studs: [1, 1],
            height_plates: 1,
            attachment_rows: vec!["b".into()],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![Quad {
                face: Face::Omni,
                surface: Surface::Side,
                vertices: [
                    [1.0, 0.0, 0.0],
                    [2.0, 0.0, 0.0],
                    [2.0, 1.0, 0.0],
                    [1.0, 1.0, 0.0],
                ]
                .map(|position| Vertex {
                    position,
                    normal: [0.0, 0.0, 1.0],
                    uv: [0.25, 0.75],
                }),
                colors: None,
            }],
        }
    }
    fn world() -> PublicWorld {
        PublicWorld {
            name: "Test".into(),
            map_id: "map/test".into(),
            palette: vec![[0.9, 0.2, 0.1, 1.0], [0.2, 0.4, 0.8, 0.5]],
            bricks: Default::default(),
        }
    }
    fn brick(position: [f32; 3]) -> bri_world::Brick {
        bri_world::Brick::new(ContentRef::Resolved("definition/a".into()), position, 1)
    }

    #[test]
    fn paint_hidden_rotation_and_translucent_geometry() {
        let meshes = BTreeMap::from([("definition/a".into(), mesh())]);
        let mut world = world();
        let mut rotated = brick([3.0, 4.0, 5.0]);
        rotated.quarter_turns = 1;
        let mut translucent = brick([10.0, 0.0, 0.0]);
        translucent.color = 1;
        let mut hidden = brick([50.0, 50.0, 50.0]);
        hidden.visible = false;
        hidden.definition = ContentRef::Resolved("not_loaded_but_hidden".into());
        world.bricks.insert(1, rotated);
        world.bricks.insert(2, translucent);
        world.bricks.insert(3, hidden);
        let scene = build_world_scene(&world, &meshes, 4).unwrap();
        assert_eq!((scene.vertices.len(), scene.indices.len()), (8, 12));
        let position = scene.vertices[0].position;
        for (actual, expected) in position.into_iter().zip([3.0, 4.0, 6.0]) {
            assert!((actual - expected).abs() < 0.0001);
        }
        assert_eq!(scene.vertices[0].color, world.palette[0]);
        assert_eq!(scene.vertices[4].color, world.palette[1]);
        assert_eq!(scene.vertices[0].uv, [0.25, 0.75]);
        assert!(scene.materials.iter().any(|m| m.alpha == AlphaMode::Blend));
        assert!(scene.omissions.iter().any(|s| s.contains("not yet bound")));
    }

    #[test]
    fn opaque_coalescing_and_missing_or_overbudget_worlds_reject() {
        let meshes = BTreeMap::from([("definition/a".into(), mesh())]);
        let mut world = world();
        world.bricks.insert(1, brick([0.0; 3]));
        world.bricks.insert(2, brick([2.0, 0.0, 0.0]));
        let scene = build_world_scene(&world, &meshes, 4).unwrap();
        assert_eq!(scene.batches.len(), 1);
        assert_eq!(scene.indices.len(), 12);
        assert!(
            build_world_scene(&world, &meshes, 3)
                .unwrap_err()
                .to_string()
                .contains("budget")
        );
        assert!(
            build_world_scene(&world, &BTreeMap::new(), 4)
                .unwrap_err()
                .to_string()
                .contains("no native render mesh")
        );
        world.bricks.get_mut(&1).unwrap().definition = ContentRef::Unresolved {
            namespace: "stock".into(),
            name: "unknown".into(),
        };
        assert!(
            build_world_scene(&world, &meshes, 4)
                .unwrap_err()
                .to_string()
                .contains("unresolved")
        );
    }

    #[test]
    fn signed_paint_offsets_are_not_negative_opacity_or_literal_colors() {
        let mut geometry = mesh();
        geometry.quads[0].colors = Some([
            [0.2, -0.3, 0.1, -1.0],
            [-0.1, 0.0, 0.0, -1.0],
            [0.3, 0.5, 0.7, 0.15],
            [200.0, 150.0, 0.0, 1.0],
        ]);
        let meshes = BTreeMap::from([("definition/a".into(), geometry)]);
        let mut world = world();
        world.bricks.insert(1, brick([0.0; 3]));
        let scene = build_world_scene(&world, &meshes, 2).unwrap();
        assert_eq!(scene.vertices[0].color, [1.0, 0.0, 0.2, 1.0]);
        assert!((scene.vertices[1].color[0] - 0.8).abs() < 0.00001);
        assert_eq!(scene.vertices[2].color, [0.3, 0.5, 0.7, 0.15]);
        assert_eq!(scene.vertices[3].color, [200.0, 150.0, 0.0, 1.0]);
        assert_eq!(
            meshes["definition/a"].quads[0].colors.unwrap()[3],
            [200., 150., 0., 1.]
        );
        assert!(
            scene
                .omissions
                .iter()
                .any(|s| s.contains("out-of-range literal input conversion remains unverified"))
        );
        world.bricks.get_mut(&1).unwrap().color = 1;
        let scene = build_world_scene(&world, &meshes, 2).unwrap();
        assert_eq!(scene.vertices[0].color[3], 0.5);
        assert!(
            scene
                .omissions
                .iter()
                .any(|s| s.contains("native sentinel/opacity adaptation"))
        );
    }

    #[test]
    fn authored_vertex_colors_override_paint_and_empty_world_is_valid() {
        let empty = world();
        assert!(
            build_world_scene(&empty, &BTreeMap::new(), 0)
                .unwrap()
                .vertices
                .is_empty()
        );
        let mut geometry = mesh();
        geometry.quads[0].colors = Some([[0.3, 0.5, 0.7, 0.25]; 4]);
        let meshes = BTreeMap::from([("definition/a".into(), geometry)]);
        let mut world = world();
        world.bricks.insert(1, brick([0.0; 3]));
        let scene = build_world_scene(&world, &meshes, 2).unwrap();
        assert_eq!(scene.vertices[0].color, [0.3, 0.5, 0.7, 0.25]);
        assert_eq!(
            scene.materials[scene.batches[0].material].alpha,
            AlphaMode::Blend
        );
        world.palette[0][0] = f32::NAN;
        assert!(build_world_scene(&world, &meshes, 2).is_err());
    }
}
