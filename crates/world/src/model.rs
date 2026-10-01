use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const WORLD_SCHEMA: u32 = 2;
pub const MAX_BRICKS: usize = 1_000_000;
/// Native admission bound, not the original 100-row editor limit. Runtime work
/// budgets and usable large-list editing are separate acceptance requirements.
pub const MAX_EVENTS_PER_BRICK: usize = 1024;
/// What a world's bricks may add up to by [`Brick::stored_bound`]: the save
/// file's limit less room for the owner table and the rest of the world.
/// Admission enforces it, so every world a server accepts can be saved and
/// streamed to a joining client.
pub const MAX_STORED_BYTES: u64 = crate::persistence::MAX_SAVE_BYTES - 64 * 1024 * 1024;
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ContentRef {
    Resolved(String),
    /// Boxed: unresolved references are rare (imported saves), and inline
    /// they would double the size of every reference a brick holds. Encodes
    /// exactly as the struct variant it was.
    Unresolved(Box<Unresolved>),
}
/// A reference by original name, not yet bound to native content.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Unresolved {
    pub namespace: String,
    pub name: String,
}
impl ContentRef {
    pub fn unresolved(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self::Unresolved(Box::new(Unresolved {
            namespace: namespace.into(),
            name: name.into(),
        }))
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Resolved(id) => ensure!(!id.is_empty() && id.len() <= 512, "Invalid content ID"),
            Self::Unresolved(u) => ensure!(
                !u.namespace.is_empty()
                    && u.namespace.len() <= 64
                    && !u.name.is_empty()
                    && u.name.len() <= 512,
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
/// The aliases [`ItemSpawn::resolve_item`] binds by: each item's trimmed
/// ASCII-lowercase display name to its id. Display names need not be unique,
/// as in v20, where any number of Add-Ons may name an item "Sniper Rifle" and
/// the item lists show them all. A name that several items share binds to the
/// first of them in `items` order, so callers pass items in load order (the
/// base game first, then Add-Ons in `packages.json` order).
pub fn item_aliases<'a>(
    items: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> BTreeMap<String, String> {
    let mut aliases = BTreeMap::new();
    for (id, name) in items {
        aliases
            .entry(name.trim().to_ascii_lowercase())
            .or_insert_with(|| id.to_string());
    }
    aliases
}

impl ItemSpawn {
    /// Bind a native imported `item_ui` reference using trusted aliases keyed
    /// by trimmed ASCII-lowercase original display name. Missing names remain
    /// unresolved; this never reads or interprets retained source records.
    pub fn resolve_item(&mut self, native_aliases: &BTreeMap<String, String>) -> Result<bool> {
        let Some(ContentRef::Unresolved(unresolved)) = self.item.as_ref() else {
            return Ok(false);
        };
        if unresolved.namespace != "item_ui" {
            return Ok(false);
        }
        let Some(id) = native_aliases.get(&unresolved.name.trim().to_ascii_lowercase()) else {
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
/// A package block drawn on this brick in place of its colour: per-face
/// textures and flipbooks (`namespace:block/name`), in one of the block's
/// named states. Game rules change `state`; clients draw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockLook {
    pub block: String,
    /// `""` is the block's own faces; other names are its declared states.
    #[serde(default)]
    pub state: String,
}
impl BlockLook {
    pub fn validate(&self) -> Result<()> {
        let plain = |s: &str| !s.chars().any(char::is_control);
        ensure!(
            !self.block.is_empty() && self.block.len() <= 160 && plain(&self.block),
            "Invalid block look"
        );
        ensure!(
            self.state.len() <= 64 && plain(&self.state),
            "Invalid block state"
        );
        Ok(())
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
    // Rare settings are boxed: a brick without them pays a pointer, not the
    // whole setting (a million-brick world is mostly plain bricks).
    pub light: Option<Box<Light>>,
    pub emitter: Option<Box<Emitter>>,
    /// Item spawn setting; appearance and pickup state are host-owned.
    /// Defaulted because generated world packs and saves on existing installs
    /// may predate the field; drop the default once both are regenerated.
    #[serde(default)]
    pub item_spawn: ItemSpawn,
    /// Music/sound brick loop (`fxDTSBrick::setSound`, `AudioEmitter`).
    pub sound: Option<ContentRef>,
    /// Vehicle spawn brick setting (`fxDTSBrick::setVehicle`).
    pub vehicle: Option<Box<VehicleSpawn>>,
    pub events: Vec<EventRow>,
    /// Opaque source records survive native save/reload; never executed.
    pub source_records: Vec<SourceRecord>,
    /// A package block's faces drawn in place of the colour.
    pub look: Option<Box<BlockLook>>,
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
            look: None,
        }
    }
    /// Every palette index this brick names: its paint, then each event
    /// `Color` parameter. The one colour rule: all of them must index the
    /// world's palette, and anything that remaps colours remaps all of them.
    pub fn colors(&self) -> impl Iterator<Item = u8> + '_ {
        std::iter::once(self.color).chain(self.events.iter().flat_map(|e| {
            e.params.iter().filter_map(|v| match v {
                EventValue::Color(c) => Some(*c),
                _ => None,
            })
        }))
    }
    /// Map every palette index [`Brick::colors`] names through `remap`.
    pub fn recolor(&mut self, mut remap: impl FnMut(u8) -> u8) {
        self.color = remap(self.color);
        for value in self.events.iter_mut().flat_map(|e| &mut e.params) {
            if let EventValue::Color(c) = value {
                *c = remap(*c);
            }
        }
    }
    pub fn transform(&self) -> glam::Mat4 {
        glam::Mat4::from_translation(glam::Vec3::from(self.position))
            * glam::Mat4::from_rotation_y(
                -(self.quarter_turns as f32) * std::f32::consts::FRAC_PI_2,
            )
    }
    /// An upper bound on this brick's size in any carrier: its JSON save
    /// entry, and (far larger than) its network encoding. Structural and
    /// allocation-free, so admission can charge it on every mutation; see
    /// [`MAX_STORED_BYTES`].
    pub fn stored_bound(&self) -> u64 {
        // A JSON string: its bytes, five more for each escaped one
        // (`\u00XX`), and quotes. Fixed-size fields fit in the constants.
        fn text(s: &str) -> u64 {
            let escaped = s
                .bytes()
                .filter(|b| *b < 0x20 || *b == b'"' || *b == b'\\')
                .count();
            (s.len() + 5 * escaped) as u64 + 2
        }
        fn content(c: &ContentRef) -> u64 {
            match c {
                ContentRef::Resolved(id) => text(id) + 40,
                ContentRef::Unresolved(u) => text(&u.namespace) + text(&u.name) + 64,
            }
        }
        let optional = |c: &Option<ContentRef>| c.as_ref().map_or(0, content);
        let mut bytes = 512
            + content(&self.definition)
            + optional(&self.print)
            + self.name.as_deref().map_or(0, text)
            + self.light.as_ref().map_or(0, |l| content(&l.asset))
            + self.emitter.as_ref().map_or(0, |e| optional(&e.asset))
            + optional(&self.item_spawn.item)
            + optional(&self.sound)
            + self.vehicle.as_ref().map_or(0, |v| content(&v.vehicle))
            + self
                .look
                .as_ref()
                .map_or(0, |l| 32 + text(&l.block) + text(&l.state));
        for row in &self.events {
            bytes += 192 + text(&row.input) + text(&row.output);
            if let Some(p) = &row.preserved {
                bytes += text(&p.original) + text(&p.diagnostic);
            }
            if let EventTarget::Named(n) | EventTarget::Derived(n) = &row.target {
                bytes += text(n);
            }
            for value in &row.params {
                bytes += 32
                    + match value {
                        EventValue::Text(t) | EventValue::Datablock(Some(t)) => text(t),
                        // Up to "65535," each.
                        EventValue::Rows(bri_events::RowSelection::Indices(rows)) => {
                            6 * rows.len() as u64
                        }
                        // Numbers, a vector of three floats, flags.
                        _ => 64,
                    };
            }
        }
        for record in &self.source_records {
            bytes += 64 + text(&record.text) + record.diagnostic.as_deref().map_or(0, text);
        }
        bytes
    }
    pub fn validate(&self, palette_len: usize) -> Result<()> {
        self.definition.validate()?;
        ensure!(
            self.position
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.0),
            "Invalid brick position"
        );
        ensure!(self.quarter_turns < 4, "Invalid brick angle");
        if let Some(c) = self.colors().find(|c| usize::from(*c) >= palette_len) {
            bail!("Color {c} outside the {palette_len}-color palette");
        }
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
        if let Some(look) = &self.look {
            look.validate()?;
        }
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
            if let EventTarget::Named(n) | EventTarget::Derived(n) = &e.target {
                ensure!(!n.is_empty() && n.len() <= 128, "Invalid target name");
            }
            for value in &e.params {
                match value {
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
    /// Who each brick owner number is: the durable principal of the player
    /// who built with it. Owner numbers are world-scoped; a returning player
    /// gets their number back from this table. Owners with no entry are
    /// unclaimed (imported or anonymous builds).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub owners: BTreeMap<OwnerId, OwnerRecord>,
    /// Bricks this server has no definition for (a removed package, an
    /// add-on brick in an imported save). They are not in the world, but they
    /// are kept exactly and saved again, so nothing is lost when the content
    /// comes back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unloaded: Vec<Brick>,
}

pub const MAX_OWNERS: usize = 65_536;
/// Longest owner record name, in characters.
pub const MAX_OWNER_NAME: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerRecord {
    /// The player's public key, 64 lowercase hex characters.
    pub principal: String,
    /// Last name the player joined with, for display while they are away.
    /// At most `MAX_OWNER_NAME` characters: names are 23, but a Windows-1252
    /// symbol such as `™` is three UTF-8 bytes, so the bound counts
    /// characters, not bytes.
    pub name: String,
}
impl OwnerRecord {
    pub fn new(principal: [u8; 32], name: String) -> Self {
        Self {
            principal: principal.iter().map(|b| format!("{b:02x}")).collect(),
            name,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.principal.len() == 64
                && self
                    .principal
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
            "Invalid owner principal"
        );
        ensure!(
            self.name.chars().count() <= MAX_OWNER_NAME && !self.name.chars().any(char::is_control),
            "Invalid owner name"
        );
        Ok(())
    }
}
impl World {
    /// The owner number a principal built with in this world.
    pub fn owner_of(&self, principal: &str) -> Option<OwnerId> {
        self.owners
            .iter()
            .find(|(_, record)| record.principal == principal)
            .map(|(owner, _)| *owner)
    }
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
            owners: BTreeMap::new(),
            unloaded: Vec::new(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_header()?;
        for b in self.bricks.values().chain(&self.unloaded) {
            b.validate(self.palette.len())?;
        }
        Ok(())
    }
    /// Everything [`Self::validate`] checks except each brick, for callers
    /// that validate every brick themselves as they go through them.
    pub fn validate_header(&self) -> Result<()> {
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
            self.bricks.len() + self.unloaded.len() <= MAX_BRICKS && !self.bricks.contains_key(&0),
            "Invalid brick IDs/count"
        );
        ensure!(
            self.next_brick_id > self.bricks.keys().next_back().copied().unwrap_or(0),
            "Brick ID would be reused"
        );
        ensure!(
            self.owners.len() <= MAX_OWNERS && !self.owners.contains_key(&0),
            "Invalid owner table"
        );
        let mut principals = std::collections::BTreeSet::new();
        for record in self.owners.values() {
            record.validate()?;
            ensure!(
                principals.insert(&record.principal),
                "A principal owns two owner numbers"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boxed_content_ref_encodes_as_it_did_inline() {
        let unresolved = ContentRef::unresolved("print", "Letters/A");
        let json = serde_json::to_string(&unresolved).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"unresolved","value":{"namespace":"print","name":"Letters/A"}}"#
        );
        assert_eq!(
            serde_json::from_str::<ContentRef>(&json).unwrap(),
            unresolved
        );
        let resolved = ContentRef::Resolved("brick".into());
        assert_eq!(
            serde_json::to_string(&resolved).unwrap(),
            r#"{"kind":"resolved","value":"brick"}"#
        );
    }

    #[test]
    fn a_brick_keeps_its_rare_fields_off_the_inline_record() {
        // Every brick in every copy of a world pays the inline size.
        assert!(std::mem::size_of::<ContentRef>() <= 24);
        assert!(
            std::mem::size_of::<Brick>() <= 256,
            "{}",
            std::mem::size_of::<Brick>()
        );
    }
}
