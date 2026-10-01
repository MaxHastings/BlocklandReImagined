//! Packages: the unit of content a server enables and a client loads.
//!
//! Every peer reads `packages.json` ([`packages::PackageSet`]) from its
//! content root: which packages to load, from which directory, and on which
//! side. Loading hashes each one into an [`environment::Environment`]; a
//! joining client's environment is compared with the server's and every
//! difference is named package by package. Content inside a package is named
//! by [`id::ContentId`] (`namespace:kind/name`).
//!
//! Format: `docs/architecture/packages.md`.
pub mod capability;
pub mod classic;
pub mod defaults;
pub mod diag;
pub mod environment;
pub mod id;
pub mod library;
pub mod packages;
pub mod path;
pub mod sync;

/// The platform API level this build provides. Packages declare the level
/// they need (`"api": 1`); it changes only when the package-facing surface
/// does, independent of the game build and the wire protocol.
pub const API_LEVEL: u32 = 1;
