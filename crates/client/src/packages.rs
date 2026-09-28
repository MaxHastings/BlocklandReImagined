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
    load_side(root, false)
}
/// Every package a host would run, server-side ones included.
pub fn load_server(root: &Path) -> (Option<Arc<Catalog>>, Vec<String>) {
    load_side(root, true)
}
fn load_side(root: &Path, server: bool) -> (Option<Arc<Catalog>>, Vec<String>) {
    let set = match bri_package::packages::PackageSet::load_root(root) {
        Ok(set) => set,
        Err(error) => return (None, vec![format!("{error:#}")]),
    };
    load_set(root, &set, server)
}
/// Mod packages of `set` whose directories are under `root`.
pub fn load_set(root: &Path, set: &bri_package::packages::PackageSet, server: bool) -> (Option<Arc<Catalog>>, Vec<String>) {
    // One broken Add-On is left out (and reported) rather than turning off
    // every other Add-On's HUD, rules and modes.
    let (catalog, problems) = Catalog::load_skipping(root, set, server);
    let problems = problems.iter().map(ToString::to_string).collect();
    if catalog.packages.is_empty() {
        (None, problems)
    } else {
        (Some(Arc::new(catalog)), problems)
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
/// What a hosted game runs: the packages, the map players see chosen, the
/// base map it stands on and the key its package state is saved under.
#[derive(Debug)]
pub struct Hosted {
    pub catalog: Option<Arc<Catalog>>,
    pub map: String,
    pub base_map: String,
    pub save_key: String,
}
/// Resolve Start Game's choice of `map` and game `mode` (None: Custom).
/// Custom on a package world runs every enabled Add-On except those needing
/// another world; Custom on a base map runs the plain base game. A mode runs
/// its own Add-Ons, on its own map when it names one.
pub fn hosted(server: Option<&Arc<Catalog>>, map: &str, mode: Option<&str>) -> Result<Hosted> {
    let problems = |p: Vec<bri_package::diag::Diagnostic>| {
        anyhow::anyhow!(p.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))
    };
    let (catalog, map, save_key) = match (server, mode) {
        (None, Some(mode)) => anyhow::bail!("The game mode {mode} is not turned on in Add-Ons"),
        (None, None) => (None, map.to_owned(), map.to_owned()),
        (Some(server), Some(mode)) => {
            let def = server
                .modes()
                .find(|(id, _)| id.as_str() == mode)
                .map(|(_, m)| m.clone())
                .with_context(|| format!("The game mode {mode} is not turned on in Add-Ons"))?;
            let catalog = server.for_mode(mode).map_err(problems)?;
            let map = def.map.clone().unwrap_or_else(|| map.to_owned());
            if let Some((_, world, _)) = catalog.world() {
                anyhow::ensure!(*world == map, "{} plays on its own world", def.name);
            } else {
                anyhow::ensure!(!map.contains(':'), "{} does not bring that world; pick a map", def.name);
            }
            let key = format!("{mode}-{map}");
            (Some(catalog), map, key)
        }
        (Some(server), None) if server.packages.values().any(|p| p.worlds.contains_key(map)) => {
            (Some(server.for_world(map).map_err(problems)?), map.to_owned(), map.to_owned())
        }
        (Some(_), None) if map.contains(':') => anyhow::bail!("No Add-On that is turned on provides {map}"),
        (Some(_), None) => (None, map.to_owned(), map.to_owned()),
    };
    // Nothing to run: host the plain base game.
    let catalog = catalog.filter(|c| c.world().is_some() || c.behaviours().next().is_some());
    let base_map = catalog
        .as_ref()
        .and_then(|c| c.world().map(|(_, _, w)| w.environment.clone()))
        .unwrap_or_else(|| map.clone());
    Ok(Hosted {
        catalog: catalog.map(Arc::new),
        map,
        base_map,
        save_key,
    })
}
/// Start Game's game modes: every mode an enabled Add-On declares.
pub fn modes(server: Option<&Arc<Catalog>>) -> Vec<bri_ui::api::GameModeInfo> {
    let Some(server) = server else {
        return Vec::new();
    };
    let mut out: Vec<_> = server
        .modes()
        .map(|(id, m)| bri_ui::api::GameModeInfo {
            id: id.clone(),
            name: m.name.clone(),
            description: m.description.clone(),
            map: m.map.clone(),
        })
        .collect();
    out.sort_by_key(|m| m.name.to_ascii_lowercase());
    out
}
/// Start Game entries for the world providers a host can run, standing on
/// their environment map (whose preview they borrow).
pub fn world_maps(catalog: &Catalog, maps: &[bri_ui::api::MapInfo]) -> Vec<bri_ui::api::MapInfo> {
    catalog
        .packages
        .values()
        .flat_map(|p| p.worlds.iter().map(move |(id, w)| (p, id, w)))
        .filter_map(|(p, id, w)| {
            let base = maps.iter().find(|m| m.id == w.environment)?;
            Some(bri_ui::api::MapInfo {
                id: id.clone(),
                name: p.manifest.name.clone(),
                // Player-facing: "Add-On", never "package".
                description: format!("{} (Add-On {} {})", p.manifest.description, p.id(), p.manifest.version),
                preview: base.preview.clone(),
            })
        })
        .collect()
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

/// One package box model to draw: feet at `position`, facing `yaw`, colours
/// following `label`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement<'a> {
    pub model: &'a str,
    pub position: [f32; 3],
    pub yaw: f32,
    pub label: &'a str,
}
/// Every entity where it stands.
pub fn entity_placements(entities: &BTreeMap<u64, EntityInfo>) -> impl Iterator<Item = Placement<'_>> {
    entities.values().map(|e| Placement { model: &e.model, position: e.position, yaw: e.yaw, label: &e.label })
}
/// Players whose archetype's look is a package model: they draw as that
/// model in place of the Blockhead.
pub fn body_placements<'a>(
    catalog: &Catalog,
    archetypes: &'a bri_sim::archetype::Archetypes,
    players: &BTreeMap<OwnerId, bri_sim::player::PlayerState>,
) -> Vec<(OwnerId, Placement<'a>)> {
    players
        .iter()
        .filter_map(|(owner, p)| {
            let model = &archetypes.get(p.archetype)?.look.model;
            catalog.model(model)?;
            Some((*owner, Placement { model, position: p.feet, yaw: p.yaw, label: "" }))
        })
        .collect()
}
/// Each entity's model as box instances.
pub fn box_instances(catalog: &Catalog, entities: &BTreeMap<u64, EntityInfo>, cube: f32) -> Vec<SceneTransform> {
    place_boxes(catalog, entity_placements(entities), cube)
}
/// Each placed model as box instances.
pub fn place_boxes<'a>(catalog: &Catalog, placements: impl IntoIterator<Item = Placement<'a>>, cube: f32) -> Vec<SceneTransform> {
    let mut out = Vec::new();
    for e in placements {
        let Some(model) = catalog.model(e.model) else { continue };
        let frame = Mat4::from_rotation_translation(Quat::from_rotation_y(-e.yaw), Vec3::from(e.position));
        for b in &model.boxes {
            let color = b.label_colors.get(e.label).copied().unwrap_or(b.color);
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
        placements: Vec<Placement<'_>>,
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
        let mut transforms = place_boxes(catalog, placements, cube);
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
        load(false)
    }
    fn load(server: bool) -> Catalog {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/stresslab");
        let bricks = PackageEntry {
            id: "v20-bricks".into(),
            version: "4.0.0".into(),
            side: Side::Shared,
            dir: "unused".into(),
            role: Some("brick_catalog".into()),
        };
        let packages = [
            ("stresslab-world", Side::Server),
            ("stresslab-creeper", Side::Server),
            ("stresslab-creeper-model", Side::Client),
            ("stresslab-economy", Side::Server),
            ("stresslab-hud", Side::Client),
            ("stresslab-mode", Side::Server),
        ]
        .into_iter()
        .map(|(id, side)| PackageEntry { id: id.into(), version: "1.0.0".into(), side, dir: id.into(), role: None })
        .chain([bricks])
        .collect();
        Catalog::load(&root, &PackageSet { schema_version: 1, packages }, server).unwrap()
    }

    #[test]
    fn hosting_runs_the_chosen_mode_or_the_plain_base_game() {
        let server = Arc::new(load(true));
        let strata = "stresslab-world:world/strata";
        let infos = modes(Some(&server));
        assert_eq!(infos.len(), 1);
        assert_eq!((infos[0].name.as_str(), infos[0].map.as_deref()), ("Stress Lab", Some(strata)));
        // The mode picks its own world, whatever map Start Game had selected.
        let mode = hosted(Some(&server), "Slate", Some(&infos[0].id)).unwrap();
        assert_eq!(mode.map, strata);
        assert_ne!(mode.base_map, strata, "stands on the world's environment");
        assert_eq!(mode.save_key, format!("{}-{strata}", infos[0].id));
        assert!(mode.catalog.as_ref().is_some_and(|c| c.world().is_some()));
        // Custom on the package world runs the Add-Ons made for it.
        let world = hosted(Some(&server), strata, None).unwrap();
        assert!(world.catalog.is_some_and(|c| c.packages.contains_key("stresslab-creeper")));
        // Custom on a base map runs no Add-On rules.
        let base = hosted(Some(&server), "Slate", None).unwrap();
        assert!(base.catalog.is_none());
        assert_eq!((base.map.as_str(), base.save_key.as_str()), ("Slate", "Slate"));
        let off = hosted(None, "Slate", Some(&infos[0].id)).unwrap_err().to_string();
        assert!(off.contains("not turned on in Add-Ons"), "{off}");
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
        // G is taken by a base-game bind here, so only H and J are offered.
        assert_eq!(keys.iter().map(|k| k.key).collect::<Vec<_>>(), ['h', 'j']);
        assert_eq!(p.keys[0], ('h', "Mine".to_string()));
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

    #[test]
    fn a_player_whose_archetype_looks_like_a_package_model_draws_as_it() {
        let catalog = catalog();
        let mut archetypes = bri_sim::archetype::Archetypes::default();
        let mut creeper = archetypes.resolve(Default::default()).clone();
        creeper.id = "stresslab-creeper:archetype/creeper".into();
        creeper.look.model = "stresslab-creeper-model:model/creeper".into();
        let id = archetypes.add(creeper).unwrap();
        let player = |archetype, x| bri_sim::player::PlayerState {
            owner: 0,
            feet: [x, 0.0, 0.0],
            velocity: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            grounded: true,
            crouched: false,
            jetting: false,
            jump: Default::default(),
            archetype,
            scale: 1.0,
            energy: 100.0,
        };
        let players = BTreeMap::from([(1, player(Default::default(), 0.0)), (2, player(id, 7.0))]);
        let bodies = body_placements(&catalog, &archetypes, &players);
        assert_eq!(bodies.len(), 1, "the Blockhead stays a Blockhead");
        assert_eq!(bodies[0].0, 2);
        let boxes = place_boxes(&catalog, bodies.into_iter().map(|(_, p)| p), 2.0);
        assert_eq!(boxes.len(), 9);
        assert!((boxes[0].transform.transform_point3(Vec3::ZERO).x - 7.0).abs() < 1e-4);
    }
}
