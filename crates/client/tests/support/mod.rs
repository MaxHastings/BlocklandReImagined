//! Shared helpers for client test targets that run on synthetic fixtures
//! and, ignored, on the generated v20 content. Use with
//! `#[macro_use] mod support;`. (`sampler.rs` is included by path where
//! needed, not from here.)
#![allow(dead_code)]

#[macro_use]
mod variants;
pub mod avatar_fixture;
pub mod brick_fixture;
pub mod content_root;
pub mod debris_fixture;
pub mod files;
pub mod gpu;
pub mod item_fixture;
pub mod vehicle_fixture;
