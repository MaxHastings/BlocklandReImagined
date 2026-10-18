//! HUD regions Add-Ons claim (`bri_content::hud`): which claim wins each
//! region for this viewer this frame, its elements resolved from the
//! game's values and the rules' replicated state, and the textures they
//! draw, uploaded for the UI. Data only: nothing here runs Add-On code.
use bri_content::hud::{
    Claim, Contact, FeedKill, Item, Layout, Live, Region, Source, TextureInfo, Value, Viewer,
};
use bri_package_runtime::Catalog;
use bri_render::scene::SceneImage;
use bri_sim::session::PackageStateView;
use bri_ui::api::HudRegionDraw;
use bri_world::OwnerId;
use std::collections::{BTreeMap, BTreeSet};

/// Texture keys of HUD region images (`bri_ui::pack::TexKey::External`).
const KEY_BASE: u64 = 0x4855_0000;
/// Most textures all layouts together draw.
const MAX_TEXTURES: usize = 512;

/// The game's own values a region can show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameValues {
    pub health: f32,
    pub health_max: f32,
    pub energy: f32,
    pub alive: bool,
    pub center_print: String,
    pub bottom_print: String,
    /// The held gun's ammunition (`Notice::Ammo`).
    pub ammo: Option<bri_sim::session::HeldAmmo>,
    pub inventory_ammo: bri_sim::session::InventoryAmmoSlots,
    /// Seconds until the dead viewer may respawn (0: click to), while dead
    /// and not held.
    pub respawn: Option<f32>,
    /// The latest deaths, oldest first, for kill feeds.
    pub kills: Vec<FeedKill>,
    /// Other players as trackers see them.
    pub contacts: Vec<Contact>,
    /// The viewer's level facing (x, z), for edge arrows.
    pub facing: [f32; 2],
}

/// The layouts of one catalog and their textures.
#[derive(Default)]
pub struct HudRegions {
    /// The Add-Ons (id, version) these were read from.
    read: Option<Vec<(String, String)>>,
    /// Its layouts: (package, HUD file id, layout).
    layouts: Vec<(String, String, Layout)>,
    /// Texture id to key and size.
    keys: BTreeMap<String, TextureInfo>,
    images: BTreeMap<u64, SceneImage>,
    uploaded: bool,
    /// Conflicts already reported: (region, winner, loser).
    reported: BTreeSet<(Region, String, String)>,
    /// The stock texts the drawn layouts replace, by key.
    texts: BTreeMap<String, String>,
    /// A drawn layout shows a kill feed.
    kill_feed: bool,
    /// Whether a drawn layout shows a motion tracker.
    tracker: bool,
}

impl HudRegions {
    /// Read `catalog`'s layouts and textures, once per set of Add-Ons.
    pub fn use_catalog(&mut self, catalog: Option<&Catalog>) {
        let fingerprint: Vec<(String, String)> = catalog
            .map(|c| {
                c.packages
                    .iter()
                    .filter(|(_, p)| !p.hud_regions.is_empty())
                    .map(|(id, p)| (id.clone(), format!("{:?}", p.version)))
                    .collect()
            })
            .unwrap_or_default();
        if self.read.as_ref() == Some(&fingerprint) {
            return;
        }
        self.read = Some(fingerprint);
        self.layouts.clear();
        self.keys.clear();
        self.images.clear();
        self.uploaded = false;
        let Some(catalog) = catalog else {
            return;
        };
        self.layouts = catalog
            .hud_regions()
            .map(|(package, id, layout)| (package.to_string(), id.clone(), layout.clone()))
            .collect();
        for (package, _, layout) in catalog.hud_regions() {
            let Some(p) = catalog.packages.get(package) else {
                continue;
            };
            for texture in layout.textures() {
                if self.keys.contains_key(texture) || self.keys.len() >= MAX_TEXTURES {
                    continue;
                }
                let decoded = p
                    .assets
                    .iter()
                    .find(|a| a.id == texture)
                    .and_then(|a| image::load_from_memory(&a.bytes).ok())
                    .map(|i| i.to_rgba8());
                let Some(rgba) = decoded else {
                    crate::add_on_health::report(bri_package::health::Problem::new(
                        package,
                        bri_package::health::Kind::Presentation,
                        texture,
                        "its HUD texture could not be read; the element is left out",
                    ));
                    continue;
                };
                let key = KEY_BASE + self.keys.len() as u64;
                let (width, height) = rgba.dimensions();
                self.keys
                    .insert(texture.to_string(), (key, [width, height]));
                self.images.insert(
                    key,
                    SceneImage {
                        label: texture.to_string(),
                        width,
                        height,
                        rgba: rgba.into_raw(),
                        srgb: false,
                    },
                );
            }
        }
    }

    /// The regions claimed for `viewer` this frame, resolved. A layout
    /// reading the state of a rule the server does not run is left out, so
    /// its region shows the stock HUD.
    pub fn draws(
        &mut self,
        state: &PackageStateView,
        owner: OwnerId,
        viewer: &Viewer<'_>,
        game: &GameValues,
    ) -> Vec<HudRegionDraw> {
        let layouts = std::mem::take(&mut self.layouts);
        let out = self.resolve(&layouts, state, owner, viewer, game);
        self.layouts = layouts;
        out
    }
    /// A drawn layout's text for a stock prompt (`Layout::texts`); `""`
    /// hides it, `None` keeps the game's own.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.texts.get(key).map(String::as_str)
    }
    /// Every stock prompt the drawn layouts replace.
    pub fn texts(&self) -> &BTreeMap<String, String> {
        &self.texts
    }
    /// Whether a drawn layout shows a kill feed, so kills stay out of chat.
    pub fn kill_feed(&self) -> bool {
        self.kill_feed
    }
    /// Whether a drawn layout shows a motion tracker, so contacts are built.
    pub fn tracker(&self) -> bool {
        self.tracker
    }
    /// Nothing is drawn now (no view or catalog): stock prompts and chat
    /// come back.
    pub fn forget_drawn(&mut self) {
        self.texts.clear();
        self.kill_feed = false;
        self.tracker = false;
    }
    fn resolve(
        &mut self,
        layouts: &[(String, String, Layout)],
        state: &PackageStateView,
        owner: OwnerId,
        viewer: &Viewer<'_>,
        game: &GameValues,
    ) -> Vec<HudRegionDraw> {
        let mut claims: BTreeMap<Region, Vec<(Claim<'_>, &str)>> = BTreeMap::new();
        for (package, id, layout) in layouts {
            if !layout.when.matches(viewer) || !serves(state, layout) {
                continue;
            }
            claims
                .entry(layout.region)
                .or_default()
                .push((Claim { package, layout }, id.as_str()));
        }
        let value = |source: &Source| -> Option<Value> {
            Some(match source {
                Source::Health => Value::Number(game.health),
                Source::HealthMax => Value::Number(game.health_max),
                Source::Energy => Value::Number(game.energy),
                Source::Alive => Value::Number(if game.alive { 1.0 } else { 0.0 }),
                Source::CenterPrint => Value::Text(game.center_print.clone()),
                Source::BottomPrint => Value::Text(game.bottom_print.clone()),
                Source::Ammo => Value::Number(game.ammo.as_ref()?.rounds? as f32),
                Source::AmmoMax => Value::Number(game.ammo.as_ref()?.size as f32),
                Source::AmmoReserve => Value::Number(game.ammo.as_ref()?.reserve? as f32),
                Source::Reloading => {
                    Value::Number(f32::from(u8::from(game.ammo.as_ref()?.reloading)))
                }
                Source::Respawn => Value::Number(game.respawn?),
                Source::ItemAmmo { item, field } => {
                    use bri_content::hud::AmmoField;
                    let ammo = game
                        .inventory_ammo
                        .iter()
                        .flatten()
                        .find(|a| a.item == *item);
                    let number = match (ammo, field) {
                        (None, _) => 0.,
                        (Some(a), AmmoField::Rounds) => a.rounds? as f32,
                        (Some(a), AmmoField::Maximum) => a.capacity as f32,
                        (Some(a), AmmoField::Reserve) => a.reserve? as f32,
                        (Some(a), AmmoField::Reloading) => f32::from(u8::from(a.reloading)),
                    };
                    Value::Number(number)
                }
                Source::State(bind) => {
                    let binding = bri_package_runtime::content::Binding::parse(bind)?;
                    match state.get(&binding, owner)? {
                        serde_json::Value::Number(n) => Value::Number(n.as_f64()? as f32),
                        serde_json::Value::Bool(b) => Value::Number(f32::from(u8::from(*b))),
                        serde_json::Value::String(s) => Value::Text(s.clone()),
                        _ => return None,
                    }
                }
            })
        };
        let keys = &self.keys;
        let texture = |id: &str| keys.get(id).copied();
        let live = Live {
            kills: &game.kills,
            contacts: &game.contacts,
            facing: game.facing,
        };
        let mut texts = BTreeMap::new();
        let mut kill_feed = false;
        let mut tracker = false;
        let mut out = Vec::new();
        for (region, mut list) in claims {
            list.sort_by_key(|(c, _)| c.rank());
            let (winner, _) = list[0];
            for (loser, file) in &list[1..] {
                let key = (
                    region,
                    winner.package.to_string(),
                    loser.package.to_string(),
                );
                if loser.package != winner.package && self.reported.insert(key) {
                    crate::add_on_health::report(bri_package::health::Problem::new(
                        loser.package,
                        bri_package::health::Kind::Presentation,
                        *file,
                        format!(
                            "its HUD region `{}` is also claimed by {}, whose claim wins (more specific, higher priority, or first by id); this one is not drawn",
                            region.name(),
                            winner.package
                        ),
                    ));
                }
            }
            for (key, text) in &winner.layout.texts {
                texts.entry(key.clone()).or_insert_with(|| text.clone());
            }
            kill_feed |= winner.layout.has_kill_feed();
            tracker |= winner.layout.has_tracker();
            let items: Vec<Item> =
                bri_content::hud::resolve_live(winner.layout, &value, &texture, &live);
            out.push(HudRegionDraw {
                region,
                pixelated: winner.layout.pixelated,
                items,
                name_tags: winner.layout.name_tags.clone(),
            });
        }
        self.texts = texts;
        self.kill_feed = kill_feed;
        self.tracker = tracker;
        out
    }

    /// Which Add-On's layout owns each region now, from the last
    /// [`Self::draws`] inputs: for Add-On code's `hud_region`.
    pub fn owners(
        &self,
        state: &PackageStateView,
        viewer: &Viewer<'_>,
    ) -> BTreeMap<String, Vec<Region>> {
        let mut best: BTreeMap<Region, Claim<'_>> = BTreeMap::new();
        for (package, _, layout) in &self.layouts {
            if !layout.when.matches(viewer) || !serves(state, layout) {
                continue;
            }
            let claim = Claim { package, layout };
            match best.get(&layout.region) {
                Some(held) if held.rank() <= claim.rank() => {}
                _ => {
                    best.insert(layout.region, claim);
                }
            }
        }
        let mut out: BTreeMap<String, Vec<Region>> = BTreeMap::new();
        for (region, claim) in best {
            out.entry(claim.package.to_string())
                .or_default()
                .push(region);
        }
        out
    }

    pub fn gpu_stopped(&mut self) {
        self.uploaded = false;
    }
    /// Upload the textures for the UI, once per catalog and GPU.
    pub fn register_textures(&mut self, frame: &mut crate::platform::RenderContext<'_>) {
        if self.uploaded {
            return;
        }
        for (&id, image) in &self.images {
            let size = wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            };
            let texture = frame.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Add-On HUD region texture"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            frame.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &image.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * image.width),
                    rows_per_image: Some(image.height),
                },
                size,
            );
            frame.ui_renderer.set_external(
                id,
                texture.create_view(&Default::default()),
                (image.width, image.height),
            );
        }
        self.uploaded = true;
    }
}

/// Every rule a layout reads is one the server runs.
fn serves(state: &PackageStateView, layout: &Layout) -> bool {
    layout.sources().iter().all(|s| match s {
        Source::State(bind) => bri_package_runtime::content::Binding::parse(bind)
            .is_some_and(|b| state.packages.contains_key(&b.package)),
        _ => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(json: serde_json::Value) -> Layout {
        let l: Layout = serde_json::from_value(json).unwrap();
        bri_package_runtime::content::hud_layout(&l).unwrap();
        l
    }
    fn regions(packages: &[(&str, Vec<Layout>)]) -> HudRegions {
        HudRegions {
            layouts: packages
                .iter()
                .flat_map(|(id, layouts)| {
                    layouts
                        .iter()
                        .enumerate()
                        .map(move |(i, l)| (id.to_string(), format!("{id}:hud/{i}"), l.clone()))
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_body_claim_beats_a_plain_one_and_switches_with_the_body() {
        let plain = layout(
            serde_json::json!({ "schema_version": 1, "region": "top_right",
            "elements": [{ "type": "text", "at": [0, 0], "text": "plain" }] }),
        );
        let runner = layout(
            serde_json::json!({ "schema_version": 1, "region": "top_right",
            "when": { "body": ["a:archetype/runner"] },
            "elements": [{ "type": "text", "at": [0, 0], "value": "health" }] }),
        );
        let mut regions = regions(&[("aaa", vec![plain]), ("zzz", vec![runner])]);
        let state = PackageStateView::default();
        let game = GameValues {
            health: 42.0,
            health_max: 100.0,
            alive: true,
            ..Default::default()
        };
        let draws = |regions: &mut HudRegions, body| {
            regions.draws(
                &state,
                1,
                &Viewer {
                    body,
                    ..Default::default()
                },
                &game,
            )
        };
        let as_runner = draws(&mut regions, "a:archetype/runner");
        assert_eq!(as_runner.len(), 1);
        assert!(matches!(&as_runner[0].items[0], Item::Text { text, .. } if text == "42"));
        let as_blockhead = draws(&mut regions, "v20.player.playerstandardarmor");
        assert!(matches!(&as_blockhead[0].items[0], Item::Text { text, .. } if text == "plain"));
        let owners = regions.owners(
            &state,
            &Viewer {
                body: "a:archetype/runner",
                ..Default::default()
            },
        );
        assert_eq!(owners["zzz"], [Region::TopRight]);
        assert!(!owners.contains_key("aaa"));
    }

    #[test]
    fn a_layout_reading_a_rule_the_server_does_not_run_falls_back_to_stock() {
        let coins = layout(
            serde_json::json!({ "schema_version": 1, "region": "crosshair",
            "elements": [{ "type": "text", "at": [0, 0], "value": "game:player/coins" }] }),
        );
        let mut regions = regions(&[("hud", vec![coins])]);
        let draws = regions.draws(
            &PackageStateView::default(),
            1,
            &Viewer::default(),
            &GameValues::default(),
        );
        assert!(draws.is_empty(), "the stock crosshair stays");
    }

    #[test]
    fn an_ammo_counter_reads_the_held_gun_and_hides_with_none() {
        let counter = layout(serde_json::json!({ "schema_version": 1, "region": "ammo",
            "when": { "holding": ["guns"] },
            "elements": [
                { "type": "text", "at": [0, 0], "text": "{}", "value": "ammo", "digits": 2 },
                { "type": "text", "at": [0, 20], "text": "/{}", "value": "ammo_reserve" },
                { "type": "bar", "rect": [0, 40, 100, 4], "value": "ammo", "fill": { "color": [1, 1, 1, 1] } },
                { "type": "text", "at": [0, 50], "text": "RELOAD", "show": { "value": "reloading", "above": 0.5 } }
            ] }));
        let mut regions = regions(&[("hud", vec![counter])]);
        let state = PackageStateView::default();
        let viewer = Viewer {
            holding: "guns:image/carbine",
            ..Default::default()
        };
        let mut game = GameValues {
            ammo: Some(bri_sim::session::HeldAmmo {
                item: "guns:item/carbine".into(),
                rounds: Some(6),
                size: 24,
                reserve: Some(48),
                reloading: false,
                printed: true,
            }),
            ..Default::default()
        };
        let texts = |draws: &[HudRegionDraw]| -> Vec<String> {
            draws[0]
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect()
        };
        let draws = regions.draws(&state, 1, &viewer, &game);
        assert_eq!(draws[0].region, Region::Ammo);
        assert_eq!(texts(&draws), ["06", "/48"]);
        let bar = draws[0].items.iter().find_map(|i| match i {
            Item::Fill { rect, .. } => Some(rect[2]),
            _ => None,
        });
        assert_eq!(bar, Some(25.0), "a quarter of the magazine");
        game.ammo.as_mut().unwrap().reloading = true;
        game.ammo.as_mut().unwrap().reserve = None;
        let draws = regions.draws(&state, 1, &viewer, &game);
        assert_eq!(texts(&draws), ["06", "/-", "RELOAD"]);
        // Gun put away: the claim's `holding` no longer matches.
        let draws = regions.draws(&state, 1, &Viewer::default(), &game);
        assert!(draws.is_empty());
    }
    #[test]
    fn stowed_item_counters_do_not_depend_on_the_held_gun() {
        let counter = layout(
            serde_json::json!({"schema_version":1,"region":"top_left","elements":[
                {"type":"text","at":[0,0],"value":"item_ammo:items:weapon/nade"},
            {"type":"text","at":[0,6],"value":"item_ammo_max:items:weapon/nade"},
            {"type":"text","at":[0,12],"value":"item_ammo_reserve:items:weapon/nade"},
            {"type":"text","at":[0,18],"value":"item_reloading:items:weapon/nade"},
                {"type":"bar","rect":[0,20,100,4],"value":"item_ammo:items:weapon/nade","fill":{"color":[1,1,1,1]}}
            ]}),
        );
        let mut regions = regions(&[("hud", vec![counter])]);
        let state = PackageStateView::default();
        let viewer = Viewer::default();
        let mut game = GameValues::default();
        game.inventory_ammo[4] = Some(bri_sim::session::InventoryAmmo {
            item: "items:weapon/nade".into(),
            rounds: Some(2),
            capacity: 4,
            reserve: Some(2),
            reloading: false,
        });
        let draws = regions.draws(&state, 1, &viewer, &game);
        let texts = |draws: &[HudRegionDraw]| {
            draws[0]
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(&draws), ["2", "4", "2", "0"]);
        assert!(
            draws[0]
                .items
                .iter()
                .any(|i| matches!(i,Item::Fill{rect,..} if rect[2]==50.))
        );
        let ammo = game.inventory_ammo[4].as_mut().unwrap();
        ammo.rounds = None;
        ammo.reserve = None;
        ammo.reloading = true;
        let draws = regions.draws(&state, 1, &viewer, &game);
        assert_eq!(texts(&draws), ["-", "4", "-", "1"]);
        game.inventory_ammo = Default::default();
        let draws = regions.draws(&state, 1, &viewer, &game);
        assert!(
            draws[0]
                .items
                .iter()
                .any(|i| matches!(i,Item::Text{text,..} if text=="0"))
        );
    }
}
