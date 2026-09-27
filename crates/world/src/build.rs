//! Native build snapshots and atomic append planning. A build is not a running
//! simulation checkpoint: queued actions are never replayed when planting it.
use crate::{Brick, BrickId, EventValue, OwnerId, OwnerRecord, World};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Leaves room for the reliable command envelope within the 64 MiB request cap.
pub const MAX_BUILD_BYTES: u64 = 63 * 1024 * 1024;
pub fn encode(build: &SavedBuild) -> Result<Vec<u8>> {
    build.validate()?;
    struct Bounded(Vec<u8>);
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) as u64 > MAX_BUILD_BYTES {
                return Err(std::io::Error::other(
                    "Native build exceeds storage/transfer limit",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Bounded(Vec::new());
    serde_json::to_writer(&mut buffer, build)?;
    Ok(buffer.0)
}
pub fn decode(bytes: &[u8]) -> Result<SavedBuild> {
    ensure!(
        bytes.len() as u64 <= MAX_BUILD_BYTES,
        "Native build exceeds transfer/storage limit"
    );
    // Parse directly from JSON: serde's untagged intermediate representation
    // cannot recover numeric brick-map keys from JSON object keys.
    let build = match serde_json::from_slice::<SavedBuild>(bytes) {
        Ok(build) => build,
        Err(build_error) => SavedBuild {
            schema_version: BUILD_SCHEMA,
            world: serde_json::from_slice::<World>(bytes).with_context(|| {
                format!("Invalid native build ({build_error}) or imported world")
            })?,
        },
    };
    build.validate()?;
    Ok(build)
}

pub const BUILD_SCHEMA: u32 = 2;

/// A saved build. Brick owners are the world's owner numbers, and the
/// world's owner table says which player (principal) each one is, so loading
/// the build on any server gives each builder's bricks back to that player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedBuild {
    pub schema_version: u32,
    pub world: World,
}
impl SavedBuild {
    pub fn new(world: World) -> Self {
        Self {
            schema_version: BUILD_SCHEMA,
            world,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == BUILD_SCHEMA,
            "Unsupported native build schema"
        );
        self.world.validate()
    }
    pub fn capture(world: &World, events: bool, ownership: bool) -> Result<Self> {
        let mut world = world.clone();
        crate::update_bricks(&mut world.bricks, |brick| {
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
        });
        if !ownership {
            world.owners.clear();
        } else {
            // Only the owners whose bricks are saved travel with the build.
            let used: std::collections::BTreeSet<OwnerId> =
                world.bricks.values().map(|b| b.owner).collect();
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
        build.validate()?;
        target.validate()?;
        ensure!(
            load_owner > 0 && next_owner > load_owner,
            "Invalid native owner allocation"
        );
        ensure!(
            target
                .bricks
                .len()
                .checked_add(build.world.bricks.len())
                .is_some_and(|n| n <= crate::MAX_BRICKS),
            "Loaded build exceeds world brick limit"
        );
        ensure!(!build.world.bricks.is_empty(), "Build contains no bricks");
        let next_id = target
            .next_brick_id
            .checked_add(build.world.bricks.len() as u64)
            .context("Brick IDs exhausted")?;
        target
            .revision
            .checked_add(1)
            .context("World revision exhausted")?;
        let mut palette = target.palette.clone();
        let mut colors = Vec::new();
        for color in &build.world.palette {
            let index = if let Some(index) = palette.iter().position(|c| c == color) {
                index
            } else {
                ensure!(
                    palette.len() < 256,
                    "Merged colorsets exceed 256 colors; no colors were approximated"
                );
                palette.push(*color);
                palette.len() - 1
            };
            colors.push(index as u8);
        }
        // Saved owner number -> owner number in the target world.
        let mut owners: BTreeMap<OwnerId, OwnerId> = BTreeMap::new();
        let mut new_owners = BTreeMap::new();
        let mut next_owner = next_owner;
        let mut bricks = BTreeMap::new();
        for (offset, mut brick) in build.world.bricks.into_iter().map(|(_, b)| b).enumerate() {
            brick.color = colors[brick.color as usize];
            for event in &mut brick.events {
                for value in &mut event.params {
                    if let EventValue::Color(color) = value {
                        *color = colors[*color as usize];
                    }
                }
            }
            brick.owner = if !preserve_ownership {
                load_owner
            } else if brick.owner == 0 {
                0
            } else if let Some(owner) = owners.get(&brick.owner) {
                *owner
            } else {
                // A known player gets their bricks back under the number
                // they have in this world; anyone else gets a fresh number,
                // claimed by their principal when there is one.
                let record = build.world.owners.get(&brick.owner);
                let owner = match record.and_then(|r| target.owner_of(&r.principal)) {
                    Some(owner) => owner,
                    None => {
                        let owner = next_owner;
                        next_owner = next_owner.checked_add(1).context("Owner IDs exhausted")?;
                        if let Some(record) = record {
                            new_owners.insert(owner, record.clone());
                        }
                        owner
                    }
                };
                owners.insert(brick.owner, owner);
                owner
            };
            brick.validate(palette.len())?;
            bricks.insert(target.next_brick_id + offset as u64, brick);
        }
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
