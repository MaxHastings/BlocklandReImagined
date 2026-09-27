//! v20's selectable player datablocks: PlayerStandardArmor and the stock
//! Player_* and Vehicle_Horse add-ons. Each is its PlayerStandardArmor
//! inheritance with the add-on's overrides, converted from Torque ticks.
use crate::player::{PlayerTuning, TORQUE_TICK};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PlayerType {
    #[default]
    Standard,
    NoJet,
    FuelJet,
    JumpJet,
    LeapJet,
    Quake,
    Horse,
    /// Item_Sports' `BallShootPlayer`: a no-jet player lining up a basketball
    /// shot can only jump. Not selectable.
    BallShoot,
}

impl PlayerType {
    /// In the mini-game dialog's order: the stock list sorted by `uiName`.
    pub const ALL: [Self; 7] = [
        Self::FuelJet,
        Self::Horse,
        Self::JumpJet,
        Self::LeapJet,
        Self::NoJet,
        Self::Quake,
        Self::Standard,
    ];
    /// Native datablock id, as mini-game settings and events name it.
    pub fn id(self) -> &'static str {
        match self {
            Self::Standard => "v20.player.playerstandardarmor",
            Self::NoJet => "v20.player.playernojet",
            Self::FuelJet => "v20.player.playerfueljet",
            Self::JumpJet => "v20.player.playerjumpjet",
            Self::LeapJet => "v20.player.playerleapjet",
            Self::Quake => "v20.player.playerquakearmor",
            Self::Horse => "v20.player.horsearmor",
            Self::BallShoot => "v20.player.ballshootplayer",
        }
    }
    /// Script datablock name, as wrench events and saves name it.
    pub fn datablock_name(self) -> &'static str {
        match self {
            Self::Standard => "PlayerStandardArmor",
            Self::NoJet => "PlayerNoJet",
            Self::FuelJet => "PlayerFuelJet",
            Self::JumpJet => "PlayerJumpJet",
            Self::LeapJet => "PlayerLeapJet",
            Self::Quake => "PlayerQuakeArmor",
            Self::Horse => "HorseArmor",
            Self::BallShoot => "BallShootPlayer",
        }
    }
    pub fn from_datablock_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.datablock_name().eq_ignore_ascii_case(name))
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.id().eq_ignore_ascii_case(id))
    }
    /// `uiName`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard Player",
            Self::NoJet => "No-Jet Player",
            Self::FuelJet => "Fuel-Jet Player",
            Self::JumpJet => "Jump-Jet Player",
            Self::LeapJet => "Leap-Jet Player",
            Self::Quake => "Quake-Like Player",
            Self::Horse => "Horse",
            Self::BallShoot => "",
        }
    }
    /// `showEnergyBar`.
    pub fn shows_energy(self) -> bool {
        matches!(self, Self::FuelJet | Self::LeapJet)
    }
    /// `maxDamage`.
    pub fn max_health(self) -> f32 {
        match self {
            Self::Horse => 250.0,
            _ => 100.0,
        }
    }
    /// `rideable`: other players may mount it.
    pub fn rideable(self) -> bool {
        self == Self::Horse
    }
    /// `canRide`: it may mount vehicles and horses.
    pub fn can_ride(self) -> bool {
        self != Self::Horse
    }
    pub fn tuning(self) -> PlayerTuning {
        let standard = PlayerTuning::default();
        let per_tick = |v: f32| v / TORQUE_TICK;
        match self {
            Self::Standard => standard,
            Self::NoJet => PlayerTuning {
                can_jet: false,
                ..standard
            },
            Self::FuelJet => PlayerTuning {
                min_jet_energy: 2.0,
                jet_drain: per_tick(2.0),
                ..standard
            },
            Self::JumpJet => PlayerTuning {
                min_jet_energy: 10.0,
                jet_drain: per_tick(10.0),
                recharge: per_tick(3.0),
                ..standard
            },
            Self::LeapJet => PlayerTuning {
                min_jet_energy: 5.0,
                jet_drain: per_tick(5.0),
                recharge: per_tick(1.5),
                ..standard
            },
            Self::Quake => PlayerTuning {
                acceleration: 100.0,
                forward: 15.0,
                backward: 15.0,
                sideways: 15.0,
                crouch_forward: 7.0,
                crouch_backward: 7.0,
                crouch_sideways: 7.0,
                jump_speed: 9.0,
                jump_delay_ticks: 0,
                can_jet: false,
                slope_degrees: 55.0,
                jump_surface_degrees: 55.0,
                ..standard
            },
            Self::BallShoot => PlayerTuning {
                forward: 0.0,
                backward: 0.0,
                sideways: 0.0,
                crouch_forward: 0.0,
                crouch_backward: 0.0,
                crouch_sideways: 0.0,
                underwater_forward: 0.0,
                underwater_backward: 0.0,
                underwater_sideways: 0.0,
                step_height: 0.0,
                can_jet: false,
                min_jet_energy: 0.0,
                jet_drain: 0.0,
                ..standard
            },
            Self::Horse => PlayerTuning {
                width: 2.5,
                stand_height: 2.4,
                crouch_height: 2.4,
                // horse.dts `Eye` node in its bind pose.
                stand_eye: 2.4,
                crouch_eye: 2.4,
                acceleration: 28.0,
                forward: 12.0,
                backward: 6.0,
                sideways: 1.0,
                crouch_forward: 12.0,
                crouch_backward: 6.0,
                crouch_sideways: 1.0,
                jump_speed: 17.0,
                jump_delay_ticks: 0,
                can_jet: false,
                max_energy: 10.0,
                recharge: per_tick(0.4),
                slope_degrees: 85.0,
                jump_surface_degrees: 86.0,
                ..standard
            },
        }
    }
}
