//! Linked bricks ([`bri_content::brick::Link`]): which placed brick each
//! leads to, and its open sides in world space. A pure function of the
//! world's bricks, so the host (crossing) and each player's game (views,
//! prediction) agree without anything sent: two bricks of one definition
//! and one owner with the same brick name are linked, and more than two
//! form a ring in brick id order, each leading to the next, as v20
//! teledoors of one name do.
use crate::definitions::Definitions;
use bri_content::{
    brick::Face,
    passage::{Passage, Passages},
};
use bri_world::{Brick, BrickId, ContentRef, OwnerId};
use glam::{Affine3A, Vec3};
use std::collections::{BTreeMap, BTreeSet};

/// What links bricks: their definition, owner and name (ignoring case).
type Key = (String, OwnerId, String);

/// None for a brick that does not link; Some(None) for one that could but
/// has no name.
fn key(brick: &Brick, definitions: &Definitions) -> Option<Option<Key>> {
    let ContentRef::Resolved(definition) = &brick.definition else {
        return None;
    };
    definitions.entries.get(definition)?.link.as_ref()?;
    Some(
        brick
            .name
            .as_deref()
            .filter(|n| !n.is_empty())
            .map(|name| (definition.clone(), brick.owner, name.to_ascii_lowercase())),
    )
}

/// One open side of a linked brick, in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Side {
    pub brick: BrickId,
    pub partner: BrickId,
    pub face: Face,
    /// The picture: corners counterclockwise seen from in front.
    pub view: [Vec3; 4],
    /// Where bodies pass (`carry` takes them to the partner).
    pub passage: Passage,
}

/// The open sides of `brick`, linked to `partner` (both of `definitions`'
/// one linking definition).
pub fn sides(
    definitions: &Definitions,
    id: BrickId,
    brick: &Brick,
    partner_id: BrickId,
    partner: &Brick,
) -> Vec<Side> {
    let Ok(definition) = definitions.get(brick) else {
        return vec![];
    };
    let Some(link) = &definition.link else {
        return vec![];
    };
    let mesh = &definition.mesh;
    let here = Affine3A::from_mat4(brick.transform());
    let there = Affine3A::from_mat4(partner.transform());
    link.views(mesh)
        .into_iter()
        .zip(link.passages(mesh))
        .map(|(view, pass)| {
            let point = |p: Vec3| here.transform_point3(p);
            let vector = |v: Vec3| here.transform_vector3(v);
            let corners = |o: &bri_content::brick::Opening| {
                let (du, dv) = (o.u * o.half.x, o.v * o.half.y);
                // A millimetre out, so the picture draws over the brick.
                let c = o.centre + o.normal * 0.001;
                [c - du - dv, c + du - dv, c + du + dv, c - du + dv].map(point)
            };
            Side {
                brick: id,
                partner: partner_id,
                face: view.face,
                view: corners(&view),
                passage: Passage {
                    brick: id,
                    centre: point(pass.centre),
                    normal: vector(pass.normal),
                    u: vector(pass.u),
                    v: vector(pass.v),
                    half: pass.half,
                    carry: there * link.carry(mesh, pass.face) * here.inverse(),
                },
            }
        })
        .collect()
}

/// The linked bricks of a world, kept up to date as bricks change.
#[derive(Default)]
pub struct Links {
    /// Every brick of a linking definition, by what links it.
    members: BTreeMap<BrickId, Option<Key>>,
    rings: BTreeMap<Key, BTreeSet<BrickId>>,
    /// Bricks to look at again before the links are next read.
    pending: BTreeSet<BrickId>,
    /// Every open side, rebuilt after a change.
    sides: Vec<Side>,
    passages: Passages,
    /// The openings of passable bricks with no partner: closed panes.
    closed: Vec<Passage>,
    /// Changes whenever the sides do.
    generation: u64,
}
impl Links {
    /// Note that `id` may have changed (placed, removed, renamed, turned).
    /// Cheap: only bricks that are or may become links need noting, and
    /// the caller may filter by [`Self::may_link`].
    pub fn touch(&mut self, id: BrickId) {
        self.pending.insert(id);
    }
    /// Whether a brick could be linked (its definition links), or was.
    pub fn may_link(&self, id: BrickId, brick: Option<&Brick>, definitions: &Definitions) -> bool {
        self.members.contains_key(&id)
            || brick.is_some_and(|b| {
                definitions
                    .get(b)
                    .is_ok_and(|d| d.link.is_some())
            })
    }
    /// Forget everything and read `bricks` whole.
    pub fn reset(&mut self, bricks: &bri_world::Bricks, definitions: &Definitions) {
        self.members.clear();
        self.rings.clear();
        self.pending.clear();
        let has_links = definitions.entries.values().any(|d| d.link.is_some());
        if has_links {
            for (id, brick) in bricks {
                if let Some(key) = key(brick, definitions) {
                    self.join(*id, key);
                }
            }
        }
        self.rebuild(bricks, definitions);
    }
    /// Apply the bricks noted since the last read. Returns whether any
    /// side changed.
    pub fn flush(&mut self, bricks: &bri_world::Bricks, definitions: &Definitions) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        for id in std::mem::take(&mut self.pending) {
            if let Some(Some(old)) = self.members.remove(&id)
                && let Some(ring) = self.rings.get_mut(&old)
            {
                ring.remove(&id);
                if ring.is_empty() {
                    self.rings.remove(&old);
                }
            }
            if let Some(key) = bricks.get(&id).and_then(|b| key(b, definitions)) {
                self.join(id, key);
            }
        }
        let (sides, closed) = (std::mem::take(&mut self.sides), std::mem::take(&mut self.closed));
        self.rebuild(bricks, definitions);
        sides != self.sides || closed != self.closed
    }
    fn join(&mut self, id: BrickId, key: Option<Key>) {
        if let Some(key) = &key {
            self.rings.entry(key.clone()).or_default().insert(id);
        }
        self.members.insert(id, key);
    }
    fn rebuild(&mut self, bricks: &bri_world::Bricks, definitions: &Definitions) {
        self.sides.clear();
        self.closed.clear();
        for ring in self.rings.values() {
            if ring.len() < 2 {
                continue;
            }
            let ids: Vec<BrickId> = ring.iter().copied().collect();
            for (i, id) in ids.iter().enumerate() {
                let partner = ids[(i + 1) % ids.len()];
                let (Some(brick), Some(other)) = (bricks.get(id), bricks.get(&partner)) else {
                    continue;
                };
                self.sides
                    .extend(sides(definitions, *id, brick, partner, other));
            }
        }
        let passes = |s: &&Side| {
            bricks
                .get(&s.brick)
                .and_then(|b| definitions.get(b).ok())
                .and_then(|d| d.link.as_ref())
                .is_some_and(|l| l.pass)
        };
        let list = self.sides.iter().filter(passes).map(|s| s.passage).collect();
        // Passable bricks with no partner are shut: their openings are
        // panes bodies stop at.
        for id in self.members.keys() {
            if self.partner(*id).is_none()
                && let Some(brick) = bricks.get(id)
                && definitions
                    .get(brick)
                    .ok()
                    .and_then(|d| d.link.as_ref())
                    .is_some_and(|l| l.pass)
            {
                self.closed
                    .extend(sides(definitions, *id, brick, *id, brick).iter().map(|s| s.passage));
            }
        }
        self.passages = Passages {
            list,
            closed: self.closed.clone(),
        };
        self.generation = self.generation.wrapping_add(1);
    }
    /// The brick `id` leads to, if it is linked.
    pub fn partner(&self, id: BrickId) -> Option<BrickId> {
        let ring = self.rings.get(self.members.get(&id)?.as_ref()?)?;
        if ring.len() < 2 {
            return None;
        }
        ring.range(id + 1..)
            .next()
            .or_else(|| ring.iter().next())
            .copied()
    }
    /// Every open side of every linked brick.
    pub fn sides(&self) -> &[Side] {
        &self.sides
    }
    /// The openings bodies pass through.
    pub fn passages(&self) -> &Passages {
        &self.passages
    }
    /// The openings of passable bricks with no partner, which stay shut.
    pub fn closed(&self) -> &[Passage] {
        &self.closed
    }
    /// Changes whenever the sides do (only after a flush or reset).
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Bricks of linking definitions, linked or not.
    pub fn members(&self) -> impl Iterator<Item = BrickId> + '_ {
        self.members.keys().copied()
    }
}
