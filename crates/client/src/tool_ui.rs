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
    /// Every installed music loop; the wrench lists those the host offers.
    music: Vec<Choice>,
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
        let variants = catalog
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
            .collect();
        Ok(Self {
            catalog: tool_catalog,
            prints,
            print_aliases,
            item_aliases: BTreeMap::new(),
            datablocks,
            variants,
            inspection: None,
            events: None,
            music: Vec::new(),
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
        let mut aliases = BTreeMap::new();
        for (id, name) in items {
            ensure!(choices.len() < 1024, "Too many native item choices");
            ensure!(
                !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control),
                "Invalid native item display name"
            );
            let key = name.trim().to_ascii_lowercase();
            ensure!(
                aliases.insert(key, id.clone()).is_none(),
                "Ambiguous native item display name"
            );
            choices.push(Choice { id, name });
        }
        let mut catalog = self.catalog.clone();
        catalog.install_items(choices.iter().map(|c| c.id.clone()))?;
        choices.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.id.cmp(&b.id))
        });
        self.catalog = catalog;
        self.item_aliases = aliases;
        self.datablocks.insert("ItemData".into(), choices);
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
        self.datablocks.insert("Music".into(), self.music.clone());
        self.datablocks.insert("Vehicle".into(), menu(vehicles));
        self.invalidate();
        Ok(())
    }
    /// The wrench's Music list shows only the loops the host offers (its
    /// Music Files), as v20 clients knew only the host's music datablocks.
    pub fn offer_music(&mut self, offered: &std::collections::BTreeSet<String>) {
        let music = self
            .music
            .iter()
            .filter(|c| offered.contains(&c.id))
            .cloned()
            .collect();
        self.datablocks.insert("Music".into(), music);
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
        self.events = Some(catalog);
        self.invalidate();
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
            return self.fill_wrench(data, fields).map(|fill| Some(Command::WrenchCopy(fill)));
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
    let row = bri_events::migration::ui_event(&serde_json::to_value(line)?)?;
    let row = bri_events::migration::normalize_ui_row(catalog, row)?;
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
    let (target, named_target, class) = match &row.target {
        EventTarget::Slot(slot) => {
            let (name, class) = input
                .targets
                .iter()
                .find(|(s, _)| bri_events::Slot::parse(s) == Some(*slot))
                .context("Target unavailable for input")?;
            (
                name.clone(),
                None,
                bri_events::Class::parse(class).context("Unknown target class")?,
            )
        }
        EventTarget::Named(name) => (
            NAMED_BRICK.to_string(),
            Some(name.clone()),
            bri_events::Class::Brick,
        ),
    };
    let output = catalog
        .output(class, &row.output)
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
            music: Vec::new(),
        }
    }
    #[test]
    fn the_wrench_lists_only_the_music_the_host_offers() {
        let mut ui = fixture();
        let loops = vec![
            ("music/bass".to_string(), "Bass 1".to_string()),
            ("music/rock".to_string(), "Rock".to_string()),
        ];
        ui.install_special(loops, vec![]).unwrap();
        assert_eq!(ui.datablocks["Music"].len(), 2);
        ui.offer_music(&["music/rock".to_string()].into());
        let listed: Vec<_> = ui.datablocks["Music"]
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(listed, ["Rock"]);
        // The next host's list starts again from every installed loop.
        ui.offer_music(&["music/bass".to_string(), "music/rock".to_string()].into());
        assert_eq!(ui.datablocks["Music"].len(), 2);
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

    #[test]
    #[ignore = "requires converted weapons-pack-009 native JSON; no window or original reads"]
    fn native_weapon_pack_and_core_tools_expose_all_21_item_choices() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/weapons-pack-009/weapons.json");
        let bytes = std::fs::read(root).unwrap();
        let pack: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(pack["schema_version"], bri_weapons::SCHEMA);
        let rows: Vec<(String, String)> = pack["items"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, item)| {
                assert_eq!(item["id"].as_str(), Some(id.as_str()));
                (id.clone(), item["ui_name"].as_str().unwrap().to_owned())
            })
            .collect();
        // The pack carries the four core tools with their v20 uiNames.
        assert_eq!(rows.len(), 21);
        assert!(rows.contains(&("v20.weapon.hammeritem".into(), "Hammer ".into())));
        assert!(rows.contains(&("v20.weapon.wrenchitem".into(), "wrench".into())));
        assert!(rows.contains(&("v20.weapon.printgun".into(), "Printer".into())));
        assert!(rows.contains(&("v20.weapon.wanditem".into(), "Wand".into())));
        let mut ui = fixture();
        ui.install_items(rows.clone()).unwrap();
        assert_eq!(ui.datablocks["ItemData"].len(), 21);
        assert_eq!(ui.server_catalog().items.len(), 21);
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
    fn preserved_events_cannot_be_dropped_duplicated_or_modified() {
        let mut ui = fixture();
        let mut b = brick();
        b.events = vec![
            row("setColor", vec![EventValue::Color(1)]),
            Row {
                delay_ms: 60_000,
                ..row("setRendering", vec![EventValue::Bool(false)])
            },
            Row {
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
        assert_eq!(rows.len(), 3);
        assert!(matches!(rows[0], EventRow::Editable(_)));
        assert!(matches!(rows[1], EventRow::Preserved { .. }));
        for mutation in 0..4 {
            let mut corrupted = rows.clone();
            if mutation == 0 {
                corrupted.pop();
            } else if let EventRow::Preserved {
                enabled,
                text,
                token,
            } = &mut corrupted[2]
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
        duplicated.push(rows[2].clone());
        assert!(
            ui.action_command(&UiAction::SendEvents {
                brick: 7,
                rows: duplicated
            })
            .is_err()
        );
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
