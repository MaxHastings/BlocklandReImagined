//! Package-provided gameplay, run by the engine without knowing the game.
//!
//! A package is a directory with a `package.json` manifest and the files it
//! provides. This crate loads a set of packages, computes their identities
//! and hashes, and runs their server behaviour in a sandboxed script runtime.
//! It knows nothing about mining, creatures or currencies: those are package
//! policy. What it offers are the general seams a package composes:
//!
//! - declared **commands** a client may send (v20's `serverCmd`),
//! - namespaced, server-owned **state** per player and per server, each key
//!   declaring its audience (`visible`: `server`, `owner` for a player's
//!   secret such as a hand of cards, or `everyone`),
//! - **hooks** the engine calls when something happens: `on_join`,
//!   `on_tick`, and `on_death(victim, killer)` for every death, however
//!   caused (the engine decides deaths; packages must not have to poll),
//! - **entities** whose behaviour is a package `think` function,
//! - a **chunk provider** that generates world chunks on demand,
//! - player **archetypes**: what a player is (movement, collision body,
//!   health, mounting rules, model and camera) as data, assigned per player
//!   with `set_archetype`; the host sends its archetype table to clients,
//!   which predict every archetype with the same motor,
//! - typed **operations** (place or remove a world brick, explode, spawn entity, move or
//!   respawn a player, ...) that the engine checks against the package's
//!   declared capabilities in one place ([`ops::authorize`]) before applying
//!   them,
//! - declarative client **HUD panels** and **box models** bound to replicated
//!   state. Clients never receive or run package code.
//!
//! Every call has its own operation budget ([`script::Budget`]), and the
//! server bounds what calls add up to by origin, so no one package or player
//! can take capacity everyone needs: script work the engine runs (thinks,
//! hooks, generation) is a per-package share of each tick, a player's
//! commands a per-player share, and destruction, chat lines, entity slots
//! and state bytes each have a per-package (or per-player) allowance. A
//! call past a share waits or is refused with a named reason (`command.busy`,
//! `state.budget`); state is budgeted against the save file's limit
//! ([`state::MAX_STATE_BYTES`]) so admitted state can always be saved.
pub mod check;
pub mod content;
pub mod manifest;
pub mod noise;
pub mod ops;
pub mod package;
pub mod script;
pub mod state;

pub use bri_package::diag::Diagnostic;
pub use ops::Op;
pub use package::{Catalog, Package};
pub use rhai::{self, Dynamic};
pub use state::{PlayerKey, Store};
