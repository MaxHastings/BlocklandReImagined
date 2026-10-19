//! A node's baked light from its node custom property `bri_vertex_lights` ([`bri_content::vertex_lighting`]): the lights that reach the
//! mesh, in the node's own frame and length unit like its POSITIONs, with a visibility mask per light (hex, least significant bit
//! first, one bit per vertex of the node's mesh counted across its primitives):
//!
//! ```json
//! "extras": { "bri_vertex_lights": { "ambient": [0.3, 0.3, 0.3],
//!   "lights": [{ "position": [1, 2, 3], "color": [1, 0.8, 0.6], "radius": 4, "effect": { "kind": "point" } }],
//!   "visible": ["ff0f"] } }
//! ```
//!
//! A layer-blended surface is lit by its vertex colours alone, so the computed light becomes its COLOR_0 (times any it already has).
use super::point;
use anyhow::{Context, Result, ensure};
use bri_content::vertex_lighting::{Effect, Light, Set};
use glam::Vec3;
use serde_json::Value;

fn hex(text: &str) -> Result<Vec<u8>> {
    ensure!(
        text.len().is_multiple_of(2),
        "a visibility mask has an odd number of hex digits"
    );
    (0..text.len() / 2)
        .map(|i| {
            u8::from_str_radix(&text[2 * i..2 * i + 2], 16)
                .with_context(|| format!("`{}` is not hex", &text[2 * i..2 * i + 2]))
        })
        .collect()
}

/// The node's lights in the importer's frame (the vertices' own), or none when it names none.
pub(super) fn read(extras: &Value, own_scale: Vec3, vertices: usize) -> Result<Option<Set>> {
    let Some(spec) = extras.get("bri_vertex_lights").filter(|v| v.is_object()) else {
        return Ok(None);
    };
    let mut set = Set {
        ambient: spec
            .get("ambient")
            .map(|a| serde_json::from_value(a.clone()))
            .transpose()
            .context("bri_vertex_lights `ambient` is three numbers")?
            .unwrap_or([0.0; 3]),
        lights: serde_json::from_value(spec["lights"].clone())
            .context("bri_vertex_lights `lights`")?,
        visible: spec["visible"]
            .as_array()
            .context("bri_vertex_lights needs `visible`, a mask per light")?
            .iter()
            .map(|m| hex(m.as_str().unwrap_or_default()))
            .collect::<Result<_>>()?,
    };
    set.validate(vertices)?;
    let length = (own_scale.x.abs() + own_scale.y.abs() + own_scale.z.abs()) / 3.0;
    for light in &mut set.lights {
        let Light {
            position,
            radius,
            effect,
            ..
        } = light;
        *position = point(Vec3::from(*position) * own_scale).to_array();
        *radius *= length;
        if let Effect::Directional { direction } = effect {
            *direction = point(Vec3::from(*direction)).to_array();
        }
    }
    Ok(Some(set))
}
