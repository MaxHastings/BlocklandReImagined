//! Particle, spark, mesh, beam, trail and rope emitters, coronas, sun lens flare, laser and particle projectors and fog, running on
//! their own authored properties ([`props`]). Pure simulation and vertex building: no package reader, no GPU, no world. The host
//! supplies the owner's transform, the camera and a collision query; this crate returns vertices and the draw state
//! ([`draw::DrawState`]) for a general dynamic-batch renderer (`bri-effects-render`). Its behaviour follows a reference engine's, and
//! the reference engine's own names and units are mapped into these types by an importer (`bri-ue2-import`), never used by a game.
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
