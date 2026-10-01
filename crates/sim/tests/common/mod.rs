//! Shared session test helpers for the v20 tool images.
#![allow(dead_code)]
use bri_sim::{
    player::MoveInput,
    session::{Command, InspectMode, Notice, Session},
};
use bri_world::Brick;

/// The generated native weapon pack, which includes the core tool images.
pub fn weapon_pack() -> bri_weapons::Pack {
    let path = content_root().join("content/weapons-pack-009/weapons.json");
    bri_weapons::Pack::from_json(&std::fs::read(path).expect("Run the documented importer first"))
        .unwrap()
}

/// The repository root, where the generated `content/` folder lives.
fn content_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The content a test body runs on: made up, or the generated native packs.
/// Tests written with [`on_both!`] run on the made-up content everywhere
/// and again on the real content in the push gate. The heavier packs
/// (bricks, vehicles) load on first use.
pub struct Fixture {
    pub weapons: bri_weapons::Pack,
    /// The generated native event catalog, on the real content.
    native_events: Option<bri_events::Catalog>,
    native: bool,
    bricks: std::sync::OnceLock<bri_sim::definitions::Definitions>,
    vehicles: std::sync::OnceLock<bri_vehicles::Pack>,
}

/// A vehicle by the part it plays in a test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vehicle {
    /// A strafe-steered car with a driver and passenger seats (the Jeep).
    Car,
    /// A mouse-steered tank whose gunner (seat 2) aims a turret.
    Tank,
    /// A driver vehicle that steers by the mouse, or by the strafe keys with
    /// the driver's strafe steering on: the Tank of Max's v0.1.4 report on
    /// the real content (the synthetic tank is mouse-steered only), the car
    /// on the made-up.
    StrafeSteered,
    /// A wheeled car that also flies.
    FlyingCar,
    /// A hovering flyer (the Magic Carpet).
    Carpet,
    /// The ridden horse.
    Horse,
    /// A rowboat with a passenger seat.
    Rowboat,
    /// A gun that charges its shot (the pirate cannon).
    Cannon,
    /// The standalone tank turret.
    Turret,
    /// The skis the skis item boards.
    Skis,
}

/// An item by the part it plays in a test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Gun,
    /// Boards its holder onto skis.
    Skis,
    /// Turns the player it hits into a horse.
    HorseRay,
    /// The rocket launcher of the default minigame loadout.
    Rocket,
    /// Two guns: the right fires on press, the left on release.
    Akimbo,
    /// The sports balls (Item_Sports).
    Basketball,
    Dodgeball,
    Football,
    SoccerBall,
}

/// A brick by the part it plays in a test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrickRole {
    Checkpoint,
    Teledoor,
    TreasureChest,
    TreasureChestOpen,
    /// A water brick taller than a player.
    DeepWater,
    /// The player spawn brick.
    SpawnPoint,
}

impl Fixture {
    pub fn synthetic() -> Self {
        Self {
            weapons: bri_weapons::testing::pack(),
            native_events: None,
            native: false,
            bricks: Default::default(),
            vehicles: Default::default(),
        }
    }
    pub fn content() -> Self {
        let root = content_root().join("content");
        Self {
            weapons: weapon_pack(),
            native_events: Some(
                bri_events::Catalog::load(root.join("events-pack-002/catalog.json"))
                    .expect("Run the documented importer first"),
            ),
            native: true,
            bricks: Default::default(),
            vehicles: Default::default(),
        }
    }
    /// Whether this is the generated native content (the gate's variant).
    pub fn is_native(&self) -> bool {
        self.native
    }
    /// The event catalog for tests about particular native events: the
    /// generated one on the real content, else the small made-up one.
    pub fn events(&self) -> bri_events::Catalog {
        self.native_events
            .clone()
            .unwrap_or_else(bri_events::testing::catalog)
    }
    /// The brick definitions: the stock catalog and map bricks, or
    /// [`bri_sim::testing::definitions`].
    pub fn bricks(&self) -> bri_sim::definitions::Definitions {
        self.bricks
            .get_or_init(|| {
                if self.native {
                    let root = content_root().join("content");
                    bri_sim::definitions::Definitions::load(
                        &root.join("stock-catalog-004"),
                        &root.join("maps-pass-008"),
                    )
                    .expect("Run the documented importer first")
                } else {
                    bri_sim::testing::definitions()
                }
            })
            .clone()
    }
    /// The vehicle pack: the converted native one, or
    /// [`bri_vehicles::testing::pack`].
    pub fn vehicles(&self) -> bri_vehicles::Pack {
        self.vehicles
            .get_or_init(|| {
                if self.native {
                    bri_vehicles::Pack::load(
                        content_root().join("content/vehicles-pack-012/vehicles.json"),
                    )
                    .expect("Run the documented importer first")
                } else {
                    bri_vehicles::testing::pack()
                }
            })
            .clone()
    }
    /// One vehicle's definition.
    pub fn vehicle_definition(&self, role: Vehicle) -> bri_vehicles::Definition {
        let id = self.vehicle(role);
        self.vehicles()
            .definitions
            .into_iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("no vehicle {id}"))
    }
    /// A brick's id in [`Fixture::bricks`].
    pub fn brick(&self, role: BrickRole) -> &'static str {
        use bri_sim::testing as t;
        match (self.native, role) {
            (false, BrickRole::Checkpoint) => t::CHECKPOINT,
            (false, BrickRole::Teledoor) => t::TELEDOOR,
            (_, BrickRole::TreasureChest) => t::TREASURE_CHEST,
            (_, BrickRole::TreasureChestOpen) => t::TREASURE_CHEST_OPEN,
            (false, BrickRole::DeepWater) => t::DEEP_WATER,
            (_, BrickRole::SpawnPoint) => t::SPAWN_POINT,
            (true, BrickRole::Checkpoint) => "v20/brick/brickcheckpointdata",
            (true, BrickRole::Teledoor) => "v20/brick/brickteledoordata",
            (true, BrickRole::DeepWater) => "v20/brick/brick8xwaterdata",
        }
    }
    /// The brick that holds a vehicle.
    pub fn vehicle_spawn_brick(&self) -> &'static str {
        if self.native {
            "v20/brick/brickvehiclespawndata"
        } else {
            bri_sim::testing::VEHICLE_SPAWN
        }
    }
    /// A vehicle's id.
    pub fn vehicle(&self, role: Vehicle) -> &'static str {
        use bri_vehicles::testing as t;
        match (self.native, role) {
            (false, Vehicle::Car) => t::CAR,
            (false, Vehicle::Tank) => t::TANK,
            (false, Vehicle::StrafeSteered) => t::CAR,
            (false, Vehicle::FlyingCar) => t::FLYING_CAR,
            (false, Vehicle::Carpet) => t::CARPET,
            (false, Vehicle::Horse) => t::HORSE,
            (false, Vehicle::Rowboat) => t::ROWBOAT,
            (false, Vehicle::Cannon) => t::CANNON,
            (false, Vehicle::Turret) => t::TURRET,
            (false, Vehicle::Skis) => t::SKIS,
            (true, Vehicle::Car) => "v20.vehicle.jeepvehicle",
            (true, Vehicle::Tank | Vehicle::StrafeSteered) => "v20.vehicle.tankvehicle",
            (true, Vehicle::FlyingCar) => "v20.vehicle.flyingwheeledjeepvehicle",
            (true, Vehicle::Carpet) => "v20.vehicle.magiccarpetvehicle",
            (true, Vehicle::Horse) => "v20.vehicle.horsearmor",
            (true, Vehicle::Rowboat) => "v20.vehicle.rowboatarmor",
            (true, Vehicle::Cannon) => "v20.vehicle.cannonturret",
            (true, Vehicle::Turret) => "v20.vehicle.tankturretplayer",
            (true, Vehicle::Skis) => "v20.vehicle.skivehicle",
        }
    }
    /// An item's id.
    pub fn item(&self, role: Item) -> &'static str {
        use bri_weapons::testing as t;
        match (self.native, role) {
            (_, Item::Gun) => t::GUN_ITEM,
            (false, Item::Skis) => t::SKIS_ITEM,
            (false, Item::HorseRay) => t::HORSE_RAY_ITEM,
            (true, Item::Skis) => "v20.weapon.skiitem",
            (true, Item::HorseRay) => "v20.weapon.horserayitem",
            (false, Item::Rocket) => t::ROCKET_ITEM,
            (true, Item::Rocket) => "v20.weapon.rocketlauncheritem",
            (false, Item::Akimbo) => t::AKIMBO_ITEM,
            (true, Item::Akimbo) => "v20.weapon.akimbogunitem",
            (false, Item::Basketball) => t::BASKETBALL_ITEM,
            (false, Item::Dodgeball) => t::DODGEBALL_ITEM,
            (_, Item::Football) => t::FOOTBALL_ITEM,
            (_, Item::SoccerBall) => t::SOCCER_ITEM,
            (true, Item::Basketball) => "v20.weapon.basketballitem",
            (true, Item::Dodgeball) => "v20.weapon.dodgeballitem",
        }
    }
    /// The default minigame settings, their loadout's rocket launcher the
    /// fixture's own (the rest are the core tools and the gun, which every
    /// fixture has under their v20 ids).
    pub fn minigame_settings(&self) -> bri_minigames::Settings {
        let mut settings = bri_minigames::Settings::default();
        settings.loadout[4] = Some(self.item(Item::Rocket).into());
        settings
    }
}

/// One test body, run twice: on the made-up [`Fixture::synthetic`] content,
/// and on the generated native content as an ignored test the push gate
/// runs (`--include-ignored`). The tests are `<name>::synthetic` and
/// `<name>::content`.
#[macro_export]
macro_rules! on_both {
    ($(#[$meta:meta])* fn $name:ident($f:ident: &Fixture) $(-> $ret:ty)? $body:block) => {
        $(#[$meta])*
        mod $name {
            #[allow(unused_imports)]
            use super::*;
            #[allow(unused_variables)]
            fn body($f: &Fixture) $(-> $ret)? $body
            #[test]
            fn synthetic() $(-> $ret)? {
                body(&Fixture::synthetic())
            }
            #[test]
            #[ignore = "requires generated v20 content"]
            fn content() $(-> $ret)? {
                body(&Fixture::content())
            }
        }
    };
}

/// A movement sequence newer than any the tests sent before.
pub fn move_sequence(s: &Session) -> u64 {
    1_000_000 + s.simulation().state().tick
}

/// Keep the player's current look and refresh the input lease.
pub fn hold_still(s: &mut Session, owner: u64) {
    let player = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    let sequence = move_sequence(s);
    s.movement(
        owner,
        sequence,
        MoveInput {
            yaw: player.yaw,
            pitch: player.pitch,
            ..Default::default()
        },
    )
    .unwrap();
}

/// Equip a tool slot and click: press until the swing lands, then let go
/// and wait for the image to be ready again. The trigger is the player's
/// held button, as in v20: held, the hammer would keep swinging.
pub fn swing(s: &mut Session, owner: u64, seq: u64, slot: usize) -> anyhow::Result<()> {
    s.equip_tool(owner, Some(slot))?;
    hold_still(s, owner);
    s.command(owner, seq, Command::WeaponTrigger { down: true })?;
    for _ in 0..8 {
        s.step()?;
    }
    s.release_trigger(owner)?;
    // The wrench's Fire alone lasts half a second.
    for _ in 0..240 {
        let ready = s.weapon_view().images.get(&owner).is_none_or(|images| {
            images
                .iter()
                .all(|image| image.hand != 0 || image.state == "Ready")
        });
        if ready {
            break;
        }
        s.step()?;
    }
    Ok(())
}

/// The dialog a wrench or printer hit opened for this player, if any.
pub fn opened(s: &mut Session, owner: u64) -> Option<(u64, Brick, InspectMode)> {
    s.take_private_notices()
        .into_iter()
        .rev()
        .find_map(|(to, notice)| match notice {
            Notice::Inspected {
                brick_id,
                brick,
                mode,
            } if to == owner => Some((brick_id, *brick, mode)),
            _ => None,
        })
}

/// Centre prints sent to this player.
pub fn center_prints(s: &mut Session, owner: u64) -> Vec<String> {
    s.take_private_notices()
        .into_iter()
        .filter_map(|(to, notice)| match notice {
            Notice::Center { text, .. } if to == owner => Some(text),
            _ => None,
        })
        .collect()
}
