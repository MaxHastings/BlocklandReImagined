//! Stock building tools. The hammer, wrench, printer, wands and spray cans
//! are v20 images run by the weapon state machine; their `onFire` scripts
//! land here as server raycasts from the swinger's eye. No client positions,
//! identities or arbitrary source records cross this boundary.
use super::undo::UndoEntry;
use super::*;
use bri_weapons::{ActorId, TargetId};
use bri_world::authority::trust as level;

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
    /// Open the events dialog over the brick the wrench last hit.
    Inspect { mode: InspectMode },
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
    /// `serverCmdUndoBrick` (Ctrl+Z).
    UndoBrick,
    /// Vehicle spawn wrench `< Respawn >`.
    RespawnVehicle { brick: BrickId },
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
    /// Stable brick definition ID -> "Category/Subcategory/Name" as the
    /// build menu shows it, so packages can name bricks to players.
    pub brick_names: BTreeMap<String, String>,
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
    /// Add the emitters and lights Add-Ons name to the ones a brick may
    /// take (the base game's stay).
    pub fn install_effects(
        &mut self,
        emitters: impl IntoIterator<Item = String>,
        lights: impl IntoIterator<Item = String>,
    ) -> Result<()> {
        let mut added = (self.emitters.clone(), self.lights.clone());
        for (set, ids) in [
            (&mut added.0, emitters.into_iter().collect::<Vec<_>>()),
            (&mut added.1, lights.into_iter().collect()),
        ] {
            ensure!(ids.len() <= 1024, "Too many Add-On effect choices");
            for id in ids {
                ContentRef::Resolved(id.clone()).validate()?;
                ensure!(!id.chars().any(char::is_control), "Invalid effect ID");
                set.insert(id);
            }
        }
        (self.emitters, self.lights) = added;
        Ok(())
    }
    fn validate(&self, simulation: &Simulation) -> Result<()> {
        ensure!(
            self.lights.len() <= 100_000
                && self.emitters.len() <= 100_000
                && self.items.len() <= 1024
                && self.prints.len() <= 100_000
                && self.brick_print_aspects.len() <= 100_000
                && self.brick_names.len() <= 100_000
                && self.brick_names.values().all(|n| n.len() <= 256),
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

/// `serverCmdUseSprayCan` mounts `color<N>SprayCanImage`, which
/// `setSprayCanColor` derives from this can for every palette colour. The
/// native image keeps the palette index instead of a derived datablock.
pub const SPRAY_CAN_IMAGE: &str = "v20.image.bluespraycanimage";
/// `serverCmdUseFXCan` index order.
pub const FX_CAN_IMAGES: [&str; 9] = [
    "v20.image.flatspraycanimage",
    "v20.image.pearlspraycanimage",
    "v20.image.chromespraycanimage",
    "v20.image.glowspraycanimage",
    "v20.image.blinkspraycanimage",
    "v20.image.swirlspraycanimage",
    "v20.image.rainbowspraycanimage",
    "v20.image.stablespraycanimage",
    "v20.image.jellospraycanimage",
];
/// `serverCmdWand`.
pub const WAND_IMAGE: &str = "v20.image.wandimage";
/// `serverCmdMagicWand` (the admin Destructo Wand).
pub const ADMIN_WAND_IMAGE: &str = "v20.image.adminwandimage";

/// Brick edit a paint projectile applies on contact (`paintProjectile::onCollision`
/// and the FX cans' `<fx>PaintProjectile::onCollision`).
fn paint_edit(definition: &str, paint: Option<u8>) -> Option<Edit> {
    let name = definition.strip_prefix("v20.projectile.")?;
    let color_effects = [
        "flat", "pearl", "chrome", "glow", "blink", "swirl", "rainbow",
    ];
    if let Some(index) = color_effects
        .iter()
        .position(|fx| name.strip_suffix("paintprojectile") == Some(fx))
    {
        return Some(Edit::ColorEffect(index as u8));
    }
    match name {
        "stablepaintprojectile" => Some(Edit::ShapeEffect(0)),
        "jellopaintprojectile" => Some(Edit::ShapeEffect(1)),
        "bluepaintprojectile" => paint.map(Edit::Color),
        _ => None,
    }
}

fn copy_actor(actor: &Actor) -> Actor {
    actor.clone()
}

/// `containerRayCast` type masks used by the stock tools.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Hammer and wands: interiors, terrain, bricks, players and vehicles.
    Melee,
    /// Wrench: interiors, terrain and bricks.
    Wrench,
    /// Printer: bricks.
    Bricks,
}
#[derive(Clone, Copy, Debug)]
struct ToolHit {
    target: TargetId,
    position: Vec3,
    normal: Vec3,
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

    /// Trusted host edit with a player's own brick authority, for scripted
    /// setup. Network players change bricks only through their tools.
    pub fn edit_brick(&mut self, owner: OwnerId, id: BrickId, edit: Edit) -> Result<()> {
        let actor = copy_actor(&self.peers.get(&owner).context("Unknown connection")?.actor);
        if let Edit::Events(rows) = &edit {
            self.validate_event_rows(rows)?;
        }
        let brick = self
            .simulation
            .state()
            .bricks
            .get(&id)
            .context("Unknown brick")?;
        self.tool_catalog.validate_edit(brick, &edit)?;
        self.item_spawners
            .validate_edit(self.simulation.state(), id, &edit)?;
        self.simulation.edit(&actor, id, edit)?;
        self.dirty.insert(id);
        Ok(())
    }

    /// `serverCmdUseSprayCan` / `serverCmdUseFXCan`: put a can in the right
    /// hand. Like v20 this deselects the tool slot and drops a held ball.
    pub(super) fn use_spray_can(
        &mut self,
        owner: OwnerId,
        image: &str,
        paint: Option<u8>,
    ) -> Result<()> {
        if !self.tutorial_allows_spray(owner) {
            return Ok(());
        }
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Paint,
        )?;
        if let Some(color) = paint {
            ensure!(
                usize::from(color) < self.simulation.state().palette.len(),
                "Color outside world palette"
            );
        }
        let picker = self
            .weapons
            .image_state(ActorId(owner), 0)
            .filter(|(held, _)| held.paint_picker)
            .map(|(held, _)| held.id.clone());
        // A picker taken out of the tool box goes back as that tool, still
        // selected, so the holder's tool box stays on it.
        let slot = self
            .weapons
            .actor(ActorId(owner))
            .and_then(|a| a.selected)
            .filter(|_| picker.is_some());
        self.hold_image(owner, image, paint)?;
        match (slot, picker) {
            (Some(slot), Some(_)) => self.weapons.equip(ActorId(owner), Some(slot))?,
            (None, Some(picker)) => self.weapons.mount_image(ActorId(owner), &picker, None)?,
            _ => {}
        }
        // `serverCmdUseSprayCan` remembers the colour; FX cans do not, but
        // which FX can came last is kept for tools that paint with it.
        if let Some(peer) = self.peers.get_mut(&owner) {
            match paint {
                Some(color) => {
                    peer.current_color = color;
                    peer.fx_can = None;
                }
                None => {
                    peer.fx_can = FX_CAN_IMAGES
                        .iter()
                        .position(|fx| *fx == image)
                        .map(|i| i as u8)
                }
            }
        }
        Ok(())
    }

    /// `serverCmdMagicWand`: administrators get the Destructo Wand.
    pub(super) fn use_admin_wand(&mut self, owner: OwnerId) -> Result<()> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        ensure!(
            peer.actor.administrator,
            "Only administrators can use the Destructo Wand"
        );
        if !self.tutorial_allows_wand(owner) {
            return Ok(());
        }
        ensure!(peer.combat.alive, "You are dead");
        self.hold_image(owner, ADMIN_WAND_IMAGE, None)
    }

    /// `serverCmdWand`: the player wand, unless a minigame disables it or
    /// the tutorial keeps it for the wand room.
    pub fn use_wand(&mut self, owner: OwnerId) -> Result<()> {
        if !self.tutorial_allows_wand(owner) {
            return Ok(());
        }
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Wand,
        )?;
        self.hold_image(owner, WAND_IMAGE, None)
    }

    fn hold_image(&mut self, owner: OwnerId, image: &str, paint: Option<u8>) -> Result<()> {
        self.weapons.drop_ball(ActorId(owner))?;
        self.weapons.mount_image(ActorId(owner), image, paint)?;
        if let Some(peer) = self.peers.get_mut(&owner) {
            peer.inspection = None;
        }
        Ok(())
    }

    /// The `onFire` of a host tool image (hammer, wrench, printer, wands),
    /// from the image state machine at the moment the swing lands. A swing
    /// that hits nothing is not an error: the animation already played.
    pub(super) fn tool_fire(&mut self, owner: OwnerId, image: &str) -> Result<()> {
        let Some(actor) = self.weapons.actor(ActorId(owner)) else {
            return Ok(());
        };
        let start = actor.frame.eye;
        let dir = actor.frame.direction.normalize_or_zero();
        let scale = actor.frame.scale;
        if dir == Vec3::ZERO || !self.peers.contains_key(&owner) {
            return Ok(());
        }
        let melee_range = if dir.y < -0.9 { 5.5 } else { 5.0 } * scale;
        match image.strip_prefix("v20.image.").unwrap_or(image) {
            "hammerimage" => {
                let Some(hit) = self.tool_ray(owner, start, dir, melee_range, Reach::Melee)? else {
                    return Ok(());
                };
                self.tool_explosion(
                    owner,
                    "hammerExplosion",
                    hit.position - dir * 0.25,
                    Some(hit.normal),
                    scale,
                );
                self.tool_sound("hammerHitSound", hit.position);
                match hit.target {
                    TargetId::Brick(id) => {
                        // v20's hammer silently leaves any brick whose loss
                        // would strand others (`willCauseChainKill`), before
                        // it checks trust. Tutorial `noBreak` bricks survive.
                        // `indestructable` spawn bricks do not: that flag
                        // only stops explosions.
                        if !self.simulation.will_cause_chain_kill(id)?
                            && self.trusted_brick_edit(owner, id, level::FULL)
                            && !self.tutorial_protects(id)
                        {
                            // `fxDTSBrick::onToolBreak` runs its rows before
                            // `killBrick` removes the brick and its program.
                            self.fire_input(id, "onToolBreak", Some(owner));
                            self.step_events(&BTreeSet::new())?;
                            self.tool_kill_brick(owner, id)?;
                        }
                    }
                    TargetId::Actor(target) => {
                        if self.can_damage_player(owner, target.0, false) {
                            self.damage_player(
                                target.0,
                                10.0,
                                combat::DamageKind::weapon("$DamageType::HammerDirect", true),
                                Some(owner),
                            )?;
                        }
                    }
                    TargetId::Vehicle(vehicle) => {
                        self.hammer_vehicle(owner, vehicle, hit.position, dir)
                    }
                    TargetId::Entity(entity) => self.damage_entity(
                        entity,
                        10.0,
                        Some(owner),
                        "weapon",
                        "$DamageType::HammerDirect",
                    ),
                    TargetId::Map(_) | TargetId::Shape(_) => {}
                }
            }
            "wandimage" => {
                // The wand item from a loadout or spawner obeys the same
                // mini-game and tutorial rules as `/wand`.
                let may_wand = self.tutorial_allows_wand(owner)
                    && self.peers.get(&owner).is_some_and(|peer| {
                        combat::ensure_may_build(
                            &peer.combat,
                            &self.minigames,
                            bri_minigames::BuildAction::Wand,
                        )
                        .is_ok()
                    });
                if !may_wand {
                    return Ok(());
                }
                let Some(hit) = self.tool_ray(owner, start, dir, melee_range, Reach::Melee)? else {
                    return Ok(());
                };
                self.tool_explosion(
                    owner,
                    "wandExplosion",
                    hit.position - dir * 0.25,
                    Some(hit.normal),
                    scale,
                );
                self.tool_sound("wandHitSound", hit.position);
                match hit.target {
                    TargetId::Brick(id) => {
                        // Tutorial `noBreak` bricks survive tools.
                        if self.trusted_brick_edit(owner, id, level::YOU)
                            && !self.tutorial_protects(id)
                        {
                            // `fxDTSBrick::onToolBreak` runs its rows before
                            // `killBrick` removes the brick and its program.
                            self.fire_input(id, "onToolBreak", Some(owner));
                            self.step_events(&BTreeSet::new())?;
                            self.tool_kill_brick(owner, id)?;
                        }
                    }
                    TargetId::Actor(target) => {
                        let administrator = self.peers[&owner].actor.administrator;
                        if self.can_damage_player(owner, target.0, false) || administrator {
                            self.set_player_velocity(target.0, Vec3::new(0.0, 15.0, 0.0));
                        } else {
                            let name = self
                                .peers
                                .get(&target.0)
                                .map_or_else(String::new, |p| p.name.clone());
                            self.center_print(
                                owner,
                                format!("{name} does not trust you enough to do that."),
                            );
                        }
                    }
                    _ => {}
                }
            }
            "adminwandimage" => {
                if !self.peers[&owner].actor.administrator {
                    return Ok(());
                }
                let range = (500.0 * scale).min(Simulation::MAX_TARGET_DISTANCE);
                let Some(hit) = self.tool_ray(owner, start, dir, range, Reach::Melee)? else {
                    return Ok(());
                };
                self.tool_explosion(
                    owner,
                    "AdminWandExplosion",
                    hit.position - dir * 0.25,
                    Some(hit.normal),
                    scale,
                );
                self.tool_sound("wandHitSound", hit.position);
                match hit.target {
                    TargetId::Brick(id) => self.tool_kill_brick(owner, id)?,
                    TargetId::Actor(target) => {
                        let velocity = (dir + Vec3::Y).normalize() * 20.0;
                        self.set_player_velocity(target.0, velocity);
                    }
                    _ => {}
                }
            }
            "wrenchimage" => {
                let Some(hit) = self.tool_ray(owner, start, dir, 10.0 * scale, Reach::Wrench)?
                else {
                    return Ok(());
                };
                self.tool_explosion(
                    owner,
                    "wrenchExplosion",
                    hit.position - dir * 0.25,
                    None,
                    scale,
                );
                let TargetId::Brick(id) = hit.target else {
                    self.tool_sound("wrenchMissSound", hit.position);
                    return Ok(());
                };
                if !self.trusted_brick_edit(owner, id, level::BUILD) {
                    self.tool_sound("wrenchMissSound", hit.position);
                    return Ok(());
                }
                self.open_inspection(owner, id, InspectMode::Wrench);
                self.tool_sound("wrenchHitSound", hit.position);
            }
            "printgunimage" => {
                let Some(ToolHit {
                    target: TargetId::Brick(id),
                    ..
                }) = self.tool_ray(owner, start, dir, 10.0, Reach::Bricks)?
                else {
                    return Ok(());
                };
                let brick = &self.simulation.state().bricks[&id];
                if self.tool_catalog.print_aspect(brick).is_err() {
                    return Ok(());
                }
                if self.trusted_brick_edit(owner, id, level::FULL) {
                    self.open_inspection(owner, id, InspectMode::Printer);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// `paintProjectile::onCollision` for bricks: recolour, or apply a colour
    /// or shape effect for the FX cans. Players and vehicles are not painted.
    pub(super) fn paint_contact(&mut self, contact: &bri_weapons::ProjectileContact) -> Result<()> {
        let (TargetId::Brick(id), Some(edit)) = (
            contact.target,
            paint_edit(&contact.definition, contact.paint),
        ) else {
            return Ok(());
        };
        let owner = contact.source.0;
        let Some(brick) = self.simulation.state().bricks.get(&id) else {
            return Ok(());
        };
        // v20 pushes the old value onto the painter's undo stack.
        let (unchanged, undo) = match &edit {
            Edit::Color(color) => (brick.color == *color, UndoEntry::Color(id, brick.color)),
            Edit::ColorEffect(effect) => (
                brick.color_effect == *effect,
                UndoEntry::ColorEffect(id, brick.color_effect),
            ),
            Edit::ShapeEffect(effect) => (
                brick.shape_effect == *effect,
                UndoEntry::ShapeEffect(id, brick.shape_effect),
            ),
            _ => return Ok(()),
        };
        if unchanged || !self.trusted_brick_edit(owner, id, level::FULL) {
            return Ok(());
        }
        let actor = copy_actor(&self.peers[&owner].actor);
        self.simulation.edit(&actor, id, edit)?;
        self.dirty.insert(id);
        self.push_undo(owner, undo);
        Ok(())
    }

    /// `getTrustLevel` for brick tools against `$TrustLevel` `level`;
    /// administrators may edit any. Refusals show v20's centre print.
    fn trusted_brick_edit(&mut self, owner: OwnerId, id: BrickId, level: u8) -> bool {
        let Some(brick) = self.simulation.state().bricks.get(&id) else {
            return false;
        };
        let brick_owner = brick.owner;
        let Some(peer) = self.peers.get(&owner) else {
            return false;
        };
        let allowed = owner != 0 && peer.actor.trusted(brick_owner, level);
        if !allowed {
            let group = self.brick_group_name(brick_owner);
            self.center_print(
                owner,
                format!("{group} does not trust you enough to do that."),
            );
        }
        allowed
    }
    pub(super) fn brick_group_name(&self, owner: OwnerId) -> String {
        if let Some(peer) = self.peers.get(&owner) {
            peer.name.clone()
        } else if let Some((name, ..)) = self.departed.get(&owner) {
            name.clone()
        } else if owner == 0 {
            "Public".into()
        } else {
            format!("BL_ID: {owner}")
        }
    }
    pub(super) fn center_print(&mut self, owner: OwnerId, text: String) {
        self.notify(owner, Notice::Center { text, seconds: 1.0 });
    }

    /// A tool destroying a brick (`killBrick`): it falls through the world.
    pub(super) fn tool_kill_brick(&mut self, owner: OwnerId, id: BrickId) -> Result<()> {
        let actor = copy_actor(&self.peers.get(&owner).context("Unknown connection")?.actor);
        self.kill_brick(&actor, id)?;
        self.close_inspections(id);
        Ok(())
    }
    /// Wrench and printer dialogs open on a brick that is gone close.
    pub(super) fn close_inspections(&mut self, id: BrickId) {
        for peer in self.peers.values_mut() {
            if peer.inspection.as_ref().is_some_and(|i| i.id == id) {
                peer.inspection = None;
            }
        }
    }

    /// `hammerImage::onHitObject` for vehicles: flip it with an impulse of
    /// five times its mass along the swing, tilted 45 degrees up, unless the
    /// swinger rides it or may not touch it.
    fn hammer_vehicle(&mut self, owner: OwnerId, vehicle: u64, position: Vec3, dir: Vec3) {
        if self.mounted(owner).map(|(v, _)| v) == Some(vehicle) {
            return;
        }
        let Some((vehicle_owner, mass)) = self.vehicle_owner_and_mass(vehicle) else {
            return;
        };
        let flip = match self.vehicle_damage_decision(owner, vehicle) {
            Some(allowed) => allowed,
            // Outside minigames the owner's trust decides: their own vehicle,
            // an administrator, or a vehicle nobody present owns.
            None => {
                vehicle_owner == owner
                    || self.peers[&owner].actor.administrator
                    || !self.peers.contains_key(&vehicle_owner)
            }
        };
        if flip {
            let impulse = (dir + Vec3::Y).normalize() * mass * 5.0;
            self.push_vehicle(vehicle, position, impulse);
        }
    }

    /// `setVelocity` on a player (the wands' launch).
    fn set_player_velocity(&mut self, target: OwnerId, velocity: Vec3) {
        if let Some(peer) = self.peers.get_mut(&target)
            && peer.combat.alive
        {
            let current = Vec3::from(peer.player.state().velocity);
            peer.player.push(velocity - current);
        }
    }

    /// `openWrenchDlg` / `openPrintSelectorDlg`: remember the brick for the
    /// dialog's later commands and tell only this player to open it.
    fn open_inspection(&mut self, owner: OwnerId, id: BrickId, mode: InspectMode) {
        let brick = self.simulation.state().bricks[&id].clone();
        let Some(peer) = self.peers.get_mut(&owner) else {
            return;
        };
        peer.inspection = Some(Inspection {
            id,
            mode,
            original: brick.clone(),
            wrench_original: (mode == InspectMode::Wrench).then(|| brick.clone()),
        });
        self.notify(
            owner,
            Notice::Inspected {
                brick_id: id,
                brick: Box::new(brick),
                mode,
            },
        );
    }

    /// The tool projectile's explosion (`hammerProjectile` and friends are
    /// spawned at the hit and explode on their first tick).
    fn tool_explosion(
        &mut self,
        owner: OwnerId,
        definition: &str,
        position: Vec3,
        direction: Option<Vec3>,
        scale: f32,
    ) {
        self.cues.emit(
            self.simulation.state().tick,
            crate::presentation::CueKind::WeaponEffect {
                source: TargetId::Actor(ActorId(owner)),
                definition: definition.into(),
                node: String::new(),
                seconds: 0.0,
                image: None,
                hand: None,
                direction: direction.map(|d| d.to_array()),
                scale,
            },
            position.to_array(),
        );
    }
    /// `ServerPlay3D`.
    fn tool_sound(&mut self, profile: &str, position: Vec3) {
        self.cues.emit(
            self.simulation.state().tick,
            crate::presentation::CueKind::WeaponSound {
                profile: profile.into(),
            },
            position.to_array(),
        );
    }

    /// Nearest hit for a stock tool ray; never the swinger's own body or the
    /// vehicle they ride.
    fn tool_ray(
        &self,
        owner: OwnerId,
        start: Vec3,
        dir: Vec3,
        range: f32,
        reach: Reach,
    ) -> Result<Option<ToolHit>> {
        use rapier3d::prelude::*;
        let mut best: Option<(f32, ToolHit)> = None;
        let consider = |best: &mut Option<(f32, ToolHit)>, distance: f32, hit: ToolHit| {
            if best.is_none_or(|(d, _)| distance < d) {
                *best = Some((distance, hit));
            }
        };
        if let Some(hit) = self.simulation.target_bricks_always(start, dir, range)?
            && (reach != Reach::Bricks || hit.brick.is_some())
        {
            consider(
                &mut best,
                hit.distance,
                ToolHit {
                    target: hit.brick.map_or(TargetId::Map(0), TargetId::Brick),
                    position: hit.position,
                    normal: hit.normal,
                },
            );
        }
        if reach == Reach::Melee {
            let own = (1u128 << 64) | u128::from(owner);
            let riding = self
                .mounted(owner)
                .map(|(v, _)| vehicles::VEHICLE_TAG | u128::from(v));
            // Players, vehicles and package entities, never the swinger's own
            // body, seat or the entity they drive.
            let driving = match self.peers.get(&owner).map(|p| p.control) {
                Some(ControlObject::Entity(id)) => Some(ENTITY_TAG | u128::from(id)),
                _ => None,
            };
            let predicate = |_: ColliderHandle, c: &Collider| {
                let kind = c.user_data >> 64;
                (1..=3).contains(&kind)
                    && c.user_data != own
                    && Some(c.user_data) != riding
                    && Some(c.user_data) != driving
            };
            let ray = Ray::new(
                Vector::from_array(start.to_array()),
                Vector::from_array(dir.to_array()),
            );
            if let Some((handle, hit)) = self
                .simulation
                .physics
                .query_pipeline_with_filter(
                    QueryFilter::default()
                        .exclude_sensors()
                        .predicate(&predicate),
                )
                .cast_ray_and_get_normal(&ray, range, true)
            {
                let tag = self.simulation.physics.colliders[handle].user_data;
                let target = if tag >> 64 == 1 {
                    TargetId::Actor(ActorId(tag as u64))
                } else if tag >> 64 == 3 {
                    TargetId::Entity(tag as u64)
                } else {
                    TargetId::Vehicle(tag as u64)
                };
                consider(
                    &mut best,
                    hit.time_of_impact,
                    ToolHit {
                        target,
                        position: start + dir * hit.time_of_impact,
                        normal: crate::simulation::hit_normal(
                            Vec3::from_array(hit.normal.to_array()),
                            dir,
                        ),
                    },
                );
            }
        }
        Ok(best.map(|(_, hit)| hit))
    }

    /// Dialog commands act on the brick the wrench or printer last hit
    /// (`%client.wrenchBrick` / `%client.printBrick`), wherever the player
    /// has moved since.
    pub(super) fn tool_action(&mut self, owner: OwnerId, action: ToolAction) -> Result<Reply> {
        let required = match &action {
            ToolAction::UndoBrick => None,
            ToolAction::SetPrint { .. } => Some(bri_weapons::PRINTER),
            ToolAction::Inspect { .. }
            | ToolAction::SetWrench { .. }
            | ToolAction::SetEvents { .. }
            | ToolAction::RespawnVehicle { .. } => Some(bri_weapons::WRENCH),
        };
        if let Some(required) = required {
            inventory::require_equipment(&self.weapons, owner, Some(required))?;
        }
        if action == ToolAction::UndoBrick {
            return self.undo_brick(owner);
        }
        let mut action = action;
        if let ToolAction::SetWrench { brick, properties } = &mut action
            && let Some(target) = self.simulation.state().bricks.get(brick)
        {
            self.quota_wrench(target, properties);
        }
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        let id = peer
            .inspection
            .as_ref()
            .map(|i| i.id)
            .context("Hit a brick with the tool first")?;
        let Some(brick) = self.simulation.state().bricks.get(&id) else {
            peer.inspection = None;
            anyhow::bail!("Brick no longer exists");
        };
        ensure!(
            owner != 0 && peer.actor.trusted(brick.owner, level::BUILD),
            "The brick's owner does not trust you enough to do that."
        );
        if let ToolAction::Inspect { mode } = action {
            // The events dialog opens from the wrench dialog of the same brick.
            ensure!(
                mode == InspectMode::Events,
                "Swing the tool at a brick to inspect it"
            );
            let wrench_original = peer
                .inspection
                .as_ref()
                .and_then(|i| i.wrench_original.clone())
                .context("Open the wrench dialog first")?;
            peer.inspection = Some(Inspection {
                id,
                mode,
                original: brick.clone(),
                wrench_original: Some(wrench_original),
            });
            return Ok(Reply::Inspected {
                brick_id: id,
                brick: Box::new(brick.clone()),
                mode,
            });
        }
        if let ToolAction::RespawnVehicle { brick: expected } = action {
            ensure!(id == expected, "Vehicle spawn brick is no longer inspected");
            peer.inspection = None;
            self.respawn_vehicle_brick(id)?;
            return Ok(Reply::Accepted);
        }
        let (edit, expected, mode) = match action {
            ToolAction::SetPrint {
                brick: expected,
                print,
            } => {
                self.tool_catalog.print_aspect(brick)?;
                (
                    Edit::Print(print.map(ContentRef::Resolved)),
                    expected,
                    InspectMode::Printer,
                )
            }
            ToolAction::SetWrench {
                brick: expected,
                properties,
            } => (Edit::Properties(properties), expected, InspectMode::Wrench),
            ToolAction::SetEvents {
                brick: expected,
                events,
            } => (Edit::Events(events), expected, InspectMode::Events),
            _ => unreachable!(),
        };
        ensure!(id == expected, "Inspected brick changed; hit it again");
        let inspection = peer.inspection.as_ref().unwrap();
        ensure!(
            inspection.mode == mode
                || (mode == InspectMode::Wrench
                    && inspection.mode == InspectMode::Events
                    && inspection.wrench_original.is_some()),
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
        self.tool_catalog.validate_edit(brick, &edit)?;
        self.item_spawners
            .validate_edit(self.simulation.state(), id, &edit)?;
        // v20 serverCmdSetPrint remembers the choice for the brick's aspect.
        let last_print = match &edit {
            Edit::Print(Some(ContentRef::Resolved(print))) => Some((
                self.tool_catalog.print_aspect(brick)?.to_ascii_lowercase(),
                print.clone(),
            )),
            _ => None,
        };
        let edited_events = matches!(edit, Edit::Events(_));
        let sets_item = matches!(&edit, Edit::Properties(p) if p.item_spawn.item.is_some());
        // `serverCmdSetPrint` records a print change for undo.
        let undo = match &edit {
            Edit::Print(print) if *print != brick.print => {
                Some(UndoEntry::Print(id, brick.print.clone()))
            }
            _ => None,
        };
        self.simulation.edit(&peer.actor, id, edit)?;
        self.dirty.insert(id);
        if sets_item {
            self.item_spawners.restock(id, self.simulation.state().tick);
        }
        if let Some((aspect, print)) = last_print {
            self.last_prints
                .entry(owner)
                .or_default()
                .insert(aspect, print);
        }
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
        if let Some(undo) = undo {
            self.push_undo(owner, undo);
        }
        Ok(Reply::Accepted)
    }
}
