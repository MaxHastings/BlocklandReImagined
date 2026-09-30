//! Blockland ReImagined native UI.
//!
//! Layers (bottom to top):
//! - [`schema`] / [`pack`]: the converted UI pack (no Torque readers here).
//! - [`geom`], [`draw`], [`text`]: logical-pixel geometry, a renderer-neutral
//!   draw list and the original cached bitmap fonts; [`fallback`] draws
//!   glyphs the caches lack from the system's fonts.
//! - [`ml`]: the one Torque `GuiMLTextCtrl` markup parser, layout and
//!   renderer for prints, chat, message boxes and authored ML controls.
//! - [`view`]: authored control trees with Torque resize rules, skins and
//!   widget interaction (focus, text entry, popups, lists, scrolling,
//!   accelerators).
//! - [`models`]: interaction state machines (HUD inventory, chat, brick
//!   selector/favorites, wrench, events) with no content dependency.
//! - [`binds`], [`input`], [`prefs`]: stock default binds, remapping, brick
//!   key repeat, Torque key naming and `$pref::` values.
//! - [`api`]: the typed integration boundary (actions, view models, settings).
//! - [`gpu`] (feature `gpu`): wgpu renderer for the draw list.
//!
//! The screen manager and initial menus are integrated; remaining screens are in progress;
//! see README.md "Status".
pub mod api;
pub mod binds;
pub mod draw;
pub mod fallback;
pub mod geom;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod input;
pub mod ml;
pub mod models;
pub mod pack;
pub mod prefs;
pub mod schema;
pub mod text;
pub mod view;

pub mod screens;
pub mod ui;
