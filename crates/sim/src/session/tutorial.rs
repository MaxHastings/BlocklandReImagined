//! The v20 Tutorial's rules (`Add-Ons/Map_Tutorial/tutorial.cs`): trigger
//! zones that prompt, gate and reward each lesson, the doors they open, the
//! brick layouts they swap in, and the target practice. Zones and layouts are
//! native content (`crate::tutorial`); only the Tutorial map installs them,
//! so every hook here is inert elsewhere.
//!
//! Like the original, lesson progress belongs to the player's current life:
//! respawning at the start restarts the tutorial.
use super::*;
use crate::tutorial::{TutorialMap, Zone, ZoneKind};
use bri_weapons::{ActorId, PRINTER, WRENCH};

/// Tutorial triggers tick every 50 ms (`tickPeriodMS`) at 120 ticks/s.
const PERIOD: u64 = 6;
const TICKS_PER_SECOND: u64 = 120;
const fn ms(ms: u64) -> u64 {
    ms * TICKS_PER_SECOND / 1000
}

// Colour escapes `\c0`, `\c2` and `\c3`.
const C0: &str = "\u{E000}";
const C2: &str = "\u{E002}";
const C3: &str = "\u{E003}";
const TROPHY: &str = "<bitmap:base/client/ui/CI/trophy>";
/// The completion dialog counts 18 goals (Secrets included).
const GOAL_COUNT: u32 = 18;

const GUN: &str = "v20.weapon.gunitem";
const HAMMER_IMAGE: &str = "v20.image.hammerimage";
const WRENCH_IMAGE: &str = "v20.image.wrenchimage";
const PRINTER_IMAGE: &str = "v20.image.printgunimage";
const GUN_IMAGE: &str = "v20.image.gunimage";
const WAND_IMAGE: &str = "v20.image.wandimage";
const BRICK_IMAGE: &str = "v20.image.brickimage";
/// `HorseArmor.brickImage` (Vehicle_Horse): the brick sits on mount3.
const HORSE_BRICK_IMAGE: &str = "v20.image.horsebrickimage";
/// The images `hold_brick` mounts for bricks in hand. They only show the
/// brick; a click still places the client's ghost, not an image trigger.
pub const BRICK_HAND_IMAGES: [&str; 2] = [BRICK_IMAGE, HORSE_BRICK_IMAGE];
const HORSE: &str = "v20.vehicle.horsearmor";
const JEEP: &str = "v20.vehicle.jeepvehicle";
const REWARD_SOUND: &str = "v20/sound/rewardsound";
const ALARM_SOUND: &str = "v20/sound/alarmsound";
const HIT_SOUND: &str = "v20/sound/hammerhitsound";
const RAINBOW_EMITTER: &str = "v20/emitter/rainbowpaintemitter";

const WRENCH_BRICK: &str = "_TutorialWrenchBrick";
const HORSE_PAD: &str = "_TutorialVehicleBrick1";
const JEEP_PAD: &str = "_TutorialVehicleBrick2";
/// `_TutorialPrintBrick1`-`7` must spell OINKMOO.
const PRINT_WORD: [&str; 7] = ["o", "i", "n", "k", "m", "o", "o"];
const PRINT_BLANK: &str = "-minus";
const SPRAY_BRICKS: u8 = 13;
/// Bricks `tutorial.cs` marks `noBreak`: the vehicle pads, the wrench cone,
/// the item pads, the print puzzle and doors 2-6.
const UNBREAKABLE: [&str; 18] = [
    "_TutorialVehicleBrick1",
    "_TutorialVehicleBrick2",
    "_TutorialWrenchBrick",
    "_TutorialWrenchSpawnBrick",
    "_TutorialPrinterSpawnBrick",
    "_TutorialGunSpawnBrick",
    "_TutorialPrintBrick1",
    "_TutorialPrintBrick2",
    "_TutorialPrintBrick3",
    "_TutorialPrintBrick4",
    "_TutorialPrintBrick5",
    "_TutorialPrintBrick6",
    "_TutorialPrintBrick7",
    "_TutorialDoor2",
    "_TutorialDoor3",
    "_TutorialDoor4",
    "_TutorialDoor5",
    "_TutorialDoor6",
];

/// Where the water under the horse jump returns you
/// (`-2.64172 -127.042 94.75`, facing `0 0 -1 0.881075`).
const WATER_RETURN: ([f32; 3], f32) = ([-2.64172, 94.75, 127.042], 0.881075);
/// `stayAndBuild` moves the spawn into the last room.
const BUILD_SPAWN: [f32; 3] = [-83.3524, 95.958, 91.2663];
/// Targets launch at x = -44.8628 in one of three lanes and scroll along +x
/// until x > -32 (`launchTarget`, `scrollTarget`).
const TARGET_START_X: f32 = -44.8628;
const TARGET_END_X: f32 = -32.0;
const TARGET_BASE_Y: f32 = 94.4225;
const TARGET_LANES_Z: [f32; 3] = [71.371, 64.8711, 58.8758];
/// Scroll speed per `speed` 1-5: distance per step over the step period.
const TARGET_SPEEDS: [f32; 5] = [
    0.06 / 0.030,
    0.08 / 0.025,
    0.09 / 0.025,
    0.1 / 0.020,
    0.17 / 0.020,
];
/// Hit box half extents around a target's face (the board is about two
/// units across and faces the firing line along z).
const TARGET_HALF: Vec3 = Vec3::new(1.0, 1.0, 0.35);

/// Movement a player datablock allows: the tutorial swaps between
/// `PlayerTutorialNoMove`, `NoJumpNoJet`, `NoJet` and `PlayerStandardArmor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Abilities {
    pub run: bool,
    pub jump: bool,
    pub jet: bool,
}
impl Default for Abilities {
    fn default() -> Self {
        Self::STANDARD
    }
}
impl Abilities {
    pub const STANDARD: Self = Self {
        run: true,
        jump: true,
        jet: true,
    };
    const NO_MOVE: Self = Self {
        run: false,
        jump: false,
        jet: false,
    };
    const NO_JUMP_NO_JET: Self = Self {
        run: true,
        jump: false,
        jet: false,
    };
    const NO_JET: Self = Self {
        run: true,
        jump: true,
        jet: false,
    };
    /// Remove the controls this datablock lacks. Looking and crouching stay.
    pub fn apply(self, input: MoveInput) -> MoveInput {
        MoveInput {
            forward: if self.run { input.forward } else { 0.0 },
            right: if self.run { input.right } else { 0.0 },
            jump: input.jump && self.jump,
            jet: input.jet && self.jet,
            ..input
        }
    }
}

/// Client-owned building state the server cannot see: whether the brick
/// inventory holds bricks (`inventory[]`), bricks are in hand (`brickImage`)
/// and a ghost brick exists (`tempBrick`). The tutorial's prompts read it and
/// `equipped` mounts `brickImage`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrickHand {
    pub stocked: bool,
    pub equipped: bool,
    pub ghost: bool,
}

/// Per-player lesson state (`%obj.goalCompleted[...]` and friends).
#[derive(Default)]
pub(super) struct Progress {
    /// `combat.spawn_tick` of the life this progress belongs to.
    life: Option<u64>,
    /// `%obj.spawnTime`.
    started: u64,
    goals: BTreeSet<String>,
    /// Wrench subgoals and the target practice start.
    steps: BTreeSet<&'static str>,
    inside: BTreeSet<usize>,
    last_tip: u64,
    wrench_delay: u64,
    abilities: Abilities,
    sent_abilities: Option<Abilities>,
    can_wand: bool,
    can_spray: bool,
    hand: BrickHand,
    last_prompt: Option<(String, u64)>,
}

impl Progress {
    /// Movement the player's current lesson allows.
    pub(super) fn abilities(&self) -> Abilities {
        self.abilities
    }
}

struct Target {
    lane: usize,
    speed: f32,
    x: f32,
    hit: bool,
}
/// A running target practice (`beginTargetPractice`).
struct Practice {
    owner: OwnerId,
    started: u64,
    next: usize,
    targets: Vec<Target>,
}

pub(super) struct Tutorial {
    map: TutorialMap,
    /// `$TutorialCompleted`: the lessons stop and building is free.
    completed: bool,
    /// `$TutorialPart1Completed`: the second layout is in.
    part2: bool,
    goals_completed: u32,
    practice: Option<Practice>,
    targets_launched: u32,
    targets_hit: u32,
    shots: u32,
    counted_shots: BTreeSet<u64>,
}

impl Session {
    /// Install the Tutorial map's rules. Host setup only, before players join.
    pub fn set_tutorial(&mut self, map: TutorialMap) -> Result<()> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty(),
            "The tutorial must be installed before players join"
        );
        ensure!(
            self.simulation.state().map_id == crate::tutorial::MAP_ID,
            "The tutorial rules belong to the Tutorial map"
        );
        // Every lesson starts empty-handed; tools come from the item bricks.
        self.set_spawn_loadout(ToolInventory {
            slots: vec![None; TOOL_SLOTS],
            selected: None,
        })?;
        self.tutorial = Some(Box::new(Tutorial {
            map,
            completed: false,
            part2: false,
            goals_completed: 0,
            practice: None,
            targets_launched: 0,
            targets_hit: 0,
            shots: 0,
            counted_shots: BTreeSet::new(),
        }));
        Ok(())
    }

    /// Record a client's brick inventory state. Taking bricks in hand mounts
    /// the grey 2x2 `brickImage` in the right hand (`fxDTSBrickData::onUse`);
    /// putting them away unmounts it unless a tool already replaced it.
    pub(super) fn set_brick_hand(&mut self, owner: OwnerId, hand: BrickHand) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        let was = std::mem::replace(&mut peer.tutorial.hand, hand).equipped;
        let alive = peer.combat.alive;
        if !alive || was == hand.equipped {
            return Ok(());
        }
        if hand.equipped {
            self.hold_brick(owner)
        } else if self.holds_brick(owner) {
            self.weapons.equip(ActorId(owner), None)
        } else {
            Ok(())
        }
    }

    /// Bricks are in hand on the client.
    pub(super) fn brick_equipped(&self, owner: OwnerId) -> bool {
        self.peers
            .get(&owner)
            .is_some_and(|peer| peer.tutorial.hand.equipped)
    }

    /// `%player.mountImage(%player.getDataBlock().brickImage, 0)`. Packs
    /// without the image mount nothing.
    pub(super) fn hold_brick(&mut self, owner: OwnerId) -> Result<()> {
        let image = self.brick_image(owner);
        if !self.weapons.pack.images.contains_key(image)
            || self
                .weapons
                .image_state(ActorId(owner), 0)
                .is_some_and(|(held, _)| held.id == image)
        {
            return Ok(());
        }
        self.weapons.drop_ball(ActorId(owner))?;
        self.weapons.mount_image(ActorId(owner), image, None)?;
        self.weapon_triggers.remove(&owner);
        Ok(())
    }

    /// The datablock's `brickImage`.
    fn brick_image(&self, owner: OwnerId) -> &'static str {
        let horse = self.peers.get(&owner).is_some_and(|peer| {
            peer.player.state().archetype == crate::player_types::PlayerType::Horse.archetype()
        });
        if horse { HORSE_BRICK_IMAGE } else { BRICK_IMAGE }
    }

    /// Either datablock's brick is mounted.
    pub(super) fn holds_brick(&self, owner: OwnerId) -> bool {
        self.weapons
            .image_state(ActorId(owner), 0)
            .is_some_and(|(image, _)| BRICK_HAND_IMAGES.contains(&image.id.as_str()))
    }

    /// `noBreak` bricks and the vehicle pads' `vehicleLimit`.
    pub(super) fn tutorial_check(&self, command: &Command) -> Result<()> {
        if self.tutorial.as_deref().is_none_or(|t| t.completed) {
            return Ok(());
        }
        #[allow(clippy::single_match)]
        match command {
            Command::Tool(ToolAction::SetWrench { brick, properties }) => {
                let name = self.brick_name(*brick);
                let allowed = match name.as_deref() {
                    Some(n) if n.eq_ignore_ascii_case(HORSE_PAD) => Some(HORSE),
                    Some(n) if n.eq_ignore_ascii_case(JEEP_PAD) => Some(JEEP),
                    _ => None,
                };
                if let (Some(allowed), Some(vehicle)) = (allowed, &properties.vehicle) {
                    ensure!(
                        vehicle == allowed,
                        "This vehicle spawn only takes the {}",
                        if allowed == HORSE { "horse" } else { "jeep" }
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// `servercmdWand` / `servercmdMagicWand` in `TutorialParentingPackage`:
    /// until the tutorial is completed the wand only comes out in the wand
    /// room. Elsewhere the command does nothing.
    pub(super) fn tutorial_allows_wand(&self, owner: OwnerId) -> bool {
        self.tutorial.as_deref().is_none_or(|t| t.completed)
            || self.peers.get(&owner).is_some_and(|p| p.tutorial.can_wand)
    }

    /// `servercmdUseSprayCan` / `servercmdUseFXCan`: the cans only work
    /// once this life has reached the spray room (`canUseSpray`).
    pub(super) fn tutorial_allows_spray(&self, owner: OwnerId) -> bool {
        self.tutorial.as_deref().is_none_or(|t| t.completed)
            || self.peers.get(&owner).is_some_and(|p| p.tutorial.can_spray)
    }

    /// Whether the tutorial keeps this brick from being broken.
    pub(super) fn tutorial_protects(&self, brick: BrickId) -> bool {
        let Some(tutorial) = self.tutorial.as_deref() else {
            return false;
        };
        !tutorial.completed
            && self
                .brick_name(brick)
                .is_some_and(|name| UNBREAKABLE.iter().any(|n| n.eq_ignore_ascii_case(&name)))
    }

    pub(super) fn step_tutorial(&mut self) -> Result<()> {
        if self.tutorial.is_none() {
            return Ok(());
        }
        self.step_practice()?;
        if self.simulation.state().tick.is_multiple_of(PERIOD) {
            let owners: Vec<OwnerId> = self
                .peers
                .keys()
                .copied()
                .filter(|o| !self.is_bot(*o))
                .collect();
            for owner in owners {
                self.tutorial_player(owner)?;
            }
        }
        let changed: Vec<(OwnerId, Abilities)> = self
            .peers
            .iter_mut()
            .filter(|(_, p)| p.tutorial.sent_abilities != Some(p.tutorial.abilities))
            .map(|(owner, p)| {
                p.tutorial.sent_abilities = Some(p.tutorial.abilities);
                (*owner, p.tutorial.abilities)
            })
            .collect();
        for (owner, abilities) in changed {
            self.notify(owner, Notice::Abilities(abilities));
        }
        Ok(())
    }

    fn tutorial_player(&mut self, owner: OwnerId) -> Result<()> {
        let tick = self.simulation.state().tick;
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        if peer.tutorial.life != Some(peer.combat.spawn_tick) {
            let hand = peer.tutorial.hand;
            let sent_abilities = peer.tutorial.sent_abilities;
            peer.tutorial = Progress {
                life: Some(peer.combat.spawn_tick),
                started: tick,
                hand,
                sent_abilities,
                ..Default::default()
            };
        }
        if !peer.combat.alive {
            return Ok(());
        }
        let bounds = crate::player::item_bounds(&peer.player);
        let (min, max) = (Vec3::from(bounds.min), Vec3::from(bounds.max));
        let inside: BTreeSet<usize> = self
            .tutorial()
            .map
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.overlaps(min, max))
            .map(|(i, _)| i)
            .collect();
        let before = std::mem::replace(&mut self.progress_mut(owner).inside, inside.clone());
        for &zone in before.difference(&inside) {
            self.zone_leave(owner, zone)?;
        }
        for &zone in inside.difference(&before) {
            self.zone_enter(owner, zone)?;
        }
        for zone in inside {
            // A zone can move the player (the water) or end the life.
            if !self.progress(owner).inside.contains(&zone) {
                continue;
            }
            self.zone_tick(owner, zone)?;
        }
        Ok(())
    }

    fn zone_enter(&mut self, owner: OwnerId, index: usize) -> Result<()> {
        let zone = self.tutorial().map.zones[index].clone();
        let completed = self.tutorial().completed;
        match zone.kind {
            ZoneKind::Win => {
                if (completed && zone.goal != "Secrets") || self.done(owner, &zone.goal) {
                    return Ok(());
                }
                if zone.goal == "Diving" {
                    self.progress_mut(owner).abilities = Abilities::NO_JET;
                }
                if zone.goal == "Light" {
                    // The lit room is behind you; `serverCmdLight` turns it off.
                    self.peers.get_mut(&owner).unwrap().combat.light = false;
                }
                self.complete(owner, &zone.goal);
            }
            ZoneKind::Look if !completed => {
                if !self.done(owner, "Look") {
                    self.restart_tutorial(owner)?;
                } else if !self.done(owner, "Move") {
                    self.progress_mut(owner).last_tip = self.simulation.state().tick;
                }
            }
            ZoneKind::Water if !completed && !self.done(owner, "Ride") => {
                if self.mounted(owner).is_none() {
                    let (feet, yaw) = WATER_RETURN;
                    let peer = self.peers.get_mut(&owner).unwrap();
                    peer.player
                        .teleport(&mut self.simulation.physics, Vec3::from(feet), yaw)?;
                    peer.inputs.clear();
                    peer.tutorial.inside.clear();
                }
            }
            ZoneKind::Wand => self.progress_mut(owner).can_wand = true,
            ZoneKind::Spray => self.progress_mut(owner).can_spray = true,
            _ => {}
        }
        Ok(())
    }

    fn zone_leave(&mut self, owner: OwnerId, index: usize) -> Result<()> {
        let zone = self.tutorial().map.zones[index].clone();
        let completed = self.tutorial().completed;
        let held = self.held(owner);
        let spray_can = held
            .as_deref()
            .is_some_and(|i| i.ends_with("spraycanimage"));
        match zone.kind {
            ZoneKind::Tip if !completed && !self.done(owner, &zone.goal) => {
                if zone.goal == "Diving" {
                    self.progress_mut(owner).abilities = Abilities::NO_JET;
                }
            }
            ZoneKind::Build if !completed && spray_can => self.equip_tool(owner, None)?,
            ZoneKind::Spray if spray_can => self.equip_tool(owner, None)?,
            ZoneKind::Wand if !completed => {
                self.progress_mut(owner).can_wand = false;
                if held.as_deref() == Some(WAND_IMAGE) {
                    self.equip_tool(owner, None)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn zone_tick(&mut self, owner: OwnerId, index: usize) -> Result<()> {
        let zone = self.tutorial().map.zones[index].clone();
        let completed = self.tutorial().completed;
        match zone.kind {
            ZoneKind::Secret => self.secret_tip(owner),
            _ if completed => Ok(()),
            ZoneKind::Tip => self.tip(owner, &zone),
            ZoneKind::Look => self.look_tip(owner),
            ZoneKind::Brick => self.brick_tip(owner, &zone),
            ZoneKind::Build => self.build_tip(owner),
            ZoneKind::Break => self.break_tip(owner),
            ZoneKind::Ride => self.ride_tip(owner, &zone),
            ZoneKind::Wrench => self.wrench_tip(owner),
            ZoneKind::Print => self.print_tip(owner),
            ZoneKind::Targets => self.targets_tip(owner),
            ZoneKind::Wand => self.wand_tip(owner),
            ZoneKind::Spray => self.spray_tip(owner),
            ZoneKind::Drive => self.drive_tip(owner),
            ZoneKind::FullWin => self.finish(owner),
            ZoneKind::Win | ZoneKind::Water | ZoneKind::JeepPark => Ok(()),
        }
    }

    /// Entering the start zone with the Look lesson open resets everything:
    /// no movement, empty inventory, no targets and the first brick layout.
    fn restart_tutorial(&mut self, owner: OwnerId) -> Result<()> {
        self.progress_mut(owner).abilities = Abilities::NO_MOVE;
        self.weapons
            .set_inventory(ActorId(owner), &vec![None; TOOL_SLOTS])?;
        self.weapon_triggers.remove(&owner);
        let tutorial = self.tutorial_mut();
        tutorial.practice = None;
        tutorial.part2 = false;
        tutorial.goals_completed = 0;
        tutorial.targets_hit = 0;
        tutorial.targets_launched = 0;
        tutorial.shots = 0;
        self.load_layout(owner, false)?;
        self.progress_mut(owner).last_tip = self.simulation.state().tick;
        self.prompt(owner, format!("Move {C3}the mouse{C0} to look around"), 2.0);
        Ok(())
    }

    /// Replace every brick with a tutorial layout (`deleteAll` then
    /// `serverDirectSaveFileLoad`), owned by the player like the original.
    fn load_layout(&mut self, owner: OwnerId, part2: bool) -> Result<()> {
        let actor = Actor {
            owner,
            administrator: true,
            ..Default::default()
        };
        let existing: Vec<BrickId> = self.simulation.state().bricks.keys().copied().collect();
        for id in existing {
            self.simulation.remove(&actor, id)?;
            self.dirty.insert(id);
        }
        self.undo.clear();
        let tutorial = self.tutorial.as_deref().unwrap();
        let world = if part2 {
            &tutorial.map.part2
        } else {
            &tutorial.map.part1
        };
        let build = bri_world::build::SavedBuild::new(world.clone());
        let plan = bri_world::build::LoadPlan::prepare(
            self.simulation.state(),
            build,
            owner,
            false,
            self.next_owner,
        )?;
        self.item_spawners
            .validate_append(self.simulation.state(), plan.bricks())?;
        let ids = self.simulation.load_build(&actor, plan)?;
        self.dirty.extend(ids);
        self.tutorial_mut().part2 = part2;
        Ok(())
    }

    fn tip(&mut self, owner: OwnerId, zone: &Zone) -> Result<()> {
        match zone.goal.as_str() {
            "Jet" => self.progress_mut(owner).abilities = Abilities::STANDARD,
            "Light" => self.progress_mut(owner).abilities = Abilities::NO_JET,
            _ => {}
        }
        if self.done(owner, &zone.goal) {
            return Ok(());
        }
        match zone.goal.as_str() {
            "Jump" => self.progress_mut(owner).abilities = Abilities::NO_JET,
            "Light" => {
                if !self.tutorial().part2 {
                    self.load_layout(owner, true)?;
                }
                if self.peers[&owner].combat.light {
                    self.progress_mut(owner).last_tip = 0;
                    self.clear_center(owner);
                    return Ok(());
                }
            }
            "Dismount" => {
                if self.mounted(owner).is_none() {
                    self.complete(owner, "Dismount");
                    return Ok(());
                }
            }
            "Diving" => self.progress_mut(owner).abilities = Abilities::STANDARD,
            _ => {}
        }
        if self.tip_due(owner, 2500) {
            let text = format!("Press {C3}<key:{}>{C0} to {}", zone.bind, zone.task);
            self.prompt(owner, text, 2.0);
        }
        Ok(())
    }

    fn look_tip(&mut self, owner: OwnerId) -> Result<()> {
        if !self.done(owner, "Look") {
            let target = self.tutorial().map.look_target;
            let state = self.peers[&owner].player.state();
            let toward = (target - Vec3::from(state.feet)).normalize_or_zero();
            if toward.dot(state.forward()) > 0.65 {
                self.complete(owner, "Look");
                self.progress_mut(owner).abilities = Abilities::NO_JUMP_NO_JET;
                return Ok(());
            }
            if self.tip_due(owner, 2500) {
                self.prompt(owner, format!("Move {C3}the mouse{C0} to look around"), 2.0);
            }
        } else if !self.done(owner, "Move") && self.tip_due(owner, 2500) {
            let text = format!(
                "Press {C3}<key:moveforward> <key:moveleft> <key:movebackward> <key:moveright>{C0} to move"
            );
            self.prompt(owner, text, 2.0);
        }
        Ok(())
    }

    fn brick_tip(&mut self, owner: OwnerId, zone: &Zone) -> Result<()> {
        if self.done(owner, "Brick") {
            return Ok(());
        }
        let hand = self.progress(owner).hand;
        if hand.equipped || hand.stocked {
            self.open_door(1)?;
            self.complete(owner, "Bricks");
            self.progress_mut(owner).goals.insert("Brick".into());
            return Ok(());
        }
        if self.tip_due(owner, 2500) {
            let text = format!("Press {C3}<key:{}>{C0} to {}", zone.bind, zone.task);
            self.prompt(owner, text, 2.0);
        }
        Ok(())
    }

    fn build_tip(&mut self, owner: OwnerId) -> Result<()> {
        if self.done(owner, "Build") {
            return Ok(());
        }
        let hand = self.progress(owner).hand;
        if hand.ghost {
            let text = format!(
                "Press {C3}<key:shiftBrickAway>{C0},{C3} <key:shiftBrickTowards>{C0},{C3} <key:shiftBrickLeft> {C0}or{C3} <key:shiftBrickRight>{C0} to move brick laterally\n\
                 {C0}Press {C3}<key:shiftBrickUp>{C0},{C3} <key:shiftBrickDown>{C0},{C3} <key:shiftBrickThirdUp> {C0}or{C3} <key:shiftBrickThirdDown> {C0}to move brick vertically\n\n\
                 Press {C3}<key:plantBrick>{C0} to plant brick"
            );
            self.prompt(owner, text, 3.0);
        } else if hand.equipped {
            let text = format!("Press {C3}<key:mouseFire>{C0} to create a ghost brick");
            self.prompt(owner, text, 2.0);
        } else {
            self.prompt(
                owner,
                format!("Press {C3}<key:useBricks>{C0} to equip bricks"),
                2.0,
            );
        }
        Ok(())
    }

    fn break_tip(&mut self, owner: OwnerId) -> Result<()> {
        // Coming back from the jet room takes the jets away again.
        self.progress_mut(owner).abilities = Abilities::NO_JET;
        if self.done(owner, "Break") {
            return Ok(());
        }
        let text = if self.held(owner).as_deref() == Some(HAMMER_IMAGE) {
            format!(
                "Aim at the bricks and press {C3}<key:mouseFire>{C0} to break them\n\n\
                 The hammer can only break bricks on the top of the stack"
            )
        } else if self.slot_item(owner, 0).is_some() {
            format!("Press {C3}<key:useTools>{C0} to equip the hammer")
        } else {
            "Run over the hammer to pick it up".into()
        };
        self.prompt(owner, text, 2.0);
        Ok(())
    }

    fn ride_tip(&mut self, owner: OwnerId, zone: &Zone) -> Result<()> {
        if self.done(owner, "Ride") {
            return Ok(());
        }
        let horse_here = self.vehicle_infos().iter().any(|info| {
            info.definition == HORSE
                && !info.destroyed
                && self
                    .vehicle_poses()
                    .iter()
                    .any(|p| p.id == info.id && zone.contains(Vec3::from(p.position)))
        });
        let held = self.held(owner);
        let has_wrench = self.has_item(owner, WRENCH);
        let text = if self.mounted(owner).is_some() {
            "Control this like you would a Normal Player.\n\nNow jump across the gap to the next area of the Tutorial".into()
        } else if horse_here {
            format!("Jump on top of the horse by pressing {C3}<key:Jump>")
        } else if held.as_deref() == Some(WRENCH_IMAGE) {
            format!(
                "Aim at the vehicle spawn and press {C3}<key:mouseFire>{C0} to add a vehicle\n\nSelect the horse"
            )
        } else if has_wrench && held.is_none() {
            format!("Press {C3}<key:useTools>{C0} to equip the wrench")
        } else if has_wrench {
            format!("Use the {C3}mouse wheel{C0} to scroll to the wrench")
        } else {
            "Run over the wrench to pick it up".into()
        };
        self.prompt(owner, text, 0.7);
        Ok(())
    }

    fn secret_tip(&mut self, owner: OwnerId) -> Result<()> {
        if self.done(owner, "Secrets") || !self.tip_due(owner, 300) {
            return Ok(());
        }
        let text = format!(
            "Many of the maps included in Blockland contain \n{C3}secret passages{C0} and {C3}hidden spaces{C0} to build.\n\n\
             Try finding some of them with your {C3}friends{C0} later on"
        );
        self.prompt(owner, text, 2.0);
        Ok(())
    }

    fn wrench_tip(&mut self, owner: OwnerId) -> Result<()> {
        let tick = self.simulation.state().tick;
        if self.done(owner, "Wrench") || tick < self.progress(owner).wrench_delay {
            return Ok(());
        }
        let held = self.held(owner);
        let has_wrench = self.has_item(owner, WRENCH);
        let cone = self.named_brick(WRENCH_BRICK);
        let brick = cone.and_then(|id| self.simulation.state().bricks.get(&id));
        let lit = brick.is_some_and(|b| b.light.is_some());
        let emitting = brick.is_some_and(|b| b.emitter.as_ref().is_some_and(|e| e.asset.is_some()));
        let holding_item = brick.is_some_and(|b| b.item_spawn.item.is_some());
        let steps = &self.progress(owner).steps;
        let (a, b, c) = (
            steps.contains("WrenchA"),
            steps.contains("WrenchB"),
            steps.contains("WrenchC"),
        );
        if held.is_none() && has_wrench {
            self.prompt(
                owner,
                format!("Press {C3}<key:useTools>{C0} to equip the Wrench"),
                0.5,
            );
        } else if held.as_deref() != Some(WRENCH_IMAGE) && has_wrench {
            self.prompt(
                owner,
                format!("Use the {C3}mouse wheel{C0} to scroll to the Wrench"),
                0.5,
            );
        } else if !has_wrench {
            self.prompt(
                owner,
                "You need to backtrack to find a Wrench to Continue".into(),
                0.5,
            );
        } else if !a {
            if lit {
                self.wrench_step(owner, "WrenchA", format!("{C2}Well Done!"), 4.0);
            } else {
                let text =
                    format!("Click on the cone brick.\nApply a {C3}Light{C0} to it and press Send");
                self.prompt(owner, text, 3.0);
            }
        } else if !b {
            if lit {
                self.mutate_named(cone, |brick| brick.light = None)?;
            }
            if emitting {
                self.wrench_step(owner, "WrenchB", format!("{C2}Nice One!"), 4.0);
            } else {
                let text = format!(
                    "Ok, now hit the cone brick again but this time\nApply an {C3}Emitter{C0}"
                );
                self.prompt(owner, text, 3.0);
            }
        } else if !c {
            if emitting {
                self.mutate_named(cone, |brick| brick.emitter = None)?;
            }
            if holding_item {
                self.progress_mut(owner).steps.insert("WrenchC");
                self.prompt(owner, format!("{C2}Good work!"), 3.0);
            } else {
                let text = format!(
                    "Finally, Hit the cone brick again but this time\nApply an {C3}Item{C0}"
                );
                self.prompt(owner, text, 3.0);
            }
        } else {
            self.mutate_named(cone, |brick| {
                brick.emitter = Some(bri_world::Emitter {
                    asset: Some(ContentRef::Resolved(RAINBOW_EMITTER.into())),
                    direction: 0,
                })
            })?;
            self.open_door(2)?;
            self.complete_keeping_center(owner, "Wrench");
        }
        Ok(())
    }

    fn wrench_step(&mut self, owner: OwnerId, step: &'static str, text: String, seconds: f32) {
        let tick = self.simulation.state().tick;
        let progress = self.progress_mut(owner);
        progress.wrench_delay = tick + ms(2000);
        progress.steps.insert(step);
        self.prompt(owner, text, seconds);
    }

    fn print_tip(&mut self, owner: OwnerId) -> Result<()> {
        if self.done(owner, "Print") || !self.tip_due(owner, 300) {
            return Ok(());
        }
        // Each print brick turns green when right, red when wrong and stays
        // the default colour while blank.
        let mut solved = true;
        for (i, letter) in PRINT_WORD.iter().enumerate() {
            let Some(id) = self.named_brick(&format!("_TutorialPrintBrick{}", i + 1)) else {
                continue;
            };
            let print = self.simulation.state().bricks[&id]
                .print
                .as_ref()
                .map(print_name);
            let color = if print.as_deref() == Some(*letter) {
                2
            } else if print.as_deref() == Some(PRINT_BLANK) {
                solved = false;
                4
            } else {
                solved = false;
                0
            };
            if self.simulation.state().bricks[&id].color != color {
                self.simulation.mutate(id, |b| b.color = color)?;
                self.dirty.insert(id);
            }
        }
        let held = self.held(owner);
        let has_printer = self.has_item(owner, PRINTER);
        if held.is_none() && has_printer {
            self.prompt(
                owner,
                format!("Press {C3}<key:useTools>{C0} to equip the printer"),
                0.5,
            );
        } else if held.as_deref() != Some(PRINTER_IMAGE) && has_printer {
            self.prompt(
                owner,
                format!("Use the {C3}mouse wheel{C0} to scroll to the printer"),
                0.5,
            );
        } else if !has_printer {
            self.prompt(owner, "You need to find a Printer to Continue".into(), 0.5);
        } else if !solved {
            let text = "Shoot a Printable Brick to set its Print\n\
                        You can press the letter or number on your keyboard instead of selecting it\n\n\
                        Complete the Puzzle!";
            self.prompt(owner, text.into(), 0.5);
        } else {
            self.open_door(3)?;
            self.complete(owner, "Print");
        }
        Ok(())
    }

    fn targets_tip(&mut self, owner: OwnerId) -> Result<()> {
        if self.progress(owner).steps.contains("Targets") || !self.tip_due(owner, 300) {
            return Ok(());
        }
        let held = self.held(owner);
        let has_gun = self.has_item(owner, GUN);
        if held.is_none() && has_gun {
            self.prompt(
                owner,
                format!("Press {C3}<key:useTools>{C0} to equip the Gun"),
                0.5,
            );
        } else if held.as_deref() != Some(GUN_IMAGE) && has_gun {
            self.prompt(
                owner,
                format!("Use the {C3}mouse wheel{C0} to scroll to the Gun"),
                0.5,
            );
        } else if !has_gun {
            self.prompt(owner, "Run over the gun to pick it up".into(), 0.5);
        } else {
            let text = "Prepare for Target Practice!\n\nAim at the targets and use <key:mouseFire> to fire";
            self.prompt(owner, text.into(), 4.0);
            self.play(owner, ALARM_SOUND);
            self.progress_mut(owner).steps.insert("Targets");
            let tick = self.simulation.state().tick;
            let tutorial = self.tutorial_mut();
            tutorial.practice = Some(Practice {
                owner,
                started: tick + ms(4000),
                next: 0,
                targets: Vec::new(),
            });
            tutorial.targets_launched = 0;
            tutorial.targets_hit = 0;
            tutorial.shots = 0;
        }
        Ok(())
    }

    /// Launch, scroll and hit-test targets every tick; when the schedule has
    /// run out and the last target has left, the shooting goal completes.
    fn step_practice(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let tutorial = self.tutorial.as_deref_mut().unwrap();
        let Some(practice) = tutorial.practice.as_mut() else {
            return Ok(());
        };
        if tick < practice.started {
            return Ok(());
        }
        let owner = practice.owner;
        let elapsed = (tick - practice.started) * 1000 / TICKS_PER_SECOND;
        while let Some(launch) = tutorial.map.targets.get(practice.next)
            && u64::from(launch.at_ms) <= elapsed
        {
            practice.targets.push(Target {
                lane: usize::from(launch.row - 1),
                speed: TARGET_SPEEDS[usize::from(launch.speed - 1)],
                x: TARGET_START_X,
                hit: false,
            });
            practice.next += 1;
            tutorial.targets_launched += 1;
        }
        let step = 1.0 / TICKS_PER_SECOND as f32;
        for target in &mut practice.targets {
            target.x += target.speed * step;
        }
        practice.targets.retain(|t| t.x <= TARGET_END_X);
        let mut hits = Vec::new();
        for projectile in self.weapons.projectiles() {
            if projectile.source != ActorId(owner) {
                continue;
            }
            if tutorial.counted_shots.insert(projectile.id) {
                tutorial.shots += 1;
            }
            let end = projectile.position;
            let start = end - projectile.velocity * step;
            for target in practice.targets.iter_mut().filter(|t| !t.hit) {
                let center = Vec3::new(
                    target.x,
                    TARGET_BASE_Y + TARGET_HALF.y,
                    TARGET_LANES_Z[target.lane],
                );
                if segment_hits_box(start, end, center - TARGET_HALF, center + TARGET_HALF) {
                    target.hit = true;
                    tutorial.targets_hit += 1;
                    hits.push(center);
                }
            }
        }
        let live: BTreeSet<u64> = self.weapons.projectiles().map(|p| p.id).collect();
        tutorial.counted_shots.retain(|id| live.contains(id));
        let finished = practice.next == tutorial.map.targets.len()
            && elapsed >= u64::from(tutorial.map.targets_end_ms)
            && practice.targets.is_empty();
        for position in hits {
            self.cues.emit(
                tick,
                crate::presentation::CueKind::WeaponSound {
                    profile: HIT_SOUND.into(),
                },
                position.to_array(),
            );
        }
        if finished {
            let tutorial = self.tutorial_mut();
            tutorial.practice = None;
            let (hit, launched) = (tutorial.targets_hit, tutorial.targets_launched);
            let accuracy = accuracy(hit, tutorial.shots);
            if self.peers.contains_key(&owner) {
                let time = self.elapsed(owner);
                self.reward(owner, "Shooting");
                self.notify(
                    owner,
                    Notice::Bottom {
                        text: format!(
                            "{TROPHY}{C3} Goal Completed! - Shooting - Time: {time}\n({hit}/{launched} targets hit with {accuracy}% accuracy)"
                        ),
                        seconds: 8.0,
                    },
                );
                self.clear_center(owner);
            }
            self.open_door(4)?;
        }
        Ok(())
    }

    fn wand_tip(&mut self, owner: OwnerId) -> Result<()> {
        if self.done(owner, "Wand") || !self.tip_due(owner, 300) {
            return Ok(());
        }
        let text = if self.held(owner).as_deref() != Some(WAND_IMAGE) {
            format!(
                "Open the chat box by pressing {C3}<key:globalchat>{C0} and then type {C3}/wand{C0} to equip the Wand."
            )
        } else {
            // v20 adds that supported bricks fall; native bricks never collapse.
            "Now use the wand to break the bricks. Unlike the hammer, the wand\ncan break bricks anywhere in a stack.".into()
        };
        self.prompt(owner, text, 2.0);
        Ok(())
    }

    fn spray_tip(&mut self, owner: OwnerId) -> Result<()> {
        if self.done(owner, "Spray") {
            return Ok(());
        }
        let bricks: Vec<BrickId> = (1..=SPRAY_BRICKS)
            .filter_map(|i| self.named_brick(&format!("_TutorialSprayBrick{i}")))
            .collect();
        if bricks.is_empty() && self.tutorial().part2 {
            // The save counts broken spray bricks down with `onToolBreak`
            // events and removes the door at zero.
            self.open_door(6)?;
            return Ok(());
        }
        let world = self.simulation.state();
        let changed =
            |test: &dyn Fn(&Brick) -> bool| bricks.iter().any(|id| test(&world.bricks[id]));
        let colors = changed(&|b| b.color != 4);
        let color_fx = changed(&|b| b.color_effect != 0);
        let shape_fx = changed(&|b| b.shape_effect != 0);
        let spray_can = self
            .held(owner)
            .is_some_and(|image| image.ends_with("spraycanimage"));
        let keys = format!(
            "Press {C3}<key:useSprayCan>{C0} to switch paint columns\nUse the {C3}mouse wheel{C0} to move up and down"
        );
        let text = if !spray_can {
            format!(
                "Press {C3}<key:useSprayCan>{C0} to equip your Spray Can\nPress {C3}<key:useSprayCan>{C0} again to switch paint columns\n\n\
                 Use the {C3}mouse wheel{C0} to move up and down"
            )
        } else if !colors {
            format!("Try spraying some of the bricks {C3}different colors{C0}\n\n{keys}")
        } else if !color_fx {
            format!(
                "Now try giving some of the bricks an {C3}FX paint{C0} like {C3}swirl or glow{C0}\n\n{keys}"
            )
        } else if !shape_fx {
            format!("Finally, Try making some of the bricks {C3}Undulo{C0}\n\n{keys}")
        } else {
            self.open_door(6)?;
            self.complete(owner, "Spray");
            return Ok(());
        };
        self.prompt(owner, text, 4.0);
        Ok(())
    }

    fn drive_tip(&mut self, owner: OwnerId) -> Result<()> {
        if self.done(owner, "Drive") || !self.tip_due(owner, 1000) {
            return Ok(());
        }
        let Some(pad) = self.named_brick(JEEP_PAD) else {
            return Ok(());
        };
        let Some(jeep) = self
            .vehicle_infos()
            .into_iter()
            .find(|v| v.definition == JEEP && !v.destroyed)
        else {
            // The pad always offers a fresh jeep.
            let spawn = bri_world::VehicleSpawn {
                vehicle: ContentRef::Resolved(JEEP.into()),
                recolor: true,
            };
            if self.simulation.state().bricks[&pad].vehicle.as_ref() == Some(&spawn) {
                self.respawn_vehicle_brick(pad)?;
            } else {
                self.simulation.mutate(pad, |b| b.vehicle = Some(spawn))?;
                self.dirty.insert(pad);
            }
            return Ok(());
        };
        let Some(position) = self
            .vehicle_poses()
            .into_iter()
            .find(|p| p.id == jeep.id)
            .map(|p| Vec3::from(p.position))
        else {
            return Ok(());
        };
        let park = self
            .tutorial()
            .map
            .zone(ZoneKind::JeepPark)
            .context("Tutorial map has no jeep park")?;
        // The whole jeep must be inside the park box; its body reaches about
        // two units from its centre.
        let margin = Vec3::new(2.0, 0.0, 2.0);
        let parked = Zone {
            min: park.min + margin,
            max: park.max - margin,
            ..park.clone()
        }
        .contains(position);
        let driving = jeep.occupants.first().copied().flatten() == Some(owner);
        let pad_position = Vec3::from(self.simulation.state().bricks[&pad].position);
        let (text, seconds) = if !driving && !parked {
            (
                format!("Press {C3}<key:Jump>{C0} to jump into the Jeep"),
                3.0,
            )
        } else if driving && !parked {
            if pad_position.distance(position) < 5.0 {
                (
                    format!(
                        "Drive the Jeep by using {C3}<key:moveforward> <key:moveleft> <key:movebackward> <key:moveright>\n{C0}Complete the course and park the jeep"
                    ),
                    5.0,
                )
            } else {
                ("Complete the course and park the jeep".into(), 5.0)
            }
        } else if driving {
            (
                format!("You can now get out of the Jeep by pressing {C3}<key:Jet>"),
                3.0,
            )
        } else {
            self.open_door(5)?;
            self.complete(owner, "Drive");
            return Ok(());
        };
        self.prompt(owner, text, seconds);
        Ok(())
    }

    /// `TutorialFullWinTrigger`: show the results and let the player stay
    /// and build (`stayAndBuild`) with the spawn moved into the last room.
    fn finish(&mut self, owner: OwnerId) -> Result<()> {
        let time = self.elapsed(owner);
        self.play(owner, REWARD_SOUND);
        let tutorial = self.tutorial_mut();
        tutorial.completed = true;
        tutorial.practice = None;
        let goals = tutorial.goals_completed;
        let (hit, launched) = (tutorial.targets_hit, tutorial.targets_launched);
        let accuracy = accuracy(hit, tutorial.shots);
        let progress = self.progress_mut(owner);
        progress.abilities = Abilities::STANDARD;
        progress.goals.insert("Wand".into());
        self.spawn_points = vec![Vec3::from(BUILD_SPAWN)];
        self.notify(
            owner,
            Notice::Bottom {
                text: String::new(),
                seconds: 0.0,
            },
        );
        let text = format!(
            "{C3}Congratulations!\n{C0}You completed the Tutorial!\n\n\
             Total Time: {time}\nGoals Completed: {goals}/{GOAL_COUNT}\n\
             Shooting Targets: {hit}/{launched}\nShooting Accuracy: {accuracy}%\n\n\
             Stay and build, or leave from the menu"
        );
        self.prompt(owner, text, 12.0);
        Ok(())
    }

    // ------------------------------------------------------------- helpers

    fn tutorial(&self) -> &Tutorial {
        self.tutorial.as_deref().expect("tutorial installed")
    }
    fn tutorial_mut(&mut self) -> &mut Tutorial {
        self.tutorial.as_deref_mut().expect("tutorial installed")
    }
    fn progress(&self, owner: OwnerId) -> &Progress {
        &self.peers[&owner].tutorial
    }
    fn progress_mut(&mut self, owner: OwnerId) -> &mut Progress {
        &mut self.peers.get_mut(&owner).unwrap().tutorial
    }
    fn done(&self, owner: OwnerId, goal: &str) -> bool {
        self.progress(owner).goals.contains(goal)
    }
    /// Whether this lesson's repeating tip is due again (`lastTipTime`).
    fn tip_due(&mut self, owner: OwnerId, period_ms: u64) -> bool {
        let tick = self.simulation.state().tick;
        let progress = self.progress_mut(owner);
        if tick.saturating_sub(progress.last_tip) <= ms(period_ms) && progress.last_tip != 0 {
            return false;
        }
        progress.last_tip = tick;
        true
    }
    /// Center print. Lessons repeat their prompt every trigger tick; an
    /// unchanged prompt is only re-sent once half its display time is up.
    fn prompt(&mut self, owner: OwnerId, text: String, seconds: f32) {
        let tick = self.simulation.state().tick;
        let progress = self.progress_mut(owner);
        if let Some((last, at)) = &progress.last_prompt
            && *last == text
            && tick.saturating_sub(*at) < (seconds * TICKS_PER_SECOND as f32 / 2.0) as u64
        {
            return;
        }
        progress.last_prompt = Some((text.clone(), tick));
        self.notify(owner, Notice::Center { text, seconds });
    }
    fn clear_center(&mut self, owner: OwnerId) {
        self.progress_mut(owner).last_prompt = None;
        self.notify(
            owner,
            Notice::Center {
                text: String::new(),
                seconds: 0.0,
            },
        );
    }
    /// A goal's reward: sound, trophy bottom print, and the prompt cleared.
    fn complete(&mut self, owner: OwnerId, goal: &str) {
        self.complete_keeping_center(owner, goal);
        self.clear_center(owner);
    }
    fn complete_keeping_center(&mut self, owner: OwnerId, goal: &str) {
        let time = self.elapsed(owner);
        self.reward(owner, goal);
        self.notify(
            owner,
            Notice::Bottom {
                text: format!("{TROPHY}{C3} Goal Completed! - {goal} - Time: {time}"),
                seconds: 3.0,
            },
        );
    }
    fn reward(&mut self, owner: OwnerId, goal: &str) {
        self.progress_mut(owner).goals.insert(goal.into());
        self.tutorial_mut().goals_completed += 1;
        self.play(owner, REWARD_SOUND);
    }
    fn play(&mut self, owner: OwnerId, sound: &str) {
        let tick = self.simulation.state().tick;
        let feet = self.peers[&owner].player.state().feet;
        self.cues.emit(
            tick,
            crate::presentation::CueKind::WeaponSound {
                profile: sound.into(),
            },
            feet,
        );
    }
    /// `getTimeString` of the time since this life began.
    fn elapsed(&self, owner: OwnerId) -> String {
        let seconds = self
            .simulation
            .state()
            .tick
            .saturating_sub(self.progress(owner).started)
            / TICKS_PER_SECOND;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
    fn held(&self, owner: OwnerId) -> Option<String> {
        self.weapons
            .image_state(ActorId(owner), 0)
            .map(|(image, _)| image.id.clone())
    }
    fn slot_item(&self, owner: OwnerId, slot: usize) -> Option<&str> {
        self.weapons
            .actor(ActorId(owner))?
            .inventory
            .get(slot)?
            .as_deref()
    }
    fn has_item(&self, owner: OwnerId, item: &str) -> bool {
        self.weapons
            .actor(ActorId(owner))
            .is_some_and(|a| a.inventory.iter().flatten().any(|i| i == item))
    }
    fn brick_name(&self, brick: BrickId) -> Option<String> {
        self.simulation.state().bricks.get(&brick)?.name.clone()
    }
    fn named_brick(&self, name: &str) -> Option<BrickId> {
        self.simulation
            .state()
            .bricks
            .iter()
            .find(|(_, b)| {
                b.name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .map(|(id, _)| *id)
    }
    fn mutate_named(
        &mut self,
        brick: Option<BrickId>,
        change: impl FnOnce(&mut Brick),
    ) -> Result<()> {
        if let Some(id) = brick {
            self.simulation.mutate(id, change)?;
            self.dirty.insert(id);
        }
        Ok(())
    }
    /// `openTutorialDoor`: the door brick disappears.
    fn open_door(&mut self, door: u8) -> Result<()> {
        if let Some(id) = self.named_brick(&format!("_TutorialDoor{door}")) {
            let owner = self.simulation.state().bricks[&id].owner;
            self.simulation.remove(
                &Actor {
                    owner,
                    administrator: true,
                    ..Default::default()
                },
                id,
            )?;
            self.dirty.insert(id);
        }
        Ok(())
    }
}

/// `Letters/O` or `print/print_letters_default/o` -> `o`.
fn print_name(print: &ContentRef) -> String {
    let name = match print {
        ContentRef::Resolved(id) => id.as_str(),
        ContentRef::Unresolved { name, .. } => name.as_str(),
    };
    name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase()
}

fn accuracy(hit: u32, shots: u32) -> u32 {
    if shots == 0 {
        0
    } else {
        (hit * 100).div_ceil(shots)
    }
}

/// Slab test of the segment `start..end` against an axis-aligned box.
fn segment_hits_box(start: Vec3, end: Vec3, min: Vec3, max: Vec3) -> bool {
    let delta = end - start;
    let (mut enter, mut exit) = (0.0f32, 1.0f32);
    for axis in 0..3 {
        if delta[axis].abs() < 1e-6 {
            if start[axis] < min[axis] || start[axis] > max[axis] {
                return false;
            }
            continue;
        }
        let a = (min[axis] - start[axis]) / delta[axis];
        let b = (max[axis] - start[axis]) / delta[axis];
        enter = enter.max(a.min(b));
        exit = exit.min(a.max(b));
        if enter > exit {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abilities_strip_only_what_the_datablock_lacks() {
        let input = MoveInput {
            forward: 1.0,
            right: -1.0,
            yaw: 0.5,
            pitch: 0.1,
            jump: true,
            crouch: true,
            jet: true,
            head_yaw: 0.0,
        };
        let still = Abilities::NO_MOVE.apply(input);
        assert_eq!(
            (still.forward, still.right, still.jump, still.jet),
            (0.0, 0.0, false, false)
        );
        assert!(still.crouch && still.yaw == 0.5 && still.pitch == 0.1);
        let walking = Abilities::NO_JET.apply(input);
        assert!(walking.jump && !walking.jet && walking.forward == 1.0);
        assert_eq!(Abilities::STANDARD.apply(input), input);
    }
    #[test]
    fn prints_compare_by_letter() {
        assert_eq!(
            print_name(&ContentRef::Unresolved {
                namespace: "print".into(),
                name: "Letters/O".into()
            }),
            "o"
        );
        assert_eq!(
            print_name(&ContentRef::Resolved(
                "print/print_letters_default/-minus".into()
            )),
            PRINT_BLANK
        );
    }
    #[test]
    fn target_hits_need_the_segment_to_cross_the_board() {
        let (min, max) = (Vec3::new(-1.0, 0.0, -0.35), Vec3::new(1.0, 2.0, 0.35));
        assert!(segment_hits_box(
            Vec3::new(0.0, 1.0, 5.0),
            Vec3::new(0.0, 1.0, -5.0),
            min,
            max
        ));
        assert!(!segment_hits_box(
            Vec3::new(3.0, 1.0, 5.0),
            Vec3::new(3.0, 1.0, -5.0),
            min,
            max
        ));
        assert!(!segment_hits_box(
            Vec3::new(0.0, 1.0, 5.0),
            Vec3::new(0.0, 1.0, 1.0),
            min,
            max
        ));
        assert_eq!(accuracy(2, 3), 67);
        assert_eq!(accuracy(0, 0), 0);
    }
}
