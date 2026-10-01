//! A made-up native map bundle for tests that have no converted maps:
//! `bri_content::testing::map_bundle`'s lit rooms, their lamps baked into
//! the lightmaps with the renderer's own light model ([`MapLight`]), so the
//! light fit finds exactly the lamp that was baked. Every number is
//! invented; nothing is read from an original map.
use crate::map_lighting::MapLight;
use anyhow::Result;
pub use bri_content::testing::map_bundle::{LIGHTMAP_SIZE, Lamp, RoomMap, rooms, rooms_for, scene};
use std::path::Path;

/// The renderer's light for a fixture lamp.
pub fn map_light(lamp: &Lamp) -> MapLight {
    MapLight {
        position: lamp.position,
        color: lamp.color,
        inner: lamp.inner,
        outer: lamp.outer,
        channel: None,
    }
}

fn shade(lamp: &Lamp, position: glam::Vec3, normal: glam::Vec3) -> glam::Vec3 {
    map_light(lamp).shade(position, normal)
}

/// The room's interior, its lamp baked as the renderer's [`MapLight`]
/// lights a surface.
pub fn interior(map: &RoomMap) -> Result<bri_content::interior::Interior> {
    bri_content::testing::map_bundle::interior(map, &shade)
}

/// Writes `maps` as a native map bundle in `dir`, each lamp baked as the
/// renderer's [`MapLight`] lights a surface.
pub fn write_bundle(dir: &Path, maps: &[RoomMap]) -> Result<()> {
    bri_content::testing::map_bundle::write_bundle_shaded(dir, maps, &shade)
}
