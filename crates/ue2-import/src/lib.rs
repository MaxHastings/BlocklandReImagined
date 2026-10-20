//! The one place Unreal Engine 2's projector vocabulary meets the engine: its property names, package units, rotation units, left-handed
//! axes and blend codes become the neutral [`ProjectorSpec`] and [`Placement`] that `bri-effects-render` draws. Nothing downstream of
//! this crate knows any of them.
pub mod projector_bake;
pub mod projector_props;
