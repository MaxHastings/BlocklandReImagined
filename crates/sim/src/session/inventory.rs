//! One authoritative tool inventory shared by core building tools and weapons.
use super::*;
use bri_weapons::{ActorId, CORE_TOOLS, Pack, WeaponsWorld};

pub const TOOL_SLOTS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInventory {
    pub slots: Vec<Option<String>>,
    pub selected: Option<usize>,
}
impl Default for ToolInventory {
    fn default() -> Self {
        Self {
            slots: CORE_TOOLS[..3]
                .iter()
                .map(|id| Some((*id).into()))
                .chain([None, None])
                .collect(),
            selected: None,
        }
    }
}
impl ToolInventory {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.slots.len() == TOOL_SLOTS, "Invalid tool slot count");
        for item in self.slots.iter().flatten() {
            ensure!(
                item.len() <= 128
                    && item.starts_with("v20.weapon.")
                    && item.bytes().all(|c| c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || c == b'.'
                        || c == b'_'),
                "Invalid inventory item"
            );
        }
        ensure!(
            self.selected
                .is_none_or(|slot| self.slots.get(slot).is_some_and(Option::is_some)),
            "Selected empty tool slot"
        );
        Ok(())
    }
}

pub(super) fn core_runtime() -> WeaponsWorld {
    WeaponsWorld::new(Pack {
        schema_version: bri_weapons::SCHEMA,
        id: "core-tools".into(),
        items: BTreeMap::new(),
        images: BTreeMap::new(),
        projectiles: BTreeMap::new(),
        damage_types: BTreeMap::new(),
        explosions: BTreeMap::new(),
        definitions: Vec::new(),
        resources: Vec::new(),
        diagnostics: Vec::new(),
    })
    .expect("Static core tool definitions are valid")
}

impl Session {
    /// Trusted host entry point; network callers go through sequenced commands.
    /// Switching equipment revokes any dialog capability granted by a prior hit.
    /// v20's sports package: using a tool drops a held ball, while putting
    /// tools away (`serverCmdUnUseTool`) keeps it in hand.
    pub fn equip_tool(&mut self, owner: OwnerId, slot: Option<usize>) -> Result<()> {
        ensure!(self.peers.contains_key(&owner), "Unknown connection");
        if slot.is_none() && self.weapons.holds_ball(ActorId(owner)) {
            return Ok(());
        }
        if slot.is_some() {
            self.weapons.drop_ball(ActorId(owner))?;
        }
        let previous = self
            .weapons
            .actor(ActorId(owner))
            .context("Missing inventory")?
            .selected;
        self.weapons.equip(ActorId(owner), slot)?;
        self.weapon_triggers.remove(&owner);
        if previous != slot {
            self.peers.get_mut(&owner).unwrap().inspection = None;
        }
        // Choosing a brick puts tools away; the brick stays in hand whichever
        // of the two requests lands first.
        if slot.is_none() && self.brick_equipped(owner) {
            self.hold_brick(owner)?;
        }
        Ok(())
    }
    /// Called during host setup, before accepting connections. Item grants are
    /// internal authority operations; there is deliberately no remote Give command.
    pub fn set_weapon_pack(&mut self, pack: Pack) -> Result<()> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty(),
            "Cannot replace live item definitions"
        );
        let catalog = super::combat::catalog(&pack);
        let mut weapons = WeaponsWorld::new(pack)?;
        ensure!(
            self.item_spawners
                .bounds
                .keys()
                .all(|id| weapons.contains_item(id)),
            "Weapon pack invalidates item physics catalog"
        );
        ensure!(
            self.spawn_loadout
                .slots
                .iter()
                .flatten()
                .all(|id| weapons.contains_item(id)),
            "Weapon pack invalidates spawn loadout"
        );
        weapons.tick = self.simulation.state().tick;
        self.weapons = weapons;
        self.minigames = super::combat::new_world(catalog, &self.archetypes);
        self.refresh_event_bindings()
    }

    pub(super) fn spawn_inventory(&mut self, owner: OwnerId) -> Result<()> {
        let actor = ActorId(owner);
        self.weapons.add_actor(actor, TOOL_SLOTS)?;
        for (slot, item) in self.spawn_loadout.slots.iter().enumerate() {
            if let Some(item) = item {
                self.weapons.give_at(actor, slot, item)?;
            }
        }
        Ok(())
    }

    pub fn tool_inventories(&self) -> BTreeMap<OwnerId, ToolInventory> {
        self.peers
            .keys()
            .filter_map(|owner| {
                self.weapons.actor(ActorId(*owner)).map(|actor| {
                    (
                        *owner,
                        ToolInventory {
                            slots: actor.inventory.clone(),
                            selected: actor.selected,
                        },
                    )
                })
            })
            .collect()
    }

    pub fn give_item(&mut self, owner: OwnerId, item: &str) -> Result<usize> {
        ensure!(self.peers.contains_key(&owner), "Unknown connection");
        self.weapons.give(ActorId(owner), item)
    }

    /// Host configuration boundary for subsequent minigame loadout binding.
    /// The default remains the original three tools, never an implicit weapon.
    pub fn set_spawn_loadout(&mut self, loadout: ToolInventory) -> Result<()> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty(),
            "Cannot replace a live spawn loadout"
        );
        loadout.validate()?;
        ensure!(
            loadout.selected.is_none()
                && loadout
                    .slots
                    .iter()
                    .flatten()
                    .all(|id| self.weapons.contains_item(id)),
            "Invalid spawn loadout"
        );
        self.spawn_loadout = loadout;
        Ok(())
    }
}

pub(super) fn require_equipment(
    weapons: &WeaponsWorld,
    owner: OwnerId,
    expected: Option<&str>,
) -> Result<()> {
    let actor = weapons.actor(ActorId(owner)).context("Missing inventory")?;
    let selected = actor
        .selected
        .and_then(|slot| actor.inventory.get(slot))
        .and_then(Option::as_deref);
    ensure!(selected == expected, "Required tool is not equipped");
    Ok(())
}
