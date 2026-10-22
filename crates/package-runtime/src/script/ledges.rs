//! What a script asks of a ledge (`ledge_at`) and how it is told one. A
//! ledge is the top edge of a vertical wall the player faces: the engine
//! finds it by tracing the world, and the rules decide what to do with it.
use super::{Fallible, fail, float, float_entry, map, position};
use rhai::{Dynamic, Map};

/// How `ledge_at` looks for a ledge. Every length is in world units and
/// comes from the rules.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedgeProbe {
    /// How far the first trace goes along the view from the eye.
    pub reach: f32,
    /// A wall counts only within this horizontal distance of the eye. When
    /// the first trace lands on a ledge's top instead of its wall, a second
    /// goes horizontally toward it this far.
    pub wall_reach: f32,
    /// How far below that first hit the second trace starts.
    pub drop: f32,
    /// How far above the wall hit the search for its top starts.
    pub up: f32,
    /// How far along the wall either side of the hit the edge is sampled.
    pub edge_half: f32,
    /// The largest `|ny|` a wall's normal has and still counts as vertical.
    pub vertical: f32,
    /// The steepest the edge may run, rise over run.
    pub max_slope: f32,
    /// How far into the wall the search for its top starts, so it lands on
    /// the wall's solid and not the air beside it.
    pub inset: f32,
}

/// The ledge the player faces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ledge {
    /// The wall's hit, raised to the top edge.
    pub point: [f32; 3],
    /// The wall's horizontal normal (x, z), a unit vector toward the player.
    pub normal: [f32; 2],
    /// The two edge samples, either side of `point`.
    pub edge: [[f32; 3]; 2],
    /// How far `point` is above the player's feet.
    pub height: f32,
    /// The edge's rise over run.
    pub slope: f32,
}

/// Read `ledge_at`'s options. The lengths are required: the engine has no
/// notion of how high a body reaches.
pub(super) fn probe(options: &Map) -> Fallible<LedgeProbe> {
    let mut out = LedgeProbe {
        reach: 0.0,
        wall_reach: 0.0,
        drop: 0.0,
        up: 0.0,
        edge_half: 0.0,
        vertical: 1e-4,
        max_slope: 0.5,
        inset: 0.05,
    };
    let mut given = [false; 5];
    for (key, value) in options {
        let v = float(value)?;
        match key.as_str() {
            "reach" => (out.reach, given[0]) = (v, true),
            "wall_reach" => (out.wall_reach, given[1]) = (v, true),
            "drop" => (out.drop, given[2]) = (v, true),
            "up" => (out.up, given[3]) = (v, true),
            "edge_half" => (out.edge_half, given[4]) = (v, true),
            "vertical" => out.vertical = v,
            "max_slope" => out.max_slope = v,
            "inset" => out.inset = v,
            other => {
                return fail(format!(
                    "ledge_at has no option `{other}` (reach, wall_reach, drop, up, edge_half, vertical, max_slope, inset)"
                ));
            }
        }
    }
    if given.contains(&false) {
        return fail("ledge_at needs reach, wall_reach, drop, up and edge_half");
    }
    let lengths = [out.reach, out.wall_reach, out.drop, out.up, out.edge_half];
    if lengths
        .iter()
        .any(|v| !(0.0..=crate::ops::MAX_RAY_RANGE).contains(v))
        || out.reach <= 0.0
        || out.up <= 0.0
        || out.edge_half <= 0.0
        || !(0.0..=1.0).contains(&out.vertical)
        || out.max_slope < 0.0
        || !out.max_slope.is_finite()
        || !(0.0..=10.0).contains(&out.inset)
    {
        return fail("ledge_at's lengths must be positive and within a ray's range");
    }
    Ok(out)
}

pub(super) fn ledge_map(l: &Ledge) -> Dynamic {
    let [x, y, z] = position(l.point);
    let point = |p: [f32; 3]| {
        Dynamic::from_array(
            p.iter()
                .map(|c| Dynamic::from_float(f64::from(*c)))
                .collect(),
        )
    };
    map([
        x,
        y,
        z,
        float_entry("nx", l.normal[0]),
        float_entry("nz", l.normal[1]),
        float_entry("height", l.height),
        float_entry("slope", l.slope),
        ("edge_a", point(l.edge[0])),
        ("edge_b", point(l.edge[1])),
    ])
}
