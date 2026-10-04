//! UI adapters for implemented native tools. Unsupported event/source records
//! remain read-only; these adapters never invent gameplay or execute scripts.
use anyhow::{Context, Result, ensure};
use bri_content::{brick::Catalog, brick_materials::Bundle, effects::Library};
use bri_net::protocol::PublicWorld;
use bri_sim::session::{Command, InspectMode, Reply, ToolAction, ToolCatalog, WrenchProperties};
use bri_ui::{api::*, models::events::NAMED_BRICK, pack::Pack, schema::ParamSpec};
use bri_world::{Brick, ContentRef, EventRow as Row, EventTarget, EventValue, ItemSpawn};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

struct Inspection {
    id: u64,
    mode: InspectMode,
    brick: Brick,
    rows: Vec<EventRow>,
    /// Rows the dialog cannot edit (preserved imports) are retained here.
    retained: BTreeMap<String, Row>,
    wrench_original: Option<Brick>,
}

/// The print a player last applied, and every brick definition of its aspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastPrint {
    pub definitions: Vec<String>,
    pub print: String,
}

pub struct ToolUi {
    catalog: ToolCatalog,
    prints: BTreeMap<String, Vec<PrintInfo>>,
    print_aliases: BTreeMap<String, String>,
    item_aliases: BTreeMap<String, String>,
    datablocks: DatablockMenus,
    variants: BTreeMap<String, WrenchVariant>,
    inspection: Option<Inspection>,
    /// The host's wrench event catalog; empty until installed.
    events: Option<bri_events::Catalog>,
    /// The installed catalog before the server's Add-Ons' inputs and
    /// outputs.
    base_events: Option<bri_events::Catalog>,
    /// The server's Add-Ons' wrench event inputs and outputs.
    package_events: bri_events::Extension,
    /// Every installed music loop; the wrench lists those the host offers.
    music: Vec<Choice>,
    /// Current connection authority; independent of the installed host catalog.
    offered_music: Option<BTreeSet<String>>,
}

impl ToolUi {
    pub fn new(
        catalog: &Catalog,
        effects: &Library,
        materials: &Bundle,
        pack: &Pack,
    ) -> Result<Self> {
        let tool_catalog = ToolCatalog::from_native(catalog, effects, materials)?;
        let print_aliases = materials
            .prints
            .iter()
            .flat_map(|print| {
                print
                    .aliases
                    .iter()
                    .chain(std::iter::once(&print.id))
                    .map(|alias| (alias.to_ascii_lowercase(), print.id.clone()))
            })
            .collect();
        let mut prints: BTreeMap<String, Vec<PrintInfo>> = BTreeMap::new();
        for print in &materials.prints {
            let archive = print
                .icon
                .source
                .archive
                .as_deref()
                .context("Print icon has no original package")?;
            let archive = archive
                .strip_suffix(".zip")
                .context("Print icon package is not a ZIP")?;
            let source = print
                .icon
                .source
                .path
                .strip_suffix(".png")
                .context("Print icon is not a PNG")?;
            let icon = format!("{archive}/{source}").to_ascii_lowercase();
            ensure!(
                pack.data.images.contains_key(&icon),
                "UI pack lacks original print icon: {icon}"
            );
            prints
                .entry(print.aspect.clone())
                .or_default()
                .push(PrintInfo {
                    id: print.id.clone(),
                    name: print.name.clone(),
                    icon: IconRef::Pack(icon),
                });
        }
        let datablocks = [
            (
                "FxLightData".into(),
                effects
                    .lights
                    .iter()
                    .filter(|e| tool_catalog.lights.contains(&e.id))
                    .map(|e| Choice {
                        id: e.id.clone(),
                        name: e.name.clone(),
                    })
                    .collect(),
            ),
            (
                "ParticleEmitterData".into(),
                effects
                    .emitters
                    .iter()
                    .filter(|e| tool_catalog.emitters.contains(&e.id))
                    .map(|e| Choice {
                        id: e.id.clone(),
                        name: e.name.clone(),
                    })
                    .collect(),
            ),
        ]
        .into();
        let variants = wrench_variants(catalog);
        Ok(Self {
            catalog: tool_catalog,
            prints,
            print_aliases,
            item_aliases: BTreeMap::new(),
            datablocks,
            variants,
            inspection: None,
            events: None,
            base_events: None,
            package_events: Default::default(),
            music: Vec::new(),
            offered_music: None,
        })
    }
    pub fn server_catalog(&self) -> ToolCatalog {
        self.catalog.clone()
    }
    /// Install the same validated native item choices used by the authoritative
    /// host. Root supplies weapons-pack items plus core Hammer/Wrench/Printer/Wand.
    /// Existing inspection tokens are invalidated when the choice set changes.
    pub fn install_items(
        &mut self,
        items: impl IntoIterator<Item = (String, String)>,
    ) -> Result<()> {
        let mut choices = Vec::new();
        for (id, name) in items {
            ensure!(choices.len() < 1024, "Too many native item choices");
            ensure!(
                !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control),
                "Invalid native item display name"
            );
            choices.push(Choice { id, name });
        }
        // Shared display names are fine (v20 lists them all); the first given
        // binds saved bricks naming it, as `WeaponContent` orders them.
        let aliases =
            bri_world::item_aliases(choices.iter().map(|c| (c.id.as_str(), c.name.as_str())));
        let mut catalog = self.catalog.clone();
        catalog.install_items(choices.iter().map(|c| c.id.clone()))?;
        // Stable: items sharing a name keep the order given.
        choices.sort_by_key(|c| c.name.trim().to_lowercase());
        self.catalog = catalog;
        self.item_aliases = aliases;
        self.datablocks.insert("ItemData".into(), choices);
        self.invalidate();
        Ok(())
    }
    /// Offer the emitters and lights Add-Ons name in the wrench, after the
    /// base game's; one whose name the base game already uses is left out,
    /// as its effect is bound by id alone.
    pub fn install_effects(
        &mut self,
        emitters: Vec<(String, String)>,
        lights: Vec<(String, String)>,
    ) -> Result<()> {
        let mut catalog = self.catalog.clone();
        catalog.install_effects(
            emitters.iter().map(|(id, _)| id.clone()),
            lights.iter().map(|(id, _)| id.clone()),
        )?;
        for (class, added) in [("ParticleEmitterData", emitters), ("FxLightData", lights)] {
            let choices = self.datablocks.entry(class.into()).or_default();
            for (id, name) in added {
                if !choices
                    .iter()
                    .any(|c| c.id == id || c.name.eq_ignore_ascii_case(&name))
                {
                    choices.push(Choice { id, name });
                }
            }
        }
        self.catalog = catalog;
        self.invalidate();
        Ok(())
    }
    /// Install the music loops and vehicles for sound and vehicle spawn
    /// bricks (wrench "Music" and "Vehicle" lists).
    pub fn install_special(
        &mut self,
        sounds: Vec<(String, String)>,
        vehicles: Vec<(String, String)>,
    ) -> Result<()> {
        let mut catalog = self.catalog.clone();
        catalog.install_special(
            sounds.iter().map(|(id, _)| id.clone()),
            vehicles.iter().map(|(id, _)| id.clone()),
        )?;
        self.catalog = catalog;
        let menu = |entries: Vec<(String, String)>| {
            let mut choices: Vec<_> = entries
                .into_iter()
                .map(|(id, name)| Choice { id, name })
                .collect();
            choices.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            choices
        };
        self.music = menu(sounds);
        let _ = self.refresh_music();
        self.datablocks.insert("Vehicle".into(), menu(vehicles));
        self.invalidate();
        Ok(())
    }
    /// The wrench's Music list shows only the loops the host offers (its
    /// Music Files), as v20 clients knew only the host's music datablocks.
    pub fn offer_music(&mut self, offered: &BTreeSet<String>) -> Option<UiUpdate> {
        self.offered_music = Some(offered.clone());
        self.refresh_music()
    }
    /// A new connection starts with its own authority, never the previous host's.
    pub fn reset_music_offer(&mut self) -> Option<UiUpdate> {
        self.offered_music = None;
        self.refresh_music()
    }
    fn refresh_music(&mut self) -> Option<UiUpdate> {
        let music: Vec<_> = self
            .music
            .iter()
            .filter(|c| {
                self.offered_music
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&c.id))
            })
            .cloned()
            .collect();
        if self.datablocks.get("Music") == Some(&music) {
            return None;
        }
        self.datablocks.insert("Music".into(), music);
        Some(UiUpdate::Datablocks(self.datablocks.clone()))
    }
    /// Install the wrench event catalog and the datablock menus only events
    /// use: sounds, projectiles and player types.
    pub fn install_events(
        &mut self,
        catalog: bri_events::Catalog,
        sounds: Vec<(String, String)>,
        projectiles: Vec<(String, String)>,
    ) {
        let menu = |entries: Vec<(String, String)>| {
            let mut choices: Vec<_> = entries
                .into_iter()
                .map(|(id, name)| Choice { id, name })
                .collect();
            choices.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            choices
        };
        self.datablocks.insert("Sound".into(), menu(sounds));
        self.datablocks
            .insert("ProjectileData".into(), menu(projectiles));
        self.datablocks.insert(
            "PlayerData".into(),
            bri_sim::player_types::PlayerType::ALL
                .map(|t| Choice {
                    id: t.datablock_name().into(),
                    name: t.name().into(),
                })
                .into(),
        );
        self.base_events = Some(
            bri_events::rules::workshop_catalog(&catalog).expect("Validated core rule vocabulary"),
        );
        self.merge_events();
        self.invalidate();
    }
    /// The server's Add-Ons' wrench event inputs and outputs, added to the
    /// installed catalog. Returns the wrench's new event lists when they
    /// changed.
    pub fn offer_events(&mut self, events: &bri_events::Extension) -> Option<UiUpdate> {
        if self.package_events == *events {
            return None;
        }
        self.package_events = events.clone();
        self.merge_events();
        self.invalidate();
        Some(UiUpdate::Events(
            self.events.as_ref().map(event_catalog).unwrap_or_default(),
        ))
    }
    fn merge_events(&mut self) {
        self.events = self.base_events.as_ref().map(|base| {
            base.extended(&self.package_events).unwrap_or_else(|error| {
                bri_console::warn(format!(
                    "The server's Add-On events are left out: {error:#}"
                ));
                base.clone()
            })
        });
    }
    pub fn catalog_updates(&self) -> Vec<UiUpdate> {
        let mut updates = vec![
            UiUpdate::Events(self.events.as_ref().map(event_catalog).unwrap_or_default()),
            UiUpdate::Datablocks(self.datablocks.clone()),
        ];
        updates.extend(self.prints.iter().map(|(aspect, prints)| UiUpdate::Prints {
            aspect: aspect.clone(),
            prints: prints.clone(),
        }));
        updates
    }
    pub fn invalidate(&mut self) {
        self.inspection = None;
    }
    /// Events close only their nested dialog. Restore the base wrench while
    /// retaining its original property snapshot; other writes finish editing.
    /// Returns v20's remembered print for every brick of the printed aspect
    /// (`%client.lastPrint[%ar]`), which the next ghost of that aspect uses.
    pub fn command_accepted(&mut self, command: &Command) -> Result<Option<LastPrint>> {
        let mut last_print = None;
        if let Command::Tool(ToolAction::SetPrint {
            brick,
            print: Some(print),
        }) = command
        {
            let inspection = self
                .inspection
                .as_ref()
                .context("No active print inspection")?;
            ensure!(
                inspection.id == *brick && inspection.mode == InspectMode::Printer,
                "Accepted print does not match current inspection"
            );
            let aspect = &self.catalog.brick_print_aspects[resolved(&inspection.brick.definition)?];
            last_print = Some(LastPrint {
                definitions: self
                    .catalog
                    .brick_print_aspects
                    .iter()
                    .filter(|(_, a)| a.eq_ignore_ascii_case(aspect))
                    .map(|(definition, _)| definition.clone())
                    .collect(),
                print: print.clone(),
            });
        }
        if let Command::Tool(ToolAction::SetEvents { brick, events }) = command {
            let inspection = self
                .inspection
                .as_mut()
                .context("No active events inspection")?;
            ensure!(
                inspection.id == *brick && inspection.mode == InspectMode::Events,
                "Accepted events do not match current inspection"
            );
            if let Some(original) = &mut inspection.wrench_original {
                original.events = events.clone();
                inspection.brick = original.clone();
                inspection.mode = InspectMode::Wrench;
                inspection.rows.clear();
                inspection.retained.clear();
            } else {
                self.invalidate();
            }
        } else {
            self.invalidate();
        }
        Ok(last_print)
    }

    /// The caller must additionally reject replies from cancelled/replaced
    /// requests and old connection tokens before calling this method.
    pub fn accept_inspection(
        &mut self,
        reply: &Reply,
        expected_mode: InspectMode,
        expected_brick: Option<u64>,
        world: &PublicWorld,
        names: &BTreeMap<u64, String>,
        local_owner: u64,
    ) -> Result<Vec<UiUpdate>> {
        let Reply::Inspected {
            brick_id,
            brick,
            mode,
        } = reply
        else {
            anyhow::bail!("Expected server tool inspection")
        };
        ensure!(
            *mode == expected_mode && expected_brick.is_none_or(|id| id == *brick_id),
            "Server inspection does not match requested tool/brick"
        );
        brick.validate(world.palette.len())?;
        let definition = resolved(&brick.definition)?;
        let variant = *self
            .variants
            .get(definition)
            .context("Inspected brick definition is unavailable")?;
        let mut rows = vec![];
        let mut retained = BTreeMap::new();
        let update = match mode {
            InspectMode::Wrench => {
                let mut bound = (**brick).clone();
                bound.item_spawn.resolve_item(&self.item_aliases)?;
                let data = wrench_data(&bound)?;
                validate_choice(data.light.as_deref(), &self.catalog.lights, "light")?;
                validate_choice(data.emitter.as_deref(), &self.catalog.emitters, "emitter")?;
                validate_choice(data.item.as_deref(), &self.catalog.items, "item")?;
                UiUpdate::OpenWrench {
                    brick: *brick_id,
                    variant,
                    owner: names.get(&brick.owner).cloned().unwrap_or_else(|| {
                        if brick.owner == 0 {
                            "World".into()
                        } else {
                            format!("Owner {}", brick.owner)
                        }
                    }),
                    data,
                    admin_override: brick.owner != local_owner,
                    events_allowed: true,
                }
            }
            InspectMode::Printer => {
                let aspect = self
                    .catalog
                    .brick_print_aspects
                    .get(definition)
                    .context("Brick is not printable")?;
                // UI keys retain original casing; compatibility itself is case-insensitive.
                let aspect = self
                    .prints
                    .keys()
                    .find(|s| s.eq_ignore_ascii_case(aspect))
                    .unwrap_or(aspect)
                    .clone();
                let current = brick
                    .print
                    .as_ref()
                    .map(|reference| {
                        let token = match reference {
                            ContentRef::Resolved(id) => id,
                            ContentRef::Unresolved(u)
                                if u.namespace.eq_ignore_ascii_case("print") =>
                            {
                                &u.name
                            }
                            _ => anyhow::bail!(
                                "Current brick print has an unsupported source namespace"
                            ),
                        };
                        self.print_aliases
                            .get(&token.to_ascii_lowercase())
                            .cloned()
                            .context("Current brick print is not bound to native content")
                    })
                    .transpose()?;
                if let Some(id) = &current {
                    ensure!(
                        self.catalog.prints.contains_key(id),
                        "Current brick print is not bound to native content"
                    );
                }
                UiUpdate::OpenPrintSelector { aspect, current }
            }
            InspectMode::Events => {
                let catalog = self.events.as_ref().context("Events are unavailable")?;
                (rows, retained) = event_rows(brick, catalog)?;
                let names: BTreeSet<_> = world
                    .bricks
                    .values()
                    .filter(|b| b.owner == brick.owner)
                    .filter_map(|b| b.name.clone())
                    .collect();
                UiUpdate::OpenEvents {
                    brick: *brick_id,
                    builder: Some(brick.owner),
                    rows: rows.clone(),
                    named_targets: names.into_iter().collect(),
                    allow_named: true,
                }
            }
        };
        let wrench_original = if *mode == InspectMode::Wrench {
            Some(*brick.clone())
        } else if *mode == InspectMode::Events {
            self.inspection
                .as_ref()
                .filter(|i| i.id == *brick_id)
                .and_then(|i| i.wrench_original.clone())
        } else {
            None
        };
        self.inspection = Some(Inspection {
            id: *brick_id,
            mode: *mode,
            brick: *brick.clone(),
            rows,
            retained,
            wrench_original,
        });
        Ok(vec![update])
    }

    /// The fill wrench's ticked settings as the host takes them.
    fn fill_wrench(
        &self,
        data: &bri_ui::api::WrenchData,
        fields: &[bri_ui::models::wrench::WrenchField],
    ) -> Result<bri_sim::session::WrenchFill> {
        use bri_ui::models::wrench::WrenchField as F;
        let mut fill = bri_sim::session::WrenchFill::default();
        for field in fields {
            match field {
                F::Name => {
                    let name = data.name.trim();
                    ensure!(
                        name.len() <= 128 && !name.chars().any(char::is_control),
                        "Invalid brick name"
                    );
                    fill.name = Some((!name.is_empty()).then(|| name.to_owned()));
                }
                F::Light => {
                    validate_choice(data.light.as_deref(), &self.catalog.lights, "light")?;
                    fill.light = Some(data.light.clone());
                }
                F::Emitter => {
                    validate_choice(data.emitter.as_deref(), &self.catalog.emitters, "emitter")?;
                    fill.emitter = Some(data.emitter.clone());
                }
                F::EmitterDir => {
                    ensure!(data.emitter_dir <= 5, "Unknown emitter direction");
                    fill.emitter_direction = Some(data.emitter_dir);
                }
                F::Item => {
                    validate_choice(data.item.as_deref(), &self.catalog.items, "item")?;
                    fill.item = Some(data.item.clone());
                }
                F::ItemPos => fill.item_position = Some(data.item_pos),
                F::ItemDir => fill.item_direction = Some(data.item_dir),
                F::ItemRespawn => fill.item_respawn_ms = Some(data.item_respawn_ms),
                F::RayCasting => fill.raycast = Some(data.raycasting),
                F::Colliding => fill.colliding = Some(data.colliding),
                F::Rendering => fill.visible = Some(data.rendering),
                F::Sound | F::Vehicle | F::RecolorVehicle => {
                    anyhow::bail!("The fill wrench sets plain bricks' settings only")
                }
            }
        }
        fill.validate()?;
        Ok(fill)
    }

    pub fn action_command(&mut self, action: &UiAction) -> Result<Option<Command>> {
        if let UiAction::SendFillWrench { data, fields } = action {
            return self
                .fill_wrench(data, fields)
                .map(|fill| Some(Command::WrenchCopy(fill)));
        }
        let tool = match action {
            UiAction::CancelWrench { brick } => {
                if self.inspection.as_ref().is_some_and(|i| i.id == *brick) {
                    self.invalidate();
                }
                return Ok(None);
            }
            UiAction::ClosePrintSelector => {
                self.invalidate();
                return Ok(None);
            }
            UiAction::RequestEvents { brick } => {
                let inspection = self
                    .inspection
                    .as_ref()
                    .context("No active server tool inspection")?;
                ensure!(
                    inspection.id == *brick,
                    "Events request does not match inspected brick"
                );
                ToolAction::Inspect {
                    mode: InspectMode::Events,
                }
            }
            UiAction::SetPrint { print } => {
                let inspection = self.inspection(InspectMode::Printer, None)?;
                let aspect =
                    &self.catalog.brick_print_aspects[resolved(&inspection.brick.definition)?];
                let print_aspect = self.catalog.prints.get(print).context("Unknown print ID")?;
                ensure!(
                    print_aspect.eq_ignore_ascii_case(aspect)
                        || print_aspect.eq_ignore_ascii_case("Letters"),
                    "Print does not fit this brick"
                );
                ToolAction::SetPrint {
                    brick: inspection.id,
                    print: Some(print.clone()),
                }
            }
            UiAction::SendWrench {
                brick,
                variant,
                data,
            } => {
                self.inspection(InspectMode::Wrench, Some(*brick))?;
                ensure!(
                    (*variant == WrenchVariant::Sound || data.sound.is_none())
                        && (*variant == WrenchVariant::VehicleSpawn
                            || (data.vehicle.is_none() && !data.recolor_vehicle)),
                    "This brick cannot hold that sound or vehicle"
                );
                validate_choice(data.sound.as_deref(), &self.catalog.sounds, "music")?;
                if let Some(offered) = &self.offered_music {
                    validate_choice(data.sound.as_deref(), offered, "host-offered music")?;
                }
                validate_choice(data.vehicle.as_deref(), &self.catalog.vehicles, "vehicle")?;
                validate_choice(data.light.as_deref(), &self.catalog.lights, "light")?;
                validate_choice(data.emitter.as_deref(), &self.catalog.emitters, "emitter")?;
                validate_choice(data.item.as_deref(), &self.catalog.items, "item")?;
                let item_spawn = ItemSpawn {
                    item: data.item.clone().map(ContentRef::Resolved),
                    position: data.item_pos,
                    direction: data.item_dir,
                    respawn_ms: data.item_respawn_ms,
                };
                item_spawn.validate()?;
                ensure!(data.emitter_dir <= 5, "Unknown emitter direction");
                let name = data.name.trim();
                ensure!(
                    name.len() <= 128 && !name.chars().any(char::is_control),
                    "Invalid brick name"
                );
                ToolAction::SetWrench {
                    brick: *brick,
                    properties: WrenchProperties {
                        rule_region: data.rule_region,
                        name: (!name.is_empty()).then(|| name.to_owned()),
                        light: data.light.clone(),
                        emitter: data.emitter.clone(),
                        emitter_direction: data.emitter_dir,
                        item_spawn,
                        sound: data.sound.clone(),
                        vehicle: data.vehicle.clone(),
                        recolor_vehicle: data.recolor_vehicle,
                        raycast: data.raycasting,
                        colliding: data.colliding,
                        visible: data.rendering,
                    },
                }
            }
            UiAction::SendEvents { brick, rows } => {
                let inspection = self.inspection(InspectMode::Events, Some(*brick))?;
                let original: Vec<_> = inspection
                    .rows
                    .iter()
                    .filter(|r| matches!(r, EventRow::Preserved { .. }))
                    .collect();
                let submitted: Vec<_> = rows
                    .iter()
                    .filter(|r| matches!(r, EventRow::Preserved { .. }))
                    .collect();
                ensure!(
                    original == submitted,
                    "Preserved event rows were changed, removed or forged"
                );
                let catalog = self.events.as_ref().context("Events are unavailable")?;
                let mut events = vec![];
                for row in rows {
                    match row {
                        EventRow::Editable(line) => events.push(native_event(line, catalog)?),
                        EventRow::Preserved { token, .. } => {
                            if let Some(event) = inspection.retained.get(token) {
                                events.push(event.clone());
                            }
                        }
                    }
                }
                // Native authority performs the full world/event bound check.
                let mut candidate = inspection.brick.clone();
                candidate.events = events.clone();
                // Palette-dependent validation is performed by the server.
                candidate.validate(256)?;
                ToolAction::SetEvents {
                    brick: *brick,
                    events,
                }
            }
            UiAction::RespawnVehicle { brick, .. } => {
                self.inspection(InspectMode::Wrench, Some(*brick))?;
                ToolAction::RespawnVehicle { brick: *brick }
            }
            _ => return Ok(None),
        };
        Ok(Some(Command::Tool(tool)))
    }
    fn inspection(&self, mode: InspectMode, id: Option<u64>) -> Result<&Inspection> {
        let inspection = self
            .inspection
            .as_ref()
            .context("No active server tool inspection")?;
        ensure!(
            (inspection.mode == mode
                || (mode == InspectMode::Wrench
                    && inspection.mode == InspectMode::Events
                    && inspection.wrench_original.is_some()))
                && id.is_none_or(|id| id == inspection.id),
            "Tool action does not match inspected brick/mode"
        );
        Ok(inspection)
    }
}

fn resolved(reference: &ContentRef) -> Result<&str> {
    match reference {
        ContentRef::Resolved(id) => Ok(id),
        ContentRef::Unresolved(_) => {
            anyhow::bail!("Original resource has no native content binding")
        }
    }
}
fn validate_choice(value: Option<&str>, choices: &BTreeSet<String>, kind: &str) -> Result<()> {
    ensure!(
        value.is_none_or(|v| choices.contains(v)),
        "Unknown native {kind}"
    );
    Ok(())
}
fn wrench_data(brick: &Brick) -> Result<WrenchData> {
    Ok(WrenchData {
        rule_region: brick.rule_region,
        rule_region_default: None,
        region_inputs: bri_world::regions::has_region_input(brick),
        name: brick.name.clone().unwrap_or_default(),
        light: brick
            .light
            .as_ref()
            .map(|l| resolved(&l.asset).map(str::to_owned))
            .transpose()?,
        emitter: brick
            .emitter
            .as_ref()
            .and_then(|e| e.asset.as_ref())
            .map(resolved)
            .transpose()?
            .map(str::to_owned),
        emitter_dir: brick.emitter.as_ref().map_or(0, |e| e.direction),
        item: brick
            .item_spawn
            .item
            .as_ref()
            .map(resolved)
            .transpose()?
            .map(str::to_owned),
        item_pos: brick.item_spawn.position,
        item_dir: brick.item_spawn.direction,
        item_respawn_ms: brick.item_spawn.respawn_ms,
        raycasting: brick.raycast,
        colliding: brick.colliding,
        rendering: brick.visible,
        sound: brick
            .sound
            .as_ref()
            .map(resolved)
            .transpose()?
            .map(str::to_owned),
        vehicle: brick
            .vehicle
            .as_ref()
            .map(|v| resolved(&v.vehicle))
            .transpose()?
            .map(str::to_owned),
        recolor_vehicle: brick.vehicle.as_ref().is_some_and(|v| v.recolor),
    })
}

/// The dialog's view of the host catalog: every vanilla input and output.
pub fn event_catalog(catalog: &bri_events::Catalog) -> EventCatalog {
    let param = |p: &bri_events::Param| match p.clone() {
        bri_events::Param::Int { min, max, default } => ParamSpec::Int { min, max, default },
        bri_events::Param::Float {
            min,
            max,
            step,
            default,
        } => ParamSpec::Float {
            min,
            max,
            step,
            default,
        },
        bri_events::Param::Bool => ParamSpec::Bool,
        bri_events::Param::String { max_length, width } => ParamSpec::String { max_length, width },
        bri_events::Param::Datablock { class_name } => ParamSpec::Datablock { class: class_name },
        bri_events::Param::Vector { max_length } => ParamSpec::Vector { max: max_length },
        bri_events::Param::PaintColor { default } => ParamSpec::PaintColor {
            default: i64::from(default),
        },
        bri_events::Param::IntList { width } => ParamSpec::IntList { width },
        bri_events::Param::List { items } => ParamSpec::List { items },
    };
    EventCatalog {
        inputs: catalog
            .inputs
            .iter()
            .map(|i| EventInputInfo {
                name: i.name.clone(),
                targets: i.targets.clone(),
                supported: true,
            })
            .collect(),
        outputs: catalog
            .outputs
            .iter()
            .map(|o| EventOutputInfo {
                provider: o.package.clone().unwrap_or_else(|| "core".into()),
                class: o.class_name.clone(),
                name: o.name.clone(),
                params: o.params.iter().map(param).collect(),
                supported: true,
            })
            .collect(),
    }
}
/// Dialog line to engine row. The dialog shows Torque's X/Y/Z vector order;
/// datablock membership and the palette are checked by the server.
fn native_event(line: &EventLine, catalog: &bri_events::Catalog) -> Result<Row> {
    ensure!(
        line.delay_ms <= bri_ui::models::events::MAX_DELAY_MS,
        "Event delay exceeds supported dialog range"
    );
    if let Some(name) = &line.named_target {
        ensure!(
            !name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control),
            "Invalid named event target"
        );
    }
    let row = bri_events::convert::ui_event(&serde_json::to_value(line)?)?;
    let row = bri_events::convert::normalize_ui_row(catalog, row)?;
    // Check everything but datablock membership, which only the host knows.
    let mut shape = row.clone();
    for value in &mut shape.params {
        if let EventValue::Datablock(id) = value {
            *id = None;
        }
    }
    let bindings = bri_events::Bindings {
        palette_len: 256,
        datablocks: Default::default(),
    };
    catalog.validate_row(&shape, &bindings)?;
    Ok(row)
}
/// Engine row to dialog line.
fn ui_event(row: &Row, catalog: &bri_events::Catalog) -> Result<EventLine> {
    ensure!(row.preserved.is_none(), "Preserved rows are not editable");
    let input = catalog.input(&row.input).context("Unknown event input")?;
    let (target, named_target) = match &row.target {
        EventTarget::Slot(slot) => (
            input
                .targets
                .iter()
                .find(|(s, _)| bri_events::Slot::parse(s) == Some(*slot))
                .context("Target unavailable for input")?
                .0
                .clone(),
            None,
        ),
        EventTarget::Named(name) => (NAMED_BRICK.to_string(), Some(name.clone())),
        EventTarget::Derived(name) => (name.clone(), None),
    };
    let (_, output) = catalog
        .row_output(&row.input, &row.target, &row.output)
        .context("Unknown event output")?;
    ensure!(
        output.params.len() == row.params.len(),
        "Wrong parameter count"
    );
    let params = output
        .params
        .iter()
        .zip(&row.params)
        .map(|(spec, value)| match (spec, value) {
            (bri_events::Param::List { .. }, EventValue::Int(v)) => ParamValue::List(*v),
            (_, EventValue::Int(v)) => ParamValue::Int(*v),
            (_, EventValue::Float(v)) => ParamValue::Float(*v),
            (_, EventValue::Bool(v)) => ParamValue::Bool(*v),
            (_, EventValue::Text(v)) => ParamValue::Text(v.clone()),
            (_, EventValue::Datablock(v)) => ParamValue::Datablock(v.clone()),
            (_, EventValue::Vector(v)) => ParamValue::Vector([v.x, -v.z, v.y]),
            (_, EventValue::Color(v)) => ParamValue::PaintColor(u32::from(*v)),
            (_, EventValue::Rows(bri_events::RowSelection::All)) => ParamValue::Text("ALL".into()),
            (_, EventValue::Rows(bri_events::RowSelection::Indices(v))) => {
                ParamValue::Text(v.iter().map(u16::to_string).collect::<Vec<_>>().join(" "))
            }
        })
        .collect();
    Ok(EventLine {
        conditions: row.conditions.clone(),
        enabled: row.enabled,
        delay_ms: row.delay_ms,
        input: input.name.clone(),
        target,
        named_target,
        output: output.name.clone(),
        params,
    })
}
fn event_rows(
    brick: &Brick,
    catalog: &bri_events::Catalog,
) -> Result<(Vec<EventRow>, BTreeMap<String, Row>)> {
    let mut rows = vec![];
    let mut retained = BTreeMap::new();
    for (index, row) in brick.events.iter().enumerate() {
        let editable = ui_event(row, catalog).ok().filter(|line| {
            native_event(line, catalog)
                .as_ref()
                .is_ok_and(|native| native == row)
        });
        if let Some(line) = editable {
            rows.push(EventRow::Editable(line));
            continue;
        }
        let text = match &row.preserved {
            Some(p) => p.original.clone(),
            None => serde_json::to_string(row)?,
        };
        let token = format!(
            "native:{index}:{:x}",
            Sha256::digest(serde_json::to_vec(row)?)
        );
        retained.insert(token.clone(), row.clone());
        rows.push(EventRow::Preserved {
            enabled: row.enabled,
            text,
            token,
        });
    }
    Ok((rows, retained))
}

/// The wrench window each brick opens, by its special kind: every brick in
/// `catalog`, so `catalog` must hold the Add-Ons' bricks as well as the base
/// game's (`bri_sim::definitions::catalog_with`).
fn wrench_variants(catalog: &Catalog) -> BTreeMap<String, WrenchVariant> {
    catalog
        .bricks
        .iter()
        .map(|b| {
            let variant = match b.special_kind.as_deref() {
                Some("Sound") => WrenchVariant::Sound,
                Some("VehicleSpawn") => WrenchVariant::VehicleSpawn,
                _ => WrenchVariant::Normal,
            };
            (b.id.clone(), variant)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> ToolUi {
        ToolUi {
            catalog: ToolCatalog {
                items: ["v20.weapon.gunitem".into(), "v20.weapon.hammeritem".into()].into(),
                lights: ["light/red".into()].into(),
                emitters: ["emitter/smoke".into()].into(),
                prints: [
                    ("print/A".into(), "Letters".into()),
                    ("print/face".into(), "2x2f".into()),
                    ("print/wide".into(), "2x1".into()),
                ]
                .into(),
                brick_print_aspects: [("plate".into(), "2x2f".into())].into(),
                default_print: Some("print/A".into()),
                ..Default::default()
            },
            prints: BTreeMap::new(),
            print_aliases: [
                ("letters/a".into(), "print/A".into()),
                ("print/a".into(), "print/A".into()),
                ("2x2f/face".into(), "print/face".into()),
            ]
            .into(),
            item_aliases: BTreeMap::new(),
            datablocks: BTreeMap::new(),
            variants: [("plate".into(), WrenchVariant::Normal)].into(),
            inspection: None,
            events: Some(events()),
            base_events: Some(events()),
            package_events: Default::default(),
            music: Vec::new(),
            offered_music: None,
        }
    }
    #[test]
    fn add_on_emitters_and_lights_join_the_wrench_lists() {
        let mut ui = fixture();
        ui.datablocks.insert(
            "ParticleEmitterData".into(),
            vec![Choice {
                id: "emitter/smoke".into(),
                name: "Smoke".into(),
            }],
        );
        ui.install_effects(
            vec![
                (
                    "crit:emitter/critemitter".into(),
                    "Emote - Critical Hit".into(),
                ),
                ("other:emitter/smoke".into(), "smoke".into()),
            ],
            vec![("crit:light/glow".into(), "Glow".into())],
        )
        .unwrap();
        let names = |class: &str| -> Vec<String> {
            ui.datablocks[class]
                .iter()
                .map(|c| c.name.clone())
                .collect()
        };
        assert_eq!(
            names("ParticleEmitterData"),
            ["Smoke", "Emote - Critical Hit"]
        );
        assert_eq!(names("FxLightData"), ["Glow"]);
        assert!(ui.catalog.emitters.contains("crit:emitter/critemitter"));
        assert!(
            ui.catalog.emitters.contains("emitter/smoke"),
            "the base game's stay"
        );
        assert!(ui.catalog.lights.contains("crit:light/glow"));
        assert!(
            ui.install_effects(vec![("bad\nid".into(), "Bad".into())], vec![])
                .is_err()
        );
    }
    #[test]
    fn the_wrench_lists_the_servers_add_on_inputs_targets_and_outputs() {
        let mut ui = fixture();
        let flag = bri_events::InputDef {
            id: "ctf:onFlagPickedUp".into(),
            class_name: "fxDTSBrick".into(),
            name: "onFlagPickedUp".into(),
            targets: vec![("Self".into(), "fxDTSBrick".into())],
            source: "ctf".into(),
            source_line: 0,
        };
        let inputs = |inputs: Vec<bri_events::InputDef>| bri_events::Extension {
            inputs,
            ..Default::default()
        };
        let Some(UiUpdate::Events(lists)) = ui.offer_events(&inputs(vec![flag.clone()])) else {
            panic!("the lists change");
        };
        assert!(lists.inputs.iter().any(|i| i.name == "onFlagPickedUp"));
        assert!(
            ui.offer_events(&inputs(vec![flag.clone()])).is_none(),
            "no change"
        );
        // A server without them takes them away again.
        let Some(UiUpdate::Events(lists)) = ui.offer_events(&Default::default()) else {
            panic!("the lists change");
        };
        assert!(!lists.inputs.iter().any(|i| i.name == "onFlagPickedUp"));
        // One that clashes with the host's own is left out.
        let clash = bri_events::InputDef {
            name: "onActivate".into(),
            ..flag.clone()
        };
        let Some(UiUpdate::Events(lists)) = ui.offer_events(&inputs(vec![clash])) else {
            panic!("the lists change");
        };
        assert_eq!(
            lists
                .inputs
                .iter()
                .filter(|i| i.name == "onActivate")
                .count(),
            1
        );
        // Outputs join the target's class, with their parameters, and an
        // Add-On's target joins every input with its base slot.
        let output = |class: &str, name: &str, params| bri_events::OutputDef {
            id: format!("slayer:{class}:{name}"),
            class_name: class.into(),
            name: name.into(),
            params,
            append_client: false,
            source: "slayer".into(),
            source_line: 0,
            package: Some("slayer".into()),
        };
        let events = bri_events::Extension {
            inputs: vec![flag],
            targets: vec![bri_events::TargetDef {
                id: "slayer:Team(Client)".into(),
                name: "Team(Client)".into(),
                class_name: "Slayer_TeamSO".into(),
                from: "Client".into(),
                package: "slayer".into(),
                source: "slayer".into(),
                source_line: 0,
            }],
            outputs: vec![
                output(
                    "fxDTSBrick",
                    "setTeamControl",
                    vec![bri_events::Param::PaintColor { default: 0 }],
                ),
                output(
                    "Slayer_TeamSO",
                    "IncScore",
                    vec![bri_events::Param::Int {
                        min: -10,
                        max: 10,
                        default: 1,
                    }],
                ),
            ],
        };
        let Some(UiUpdate::Events(lists)) = ui.offer_events(&events) else {
            panic!("the lists change");
        };
        let listed = lists
            .outputs
            .iter()
            .find(|o| o.name == "setTeamControl")
            .expect("listed");
        assert_eq!(listed.class, "fxDTSBrick");
        assert_eq!(listed.params.len(), 1);
        let activate = lists
            .inputs
            .iter()
            .find(|i| i.name == "onActivate")
            .unwrap();
        assert!(
            activate
                .targets
                .contains(&("Team(Client)".into(), "Slayer_TeamSO".into()))
        );
        let flag = lists
            .inputs
            .iter()
            .find(|i| i.name == "onFlagPickedUp")
            .unwrap();
        assert!(
            !flag.targets.iter().any(|(t, _)| t == "Team(Client)"),
            "an input without a client"
        );
        // A row aimed at the team round-trips through the dialog.
        let catalog = ui.events.clone().unwrap();
        let row = Row {
            conditions: vec![],
            preserved: None,
            enabled: true,
            input: "onActivate".into(),
            delay_ms: 0,
            target: EventTarget::Derived("Team(Client)".into()),
            output: "IncScore".into(),
            params: vec![EventValue::Int(3)],
        };
        let line = ui_event(&row, &catalog).unwrap();
        assert_eq!(line.target, "Team(Client)");
        assert_eq!(native_event(&line, &catalog).unwrap(), row);
    }
    #[test]
    fn the_wrench_lists_only_the_music_the_host_offers() {
        use bri_ui::binds::Platform;
        use bri_ui::screens::ScreenId;
        use bri_ui::ui::{Ui, UiConfig};
        let mut tools = fixture();
        let loops = vec![
            ("music/bass".to_string(), "Bass 1".to_string()),
            ("music/rock".to_string(), "Rock".to_string()),
        ];
        tools.install_special(loops.clone(), vec![]).unwrap();
        let mut guest = Ui::new(
            bri_ui::testing::screens_pack(),
            UiConfig {
                size: (1024, 768),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        for update in tools.catalog_updates() {
            guest.apply(update);
        }
        guest.apply(UiUpdate::OpenWrench {
            brick: 7,
            variant: WrenchVariant::Sound,
            owner: "Builder".into(),
            data: WrenchData {
                sound: Some("music/bass".into()),
                ..Default::default()
            },
            admin_override: false,
            events_allowed: true,
        });
        let allowed: BTreeSet<_> = ["music/rock".into(), "music/unknown".into()].into();
        guest.apply(
            tools
                .offer_music(&allowed)
                .expect("changed menu reaches guest"),
        );
        guest.update(0);
        assert_eq!(
            guest.core.datablocks["Music"]
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["Rock"]
        );
        let view = guest
            .screen(ScreenId::Wrench(WrenchVariant::Sound))
            .unwrap()
            .view();
        let menu = view.id("WrenchSound_Sounds").unwrap();
        let labels: Vec<_> = view
            .node(menu)
            .state
            .items
            .iter()
            .map(|(label, _)| label.as_str())
            .collect();
        assert_eq!(labels, [" NONE", "Rock", "Unavailable: music/bass"]);
        assert_eq!(
            guest
                .core
                .wrench
                .values(WrenchVariant::Sound)
                .sound
                .as_deref(),
            Some("music/bass")
        );
        assert!(
            tools.offer_music(&allowed).is_none(),
            "identical authority needs no UI churn"
        );
        // Refreshing installed content cannot restore a track the current host excludes.
        tools.install_special(loops, vec![]).unwrap();
        assert_eq!(tools.datablocks["Music"].len(), 1);
        // UI filtering must not narrow the installed catalog used by a future local host.
        assert_eq!(tools.server_catalog().sounds.len(), 2);
        guest.apply(tools.offer_music(&BTreeSet::new()).unwrap());
        assert!(guest.core.datablocks["Music"].is_empty());
        guest.apply(tools.reset_music_offer().unwrap());
        assert_eq!(guest.core.datablocks["Music"].len(), 2);
        guest.apply(tools.offer_music(&["music/bass".into()].into()).unwrap());
        assert_eq!(guest.core.datablocks["Music"][0].id, "music/bass");
    }

    #[test]
    fn a_changed_music_offer_keeps_inspection_but_rejects_unoffered_actions() {
        let mut tools = fixture();
        tools
            .install_special(
                vec![
                    ("music/bass".into(), "Bass".into()),
                    ("music/rock".into(), "Rock".into()),
                ],
                vec![],
            )
            .unwrap();
        tools.variants.insert("plate".into(), WrenchVariant::Sound);
        let b = brick();
        let updates = open(&mut tools, &b, InspectMode::Wrench);
        let UiUpdate::OpenWrench { data, .. } = &updates[0] else {
            panic!("ordinary inspection")
        };
        let _ = tools.offer_music(&["music/rock".into()].into());
        let action = |sound| UiAction::SendWrench {
            brick: 7,
            variant: WrenchVariant::Sound,
            data: WrenchData {
                sound,
                ..data.clone()
            },
        };
        assert!(
            tools
                .action_command(&action(Some("music/bass".into())))
                .is_err()
        );
        assert!(
            tools
                .action_command(&action(Some("music/rock".into())))
                .unwrap()
                .is_some()
        );
        assert!(tools.action_command(&action(None)).unwrap().is_some());
        assert!(
            tools.inspection.is_some(),
            "catalog updates retain an ordinary draft's inspection"
        );
    }
    fn events() -> bri_events::Catalog {
        use bri_events::{InputDef, OutputDef, Param};
        let targets = [
            ("Self", "fxDTSBrick"),
            ("Player", "Player"),
            ("Client", "GameConnection"),
        ];
        let input = |name: &str| InputDef {
            id: format!("in/{name}"),
            class_name: "fxDTSBrick".into(),
            name: name.into(),
            targets: targets
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            source: "fixture".into(),
            source_line: 1,
        };
        let output = |class: &str, name: &str, params: Vec<Param>| OutputDef {
            id: format!("out/{class}/{name}"),
            class_name: class.into(),
            name: name.into(),
            params,
            append_client: false,
            source: "fixture".into(),
            source_line: 1,
            package: None,
        };
        bri_events::Catalog {
            schema_version: 1,
            inputs: vec![input("onActivate"), input("onPlayerTouch")],
            outputs: vec![
                output(
                    "fxDTSBrick",
                    "setColor",
                    vec![Param::PaintColor { default: 0 }],
                ),
                output(
                    "fxDTSBrick",
                    "setColorFX",
                    vec![Param::List {
                        items: (0..7).map(|i| (format!("fx{i}"), i)).collect(),
                    }],
                ),
                output("fxDTSBrick", "setRendering", vec![Param::Bool]),
                output(
                    "fxDTSBrick",
                    "setEventEnabled",
                    vec![Param::IntList { width: 157 }, Param::Bool],
                ),
                output(
                    "fxDTSBrick",
                    "setLight",
                    vec![Param::Datablock {
                        class_name: "FxLightData".into(),
                    }],
                ),
                output(
                    "Player",
                    "addVelocity",
                    vec![Param::Vector { max_length: 200.0 }],
                ),
            ],
            targets: vec![],
            sources: vec![],
            scope: serde_json::Value::Null,
        }
    }
    fn brick() -> Brick {
        Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, -2.25], 1)
    }
    fn world(brick: &Brick) -> PublicWorld {
        let mut other = brick.clone();
        other.owner = 2;
        other.name = Some("private target".into());
        let mut target = brick.clone();
        target.name = Some("owned target".into());
        PublicWorld {
            name: "test".into(),
            map_id: "test".into(),
            palette: vec![[1.0; 4], [0.0; 4]],
            bricks: [
                (7_u64, bri_net::protocol::public_brick(brick)),
                (8, other),
                (9, target),
            ]
            .into_iter()
            .collect(),
        }
    }
    fn open(ui: &mut ToolUi, brick: &Brick, mode: InspectMode) -> Vec<UiUpdate> {
        ui.accept_inspection(
            &Reply::Inspected {
                brick_id: 7,
                brick: Box::new(brick.clone()),
                mode,
            },
            mode,
            Some(7),
            &world(brick),
            &[(1, "Builder".into())].into(),
            1,
        )
        .unwrap()
    }
    fn row(output: &str, params: Vec<EventValue>) -> Row {
        Row {
            conditions: vec![],
            preserved: None,
            enabled: true,
            input: "onActivate".into(),
            delay_ms: 0,
            target: EventTarget::Slot(bri_events::Slot::SelfBrick),
            output: output.into(),
            params,
        }
    }
    fn line(output: &str, params: Vec<EventValue>) -> EventLine {
        ui_event(&row(output, params), &events()).unwrap()
    }
    #[test]
    fn item_choices_install_atomically_and_wrench_roundtrips_fields() {
        let mut ui = fixture();
        ui.install_items([
            ("v20.weapon.gunitem".into(), "Gun".into()),
            ("v20.weapon.hammeritem".into(), "Hammer ".into()),
            ("v20.weapon.wrenchitem".into(), "Wrench".into()),
            ("v20.weapon.printgun".into(), "Printer".into()),
            ("v20.weapon.wanditem".into(), "Wand".into()),
        ])
        .unwrap();
        assert_eq!(ui.server_catalog().items.len(), 5);
        assert_eq!(ui.datablocks["ItemData"].len(), 5);
        let before = ui.server_catalog();
        assert!(ui.install_items([("id".into(), "".into())]).is_err());
        assert_eq!(ui.server_catalog(), before);
        let mut b = brick();
        b.rule_region = Some([8.0, 5.0, 8.0]);
        b.item_spawn = ItemSpawn {
            item: Some(ContentRef::Resolved("v20.weapon.gunitem".into())),
            position: 4,
            direction: 5,
            respawn_ms: 17000,
        };
        let updates = open(&mut ui, &b, InspectMode::Wrench);
        let UiUpdate::OpenWrench { data, .. } = &updates[0] else {
            panic!()
        };
        assert_eq!(
            (data.item_pos, data.item_dir, data.item_respawn_ms),
            (4, 5, 17000)
        );
        let Some(Command::Tool(ToolAction::SetWrench { properties, .. })) = ui
            .action_command(&UiAction::SendWrench {
                brick: 7,
                variant: WrenchVariant::Normal,
                data: data.clone(),
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(properties.item_spawn, b.item_spawn);
        assert_eq!(properties.rule_region, Some([8.0, 5.0, 8.0]));
        let mut clear = data.clone();
        clear.item = None;
        let Some(Command::Tool(ToolAction::SetWrench { properties, .. })) = ui
            .action_command(&UiAction::SendWrench {
                brick: 7,
                variant: WrenchVariant::Normal,
                data: clear,
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            properties.item_spawn,
            ItemSpawn {
                item: None,
                ..b.item_spawn.clone()
            }
        );
        let mut unknown = data.clone();
        unknown.item = Some("unknown".into());
        assert!(
            ui.action_command(&UiAction::SendWrench {
                brick: 7,
                variant: WrenchVariant::Normal,
                data: unknown
            })
            .is_err()
        );
        let mut invalid = data.clone();
        invalid.item_respawn_ms = 999;
        assert!(
            ui.action_command(&UiAction::SendWrench {
                brick: 7,
                variant: WrenchVariant::Normal,
                data: invalid
            })
            .is_err()
        );
    }

    #[test]
    fn items_sharing_a_display_name_are_all_choices_and_bind_the_first_given() {
        let mut ui = fixture();
        ui.install_items([
            (
                "zz_sniper:weapon/sniperrifleitem".into(),
                "Sniper Rifle".into(),
            ),
            ("v20.weapon.gunitem".into(), "Gun".into()),
            (
                "aa_adventure:weapon/sniperrifleitem".into(),
                "Sniper Rifle".into(),
            ),
        ])
        .unwrap();
        let listed: Vec<_> = ui.datablocks["ItemData"]
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(
            listed,
            [
                "v20.weapon.gunitem",
                "zz_sniper:weapon/sniperrifleitem",
                "aa_adventure:weapon/sniperrifleitem"
            ]
        );
        let mut b = brick();
        b.item_spawn.item = Some(ContentRef::unresolved("item_ui", "sniper rifle"));
        let updates = open(&mut ui, &b, InspectMode::Wrench);
        assert!(matches!(
            &updates[0],
            UiUpdate::OpenWrench { data, .. }
                if data.item.as_deref() == Some("zz_sniper:weapon/sniperrifleitem")
        ));
    }

    #[test]
    fn imported_item_name_resolves_without_rewriting_inspection_or_source_records() {
        let mut ui = fixture();
        ui.install_items([("v20.weapon.hammeritem".into(), "Hammer ".into())])
            .unwrap();
        let mut b = brick();
        b.item_spawn.item = Some(ContentRef::unresolved("item_ui", "hAmMeR"));
        b.source_records.push(bri_world::SourceRecord {
            line: 1,
            text: "+-ITEM Hammer \" 0 2 4000".into(),
            diagnostic: Some("Original imported name".into()),
        });
        let original = b.clone();
        let updates = open(&mut ui, &b, InspectMode::Wrench);
        assert!(
            matches!(&updates[0],UiUpdate::OpenWrench {data,..} if data.item.as_deref()==Some("v20.weapon.hammeritem"))
        );
        assert_eq!(ui.inspection.as_ref().unwrap().brick, original);
        let aliases = [("hammer".into(), "v20.weapon.hammeritem".into())].into();
        assert!(b.item_spawn.resolve_item(&aliases).unwrap());
        assert_eq!(b.source_records, original.source_records);
        assert!(!b.item_spawn.resolve_item(&aliases).unwrap());
    }

    /// A weapons pack's item list, (id, uiName): `bri_weapons::testing`'s,
    /// or the converted v20 pack's, read from its JSON.
    struct ItemRows {
        rows: Vec<(String, String)>,
    }
    impl ItemRows {
        fn synthetic() -> anyhow::Result<Self> {
            let pack = serde_json::to_value(bri_weapons::testing::pack())?;
            Ok(Self {
                rows: Self::rows(&pack),
            })
        }
        fn content() -> anyhow::Result<Self> {
            let root = bri_package::testing::pack_dir(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
                "weapons",
            )
            .join("weapons.json");
            let pack: serde_json::Value = serde_json::from_slice(&std::fs::read(root)?)?;
            assert_eq!(pack["schema_version"], bri_weapons::SCHEMA);
            Ok(Self {
                rows: Self::rows(&pack),
            })
        }
        fn rows(pack: &serde_json::Value) -> Vec<(String, String)> {
            pack["items"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, item)| {
                    assert_eq!(item["id"].as_str(), Some(id.as_str()));
                    (id.clone(), item["ui_name"].as_str().unwrap().to_owned())
                })
                .collect()
        }
    }
    /// The real pack: 21 items, the core tools with their v20 uiNames.
    #[test]
    #[ignore = "requires generated v20 content"]
    fn native_weapon_pack_has_21_items_and_the_core_tools_v20_names() -> anyhow::Result<()> {
        let rows = ItemRows::content()?.rows;
        assert_eq!(rows.len(), 21);
        assert!(rows.contains(&("v20.weapon.hammeritem".into(), "Hammer ".into())));
        assert!(rows.contains(&("v20.weapon.wrenchitem".into(), "wrench".into())));
        assert!(rows.contains(&("v20.weapon.printgun".into(), "Printer".into())));
        assert!(rows.contains(&("v20.weapon.wanditem".into(), "Wand".into())));
        Ok(())
    }
    crate::testing::synthetic_and_content!(
        ItemRows: native_weapon_pack_and_core_tools_expose_all_21_item_choices
    );
    fn native_weapon_pack_and_core_tools_expose_all_21_item_choices(
        fx: &ItemRows,
    ) -> anyhow::Result<()> {
        let rows = fx.rows.clone();
        // The pack carries the four core tools.
        for tool in [
            bri_weapons::runtime::HAMMER,
            bri_weapons::runtime::WRENCH,
            bri_weapons::runtime::PRINTER,
            bri_weapons::runtime::WAND,
        ] {
            assert!(rows.iter().any(|(id, _)| id == tool), "{tool}");
        }
        let mut ui = fixture();
        ui.install_items(rows.clone()).unwrap();
        assert_eq!(ui.datablocks["ItemData"].len(), rows.len());
        assert_eq!(ui.server_catalog().items.len(), rows.len());
        for (id, _) in rows {
            let mut b = brick();
            b.item_spawn.item = Some(ContentRef::Resolved(id.clone()));
            let updates = open(&mut ui, &b, InspectMode::Wrench);
            let UiUpdate::OpenWrench { data, .. } = &updates[0] else {
                panic!()
            };
            let Some(Command::Tool(ToolAction::SetWrench { properties, .. })) = ui
                .action_command(&UiAction::SendWrench {
                    brick: 7,
                    variant: WrenchVariant::Normal,
                    data: data.clone(),
                })
                .unwrap()
            else {
                panic!()
            };
            assert_eq!(properties.item_spawn.item, Some(ContentRef::Resolved(id)));
        }
        Ok(())
    }
    #[test]
    fn event_rows_roundtrip_through_dialog_lines_without_coercion() {
        let catalog = events();
        let capabilities = event_catalog(&catalog);
        assert_eq!(capabilities.inputs.len(), 2);
        assert_eq!(capabilities.outputs.len(), 6);
        assert!(capabilities.outputs.iter().all(|o| o.supported));
        for (output, params) in [
            ("setColor", vec![EventValue::Color(1)]),
            ("setColorFX", vec![EventValue::Int(6)]),
            ("setRendering", vec![EventValue::Bool(false)]),
            (
                "setEventEnabled",
                vec![
                    EventValue::Rows(bri_events::RowSelection::Indices(vec![0, 2])),
                    EventValue::Bool(true),
                ],
            ),
            (
                "setLight",
                vec![EventValue::Datablock(Some("light/red".into()))],
            ),
        ] {
            let mut original = row(output, params);
            original.input = "onPlayerTouch".into();
            original.target = EventTarget::Named("lamp".into());
            let line = ui_event(&original, &catalog).unwrap();
            assert_eq!(native_event(&line, &catalog).unwrap(), original);
            let mut invalid = line.clone();
            invalid.target = "Player".into();
            invalid.named_target = None;
            assert!(native_event(&invalid, &catalog).is_err());
            let mut invalid = line.clone();
            invalid.params.push(ParamValue::Int(0));
            assert!(native_event(&invalid, &catalog).is_err());
        }
        // The dialog shows Torque X/Y/Z; the engine stores native Y-up.
        let mut velocity = row(
            "addVelocity",
            vec![EventValue::Vector(glam::Vec3::new(1.0, 5.0, -2.0))],
        );
        velocity.target = EventTarget::Slot(bri_events::Slot::Player);
        let line = ui_event(&velocity, &catalog).unwrap();
        assert_eq!(line.params, vec![ParamValue::Vector([1.0, 2.0, 5.0])]);
        assert_eq!(native_event(&line, &catalog).unwrap(), velocity);
        let mut invalid = line.clone();
        invalid.output = "relay".into();
        assert!(native_event(&invalid, &catalog).is_err());
    }
    #[test]
    fn event_delays_above_the_cap_are_rejected_by_inspection_and_submission() {
        let mut ui = fixture();
        let mut b = brick();
        b.events = vec![Row {
            delay_ms: bri_ui::models::events::MAX_DELAY_MS + 1,
            ..row("setRendering", vec![EventValue::Bool(false)])
        }];
        assert!(
            ui.accept_inspection(
                &Reply::Inspected {
                    brick_id: 7,
                    brick: Box::new(b.clone()),
                    mode: InspectMode::Events
                },
                InspectMode::Events,
                Some(7),
                &world(&b),
                &[(1, "Builder".into())].into(),
                1,
            )
            .is_err()
        );
        let mut supported = b.events[0].clone();
        supported.delay_ms = bri_ui::models::events::MAX_DELAY_MS;
        let mut line = ui_event(&supported, ui.events.as_ref().unwrap()).unwrap();
        line.delay_ms += 1;
        assert!(native_event(&line, ui.events.as_ref().unwrap()).is_err());
    }

    #[test]
    fn preserved_events_cannot_be_dropped_duplicated_or_modified() {
        let mut ui = fixture();
        let mut b = brick();
        b.events = vec![
            row("setColor", vec![EventValue::Color(1)]),
            Row {
                delay_ms: bri_ui::models::events::MAX_DELAY_MS,
                ..row("setRendering", vec![EventValue::Bool(false)])
            },
            Row {
                delay_ms: bri_ui::models::events::MAX_DELAY_MS,
                ..row("futureRenderingAction", vec![EventValue::Bool(false)])
            },
            Row {
                conditions: vec![],
                preserved: Some(bri_events::PreservedRow {
                    original: "+-EVENT\t2\t1\tonUnknown\t0\tSelf\t\tfireRelay\t\t\t\t".into(),
                    diagnostic: "Unknown input".into(),
                }),
                enabled: true,
                input: String::new(),
                delay_ms: 0,
                target: EventTarget::Slot(bri_events::Slot::SelfBrick),
                output: String::new(),
                params: vec![],
            },
        ];
        let updates = open(&mut ui, &b, InspectMode::Events);
        let UiUpdate::OpenEvents {
            rows,
            named_targets,
            ..
        } = &updates[0]
        else {
            panic!()
        };
        assert_eq!(named_targets, &["owned target"]);
        assert_eq!(rows.len(), 4);
        assert!(matches!(rows[0], EventRow::Editable(_)));
        let EventRow::Editable(long_delay) = &rows[1] else {
            panic!("the supported five-minute delay must remain editable")
        };
        assert_eq!(long_delay.delay_ms, bri_ui::models::events::MAX_DELAY_MS);
        for preserved in [2, 3] {
            assert!(matches!(rows[preserved], EventRow::Preserved { .. }));
            for mutation in 0..4 {
                let mut corrupted = rows.clone();
                if mutation == 0 {
                    corrupted.remove(preserved);
                } else if let EventRow::Preserved {
                    enabled,
                    text,
                    token,
                } = &mut corrupted[preserved]
                {
                    match mutation {
                        1 => *enabled = false,
                        2 => text.push_str(" forged"),
                        _ => token.push_str(" forged"),
                    }
                }
                assert!(
                    ui.action_command(&UiAction::SendEvents {
                        brick: 7,
                        rows: corrupted
                    })
                    .is_err()
                );
            }
            let mut duplicated = rows.clone();
            duplicated.push(rows[preserved].clone());
            assert!(
                ui.action_command(&UiAction::SendEvents {
                    brick: 7,
                    rows: duplicated
                })
                .is_err()
            );
        }
        let Some(Command::Tool(ToolAction::SetEvents { brick, events })) = ui
            .action_command(&UiAction::SendEvents {
                brick: 7,
                rows: rows.clone(),
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(brick, 7);
        assert_eq!(events, b.events);
        let mut edited = rows.clone();
        let EventRow::Editable(long_delay) = &mut edited[1] else {
            unreachable!()
        };
        long_delay.delay_ms = 60_000;
        let Some(Command::Tool(ToolAction::SetEvents { events, .. })) = ui
            .action_command(&UiAction::SendEvents {
                brick: 7,
                rows: edited,
            })
            .unwrap()
        else {
            panic!("a supported long delay can be edited and sent")
        };
        assert_eq!(events[1].delay_ms, 60_000);
        assert_eq!(events[2..], b.events[2..]);
    }
    /// Max, b5d99c948: wrenching a Portal brick said "Inspected brick
    /// definition is unavailable", so its Name, which pairs portals, could
    /// not be set. The wrench knew only the base game's bricks.
    #[test]
    fn the_wrench_opens_on_an_add_on_brick_and_renames_a_portal() {
        let base = std::env::temp_dir().join(format!("bri-wrench-base-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            base.join("stock-catalog.json"),
            serde_json::to_vec(&serde_json::json!({"schema_version": 1, "bricks": [{
                "id": "plate", "display_name": "Plate", "category": "Bricks",
                "subcategory": "Plates", "mesh_id": "plate", "collision_source": null,
                "icon_source": "", "print_aspect_ratio": null, "orientation_fix": 0,
                "can_cover": false, "indestructible": false, "special_kind": null,
                "other_properties": {}
            }]}))
            .unwrap(),
        )
        .unwrap();
        let portal_package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/brick_portal/assets/brick-catalog");
        let catalog =
            bri_sim::definitions::catalog_with(&base, &[("brick_portal".into(), portal_package)]);
        std::fs::remove_dir_all(&base).unwrap();
        let mut ui = fixture();
        ui.variants = wrench_variants(&catalog.unwrap());
        let portal = "brick_portal:brick/brickportal1x14x10data";
        let mut b = Brick::new(ContentRef::Resolved(portal.into()), [0.5, 0.1, -2.25], 1);
        b.name = Some("blue".into());
        let updates = open(&mut ui, &b, InspectMode::Wrench);
        let UiUpdate::OpenWrench { variant, data, .. } = &updates[0] else {
            panic!("the wrench window opens")
        };
        assert_eq!(*variant, WrenchVariant::Normal);
        assert_eq!(data.name, "blue");
        // Naming it after another portal pairs the two.
        let renamed = WrenchData {
            name: "orange".into(),
            ..data.clone()
        };
        let Some(Command::Tool(ToolAction::SetWrench { brick, .. })) = ui
            .action_command(&UiAction::SendWrench {
                brick: 7,
                variant: WrenchVariant::Normal,
                data: renamed,
            })
            .unwrap()
        else {
            panic!("the new name goes to the host")
        };
        assert_eq!(brick, 7);
    }
    #[test]
    fn tool_context_identity_wrench_rejection_and_print_compatibility() {
        let mut ui = fixture();
        let b = brick();
        let reply = Reply::Inspected {
            brick_id: 7,
            brick: Box::new(b.clone()),
            mode: InspectMode::Wrench,
        };
        assert!(
            ui.accept_inspection(
                &reply,
                InspectMode::Events,
                Some(7),
                &world(&b),
                &BTreeMap::new(),
                1
            )
            .is_err()
        );
        assert!(
            ui.accept_inspection(
                &reply,
                InspectMode::Wrench,
                Some(8),
                &world(&b),
                &BTreeMap::new(),
                1
            )
            .is_err()
        );
        let updates = open(&mut ui, &b, InspectMode::Wrench);
        let UiUpdate::OpenWrench { data, .. } = &updates[0] else {
            panic!()
        };
        let action = UiAction::SendWrench {
            brick: 7,
            variant: WrenchVariant::Normal,
            data: data.clone(),
        };
        assert!(matches!(
            ui.action_command(&action).unwrap(),
            Some(Command::Tool(ToolAction::SetWrench { .. }))
        ));
        let mut unsupported = data.clone();
        unsupported.sound = Some("music/not-integrated".into());
        assert!(
            ui.action_command(&UiAction::SendWrench {
                brick: 7,
                variant: WrenchVariant::Normal,
                data: unsupported
            })
            .is_err()
        );
        assert!(
            ui.action_command(&UiAction::RequestEvents { brick: 8 })
                .is_err()
        );
        open(&mut ui, &b, InspectMode::Printer);
        assert!(
            ui.action_command(&UiAction::SetPrint {
                print: "print/wide".into()
            })
            .is_err()
        );
        for id in ["print/A", "print/face"] {
            assert!(
                ui.action_command(&UiAction::SetPrint { print: id.into() })
                    .unwrap()
                    .is_some()
            );
        }
        ui.action_command(&UiAction::ClosePrintSelector).unwrap();
        assert!(
            ui.action_command(&UiAction::SetPrint {
                print: "print/A".into()
            })
            .is_err()
        );
        ui.variants.insert("plate".into(), WrenchVariant::Sound);
        assert!(
            ui.accept_inspection(
                &reply,
                InspectMode::Wrench,
                None,
                &world(&b),
                &BTreeMap::new(),
                1
            )
            .is_ok()
        );
    }
    #[test]
    fn nested_event_save_and_cancel_leave_base_wrench_usable() {
        let mut ui = fixture();
        let b = brick();
        let data = wrench_data(&b).unwrap();
        let wrench = UiAction::SendWrench {
            brick: 7,
            variant: WrenchVariant::Normal,
            data,
        };
        open(&mut ui, &b, InspectMode::Wrench);
        open(&mut ui, &b, InspectMode::Events);
        // Cancelling just the nested UI emits no write or context invalidation.
        assert!(ui.action_command(&wrench).unwrap().is_some());
        let rows = vec![EventRow::Editable(line(
            "setColor",
            vec![EventValue::Color(1)],
        ))];
        let command = ui
            .action_command(&UiAction::SendEvents { brick: 7, rows })
            .unwrap()
            .unwrap();
        ui.command_accepted(&command).unwrap();
        assert_eq!(ui.inspection.as_ref().unwrap().mode, InspectMode::Wrench);
        assert_eq!(ui.inspection.as_ref().unwrap().brick.events.len(), 1);
        let command = ui.action_command(&wrench).unwrap().unwrap();
        ui.command_accepted(&command).unwrap();
        assert!(ui.action_command(&wrench).is_err());
    }
    #[test]
    fn more_than_100_event_rows_keep_zero_delay_and_order() {
        let mut ui = fixture();
        let mut b = brick();
        b.events = (0..bri_world::MAX_EVENTS_PER_BRICK)
            .map(|i| row("setColor", vec![EventValue::Color((i % 2) as u8)]))
            .collect();
        let updates = open(&mut ui, &b, InspectMode::Events);
        let UiUpdate::OpenEvents { rows, .. } = &updates[0] else {
            panic!()
        };
        assert_eq!(rows.len(), bri_world::MAX_EVENTS_PER_BRICK);
        let Some(Command::Tool(ToolAction::SetEvents { events, .. })) = ui
            .action_command(&UiAction::SendEvents {
                brick: 7,
                rows: rows.clone(),
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(events, b.events);
        assert!(events.iter().all(|e| e.delay_ms == 0));
    }
    #[test]
    fn imported_bls_print_alias_is_bound_without_rewriting_source_state() {
        let mut ui = fixture();
        let mut b = brick();
        b.print = Some(ContentRef::unresolved("print", "Letters/A"));
        let original = b.clone();
        let updates = open(&mut ui, &b, InspectMode::Printer);
        assert!(
            matches!(&updates[0], UiUpdate::OpenPrintSelector { current: Some(id), .. } if id == "print/A")
        );
        assert_eq!(ui.inspection.as_ref().unwrap().brick, original);
        assert!(
            matches!(ui.action_command(&UiAction::SetPrint { print: "print/A".into() }).unwrap(), Some(Command::Tool(ToolAction::SetPrint { brick: 7, print: Some(id) })) if id == "print/A")
        );
        b.print = Some(ContentRef::unresolved("print", "Community/unknown"));
        assert!(
            ui.accept_inspection(
                &Reply::Inspected {
                    brick_id: 7,
                    brick: Box::new(b.clone()),
                    mode: InspectMode::Printer
                },
                InspectMode::Printer,
                None,
                &world(&b),
                &BTreeMap::new(),
                1
            )
            .is_err()
        );
    }
    #[test]
    #[ignore = "requires generated native stock content, no window"]
    fn real_native_catalog_has_original_icons_and_default_letter() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = crate::content::ClientContent::load(&root).unwrap();
        let materials: Bundle = serde_json::from_slice(
            &std::fs::read(content.paths.brick_materials.join("brick-materials.json")).unwrap(),
        )
        .unwrap();
        let ui = ToolUi::new(
            &content.catalog,
            &content.effects,
            &materials,
            &content.ui_pack,
        )
        .unwrap();
        assert_eq!(ui.server_catalog().prints.len(), materials.prints.len());
        assert_eq!(
            ui.catalog.default_print.as_deref(),
            Some(materials.resolve("Letters/A").unwrap().id.as_str())
        );
        assert_eq!(
            ui.prints.values().map(Vec::len).sum::<usize>(),
            materials.prints.len()
        );
        assert!(ui.prints.values().flatten().all(|p| matches!(&p.icon, IconRef::Pack(key) if content.ui_pack.data.images.contains_key(key))));
        eprintln!(
            "{} native prints, {} printable definitions, {} lights, {} emitters; every original UI icon resolves",
            ui.catalog.prints.len(),
            ui.catalog.brick_print_aspects.len(),
            ui.catalog.lights.len(),
            ui.catalog.emitters.len()
        );
    }
}
