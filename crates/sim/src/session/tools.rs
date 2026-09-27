//! Native editing authority. No client positions, identities or arbitrary
//! source records cross this boundary. Minigame permissions,
//! spray projectile flight and audiovisual effects remain separate adapters.
use super::*;

/// Original game.cs constructs New_QueueSO(512). This queue currently records
/// planting only; vanilla paint/FX/print undo and chain-kill effects remain work.
pub const UNDO_PLANT_LIMIT: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectMode {
    Wrench,
    Printer,
    Events,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ToolAction {
    Hammer,
    Paint {
        color: u8,
    },
    ColorEffect {
        effect: u8,
    },
    ShapeEffect {
        effect: u8,
    },
    Inspect {
        mode: InspectMode,
    },
    SetPrint {
        brick: BrickId,
        print: Option<String>,
    },
    SetWrench {
        brick: BrickId,
        properties: WrenchProperties,
    },
    /// Only the implemented native event subset. Unsupported source records
    /// remain untouched on the server and cannot be forged or removed here.
    SetEvents {
        brick: BrickId,
        events: Vec<bri_world::EventRow>,
    },
    UndoPlant,
    /// Vehicle spawn wrench `< Respawn >`.
    RespawnVehicle {
        brick: BrickId,
    },
}

/// Server-configured bindings, never supplied by a remote player. An empty
/// catalog permits native colors/flags but denies nonempty content assignments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolCatalog {
    pub lights: BTreeSet<String>,
    pub emitters: BTreeSet<String>,
    /// ItemData choices installed by the host from the validated native item
    /// pack, including the core tools. Empty denies every nonempty assignment.
    pub items: BTreeSet<String>,
    /// Stable print ID -> original print aspect (including universal Letters).
    pub prints: BTreeMap<String, String>,
    /// Stable brick definition ID -> original print aspect.
    pub brick_print_aspects: BTreeMap<String, String>,
    /// Original new printable bricks use Letters/A unless a last-print choice
    /// exists. None is useful for synthetic servers without a print catalog.
    pub default_print: Option<String>,
    /// Music loops a sound brick may play.
    pub sounds: BTreeSet<String>,
    /// Vehicles a vehicle spawn brick may hold.
    pub vehicles: BTreeSet<String>,
    /// Brick definitions that accept a sound / a vehicle.
    pub sound_bricks: BTreeSet<String>,
    pub vehicle_bricks: BTreeSet<String>,
}

impl ToolCatalog {
    /// Replace the item allowlist atomically before publishing to a Session.
    /// Does not read original resources or trust remote client-supplied IDs.
    pub fn install_items(&mut self, items: impl IntoIterator<Item = String>) -> Result<()> {
        let mut allowed = BTreeSet::new();
        for id in items {
            ensure!(allowed.len() < 1024, "Too many native item choices");
            ContentRef::Resolved(id.clone()).validate()?;
            ensure!(!id.chars().any(char::is_control), "Invalid native item ID");
            ensure!(allowed.insert(id), "Duplicate native item ID");
        }
        self.items = allowed;
        Ok(())
    }
    fn validate(&self, simulation: &Simulation) -> Result<()> {
        ensure!(
            self.lights.len() <= 100_000
                && self.emitters.len() <= 100_000
                && self.items.len() <= 1024
                && self.prints.len() <= 100_000
                && self.brick_print_aspects.len() <= 100_000,
            "Tool catalog exceeds limit"
        );
        for id in self
            .lights
            .iter()
            .chain(self.emitters.iter())
            .chain(self.items.iter())
            .chain(self.prints.keys())
        {
            ContentRef::Resolved(id.clone()).validate()?;
        }
        for (definition, aspect) in &self.brick_print_aspects {
            ensure!(
                simulation.definitions.entries.contains_key(definition),
                "Print brick definition is unavailable: {definition}"
            );
            validate_aspect(aspect)?;
        }
        for aspect in self.prints.values() {
            validate_aspect(aspect)?;
        }
        if let Some(id) = &self.default_print {
            ensure!(
                self.prints
                    .get(id)
                    .is_some_and(|aspect| aspect.eq_ignore_ascii_case("Letters")),
                "Default print must be an available universal Letters print"
            );
        }
        Ok(())
    }
    fn print_aspect<'a>(&'a self, brick: &Brick) -> Result<&'a str> {
        let ContentRef::Resolved(id) = &brick.definition else {
            anyhow::bail!("Unresolved brick cannot be printed");
        };
        self.brick_print_aspects
            .get(id)
            .map(String::as_str)
            .context("Brick has no configured print aspect")
    }
    fn validate_print(&self, brick: &Brick, print: &ContentRef) -> Result<()> {
        let aspect = self.print_aspect(brick)?;
        let ContentRef::Resolved(id) = print else {
            anyhow::bail!("Cannot assign unresolved print");
        };
        let print_aspect = self.prints.get(id).context("Print is unavailable")?;
        ensure!(
            print_aspect.eq_ignore_ascii_case(aspect)
                || print_aspect.eq_ignore_ascii_case("Letters"),
            "Print aspect does not match brick"
        );
        Ok(())
    }
    pub(super) fn validate_edit(&self, brick: &Brick, edit: &Edit) -> Result<()> {
        match edit {
            Edit::Print(Some(print)) => self.validate_print(brick, print)?,
            Edit::Properties(properties) => {
                properties.item_spawn.validate()?;
                if let Some(item) = &properties.item_spawn.item {
                    validate_asset(item, &self.items, "Item")?;
                }
                if let Some(light) = &properties.light {
                    validate_asset(&ContentRef::Resolved(light.clone()), &self.lights, "Light")?;
                }
                if let Some(emitter) = &properties.emitter {
                    validate_asset(
                        &ContentRef::Resolved(emitter.clone()),
                        &self.emitters,
                        "Emitter",
                    )?;
                }
                if let Some(name) = &properties.name {
                    ensure!(!name.chars().any(char::is_control), "Invalid brick name");
                }
                let definition = match &brick.definition {
                    ContentRef::Resolved(id) => id.as_str(),
                    _ => "",
                };
                if let Some(sound) = &properties.sound {
                    ensure!(
                        self.sound_bricks.contains(definition),
                        "Only music bricks can play music"
                    );
                    validate_asset(&ContentRef::Resolved(sound.clone()), &self.sounds, "Music")?;
                }
                if let Some(vehicle) = &properties.vehicle {
                    ensure!(
                        self.vehicle_bricks.contains(definition),
                        "Only vehicle spawn bricks can hold vehicles"
                    );
                    validate_asset(
                        &ContentRef::Resolved(vehicle.clone()),
                        &self.vehicles,
                        "Vehicle",
                    )?;
                }
            }
            // Event rows are checked against the event catalog's bindings.
            _ => {}
        }
        Ok(())
    }
}
fn validate_aspect(aspect: &str) -> Result<()> {
    ensure!(
        !aspect.is_empty() && aspect.len() <= 128 && !aspect.chars().any(char::is_control),
        "Invalid print aspect"
    );
    Ok(())
}
fn validate_asset(reference: &ContentRef, allowed: &BTreeSet<String>, kind: &str) -> Result<()> {
    ensure!(
        matches!(reference, ContentRef::Resolved(id) if allowed.contains(id)),
        "{kind} is unavailable or unresolved"
    );
    Ok(())
}
pub(super) struct Inspection {
    id: BrickId,
    mode: InspectMode,
    original: Brick,
    wrench_original: Option<Brick>,
}

impl Session {
    /// Catalog installation is an atomic local-server decision. Existing world
    /// source references may remain unresolved; new assignments may not.
    pub fn set_tool_catalog(&mut self, catalog: ToolCatalog) -> Result<()> {
        catalog.validate(&self.simulation)?;
        self.tool_catalog = catalog;
        for peer in self.peers.values_mut() {
            peer.inspection = None;
        }
        self.refresh_event_bindings()
    }
    pub(super) fn tool_action(
        &mut self,
        owner: OwnerId,
        action: ToolAction,
        direction: glam::Vec3,
    ) -> Result<Reply> {
        let required = match &action {
            ToolAction::UndoPlant => None,
            ToolAction::Hammer => Some(Some(bri_weapons::CORE_TOOLS[0])),
            ToolAction::Inspect {
                mode: InspectMode::Printer,
            }
            | ToolAction::SetPrint { .. } => Some(Some(bri_weapons::CORE_TOOLS[2])),
            ToolAction::Inspect { .. }
            | ToolAction::SetWrench { .. }
            | ToolAction::SetEvents { .. }
            | ToolAction::RespawnVehicle { .. } => Some(Some(bri_weapons::CORE_TOOLS[1])),
            ToolAction::Paint { .. }
            | ToolAction::ColorEffect { .. }
            | ToolAction::ShapeEffect { .. } => Some(None),
        };
        if let Some(required) = required {
            inventory::require_equipment(&self.weapons, owner, required)?;
        }
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        if action == ToolAction::UndoPlant {
            let undo = self.plant_undo.entry(owner).or_default();
            // Missing entries may have been hammered by their owner/admin.
            while let Some(id) = undo.back().copied() {
                let Some(brick) = self.simulation.state().bricks.get(&id) else {
                    undo.pop_back();
                    continue;
                };
                ensure!(brick.owner == owner, "Undo brick ownership changed");
                self.simulation.remove(&peer.actor, id)?;
                undo.pop_back();
                self.dirty.insert(id);
                peer.inspection = None;
                return Ok(Reply::Undone(Some(id)));
            }
            return Ok(Reply::Undone(None));
        }
        let range = match action {
            ToolAction::Hammer => {
                if direction.y < -0.9 {
                    5.5
                } else {
                    5.0
                }
            }
            // Bounded initial targeting adapter: original spray projectile is
            // 20 units/s for 400ms; FX uses 525ms. This is not flight simulation.
            ToolAction::Paint { .. } => 8.0,
            ToolAction::ColorEffect { .. } | ToolAction::ShapeEffect { .. } => 10.5,
            _ => 10.0,
        };
        let id = self
            .simulation
            .target_bricks_always(peer.player.eye(), direction, range)?
            .and_then(|hit| hit.brick)
            .context("Tool target is out of reach or obstructed")?;
        let brick = &self.simulation.state().bricks[&id];
        ensure!(
            peer.actor.administrator || (owner != 0 && brick.owner == owner),
            "Brick edit denied"
        );
        if let ToolAction::Inspect { mode } = action {
            if mode == InspectMode::Printer {
                self.tool_catalog.print_aspect(brick)?;
            }
            peer.inspection = Some(Inspection {
                id,
                mode,
                original: brick.clone(),
                wrench_original: if mode == InspectMode::Wrench {
                    Some(brick.clone())
                } else if mode == InspectMode::Events {
                    peer.inspection
                        .as_ref()
                        .filter(|previous| previous.id == id)
                        .and_then(|previous| previous.wrench_original.clone())
                } else {
                    None
                },
            });
            let reply = Reply::Inspected {
                brick_id: id,
                brick: Box::new(brick.clone()),
                mode,
            };
            if mode == InspectMode::Wrench {
                self.cues.emit(
                    self.simulation.state().tick,
                    crate::presentation::CueKind::WrenchHit,
                    brick.position,
                );
            }
            return Ok(reply);
        }
        if let ToolAction::RespawnVehicle { brick: expected } = action {
            ensure!(id == expected, "Vehicle spawn brick is out of reach");
            peer.inspection = None;
            self.respawn_vehicle_brick(id)?;
            return Ok(Reply::Accepted);
        }
        if action == ToolAction::Hammer {
            let position = brick.position;
            let actor = Actor {
                owner: peer.actor.owner,
                administrator: peer.actor.administrator,
            };
            peer.inspection = None;
            self.kill_brick(&actor, id, super::debris::BrickBlast::pop(position.into()))?;
            self.cues.emit(
                self.simulation.state().tick,
                crate::presentation::CueKind::HammerHit,
                position,
            );
            return Ok(Reply::Accepted);
        }
        let (edit, dialog) = match action {
            ToolAction::Paint { color } => (Edit::Color(color), None),
            ToolAction::ColorEffect { effect } => (Edit::ColorEffect(effect), None),
            ToolAction::ShapeEffect { effect } => (Edit::ShapeEffect(effect), None),
            ToolAction::SetPrint {
                brick: expected,
                print,
            } => {
                self.tool_catalog.print_aspect(brick)?;
                (
                    Edit::Print(print.map(ContentRef::Resolved)),
                    Some((expected, InspectMode::Printer)),
                )
            }
            ToolAction::SetWrench {
                brick: expected,
                properties,
            } => (
                Edit::Properties(properties),
                Some((expected, InspectMode::Wrench)),
            ),
            ToolAction::SetEvents {
                brick: expected,
                events,
            } => (Edit::Events(events), Some((expected, InspectMode::Events))),
            _ => unreachable!(),
        };
        if let Some((expected, mode)) = dialog {
            ensure!(
                id == expected,
                "Inspected brick is out of reach or obstructed"
            );
            let inspection = peer
                .inspection
                .as_ref()
                .context("Inspect the brick before editing")?;
            ensure!(
                inspection.id == id
                    && (inspection.mode == mode
                        || (mode == InspectMode::Wrench
                            && inspection.mode == InspectMode::Events
                            && inspection.wrench_original.is_some())),
                "Inspection does not match this edit"
            );
            ensure!(
                (if mode == InspectMode::Wrench {
                    inspection
                        .wrench_original
                        .as_ref()
                        .unwrap_or(&inspection.original)
                } else {
                    &inspection.original
                }) == brick,
                "Brick changed since inspection; inspect it again"
            );
        }
        self.tool_catalog.validate_edit(brick, &edit)?;
        self.item_spawners
            .validate_edit(self.simulation.state(), id, &edit)?;
        let edited_events = matches!(edit, Edit::Events(_));
        self.simulation.edit(&peer.actor, id, edit)?;
        self.dirty.insert(id);
        if edited_events && let Some(inspection) = &mut peer.inspection {
            // Event dialogs sit above the still-open wrench. Update only the
            // events we just authored; retain the original wrench property
            // snapshot so concurrent property edits cannot be overwritten.
            if let Some(original) = &mut inspection.wrench_original {
                original.events = self.simulation.state().bricks[&id].events.clone();
                inspection.original = original.clone();
                inspection.mode = InspectMode::Wrench;
            } else {
                peer.inspection = None;
            }
        } else {
            peer.inspection = None;
        }
        Ok(Reply::Accepted)
    }
}
