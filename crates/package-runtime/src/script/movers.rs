//! What a script sees of the map's keyed movers (doors).
use rhai::{Dynamic, Map};
use std::collections::BTreeMap;

/// One mover, as `movers()` lists it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoverInfo {
    pub id: String,
    /// The key it rests at, or None between keys.
    pub key: Option<u8>,
    /// The key it left, and the key it is heading to or last reached.
    pub from: u8,
    pub to: u8,
    pub moving: bool,
    /// Held by a body in its way.
    pub blocked: bool,
    /// How far along its motion, 0 to 1.
    pub progress: f32,
    pub keys: usize,
    /// Where its origin is now, and where it is placed at rest.
    pub position: [f32; 3],
    pub rest: [f32; 3],
    /// The package's own facts about it.
    pub params: BTreeMap<String, String>,
}

pub(super) fn mover_map(m: &MoverInfo) -> Dynamic {
    let array = |v: [f32; 3]| -> Dynamic {
        v.iter()
            .map(|x| Dynamic::from_float(f64::from(*x)))
            .collect::<Vec<_>>()
            .into()
    };
    let mut params = Map::new();
    for (k, v) in &m.params {
        params.insert(k.as_str().into(), v.clone().into());
    }
    let mut map = Map::new();
    map.insert("id".into(), m.id.clone().into());
    map.insert(
        "key".into(),
        m.key
            .map_or(Dynamic::from_int(-1), |k| Dynamic::from_int(i64::from(k))),
    );
    map.insert("from".into(), Dynamic::from_int(i64::from(m.from)));
    map.insert("to".into(), Dynamic::from_int(i64::from(m.to)));
    map.insert("moving".into(), m.moving.into());
    map.insert("blocked".into(), m.blocked.into());
    map.insert(
        "progress".into(),
        Dynamic::from_float(f64::from(m.progress)),
    );
    map.insert("keys".into(), Dynamic::from_int(m.keys as i64));
    map.insert("position".into(), array(m.position));
    map.insert("rest".into(), array(m.rest));
    map.insert("params".into(), params.into());
    map.into()
}
