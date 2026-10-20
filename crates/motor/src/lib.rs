//! The v20 player motor, shared by players and player-type mounts (horses,
//! rowboats, cannons, turrets): one kinematic box moved by `PlayerData` constants.
pub mod archetype;
pub mod climb;
pub mod controller;
pub mod crouch;
pub mod momentum;
pub mod player;
pub mod player_types;
pub mod torque;
