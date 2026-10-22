//! What a script sees of the map's volumes (named boxes the map authors).
use rhai::{Dynamic, Map};
use std::collections::BTreeMap;

/// One volume holding a point, as `volumes_at` lists it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VolumeInfo {
    pub id: String,
    /// Among volumes holding one point, the higher comes first.
    pub priority: i32,
    /// The map's own text for it (a place's name, what it is for).
    pub properties: BTreeMap<String, String>,
}

pub(super) fn volume_map(v: &VolumeInfo) -> Dynamic {
    let mut properties = Map::new();
    for (k, value) in &v.properties {
        properties.insert(k.as_str().into(), value.clone().into());
    }
    let mut map = Map::new();
    map.insert("id".into(), v.id.clone().into());
    map.insert("priority".into(), Dynamic::from_int(i64::from(v.priority)));
    map.insert("properties".into(), properties.into());
    map.into()
}
