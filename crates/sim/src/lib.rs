//! Shared simulation adapters used by solo and multiplayer authority.
#![allow(
    clippy::disallowed_methods,
    reason = "f32::clamp here is not yet bri_console::Clamp::clamped"
)]
pub mod archetype;
pub mod blueprint;
pub mod bot_kind;
pub mod chunks;
pub mod crouch;
pub mod definitions;
pub mod drop_later;
pub mod ghost;
pub mod grid;
pub mod id_map;
pub mod item_spawners;
pub mod links;
pub mod map;
pub mod mirror;
pub mod nav;
pub mod parking;
pub mod player;
pub mod player_types;
pub mod prediction;
pub mod presentation;
pub mod reach;
pub mod route;
pub mod session;
pub mod simulation;
pub mod spawn;
pub mod testing;
pub mod tool_catalog;
pub mod tutorial;
pub mod water;
pub mod weapon_query;
