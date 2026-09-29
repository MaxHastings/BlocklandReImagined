//! Compact brick encoding for world transfers and updates.
//!
//! Most bricks are plain: a definition, a position, a turn, a colour, a few
//! flags, an owner and maybe a print. Written as named MessagePack each one
//! repeats twenty field names and its definition's full id. Here a batch of
//! bricks travels as columns instead: every content id once in a table, id
//! gaps, definition indices, positions split into byte planes (so the zstd
//! frame around them finds the repeats), three bytes of colour and flags,
//! and owners as runs. A plain brick's source records (the private original
//! save lines a build upload keeps) ride alongside it. Bricks with anything
//! more (a name, events, lights, emitters, items, sounds, vehicles, a block
//! look) travel whole, so every brick round-trips exactly.
//!
//! Decoding checks every length and index before it allocates or indexes.
use bri_world::{Brick, BrickId, ContentRef, ItemSpawn, OwnerId, SourceRecord};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use std::collections::BTreeMap;

const PLAIN: u8 = 0;
const WHOLE: u8 = 1;
const REMOVED: u8 = 2;

const TURNS: u8 = 0b11;
const BASE_PLATE: u8 = 1 << 2;
const RAYCAST: u8 = 1 << 3;
const COLLIDING: u8 = 1 << 4;
const VISIBLE: u8 = 1 << 5;
const PRINTED: u8 = 1 << 6;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Packed {
    /// What each entry is: plain, whole or removed.
    #[serde(with = "serde_bytes")]
    kinds: Vec<u8>,
    /// Each entry's id minus the one before it (the first minus zero).
    ids: Vec<i64>,
    /// Content ids the plain bricks name, each once.
    names: Vec<ContentRef>,
    /// Per plain brick: its definition's index in `names`.
    definitions: Vec<u32>,
    /// Per plain brick: x, y and z as little-endian f32, stored as twelve
    /// planes of one byte from every brick each.
    #[serde(with = "serde_bytes")]
    positions: Vec<u8>,
    /// Per plain brick: colour, flags (turns, base plate, raycast,
    /// colliding, visible, printed), colour effect | shape effect << 3.
    #[serde(with = "serde_bytes")]
    looks: Vec<u8>,
    /// Plain bricks' owners as (owner, count) runs.
    owners: Vec<(OwnerId, u32)>,
    /// Per printed plain brick: its print's index in `names`.
    prints: Vec<u32>,
    /// The whole bricks, in order.
    whole: Vec<Brick>,
    /// Source records of plain bricks that have them, by entry index.
    records: Vec<(u32, Vec<SourceRecord>)>,
}

fn plain(brick: &Brick) -> bool {
    brick.name.is_none()
        && brick.light.is_none()
        && brick.emitter.is_none()
        && brick.item_spawn == ItemSpawn::default()
        && brick.sound.is_none()
        && brick.vehicle.is_none()
        && brick.events.is_empty()
        && brick.look.is_none()
        && brick.quarter_turns <= TURNS
        && brick.color_effect < 8
        && brick.shape_effect < 32
}

#[derive(Default)]
struct Names {
    table: Vec<ContentRef>,
    index: std::collections::HashMap<ContentRef, u32>,
}
impl Names {
    fn of(&mut self, name: &ContentRef) -> u32 {
        if let Some(i) = self.index.get(name) {
            return *i;
        }
        let i = self.table.len() as u32;
        self.table.push(name.clone());
        self.index.insert(name.clone(), i);
        i
    }
}

impl Packed {
    fn pack<'a>(entries: impl ExactSizeIterator<Item = (BrickId, Option<&'a Brick>)>) -> Self {
        let mut out = Packed {
            kinds: Vec::with_capacity(entries.len()),
            ids: Vec::with_capacity(entries.len()),
            ..Default::default()
        };
        let mut names = Names::default();
        let mut positions: Vec<[u8; 12]> = Vec::with_capacity(entries.len());
        let mut previous = 0u64;
        for (entry, (id, brick)) in entries.enumerate() {
            out.ids.push(id.wrapping_sub(previous) as i64);
            previous = id;
            let Some(brick) = brick else {
                out.kinds.push(REMOVED);
                continue;
            };
            if !plain(brick) {
                out.kinds.push(WHOLE);
                out.whole.push(brick.clone());
                continue;
            }
            out.kinds.push(PLAIN);
            out.definitions.push(names.of(&brick.definition));
            let mut bytes = [0; 12];
            for (axis, value) in brick.position.iter().enumerate() {
                bytes[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
            positions.push(bytes);
            let mut flags = brick.quarter_turns & TURNS;
            for (set, bit) in [
                (brick.base_plate, BASE_PLATE),
                (brick.raycast, RAYCAST),
                (brick.colliding, COLLIDING),
                (brick.visible, VISIBLE),
                (brick.print.is_some(), PRINTED),
            ] {
                if set {
                    flags |= bit;
                }
            }
            out.looks
                .extend([brick.color, flags, brick.color_effect | brick.shape_effect << 3]);
            match out.owners.last_mut() {
                Some((owner, run)) if *owner == brick.owner && *run < u32::MAX => *run += 1,
                _ => out.owners.push((brick.owner, 1)),
            }
            if let Some(print) = &brick.print {
                out.prints.push(names.of(print));
            }
            if !brick.source_records.is_empty() {
                out.records
                    .push((entry as u32, brick.source_records.clone()));
            }
        }
        out.names = names.table;
        out.positions = vec![0; positions.len() * 12];
        let count = positions.len();
        for (i, bytes) in positions.iter().enumerate() {
            for (plane, byte) in bytes.iter().enumerate() {
                out.positions[plane * count + i] = *byte;
            }
        }
        out
    }

    /// Entries in order; `None` is a removal. At most `limit` entries.
    fn unpack(self, limit: usize) -> Result<Vec<(BrickId, Option<Brick>)>, String> {
        let n = self.kinds.len();
        let plain = self.kinds.iter().filter(|k| **k == PLAIN).count();
        let whole = self.kinds.iter().filter(|k| **k == WHOLE).count();
        let printed = self
            .looks
            .chunks(3)
            .filter(|look| look.get(1).is_some_and(|flags| flags & PRINTED != 0))
            .count();
        let runs = self
            .owners
            .iter()
            .try_fold(0u64, |sum, (_, run)| sum.checked_add(u64::from(*run)));
        if n > limit
            || self.kinds.iter().any(|k| *k > REMOVED)
            || self.ids.len() != n
            || self.definitions.len() != plain
            || self.positions.len() != plain * 12
            || self.looks.len() != plain * 3
            || runs != Some(plain as u64)
            || self.prints.len() != printed
            || self.whole.len() != whole
            || self.records.windows(2).any(|w| w[0].0 >= w[1].0)
            || self
                .records
                .iter()
                .any(|(entry, _)| self.kinds.get(*entry as usize) != Some(&PLAIN))
            || self.names.len() > plain + printed
            || self
                .definitions
                .iter()
                .chain(&self.prints)
                .any(|i| *i as usize >= self.names.len())
        {
            return Err("Invalid packed bricks".into());
        }
        let mut out = Vec::with_capacity(n);
        let mut whole = self.whole.into_iter();
        let mut owners = self
            .owners
            .iter()
            .flat_map(|(owner, run)| std::iter::repeat_n(*owner, *run as usize));
        let (mut p, mut printed) = (0, 0);
        let mut id = 0u64;
        let mut records = self.records.into_iter().peekable();
        for (entry, (kind, gap)) in self.kinds.iter().zip(&self.ids).enumerate() {
            id = id.wrapping_add(*gap as u64);
            let brick = match *kind {
                REMOVED => None,
                WHOLE => whole.next(),
                _ => {
                    let plane = |axis: usize| {
                        let byte = |k: usize| self.positions[(axis * 4 + k) * plain + p];
                        f32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)])
                    };
                    let look = &self.looks[p * 3..p * 3 + 3];
                    let flags = look[1];
                    let mut brick = Brick::new(
                        self.names[self.definitions[p] as usize].clone(),
                        [plane(0), plane(1), plane(2)],
                        owners.next().unwrap_or_default(),
                    );
                    brick.color = look[0];
                    brick.quarter_turns = flags & TURNS;
                    brick.base_plate = flags & BASE_PLATE != 0;
                    brick.raycast = flags & RAYCAST != 0;
                    brick.colliding = flags & COLLIDING != 0;
                    brick.visible = flags & VISIBLE != 0;
                    brick.color_effect = look[2] & 0b111;
                    brick.shape_effect = look[2] >> 3;
                    if flags & PRINTED != 0 {
                        brick.print = Some(self.names[self.prints[printed] as usize].clone());
                        printed += 1;
                    }
                    if records.peek().is_some_and(|(at, _)| *at as usize == entry) {
                        brick.source_records = records.next().map(|(_, r)| r).unwrap_or_default();
                    }
                    p += 1;
                    Some(brick)
                }
            };
            out.push((id, brick));
        }
        Ok(out)
    }
}

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
    use bri_world::{Emitter, Light};
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
                b.print = Some(ContentRef::unresolved("print", format!("Letters/{}", i % 4)));
            }
            if i % 13 == 0 {
                b.name = Some(format!("brick{i}"));
                b.light = Some(Box::new(Light {
                    asset: ContentRef::Resolved("light".into()),
                    enabled: true,
                }));
            }
            if i % 5 == 1 {
                b.source_records.push(SourceRecord {
                    line: i as u32,
                    text: format!("+-OWNER {i}"),
                    diagnostic: None,
                });
            }
            if i % 17 == 0 {
                b.emitter = Some(Box::new(Emitter {
                    asset: None,
                    direction: 2,
                }));
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
    #[test]
    fn hostile_packings_are_refused() {
        let good = Packed::pack(sample().iter().map(|(id, b)| (*id, Some(b))));
        let tamper = |f: &dyn Fn(&mut Packed)| {
            let mut p = Packed::pack(sample().iter().map(|(id, b)| (*id, Some(b))));
            f(&mut p);
            p.unpack(WORLD_LIMIT).is_err()
        };
        const WORLD_LIMIT: usize = 4096;
        assert!(good.unpack(WORLD_LIMIT).is_ok());
        assert!(tamper(&|p| {
            p.kinds.push(PLAIN);
        }));
        assert!(tamper(&|p| {
            p.kinds[0] = 9;
        }));
        assert!(tamper(&|p| {
            p.definitions[0] = u32::MAX;
        }));
        assert!(tamper(&|p| {
            p.positions.pop();
        }));
        assert!(tamper(&|p| {
            p.owners[0].1 += 1;
        }));
        assert!(tamper(&|p| {
            p.owners.push((1, u32::MAX));
        }));
        assert!(tamper(&|p| {
            p.prints.push(0);
        }));
        assert!(tamper(&|p| {
            p.whole.pop();
        }));
        assert!(tamper(&|p| {
            p.looks[1] ^= PRINTED;
        }));
        assert!(tamper(&|p| {
            p.records.reverse();
        }));
        assert!(tamper(&|p| {
            p.records[0].0 = 1_000_000;
        }));
        let p = Packed::pack(sample().iter().map(|(id, b)| (*id, Some(b))));
        assert!(p.unpack(10).is_err(), "Over the entry limit");
    }
}
