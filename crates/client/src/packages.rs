//! Client presentation of the server's mod packages: HUD panels filled from
//! replicated package state, their keys, and entity box models. All of it is
//! data from client-side packages; the client never runs package code.
use anyhow::{Context, Result};
use bri_net::protocol::PublicWorld;
use bri_package_runtime::{Catalog, content};
use bri_render::scene::{GpuInstances, GpuScene, SceneRenderer, SceneTransform};
use bri_sim::session::{EntityInfo, PackageStateView};
use bri_ui::api::{PackageKey, PackagePanel, PanelAnchor};
use bri_world::{ContentRef, OwnerId};
use glam::{Mat4, Quat, Vec3};
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// The client-side packages listed in the content root's `packages.json`.
/// Problems are reported, not fatal: the base game still runs.
pub fn load(root: &Path) -> (Option<Arc<Catalog>>, Vec<String>) {
    let set = match bri_package::packages::PackageSet::load_root(root) {
        Ok(set) => set,
        Err(error) => return (None, vec![format!("{error:#}")]),
    };
    match Catalog::load(root, &set, false) {
        Ok(catalog) if catalog.packages.is_empty() => (None, Vec::new()),
        Ok(catalog) => (Some(Arc::new(catalog)), Vec::new()),
        Err(problems) => (None, problems.iter().map(ToString::to_string).collect()),
    }
}

fn rgba(c: [f32; 4]) -> [u8; 4] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}
fn show(value: Option<&serde_json::Value>) -> String {
    match value {
        None | Some(serde_json::Value::Null) => "-".into(),
        Some(serde_json::Value::Bool(b)) => if *b { "yes" } else { "no" }.into(),
        Some(serde_json::Value::Number(n)) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => format!("{:.1}", n.as_f64().unwrap_or(0.0)),
        },
        Some(serde_json::Value::String(s)) => s.chars().take(24).collect(),
        Some(other) => other.to_string().chars().take(24).collect(),
    }
}
/// HUD panels and keys for `viewer`, values read from replicated state.
/// `taken` says whether the base game already binds a letter; such keys
/// are dropped (the base game's binds win).
pub fn panels(catalog: &Catalog, state: &PackageStateView, viewer: OwnerId, taken: impl Fn(char) -> bool) -> (Vec<PackagePanel>, Vec<PackageKey>) {
    let mut panels = Vec::new();
    let mut keys = Vec::new();
    for (_, hud) in catalog.huds() {
        let text = rgba(hud.text);
        let rows = hud
            .rows
            .iter()
            .map(|row| {
                let value = content::Binding::parse(&row.bind).and_then(|b| state.get(&b, viewer));
                (row.label.clone(), show(value), row.color.map_or(text, rgba))
            })
            .collect();
        let mut hints = Vec::new();
        for k in &hud.keys {
            let letter = k.key.chars().next().unwrap_or('?').to_ascii_lowercase();
            if taken(letter) || keys.iter().any(|existing: &PackageKey| existing.key == letter) {
                continue;
            }
            hints.push((letter, k.label.clone()));
            keys.push(PackageKey { key: letter, package: k.package.clone(), command: k.command.clone() });
        }
        panels.push(PackagePanel {
            anchor: match hud.anchor {
                content::Anchor::TopLeft => PanelAnchor::TopLeft,
                content::Anchor::TopRight => PanelAnchor::TopRight,
                content::Anchor::BottomLeft => PanelAnchor::BottomLeft,
                content::Anchor::BottomRight => PanelAnchor::BottomRight,
            },
            title: hud.title.clone(),
            background: rgba(hud.background),
            accent: rgba(hud.accent),
            text,
            rows,
            keys: hints,
        });
    }
    (panels, keys)
}

/// Each entity's model as box instances: feet at the entity, facing its
/// yaw, colours following its label.
pub fn box_instances(catalog: &Catalog, entities: &BTreeMap<u64, EntityInfo>, cube: f32) -> Vec<SceneTransform> {
    let mut out = Vec::new();
    for e in entities.values() {
        let Some(model) = catalog.model(&e.model) else { continue };
        let frame = Mat4::from_rotation_translation(Quat::from_rotation_y(-e.yaw), Vec3::from(e.position));
        for b in &model.boxes {
            let color = b.label_colors.get(&e.label).copied().unwrap_or(b.color);
            let scale = Vec3::from(b.size) / cube;
            out.push(SceneTransform {
                transform: frame * Mat4::from_scale_rotation_translation(scale, Quat::IDENTITY, Vec3::from(b.center)),
                tint: color.map(|v| v.clamp(0.0, 1.0)),
            });
        }
    }
    out
}

/// One white cube drawn as a brick, instanced once per model box.
pub const MODEL_CUBE: &str = "v20/brick/brick4xcubedata";
const MAX_BOXES: usize = 4096;
#[derive(Default)]
pub struct PackageModels {
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    count: usize,
}
impl PackageModels {
    pub fn clear(&mut self) {
        self.instances = None;
        self.count = 0;
    }
    #[allow(clippy::too_many_arguments)]
    pub fn upload(
        &mut self,
        catalog: Option<&Catalog>,
        entities: &BTreeMap<u64, EntityInfo>,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
        materials: &crate::materials::BrickMaterials,
    ) -> Result<()> {
        let Some(catalog) = catalog else {
            self.count = 0;
            return Ok(());
        };
        let Some(mesh) = meshes.get(MODEL_CUBE) else {
            self.count = 0;
            return Ok(());
        };
        let cube = mesh.footprint_studs[0] as f32 * 0.5;
        let mut transforms = box_instances(catalog, entities, cube);
        transforms.truncate(MAX_BOXES);
        if self.gpu.is_none() && !transforms.is_empty() {
            let world = PublicWorld {
                name: "Package models".into(),
                map_id: "package-models".into(),
                palette: vec![[1.0; 4]],
                bricks: bri_world::Bricks::unit(0, bri_world::Brick::new(ContentRef::Resolved(MODEL_CUBE.into()), [0.0; 3], 0)),
            };
            let data = crate::world_scene::build_world_scene_materials(&world, meshes, 200_000, Some(materials))?;
            self.gpu = Some(renderer.upload(device, queue, &data).context("Package model cube")?);
        }
        if self.instances.is_none() && !transforms.is_empty() {
            self.instances = Some(GpuInstances::new(device, MAX_BOXES)?);
        }
        if let Some(instances) = &mut self.instances {
            instances.update(queue, &transforms)?;
        }
        self.count = transforms.len();
        Ok(())
    }
    pub fn draws(&self) -> Vec<(&GpuScene, &GpuInstances)> {
        match (&self.gpu, &self.instances) {
            (Some(gpu), Some(instances)) if self.count > 0 => vec![(gpu, instances)],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_package::packages::{PackageEntry, PackageSet, Side};

    fn catalog() -> Catalog {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/stresslab");
        let packages = [
            ("stresslab-world", Side::Server),
            ("stresslab-creeper", Side::Server),
            ("stresslab-creeper-model", Side::Client),
            ("stresslab-economy", Side::Server),
            ("stresslab-hud", Side::Client),
        ]
        .into_iter()
        .map(|(id, side)| PackageEntry { id: id.into(), version: "1.0.0".into(), side, dir: id.into(), role: None })
        .collect();
        Catalog::load(&root, &PackageSet { schema_version: 1, packages }, false).unwrap()
    }

    #[test]
    fn miner_panel_shows_the_viewers_server_state() {
        let catalog = catalog();
        let mut state = PackageStateView::default();
        let ns = state.packages.entry("stresslab-economy".into()).or_default();
        ns.players.insert(4, [("bits".to_string(), serde_json::json!(125)), ("copper".to_string(), serde_json::json!(3))].into());
        ns.players.insert(5, [("bits".to_string(), serde_json::json!(9))].into());
        let (panels, keys) = panels(&catalog, &state, 4, |c| c == 'g');
        assert_eq!(panels.len(), 1);
        let p = &panels[0];
        assert_eq!(p.title, "STRESS LAB MINER");
        assert_eq!(p.rows[0], ("Bits".into(), "125".into(), [252, 209, 77, 255]));
        assert_eq!(p.rows[2].1, "3");
        assert_eq!(p.rows[1].1, "-", "no coal value yet");
        // G is taken by a base-game bind here, so only F is offered.
        assert_eq!(keys.iter().map(|k| k.key).collect::<Vec<_>>(), ['f']);
        assert_eq!(p.keys, vec![('f', "Mine".to_string())]);
    }

    #[test]
    fn creeper_boxes_follow_the_entity_and_its_fuse_label() {
        let catalog = catalog();
        let mut entity = EntityInfo {
            id: 1,
            kind: "stresslab-creeper:entity/creeper".into(),
            model: "stresslab-creeper-model:model/creeper".into(),
            position: [10.0, 4.0, -3.0],
            yaw: 0.0,
            label: "chase".into(),
        };
        let boxes = box_instances(&catalog, &[(1, entity.clone())].into(), 2.0);
        assert_eq!(boxes.len(), 9);
        let body = boxes[0].transform.transform_point3(Vec3::ZERO);
        assert!((body - Vec3::new(10.0, 5.35, -3.0)).length() < 1e-4);
        assert!(boxes[0].tint[1] > boxes[0].tint[0], "green");
        entity.label = "fuse_a".into();
        let lit = box_instances(&catalog, &[(1, entity)].into(), 2.0);
        assert!(lit[0].tint.iter().take(3).all(|c| *c > 0.9), "flashes white");
    }
}
