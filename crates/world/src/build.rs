//! Native build snapshots and atomic append planning. A build is not a running
//! simulation checkpoint: queued actions are never replayed when planting it.
use crate::{Brick, BrickId, OwnerId, OwnerRecord, World};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A saved build file, as large as a world checkpoint: a million bricks. A
/// build travels to the host packed and compressed, far smaller than this.
pub const MAX_BUILD_BYTES: u64 = crate::persistence::MAX_SAVE_BYTES;
/// Saved builds start with this, then a compressed header (everything but
/// the bricks, so a save list reads only that) and the compressed packed
/// bricks ([`crate::packed`]). Builds saved as JSON before still load.
pub const MAGIC: &[u8] = b"BRI-BUILD";
#[derive(Serialize, Deserialize)]
struct FileHead {
    schema_version: u32,
    /// The world without its bricks.
    world: World,
    bricks: u64,
    /// [`SavedBuild::minigame`]; builds saved before it have none.
    #[serde(default)]
    minigame: Option<serde_json::Value>,
}
#[derive(Serialize, Deserialize)]
struct FileBricks {
    bricks: crate::packed::Packed,
    /// Numbered in order from 1.
    unloaded: crate::packed::Packed,
}
pub fn encode(build: &SavedBuild) -> Result<Vec<u8>> {
    build.validate()?;
    let mut world = build.world.clone();
    let bricks = std::mem::take(&mut world.bricks);
    let unloaded = std::mem::take(&mut world.unloaded);
    let head = FileHead {
        schema_version: build.schema_version,
        world,
        bricks: bricks.len() as u64,
        minigame: build.minigame.clone(),
    };
    let body = FileBricks {
        bricks: crate::packed::Packed::pack(bricks.iter().map(|(id, b)| (*id, Some(b)))),
        unloaded: crate::packed::Packed::pack(
            unloaded
                .iter()
                .enumerate()
                .map(|(i, b)| (i as BrickId + 1, Some(b))),
        ),
    };
    let head = zstd::bulk::compress(&rmp_serde::to_vec_named(&head)?, 3)?;
    let body = zstd::bulk::compress(&rmp_serde::to_vec_named(&body)?, 3)?;
    let mut out = Vec::with_capacity(MAGIC.len() + 4 + head.len() + body.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&u32::try_from(head.len())?.to_le_bytes());
    out.extend_from_slice(&head);
    out.extend_from_slice(&body);
    ensure!(
        out.len() as u64 <= MAX_BUILD_BYTES,
        "Native build exceeds storage/transfer limit"
    );
    Ok(out)
}
/// Decompress at most `limit` bytes.
fn expand(bytes: &[u8], limit: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut decoder = zstd::stream::read::Decoder::new(bytes)?;
    decoder.window_log_max(27)?;
    decoder.take(limit + 1).read_to_end(&mut out)?;
    ensure!(
        out.len() as u64 <= limit,
        "Native build exceeds storage limit"
    );
    Ok(out)
}
/// The compressed header and bricks of a binary save.
fn parts(bytes: &[u8]) -> Result<Option<(&[u8], &[u8])>> {
    let Some(rest) = bytes.strip_prefix(MAGIC) else {
        return Ok(None);
    };
    ensure!(rest.len() >= 4, "Truncated native build");
    let length = u32::from_le_bytes(rest[..4].try_into()?) as usize;
    let rest = &rest[4..];
    ensure!(length <= rest.len(), "Truncated native build");
    Ok(Some(rest.split_at(length)))
}
fn head(bytes: &[u8]) -> Result<FileHead> {
    let head: FileHead = rmp_serde::from_slice(&expand(bytes, MAX_BUILD_BYTES)?)?;
    ensure!(
        head.world.bricks.is_empty()
            && head.world.unloaded.is_empty()
            && head.bricks <= crate::MAX_BRICKS as u64,
        "Invalid native build header"
    );
    Ok(head)
}
/// A saved build without its bricks, and how many bricks it holds: what a
/// save list shows, without reading every brick.
pub fn decode_header(bytes: &[u8]) -> Result<(World, u64)> {
    ensure!(
        bytes.len() as u64 <= MAX_BUILD_BYTES,
        "Native build exceeds transfer/storage limit"
    );
    match parts(bytes)? {
        Some((header, _)) => {
            let head = head(header)?;
            head.world.validate_header()?;
            Ok((head.world, head.bricks))
        }
        None => {
            let mut build = decode(bytes)?;
            let count = build.world.bricks.len() as u64;
            build.world.bricks = Default::default();
            build.world.unloaded.clear();
            Ok((build.world, count))
        }
    }
}
pub fn decode(bytes: &[u8]) -> Result<SavedBuild> {
    ensure!(
        bytes.len() as u64 <= MAX_BUILD_BYTES,
        "Native build exceeds transfer/storage limit"
    );
    if let Some((header, body)) = parts(bytes)? {
        let head = head(header)?;
        let body: FileBricks = rmp_serde::from_slice(&expand(body, MAX_BUILD_BYTES)?)?;
        let mut world = head.world;
        for (id, brick) in body
            .bricks
            .unpack(crate::MAX_BRICKS)
            .map_err(anyhow::Error::msg)?
        {
            let brick = brick.context("Removal in a native build")?;
            ensure!(
                world.bricks.insert(id, brick).is_none(),
                "Repeated brick in a native build"
            );
        }
        for (_, brick) in body
            .unloaded
            .unpack(crate::MAX_BRICKS)
            .map_err(anyhow::Error::msg)?
        {
            world
                .unloaded
                .push(brick.context("Removal in a native build")?);
        }
        ensure!(
            world.bricks.len() as u64 == head.bricks,
            "Native build brick count differs from its header"
        );
        let build = SavedBuild {
            schema_version: head.schema_version,
            world,
            minigame: head.minigame,
        };
        build.validate()?;
        return Ok(build);
    }
    // Parse directly from JSON: serde's untagged intermediate representation
    // cannot recover numeric brick-map keys from JSON object keys.
    let build = match serde_json::from_slice::<SavedBuild>(bytes) {
        Ok(build) => build,
        Err(build_error) => SavedBuild {
            schema_version: BUILD_SCHEMA,
            world: serde_json::from_slice::<World>(bytes).with_context(|| {
                format!("Invalid native build ({build_error}) or imported world")
            })?,
            minigame: None,
        },
    };
    build.validate()?;
    Ok(build)
}

pub const BUILD_SCHEMA: u32 = 2;
/// Most bytes a build's mini-game takes, as JSON.
pub const MAX_MINIGAME_BYTES: usize = 1 << 20;

/// A saved build. Brick owners are the world's owner numbers, and the
/// world's owner table says which player (principal) each one is, so loading
/// the build on any server gives each builder's bricks back to that player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedBuild {
    pub schema_version: u32,
    pub world: World,
    /// The mini-game the build was saved with (its settings, teams and the
    /// Add-On data kept per mini-game), as the host describes it, for the
    /// host to set up again on loading (Slayer's saved mini-game configs).
    /// Builds saved before it, or by someone running no mini-game, have
    /// none.
    #[serde(default)]
    pub minigame: Option<serde_json::Value>,
}
impl SavedBuild {
    pub fn new(world: World) -> Self {
        Self {
            schema_version: BUILD_SCHEMA,
            world,
            minigame: None,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == BUILD_SCHEMA,
            "Unsupported native build schema"
        );
        if let Some(minigame) = &self.minigame {
            ensure!(
                serde_json::to_vec(minigame)?.len() <= MAX_MINIGAME_BYTES,
                "A build's mini-game is at most {MAX_MINIGAME_BYTES} bytes"
            );
        }
        self.world.validate()
    }
    pub fn capture(world: &World, events: bool, ownership: bool) -> Result<Self> {
        let mut world = world.clone();
        let strip = |brick: &mut Brick| {
            if !events {
                brick.events.clear();
            }
            if !ownership {
                brick.owner = 0;
            }
            brick.source_records.retain(|record| {
                let tag = record.text.split_whitespace().next().unwrap_or("");
                (events || !tag.eq_ignore_ascii_case("+-EVENT"))
                    && (ownership || !tag.eq_ignore_ascii_case("+-OWNER"))
            });
        };
        // Keeping both strips nothing: the bricks stay shared, uncopied.
        if !events || !ownership {
            crate::update_bricks(&mut world.bricks, strip);
            world.unloaded.iter_mut().for_each(strip);
        }
        if !ownership {
            world.owners.clear();
        } else {
            // Only the owners whose bricks are saved travel with the build.
            let used: std::collections::BTreeSet<OwnerId> = world
                .bricks
                .values()
                .chain(&world.unloaded)
                .map(|b| b.owner)
                .collect();
            world.owners.retain(|owner, _| used.contains(owner));
        }
        let result = Self::new(world);
        result.validate()?;
        Ok(result)
    }
}

pub struct LoadPlan {
    pub(crate) base_revision: u64,
    pub(crate) first_id: BrickId,
    pub(crate) next_id: BrickId,
    pub(crate) palette: Vec<[f32; 4]>,
    pub(crate) bricks: BTreeMap<BrickId, Brick>,
    /// Owner numbers this load gives to principals new to the world.
    pub(crate) owners: BTreeMap<OwnerId, OwnerRecord>,
    pub next_owner: OwnerId,
}

impl LoadPlan {
    pub fn bricks(&self) -> &BTreeMap<BrickId, Brick> {
        &self.bricks
    }
    pub fn owners(&self) -> &BTreeMap<OwnerId, OwnerRecord> {
        &self.owners
    }
    /// Take the new owner records out, to claim them when the load starts
    /// while its bricks follow in batches.
    pub fn take_owners(&mut self) -> BTreeMap<OwnerId, OwnerRecord> {
        std::mem::take(&mut self.owners)
    }
    /// The merged palette and the prepared bricks in save order, to publish
    /// a few at a time with [`LoadPlan::batch`].
    pub fn into_parts(self) -> (Vec<[f32; 4]>, Vec<Brick>) {
        (self.palette, self.bricks.into_values().collect())
    }
    /// The next bricks of a prepared load against the world as it is now.
    /// The world palette may only have been extended by this same load.
    pub fn batch(
        target: &World,
        palette: &[[f32; 4]],
        bricks: Vec<Brick>,
        next_owner: OwnerId,
    ) -> Result<Self> {
        ensure!(
            palette.starts_with(&target.palette),
            "The colorset changed while loading"
        );
        ensure!(
            target
                .bricks
                .len()
                .checked_add(bricks.len())
                .is_some_and(|n| n <= crate::MAX_BRICKS),
            "Loaded build exceeds world brick limit"
        );
        let next_id = target
            .next_brick_id
            .checked_add(bricks.len() as u64)
            .context("Brick IDs exhausted")?;
        target
            .revision
            .checked_add(1)
            .context("World revision exhausted")?;
        Ok(Self {
            base_revision: target.revision,
            first_id: target.next_brick_id,
            next_id,
            palette: palette.to_vec(),
            bricks: (target.next_brick_id..).zip(bricks).collect(),
            owners: BTreeMap::new(),
            next_owner,
        })
    }
    pub fn prepare(
        target: &World,
        build: SavedBuild,
        load_owner: OwnerId,
        preserve_ownership: bool,
        next_owner: OwnerId,
    ) -> Result<Self> {
        let mut mapping =
            LoadMapping::new(target, &build, load_owner, preserve_ownership, next_owner)?;
        let next_id = target
            .next_brick_id
            .checked_add((build.world.bricks.len() + build.world.unloaded.len()) as u64)
            .context("Brick IDs exhausted")?;
        let mut bricks = BTreeMap::new();
        // Bricks the source world could not place are offered again: this
        // server may have their definitions.
        let saved = build.world.bricks.into_iter().map(|(_, b)| b);
        for (offset, brick) in saved.chain(build.world.unloaded).enumerate() {
            bricks.insert(target.next_brick_id + offset as u64, mapping.brick(brick)?);
        }
        let new_owners = mapping.take_owners();
        let (palette, next_owner) = (mapping.palette, mapping.next_owner);
        Ok(Self {
            base_revision: target.revision,
            first_id: target.next_brick_id,
            next_id,
            palette,
            bricks,
            owners: new_owners,
            next_owner,
        })
    }
}

/// v20's `colorMatch`: two colours are the same when every component is
/// within 0.005. Saves hold colours rounded to 8 bits and printed with six
/// decimals (the default set's 0.9 red saves as 0.898039), so exact
/// equality would call a save's own colours new.
pub fn color_match(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() <= 0.005)
}
/// A save's colour slot v20 treats as unused: alpha under 0.0001 (its
/// saves fill unused slots with `1 0 1 0`).
pub fn empty_color_slot(color: &[f32; 4]) -> bool {
    color[3] < 0.0001
}
/// `ServerLoadSaveFile_ProcessColorData` with Add More Colors: each saved
/// colour takes the first matching colour of `target` (or one appended
/// before it), else is appended. An unused slot that matches nothing is
/// not appended and, as v20 leaves it untranslated, maps to colour 0. The
/// merged set may exceed 256 colours; callers refuse that.
pub fn merge_palette(target: &[[f32; 4]], saved: &[[f32; 4]]) -> (Vec<[f32; 4]>, Vec<usize>) {
    let mut palette = target.to_vec();
    let colors = saved
        .iter()
        .map(|color| {
            if let Some(index) = palette.iter().position(|c| color_match(c, color)) {
                index
            } else if empty_color_slot(color) {
                0
            } else {
                palette.push(*color);
                palette.len() - 1
            }
        })
        .collect();
    (palette, colors)
}

/// How a save's bricks map into a target world: its colours merged into
/// the target's colorset and its builders given owner numbers there. Made
/// once when a load starts, from a read of the save; each brick is then
/// validated and mapped as it is placed ([`Self::brick`]), so a big save
/// costs nothing up front.
pub struct LoadMapping {
    /// The target colorset with the save's new colours appended.
    pub palette: Vec<[f32; 4]>,
    /// Save colour index -> merged colour index.
    colors: Vec<u8>,
    /// Save owner number -> target owner number.
    owners: BTreeMap<OwnerId, OwnerId>,
    /// Owner numbers new to the target world, claimed by these principals.
    new_owners: BTreeMap<OwnerId, OwnerRecord>,
    pub next_owner: OwnerId,
    load_owner: OwnerId,
    preserve_ownership: bool,
}
impl LoadMapping {
    pub fn new(
        target: &World,
        build: &SavedBuild,
        load_owner: OwnerId,
        preserve_ownership: bool,
        next_owner: OwnerId,
    ) -> Result<Self> {
        // Each brick is validated as it is mapped; the target world is the
        // authority's own, valid by construction.
        ensure!(
            build.schema_version == BUILD_SCHEMA,
            "Unsupported native build schema"
        );
        build.world.validate_header()?;
        ensure!(
            load_owner > 0 && next_owner > load_owner,
            "Invalid native owner allocation"
        );
        let count = build.world.bricks.len() + build.world.unloaded.len();
        ensure!(
            target
                .bricks
                .len()
                .checked_add(count)
                .is_some_and(|n| n <= crate::MAX_BRICKS),
            "Loaded build exceeds world brick limit"
        );
        ensure!(count > 0, "Build contains no bricks");
        target
            .next_brick_id
            .checked_add(count as u64)
            .context("Brick IDs exhausted")?;
        target
            .revision
            .checked_add(1)
            .context("World revision exhausted")?;
        let (palette, colors) = merge_palette(&target.palette, &build.world.palette);
        ensure!(
            palette.len() <= 256,
            "Merged colorsets exceed 256 colors; no colors were approximated"
        );
        let colors = colors.into_iter().map(|i| i as u8).collect();
        let mut mapping = Self {
            palette,
            colors,
            owners: BTreeMap::new(),
            new_owners: BTreeMap::new(),
            next_owner,
            load_owner,
            preserve_ownership,
        };
        if preserve_ownership {
            // Numbers are handed out in save order, placed bricks first.
            let saved = build.world.bricks.values().chain(&build.world.unloaded);
            for brick in saved {
                if brick.owner == 0 || mapping.owners.contains_key(&brick.owner) {
                    continue;
                }
                // A known player gets their bricks back under the number
                // they have in this world; anyone else gets a fresh number,
                // claimed by their principal when there is one.
                let record = build.world.owners.get(&brick.owner);
                let owner = match record.and_then(|r| target.owner_of(&r.principal)) {
                    Some(owner) => owner,
                    None => {
                        let owner = mapping.next_owner;
                        mapping.next_owner = owner.checked_add(1).context("Owner IDs exhausted")?;
                        if let Some(record) = record {
                            mapping.new_owners.insert(owner, record.clone());
                        }
                        owner
                    }
                };
                mapping.owners.insert(brick.owner, owner);
            }
        }
        Ok(mapping)
    }
    /// The owner numbers this load gives principals new to the world.
    pub fn take_owners(&mut self) -> BTreeMap<OwnerId, OwnerRecord> {
        std::mem::take(&mut self.new_owners)
    }
    /// The existing ownership mapping, without copying a saved brick.
    pub fn owner(&self, saved: OwnerId) -> Result<OwnerId> {
        if !self.preserve_ownership {
            Ok(self.load_owner)
        } else if saved == 0 {
            Ok(0)
        } else {
            self.owners
                .get(&saved)
                .copied()
                .context("Brick owner was not in the save")
        }
    }
    /// Validate one saved brick against its save and map its colours and
    /// owner into the target world.
    pub fn brick(&self, mut brick: Brick) -> Result<Brick> {
        // Valid against its own colorset, so the recolor stays inside the
        // merged one.
        brick.validate(self.colors.len())?;
        brick.recolor(|c| self.colors[usize::from(c)]);
        brick.owner = self.owner(brick.owner)?;
        Ok(brick)
    }
}

#[cfg(test)]
mod item_spawn_tests {
    use super::*;
    use crate::{ContentRef, ItemSpawn, SourceRecord};
    #[test]
    fn capture_and_append_retain_item_and_none_selectors_independent_of_events() {
        let mut world = World::new("items".into(), "map/test".into(), vec![[1.0; 4]]);
        for (id, item) in [
            (
                1,
                Some(ContentRef::Resolved("v20.weapon.hammeritem".into())),
            ),
            (2, None),
        ] {
            let mut b = Brick::new(
                ContentRef::Resolved("brick/test".into()),
                [id as f32, 0.0, 0.0],
                7,
            );
            b.item_spawn = ItemSpawn {
                item,
                position: 1,
                direction: 5,
                respawn_ms: 300000,
                restore: Default::default(),
            };
            b.source_records.push(SourceRecord {
                line: 1,
                text: "+-ITEM NONE\" 1 5 300000".into(),
                diagnostic: None,
            });
            world.bricks.insert(id, b);
        }
        world.next_brick_id = 3;
        let build = SavedBuild::capture(&world, false, false).unwrap();
        let restored = decode(&encode(&build).unwrap()).unwrap();
        let target = World::new("target".into(), "map/test".into(), vec![[1.0; 4]]);
        let plan = LoadPlan::prepare(&target, restored, 9, false, 10).unwrap();
        for id in [1, 2] {
            assert_eq!(plan.bricks()[&id].item_spawn, world.bricks[&id].item_spawn);
            assert_eq!(
                plan.bricks()[&id].source_records,
                world.bricks[&id].source_records
            );
            assert_eq!(plan.bricks()[&id].owner, 9);
        }
    }
}

#[cfg(test)]
mod minigame_tests {
    use super::*;

    fn build() -> SavedBuild {
        let mut world = World::new("games".into(), "map/test".into(), vec![[1.0; 4]]);
        world.bricks.insert(
            1,
            Brick::new(
                crate::ContentRef::Resolved("brick/test".into()),
                [0.0; 3],
                0,
            ),
        );
        world.next_brick_id = 2;
        SavedBuild::new(world)
    }

    #[test]
    fn a_builds_mini_game_saves_with_it_and_older_builds_load_without_one() {
        let mut saved = build();
        saved.minigame = Some(serde_json::json!({ "color": 3, "teams": [{ "name": "Red" }] }));
        let bytes = encode(&saved).unwrap();
        assert_eq!(decode(&bytes).unwrap(), saved);
        let json = serde_json::to_vec(&saved).unwrap();
        assert_eq!(decode(&json).unwrap(), saved);

        // A build saved before builds kept their mini-game: its header
        // has no such field.
        #[derive(Serialize)]
        struct OldHead {
            schema_version: u32,
            world: World,
            bricks: u64,
        }
        let (header, body) = parts(&bytes).unwrap().unwrap();
        let head = super::head(header).unwrap();
        let old = OldHead {
            schema_version: head.schema_version,
            world: head.world,
            bricks: head.bricks,
        };
        let old = zstd::bulk::compress(&rmp_serde::to_vec_named(&old).unwrap(), 3).unwrap();
        let mut file = MAGIC.to_vec();
        file.extend_from_slice(&(old.len() as u32).to_le_bytes());
        file.extend_from_slice(&old);
        file.extend_from_slice(body);
        let loaded = decode(&file).unwrap();
        assert_eq!(loaded.minigame, None);
        assert_eq!(loaded.world, saved.world);
        let mut json: serde_json::Value = serde_json::to_value(&saved).unwrap();
        json.as_object_mut().unwrap().remove("minigame");
        let loaded = decode(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(loaded.minigame, None);

        // One too big is refused.
        saved.minigame = Some(serde_json::Value::String("x".repeat(MAX_MINIGAME_BYTES)));
        assert!(saved.validate().is_err());
    }
}
