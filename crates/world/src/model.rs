use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const WORLD_SCHEMA: u32 = 2;
pub const MAX_BRICKS: usize = 1_000_000;
/// Native admission bound, not the original 100-row editor limit. Runtime work
/// budgets and usable large-list editing are separate acceptance requirements.
pub const MAX_EVENTS_PER_BRICK: usize = 1024;
pub const TICKS_PER_SECOND: u64 = 120;
pub type BrickId = u64;
/// Assigned by the server's identity service; zero is world-owned content.
pub type OwnerId = u64;
/// A world's bricks. A persistent (structurally shared) ordered map: a copy
/// is O(1) and an edit copies O(log n), so snapshots handed to other threads
/// (replication, rendering, collision) never deep-clone the world.
pub type Bricks = imbl::OrdMap<BrickId, Brick>;
/// Mutate every brick (a persistent map has no `values_mut`).
pub fn update_bricks(bricks: &mut Bricks, mut f: impl FnMut(&mut Brick)) {
    *bricks = std::mem::take(bricks)
        .into_iter()
        .map(|(id, mut brick)| {
            f(&mut brick);
            (id, brick)
        })
        .collect();
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ContentRef {
    Resolved(String),
    Unresolved { namespace: String, name: String },
}
impl ContentRef {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Resolved(id) => ensure!(!id.is_empty() && id.len() <= 512, "Invalid content ID"),
            Self::Unresolved { namespace, name } => ensure!(
                !namespace.is_empty()
                    && namespace.len() <= 64
                    && !name.is_empty()
                    && name.len() <= 512,
                "Invalid unresolved reference"
            ),
        };
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecord {
    pub line: u32,
    pub text: String,
    pub diagnostic: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Light {
    pub asset: ContentRef,
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Emitter {
    pub asset: Option<ContentRef>,
    pub direction: u8,
}
/// Vehicle spawn brick contents. `recolor` paints the vehicle with the
/// brick color (the wrench "Recolor Vehicle" checkbox).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleSpawn {
    pub vehicle: ContentRef,
    pub recolor: bool,
}
/// Vanilla ordinary-brick item attachment. Selectors persist even for NONE.
/// Positions: Up0, Down1, North2, East3, South4, West5. Facing: North2..West5.
/// These are world-axis selectors, independent of the brick's quarter turns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSpawn {
    pub item: Option<ContentRef>,
    pub position: u8,
    pub direction: u8,
    pub respawn_ms: u32,
}
impl Default for ItemSpawn {
    fn default() -> Self {
        Self {
            item: None,
            position: 0,
            direction: 2,
            respawn_ms: 4000,
        }
    }
}
impl ItemSpawn {
    /// Bind a native imported `item_ui` reference using trusted aliases keyed
    /// by trimmed ASCII-lowercase original display name. Missing names remain
    /// unresolved; this never reads or interprets retained source records.
    pub fn resolve_item(&mut self, native_aliases: &BTreeMap<String, String>) -> Result<bool> {
        let Some(ContentRef::Unresolved { namespace, name }) = self.item.as_ref() else {
            return Ok(false);
        };
        if namespace != "item_ui" {
            return Ok(false);
        }
        let Some(id) = native_aliases.get(&name.trim().to_ascii_lowercase()) else {
            return Ok(false);
        };
        let resolved = ContentRef::Resolved(id.clone());
        resolved.validate()?;
        self.item = Some(resolved);
        Ok(true)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.position <= 5, "Invalid item position selector");
        ensure!(
            (2..=5).contains(&self.direction),
            "Invalid item facing selector"
        );
        ensure!(
            (1000..=300000).contains(&self.respawn_ms),
            "Item respawn must be 1000..300000 milliseconds"
        );
        if let Some(item) = &self.item {
            item.validate()?;
        }
        Ok(())
    }
    pub fn respawn_ticks(&self) -> u64 {
        (u64::from(self.respawn_ms) * TICKS_PER_SECOND).div_ceil(1000)
    }
}
/// Wrench event rows: the vanilla input/target/output model executed by
/// `bri-events` (the single event system for bricks).
pub use bri_events::{Row as EventRow, Target as EventTarget, Value as EventValue};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Brick {
    pub definition: ContentRef,
    /// Native world coordinates: X right, Y up, negative Z forward.
    pub position: [f32; 3],
    /// Clockwise quarter turns viewed from above, matching the original angle IDs.
    pub quarter_turns: u8,
    pub base_plate: bool,
    pub owner: OwnerId,
    pub color: u8,
    pub print: Option<ContentRef>,
    pub color_effect: u8,
    pub shape_effect: u8,
    pub raycast: bool,
    pub colliding: bool,
    pub visible: bool,
    pub name: Option<String>,
    pub light: Option<Light>,
    pub emitter: Option<Emitter>,
    /// Added compatibly to schema1; absent in older native saves means NONE
    /// with original wrench defaults. Appearance/pickup state is host-owned.
    #[serde(default)]
    pub item_spawn: ItemSpawn,
    /// Music/sound brick loop (`fxDTSBrick::setSound`, `AudioEmitter`).
    #[serde(default)]
    pub sound: Option<ContentRef>,
    /// Vehicle spawn brick setting (`fxDTSBrick::setVehicle`).
    #[serde(default)]
    pub vehicle: Option<VehicleSpawn>,
    pub events: Vec<EventRow>,
    /// Opaque source records survive native save/reload; never executed.
    pub source_records: Vec<SourceRecord>,
}
impl Brick {
    pub fn new(definition: ContentRef, position: [f32; 3], owner: OwnerId) -> Self {
        Self {
            definition,
            position,
            quarter_turns: 0,
            base_plate: false,
            owner,
            color: 0,
            print: None,
            color_effect: 0,
            shape_effect: 0,
            raycast: true,
            colliding: true,
            visible: true,
            name: None,
            light: None,
            emitter: None,
            item_spawn: ItemSpawn::default(),
            sound: None,
            vehicle: None,
            events: vec![],
            source_records: vec![],
        }
    }
    pub fn transform(&self) -> glam::Mat4 {
        glam::Mat4::from_translation(glam::Vec3::from(self.position))
            * glam::Mat4::from_rotation_y(
                -(self.quarter_turns as f32) * std::f32::consts::FRAC_PI_2,
            )
    }
    pub fn validate(&self, palette_len: usize) -> Result<()> {
        self.definition.validate()?;
        ensure!(
            self.position
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.0),
            "Invalid brick position"
        );
        ensure!(
            self.quarter_turns < 4 && (self.color as usize) < palette_len,
            "Invalid brick angle/color"
        );
        ensure!(
            self.color_effect <= 6 && self.shape_effect <= 2,
            "Invalid effect code"
        );
        if let Some(p) = &self.print {
            p.validate()?;
        }
        if let Some(n) = &self.name {
            ensure!(!n.is_empty() && n.len() <= 128, "Invalid brick name");
        }
        if let Some(l) = &self.light {
            l.asset.validate()?;
        }
        if let Some(e) = &self.emitter {
            ensure!(e.direction <= 5, "Invalid emitter direction");
            if let Some(a) = &e.asset {
                a.validate()?;
            }
        }
        self.item_spawn.validate()?;
        if let Some(sound) = &self.sound {
            sound.validate()?;
        }
        if let Some(vehicle) = &self.vehicle {
            vehicle.vehicle.validate()?;
        }
        ensure!(
            self.events.len() <= MAX_EVENTS_PER_BRICK,
            "Brick exceeds the native {MAX_EVENTS_PER_BRICK}-event admission limit"
        );
        ensure!(
            self.source_records.len() <= 4096,
            "Too many retained brick source records"
        );
        for e in &self.events {
            ensure!(e.delay_ms <= 300_000, "Event delay exceeds five minutes");
            // Bounds keep a full brick's rows inside one network frame.
            ensure!(
                e.input.len() <= 128 && e.output.len() <= 128 && e.params.len() <= 4,
                "Invalid event row"
            );
            if let Some(p) = &e.preserved {
                ensure!(
                    p.original.len() <= 2048 && p.diagnostic.len() <= 1024,
                    "Oversized preserved event row"
                );
            }
            if let EventTarget::Named(n) = &e.target {
                ensure!(!n.is_empty() && n.len() <= 128, "Invalid target name");
            }
            for value in &e.params {
                match value {
                    EventValue::Color(c) => {
                        ensure!((*c as usize) < palette_len, "Event color outside palette")
                    }
                    EventValue::Text(t) => {
                        ensure!(t.chars().count() <= 200, "Event text too long")
                    }
                    EventValue::Datablock(Some(id)) => {
                        ensure!(id.len() <= 256, "Event datablock ID too long")
                    }
                    EventValue::Rows(bri_events::RowSelection::Indices(rows)) => {
                        ensure!(rows.len() <= 256, "Too many event row indices")
                    }
                    _ => {}
                }
            }
        }
        for r in &self.source_records {
            ensure!(
                r.text.len() <= 65536 && r.diagnostic.as_ref().is_none_or(|s| s.len() <= 4096),
                "Oversized source record"
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub schema_version: u32,
    pub name: String,
    pub map_id: String,
    pub description: Vec<String>,
    pub palette: Vec<[f32; 4]>,
    pub tick: u64,
    pub revision: u64,
    pub next_brick_id: BrickId,
    pub bricks: Bricks,
    pub source_sha256: Option<String>,
    pub source_encoding: Option<String>,
}
impl World {
    pub fn new(name: String, map_id: String, palette: Vec<[f32; 4]>) -> Self {
        Self {
            schema_version: WORLD_SCHEMA,
            name,
            map_id,
            description: vec![],
            palette,
            tick: 0,
            revision: 0,
            next_brick_id: 1,
            bricks: Bricks::new(),
            source_sha256: None,
            source_encoding: None,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == WORLD_SCHEMA,
            "Unsupported world schema"
        );
        ensure!(
            !self.name.is_empty()
                && self.name.len() <= 512
                && !self.map_id.is_empty()
                && self.map_id.len() <= 512,
            "Invalid world identity"
        );
        ensure!(
            self.description.len() <= 4096 && self.description.iter().all(|l| l.len() <= 65536),
            "Oversized world description"
        );
        ensure!(
            !self.palette.is_empty()
                && self.palette.len() <= 256
                && self
                    .palette
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "Invalid world palette"
        );
        ensure!(
            self.bricks.len() <= MAX_BRICKS && !self.bricks.contains_key(&0),
            "Invalid brick IDs/count"
        );
        ensure!(
            self.next_brick_id > self.bricks.keys().next_back().copied().unwrap_or(0),
            "Brick ID would be reused"
        );
        for b in self.bricks.values() {
            b.validate(self.palette.len())?;
        }
        Ok(())
    }
}
