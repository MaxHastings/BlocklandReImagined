//! Packages: the unit of content a server enables and a client downloads.
//!
//! A package is a directory with a `package.json` manifest. Its client data
//! (everything outside `server/`) is packed into one deterministic archive
//! whose SHA-256 is the package's identity; `server/` holds sandboxed server
//! behaviour and never leaves the server. The server publishes an
//! [`environment::Environment`] listing every enabled package; clients fetch
//! the archives they lack into a content-addressed [`store::Store`].
//!
//! Format and workflow: `docs/modding/`.
pub mod archive;
pub mod capability;
pub mod check;
pub mod diag;
pub mod environment;
pub mod id;
pub mod kind;
pub mod manifest;
pub mod slot;
pub mod store;

/// The platform API level this build provides. Packages declare the level
/// they need (`"api": 1`); it changes only when the package-facing surface
/// does, independent of the game build and the wire protocol.
pub const API_LEVEL: u32 = 1;
