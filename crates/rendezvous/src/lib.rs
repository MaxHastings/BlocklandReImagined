//! Join codes for Blockland ReImagined: a small, self-hostable service that
//! gives each host a short code, introduces players to hosts so both routers
//! open for each other (UDP hole punching over QUIC), and relays the
//! encrypted game traffic when that fails. See `docs/architecture/hosting.md`.
pub mod natsim;
pub mod proto;
pub mod relay;
pub mod service;
