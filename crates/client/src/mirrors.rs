//! Mirror bricks: where the world's reflective bricks put their mirrors.
//! A brick definition's `reflection` (an Add-On's) names its mirrored
//! sides; this keeps each placed brick's mirrors in world space as the
//! replica changes, for `bri_render::reflection` to draw. Purely local:
//! nothing here is simulated or sent.
use bri_net::protocol::PublicWorld;
use bri_render::reflection::Mirror;
use glam::Vec3;
use std::{collections::BTreeMap, sync::Arc};

/// One definition's mirrors in the brick's own frame.
#[derive(Clone, Debug, PartialEq)]
pub struct MirrorShape {
    quads: Vec<[Vec3; 4]>,
    tint: [f32; 3],
    strength: f32,
}
impl MirrorShape {
    /// The mirror quads, counterclockwise from the reflecting side.
    pub fn quads(&self) -> &[[Vec3; 4]] {
        &self.quads
    }
}
/// Mirror shapes by brick definition id.
pub type MirrorShapes = BTreeMap<String, MirrorShape>;

/// The definitions with mirrors, checked when the catalog loaded.
pub fn shapes(definitions: &bri_sim::definitions::Definitions) -> MirrorShapes {
    definitions
        .entries
        .iter()
        .filter_map(|(id, definition)| {
            let reflection = definition.reflection.as_ref()?;
            Some((
                id.clone(),
                MirrorShape {
                    quads: reflection
                        .quads(&definition.mesh)
                        .into_iter()
                        .map(|quad| quad.map(Vec3::from))
                        .collect(),
                    tint: reflection.tint,
                    strength: reflection.strength,
                },
            ))
        })
        .collect()
}

/// The mirrors of every shown brick in the last synced replica.
#[derive(Default)]
pub struct MirrorIndex {
    source: Option<Arc<PublicWorld>>,
    log: Option<(Arc<crate::network::WorldLog>, u64)>,
    mirrors: BTreeMap<u64, Vec<Mirror>>,
}
impl MirrorIndex {
    /// Follow a replica: only the bricks its log says changed since the
    /// last sync are read again.
    pub fn follow(
        &mut self,
        world: &Arc<PublicWorld>,
        log: &Arc<crate::network::WorldLog>,
        revision: u64,
        shapes: &MirrorShapes,
    ) {
        if self.source.as_ref().is_some_and(|s| Arc::ptr_eq(s, world)) {
            return;
        }
        let known = self
            .log
            .as_ref()
            .filter(|(synced, _)| Arc::ptr_eq(synced, log))
            .and_then(|(synced, from)| synced.between(*from, revision));
        self.sync(world, known.as_ref(), shapes);
        self.log = Some((log.clone(), revision));
    }
    /// Bring the index up to `world`. `known` lists every brick that may
    /// differ from the last synced replica; without it the world is read
    /// whole (a join or reload).
    pub fn sync(
        &mut self,
        world: &Arc<PublicWorld>,
        known: Option<&crate::network::WorldChanges>,
        shapes: &MirrorShapes,
    ) {
        if shapes.is_empty() {
            self.mirrors.clear();
        } else if let (Some(_), Some(known)) = (&self.source, known) {
            for id in &known.bricks {
                self.place(*id, world.bricks.get(id), shapes);
            }
        } else {
            self.mirrors.clear();
            for (id, brick) in &world.bricks {
                self.place(*id, Some(brick), shapes);
            }
        }
        self.source = Some(world.clone());
    }
    fn place(&mut self, id: u64, brick: Option<&bri_world::Brick>, shapes: &MirrorShapes) {
        let shape = brick.filter(|b| b.visible).and_then(|brick| {
            let bri_world::ContentRef::Resolved(definition) = &brick.definition else {
                return None;
            };
            Some((brick, shapes.get(definition)?))
        });
        let Some((brick, shape)) = shape else {
            self.mirrors.remove(&id);
            return;
        };
        let placement = brick.transform();
        let mirrors = shape
            .quads
            .iter()
            .map(|quad| Mirror {
                corners: quad.map(|p| placement.transform_point3(p)),
                tint: shape.tint,
                strength: shape.strength,
            })
            .collect();
        self.mirrors.insert(id, mirrors);
    }
    pub fn is_empty(&self) -> bool {
        self.mirrors.is_empty()
    }
    /// Every mirror, less those of bricks `hidden` says are gone (knocked
    /// out and still tumbling as debris).
    pub fn mirrors(&self, hidden: impl Fn(u64) -> bool) -> Vec<Mirror> {
        self.mirrors
            .iter()
            .filter(|(id, _)| !hidden(**id))
            .flat_map(|(_, mirrors)| mirrors.iter().copied())
            .collect()
    }
    pub fn clear(&mut self) {
        self.source = None;
        self.log = None;
        self.mirrors.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::brick::{Face, Reflection};

    fn shapes() -> MirrorShapes {
        let mut mesh = crate::world_chunks::tests::meshes()
            .remove("definition/a")
            .unwrap();
        mesh.footprint_studs = [4, 1];
        mesh.height_plates = 15;
        let reflection = Reflection {
            faces: vec![Face::North],
            depth: 0.5,
            inset: 0.0,
            tint: [1.0; 3],
            strength: 1.0,
        };
        BTreeMap::from([(
            "definition/a".to_string(),
            MirrorShape {
                quads: reflection
                    .quads(&mesh)
                    .into_iter()
                    .map(|q| q.map(Vec3::from))
                    .collect(),
                tint: [1.0; 3],
                strength: 1.0,
            },
        )])
    }
    fn brick(position: [f32; 3], turns: u8) -> bri_world::Brick {
        let mut brick = bri_world::Brick::new(
            bri_world::ContentRef::Resolved("definition/a".into()),
            position,
            1,
        );
        brick.quarter_turns = turns;
        brick
    }
    fn world(bricks: Vec<(u64, bri_world::Brick)>) -> Arc<PublicWorld> {
        Arc::new(PublicWorld {
            name: String::new(),
            map_id: String::new(),
            palette: vec![[1.0; 4]],
            bricks: bricks.into_iter().collect(),
        })
    }

    #[test]
    fn placed_mirror_bricks_follow_the_replica() {
        let shapes = shapes();
        let mut index = MirrorIndex::default();
        let first = world(vec![
            (1, brick([0.0, 1.5, 0.0], 0)),
            (2, brick([4.0, 1.5, 0.0], 1)),
        ]);
        index.sync(&first, None, &shapes);
        let mirrors = index.mirrors(|_| false);
        assert_eq!(mirrors.len(), 2);
        // Unturned, the pane faces north (-z) through the brick's middle.
        let plane = mirrors[0].plane().unwrap();
        assert!(plane.truncate().abs_diff_eq(Vec3::NEG_Z, 1e-5));
        // A quarter turn faces it west or east, around the brick's centre.
        let turned = mirrors[1].plane().unwrap().truncate();
        assert!(turned.x.abs() > 0.999);
        assert!((mirrors[1].corners.iter().map(|c| c.x).sum::<f32>() / 4.0 - 4.0).abs() < 0.01);
        // Hidden, removed and knocked-out bricks show no mirror.
        let mut hidden = brick([0.0, 1.5, 0.0], 0);
        hidden.visible = false;
        let next = world(vec![(1, hidden), (2, brick([4.0, 1.5, 0.0], 1))]);
        let known = crate::network::WorldChanges {
            bricks: [1].into(),
            palette: false,
        };
        index.sync(&next, Some(&known), &shapes);
        assert_eq!(index.mirrors(|_| false).len(), 1);
        assert!(index.mirrors(|id| id == 2).is_empty());
        let gone = world(vec![]);
        index.sync(&gone, None, &shapes);
        assert!(index.is_empty());
        // Without mirror bricks nothing is read at all.
        index.sync(&first, None, &MirrorShapes::new());
        assert!(index.is_empty());
    }
}
