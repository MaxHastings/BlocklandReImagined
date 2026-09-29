//! Bricks on the wire, packed ([`bri_world::packed`]): world chunks,
//! update changes and build uploads.
use bri_world::{Brick, BrickId, packed::Packed};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use std::collections::BTreeMap;

/// `#[serde(with)]` for a list of bricks (a world chunk), in any id order.
pub mod bricks {
    use super::*;
    pub fn serialize<S: Serializer>(bricks: &[(BrickId, Brick)], s: S) -> Result<S::Ok, S::Error> {
        Packed::pack(bricks.iter().map(|(id, b)| (*id, Some(b)))).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(BrickId, Brick)>, D::Error> {
        Packed::deserialize(d)?
            .unpack(crate::protocol::WORLD_CHUNK)
            .map_err(D::Error::custom)?
            .into_iter()
            .map(|(id, brick)| brick.map(|b| (id, b)))
            .collect::<Option<_>>()
            .ok_or_else(|| D::Error::custom("Removal in a world chunk"))
    }
}

/// `#[serde(with)]` for an update's changed bricks (`None` removes one).
pub mod changes {
    use super::*;
    pub fn serialize<S: Serializer>(
        bricks: &BTreeMap<BrickId, Option<Brick>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        Packed::pack(bricks.iter().map(|(id, b)| (*id, b.as_ref()))).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<BrickId, Option<Brick>>, D::Error> {
        let entries = Packed::deserialize(d)?
            .unpack(bri_world::MAX_BRICKS)
            .map_err(D::Error::custom)?;
        let count = entries.len();
        let map: BTreeMap<_, _> = entries.into_iter().collect();
        if map.len() != count {
            return Err(D::Error::custom("Repeated brick in an update"));
        }
        Ok(map)
    }
}

/// A build's bricks, packed and carried beside a `LoadBuild` command whose
/// build travels without them. Kept whole: source records included.
#[derive(Debug, Serialize, Deserialize)]
pub struct Upload {
    bricks: Packed,
    /// Bricks the saving server had no definition for, numbered in order.
    unloaded: Packed,
}
impl Upload {
    /// Move `build`'s bricks out into an upload.
    pub fn take(build: &mut bri_world::build::SavedBuild) -> Self {
        let bricks = std::mem::take(&mut build.world.bricks);
        let unloaded = std::mem::take(&mut build.world.unloaded);
        Self {
            bricks: Packed::pack(bricks.iter().map(|(id, b)| (*id, Some(b)))),
            unloaded: Packed::pack(
                unloaded
                    .iter()
                    .enumerate()
                    .map(|(i, b)| (i as BrickId + 1, Some(b))),
            ),
        }
    }
    /// Put the bricks back into the build they were taken from.
    pub fn restore(self, build: &mut bri_world::build::SavedBuild) -> anyhow::Result<()> {
        anyhow::ensure!(
            build.world.bricks.is_empty() && build.world.unloaded.is_empty(),
            "An uploaded build also carries bricks of its own"
        );
        let bricks = self.bricks.unpack(bri_world::MAX_BRICKS).map_err(anyhow::Error::msg)?;
        let unloaded = self.unloaded.unpack(bri_world::MAX_BRICKS).map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            bricks.len() + unloaded.len() <= bri_world::MAX_BRICKS,
            "Uploaded build exceeds the world brick limit"
        );
        for (id, brick) in bricks {
            let brick = brick.ok_or_else(|| anyhow::anyhow!("Removal in an uploaded build"))?;
            anyhow::ensure!(
                build.world.bricks.insert(id, brick).is_none(),
                "Repeated brick in an uploaded build"
            );
        }
        for (_, brick) in unloaded {
            build.world.unloaded.push(
                brick.ok_or_else(|| anyhow::anyhow!("Removal in an uploaded build"))?,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_world::{ContentRef, Emitter, Light, SourceRecord};
    fn sample() -> Vec<(BrickId, Brick)> {
        let definition = |n: u32| ContentRef::Resolved(format!("v20/brick/brick{n}x1data"));
        let mut out = Vec::new();
        for i in 0..200u64 {
            let mut b = Brick::new(
                definition((i % 3) as u32),
                [i as f32 * 0.5, 0.1 + (i % 7) as f32 * 0.2, -3.25],
                if i < 150 { 7 } else { 9 },
            );
            b.color = (i % 40) as u8;
            b.quarter_turns = (i % 4) as u8;
            b.base_plate = i % 5 == 0;
            b.raycast = i % 6 != 0;
            b.colliding = i % 7 != 0;
            b.visible = i % 8 != 0;
            b.color_effect = (i % 7) as u8;
            b.shape_effect = (i % 3) as u8;
            if i % 11 == 0 {
                b.print = Some(ContentRef::Unresolved {
                    namespace: "print".into(),
                    name: format!("Letters/{}", i % 4),
                });
            }
            if i % 13 == 0 {
                b.name = Some(format!("brick{i}"));
                b.light = Some(Light {
                    asset: ContentRef::Resolved("light".into()),
                    enabled: true,
                });
            }
            if i % 5 == 1 {
                b.source_records.push(SourceRecord {
                    line: i as u32,
                    text: format!("+-OWNER {i}"),
                    diagnostic: None,
                });
            }
            if i % 17 == 0 {
                b.emitter = Some(Emitter {
                    asset: None,
                    direction: 2,
                });
            }
            // Ids out of order, as a nearest-first chunk sends them.
            out.push((1000 - i * 3, b));
        }
        out
    }
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Chunk(#[serde(with = "bricks")] Vec<(BrickId, Brick)>);
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Update(#[serde(with = "changes")] BTreeMap<BrickId, Option<Brick>>);

    #[test]
    fn bricks_round_trip_exactly_in_any_order() {
        let chunk = Chunk(sample());
        let bytes = rmp_serde::to_vec_named(&chunk).unwrap();
        assert_eq!(rmp_serde::from_slice::<Chunk>(&bytes).unwrap(), chunk);
        let mut update: BTreeMap<_, _> = sample().into_iter().map(|(id, b)| (id, Some(b))).collect();
        update.insert(5, None);
        let update = Update(update);
        let bytes = rmp_serde::to_vec_named(&update).unwrap();
        assert_eq!(rmp_serde::from_slice::<Update>(&bytes).unwrap(), update);
        let named: usize = sample()
            .iter()
            .map(|b| rmp_serde::to_vec_named(b).unwrap().len())
            .sum();
        let packed = rmp_serde::to_vec_named(&Chunk(sample())).unwrap().len();
        assert!(packed * 4 < named, "{packed} packed vs {named} named");
    }
    #[test]
    fn an_upload_restores_the_build_exactly() {
        let mut world = bri_world::World::new("Upload".into(), "map".into(), vec![[1.0; 4]]);
        for (id, mut brick) in sample() {
            brick.color = 0;
            world.bricks.insert(id, brick);
        }
        world.unloaded = sample().into_iter().take(5).map(|(_, b)| b).collect();
        let original = bri_world::build::SavedBuild::new(world);
        let mut build = original.clone();
        let upload = Upload::take(&mut build);
        assert!(build.world.bricks.is_empty() && build.world.unloaded.is_empty());
        let bytes = rmp_serde::to_vec_named(&upload).unwrap();
        let upload: Upload = rmp_serde::from_slice(&bytes).unwrap();
        upload.restore(&mut build).unwrap();
        assert_eq!(build, original);
    }
}
