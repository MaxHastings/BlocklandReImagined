//! Particle emitters ported from the original engine's native code, running on the
//! emitter's own authored properties. Pure simulation and vertex building: no
//! package reader, no GPU, no world. The host supplies the owner's transform, the
//! camera and a collision query (a `World` trait, to come); this crate returns vertices and
//! the draw state ([`draw::DrawState`]) for a general dynamic-batch renderer.
//!
//! Every rule here is taken from the original binary; see
//! `aa25/notes/2026-10-19-ue2-emitters.md` and the specs beside it.
pub mod beam;
pub mod corona;
pub mod draw;
pub mod emitter;
pub mod fog;
pub mod laser;
pub mod lens_flare;
pub mod mesh;
pub mod particle;
pub mod particle_projector;
pub mod projector;
pub mod projector_geometry;
pub mod props;
pub mod random;
pub mod ribbon;
pub mod rope;
pub mod rotation;
pub mod shader_plan;
pub mod spark;
pub mod sprite;
pub mod trail;
