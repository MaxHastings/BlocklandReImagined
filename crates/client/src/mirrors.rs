//! Mirror bricks: where the world's reflective bricks put their mirrors.
//! A brick definition's `reflection` (an Add-On's) names its mirrored
//! sides; this keeps each placed brick's mirrors in world space as the
//! replica changes, for `bri_render::reflection` to draw. A mirror brick
//! knocked out keeps its mirrors while its debris tumbles and fades (see
//! [`debris`]). Purely local: nothing here is simulated or sent.
//!
//! Linked bricks (`link`, portals) are windows drawn the same way: each open
//! side shows the view out of its partner, or plain glass when unlinked.
//! Their sides come from the one [`Links`] the local copy of the world
//! keeps (`Motion::collision`), the same openings bodies pass through, so a
//! window shows exactly where a body would go.
use bri_net::protocol::PublicWorld;
use bri_render::reflection::{Looks, Mirror};
use bri_sim::{
    definitions::Definitions,
    links::{Links, Side},
};
use glam::{Mat4, Vec3};
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
/// Mirror shapes by brick definition id, and the definitions that link.
#[derive(Clone, Default)]
pub struct MirrorShapes {
    pub mirrors: BTreeMap<String, MirrorShape>,
    pub links: Definitions,
}
impl MirrorShapes {
    pub fn is_empty(&self) -> bool {
        self.mirrors.is_empty() && self.links.entries.is_empty()
    }
    pub fn get(&self, definition: &str) -> Option<&MirrorShape> {
        self.mirrors.get(definition)
    }
}

/// Most debris mirrors drawn at once, nearest the eye first. A chain kill
/// of a mirror wall throws hundreds; beyond these the far ones show no
/// mirror, so debris never crowds out the planted mirrors' planes.
pub const MAX_DEBRIS_MIRRORS: usize = 64;
/// Fainter than this, fading debris shows no mirror.
const DEBRIS_FAINT: f32 = 0.02;

/// Whether any debris is a mirror brick's.
pub fn debris_reflects(debris: &crate::brick_debris::BrickDebris, shapes: &MirrorShapes) -> bool {
    !shapes.mirrors.is_empty()
        && debris
            .instances()
            .any(|(look, t)| t.tint[3] > DEBRIS_FAINT && shapes.get(&look.definition).is_some())
}

/// The mirrors of knocked-out mirror bricks, posed with their debris and
/// fading with it, nearest `eye` first: at most [`MAX_DEBRIS_MIRRORS`]
/// bricks' worth, appended to `out`. The reflection renderer plans every
/// frame from scratch, so a tumbling mirror reflects what it faces now.
pub fn debris(
    debris: &crate::brick_debris::BrickDebris,
    shapes: &MirrorShapes,
    eye: Vec3,
    out: &mut Vec<Mirror>,
) {
    if shapes.mirrors.is_empty() {
        return;
    }
    let mut pieces: Vec<(f32, &MirrorShape, Mat4, f32)> = debris
        .instances()
        .filter(|(_, t)| t.tint[3] > DEBRIS_FAINT)
        .filter_map(|(look, t)| {
            let shape = shapes.get(&look.definition)?;
            let distance = t.transform.w_axis.truncate().distance_squared(eye);
            Some((distance, shape, t.transform, t.tint[3]))
        })
        .collect();
    if pieces.len() > MAX_DEBRIS_MIRRORS {
        pieces.select_nth_unstable_by(MAX_DEBRIS_MIRRORS, |a, b| a.0.total_cmp(&b.0));
        pieces.truncate(MAX_DEBRIS_MIRRORS);
    }
    for (_, shape, pose, fade) in pieces {
        out.extend(shape.quads.iter().map(|quad| {
            Mirror::reflecting(
                quad.map(|p| pose.transform_point3(p)),
                shape.tint,
                shape.strength * fade,
            )
        }));
    }
}

/// The definitions with mirrors or links, checked when the catalog loaded.
pub fn shapes(definitions: &Definitions) -> MirrorShapes {
    let links = Definitions {
        entries: definitions
            .entries
            .iter()
            .filter(|(_, d)| d.link.is_some())
            .map(|(id, d)| (id.clone(), d.clone()))
            .collect(),
    };
    let mirrors = definitions
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
        .collect();
    MirrorShapes { mirrors, links }
}

/// How deep a window the eye is about to pass through is drawn, and how
/// close in front the eye must be for it.
const RECESS: f32 = 0.2;
const RECESS_REACH: f32 = 0.25;

/// The mirrors of every shown brick in the last synced replica.
#[derive(Default)]
pub struct MirrorIndex {
    source: Option<Arc<PublicWorld>>,
    log: Option<(Arc<crate::network::WorldLog>, u64)>,
    mirrors: BTreeMap<u64, Vec<Mirror>>,
    /// The linked bricks' sides the windows were built from.
    sides: Vec<Side>,
    /// The linked bricks' windows, rebuilt when the sides change.
    windows: Vec<(u64, Mirror, bri_content::passage::Passage)>,
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
        if shapes.mirrors.is_empty() {
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
        // A side's brick may have been shown or hidden.
        if !self.sides.is_empty() {
            self.windows = windows(&self.sides, world, shapes);
        }
        self.source = Some(world.clone());
    }
    /// Take the linked bricks' sides from `links` (the local world's, which
    /// bodies pass through), rebuilding the windows when they changed.
    pub fn link(&mut self, links: &Links, shapes: &MirrorShapes) {
        if links.sides() == self.sides.as_slice() {
            return;
        }
        self.sides = links.sides().to_vec();
        self.windows = match &self.source {
            Some(world) => windows(&self.sides, world, shapes),
            None => Vec::new(),
        };
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
            .map(|quad| {
                Mirror::reflecting(
                    quad.map(|p| placement.transform_point3(p)),
                    shape.tint,
                    shape.strength,
                )
            })
            .collect();
        self.mirrors.insert(id, mirrors);
    }
    pub fn is_empty(&self) -> bool {
        self.mirrors.is_empty() && self.windows.is_empty()
    }
    /// Every mirror and window, less those of bricks `hidden` says are gone
    /// (knocked out and still tumbling as debris). A window the `eye` is
    /// about to pass through is drawn recessed so it never clips away.
    pub fn mirrors(&self, hidden: impl Fn(u64) -> bool, eye: Vec3) -> Vec<Mirror> {
        let windows =
            self.windows
                .iter()
                .filter(|(id, ..)| !hidden(*id))
                .map(|(_, mirror, passage)| {
                    let mut mirror = *mirror;
                    let side = passage.side(eye);
                    // Strictly in front, as an eye not yet carried is: one
                    // just carried out of a doorway sits a hair behind the
                    // back-to-back window it came out of, whose box would
                    // otherwise stand in front of it.
                    if matches!(mirror.looks, Looks::Through(_))
                        && side > 0.0
                        && side < RECESS_REACH
                        && passage.within(eye, 0.0)
                    {
                        mirror.recess = RECESS;
                    }
                    mirror
                });
        self.mirrors
            .iter()
            .filter(|(id, _)| !hidden(**id))
            .flat_map(|(_, mirrors)| mirrors.iter().copied())
            .chain(windows)
            .collect()
    }
    pub fn clear(&mut self) {
        self.source = None;
        self.log = None;
        self.mirrors.clear();
        self.sides.clear();
        self.windows.clear();
    }
}

/// Each side of every linked brick as a window: the view out of its
/// partner, or the brick's own glass when it leads nowhere. A linked side
/// shows its view even on a brick not drawn (`visible` off): what lies
/// through it is the partner's side of the world, and bodies still pass,
/// so the opening is a seamless hole. Unlinked glass is the brick's own
/// look, drawn only when the brick is.
fn windows(
    sides: &[Side],
    world: &PublicWorld,
    shapes: &MirrorShapes,
) -> Vec<(u64, Mirror, bri_content::passage::Passage)> {
    sides
        .iter()
        .filter_map(|side| {
            let brick = world.bricks.get(&side.brick)?;
            let definition = shapes.links.get(brick).ok()?;
            let link = definition.link.as_ref()?;
            let glass = definition.glass;
            let mirror = if side.linked {
                Mirror {
                    corners: side.view,
                    tint: link.tint,
                    strength: 1.0,
                    // What lies past the partner shows behind this side.
                    looks: Looks::Through(Mat4::from(side.passage.carry.inverse())),
                    fallback: link.idle,
                    recess: 0.0,
                }
            } else if brick.visible {
                Mirror {
                    corners: side.view,
                    tint: [1.0; 3],
                    strength: glass[3],
                    looks: Looks::Plain,
                    fallback: [glass[0], glass[1], glass[2]],
                    recess: 0.0,
                }
            } else {
                return None;
            };
            Some((side.brick, mirror, side.passage))
        })
        .collect()
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
        let mirrors = BTreeMap::from([(
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
        )]);
        MirrorShapes {
            mirrors,
            links: Default::default(),
        }
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
        let mirrors = index.mirrors(|_| false, Vec3::ZERO);
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
        assert_eq!(index.mirrors(|_| false, Vec3::ZERO).len(), 1);
        assert!(index.mirrors(|id| id == 2, Vec3::ZERO).is_empty());
        let gone = world(vec![]);
        index.sync(&gone, None, &shapes);
        assert!(index.is_empty());
        // Without mirror bricks nothing is read at all.
        index.sync(&first, None, &MirrorShapes::default());
        assert!(index.is_empty());
    }

    #[test]
    fn knocked_out_mirror_bricks_reflect_on_their_debris_until_it_fades() {
        use crate::brick_debris::{BrickDebris, tests as debris_tests};
        let (building, _) = debris_tests::building(&[(7, [0.0, 0.3, 0.0]), (8, [4.0, 0.3, 0.0])]);
        // The test catalog's 1x0.6x1 "brick", mirrored on its north side.
        let quad = [
            Vec3::new(-0.5, -0.3, -0.501),
            Vec3::new(0.5, -0.3, -0.501),
            Vec3::new(0.5, 0.3, -0.501),
            Vec3::new(-0.5, 0.3, -0.501),
        ];
        let mirrored = shapes_of(BTreeMap::from([(
            "brick".to_string(),
            MirrorShape {
                quads: vec![quad],
                tint: [0.9; 3],
                strength: 1.0,
            },
        )]));
        let mut debris = BrickDebris::new();
        let mut out = Vec::new();
        debris_tests_run(&mut debris, &building, 0.0);
        super::debris(&debris, &mirrored, Vec3::ZERO, &mut out);
        assert!(out.is_empty() && !debris_reflects(&debris, &mirrored));
        // A rocket's blast throws one, a hammer knocks the other out.
        debris
            .cues(
                &[
                    debris_tests::kill(1, 7, [0.0, 0.3, 0.0], [0.0, -0.7, 0.0], 12.0, 0.0),
                    debris_tests::tool_kill(2, 8, [4.0, 0.3, 0.0]),
                ],
                &building,
            )
            .unwrap();
        debris_tests_run(&mut debris, &building, 0.3);
        assert!(debris_reflects(&debris, &mirrored));
        assert!(!debris_reflects(&debris, &MirrorShapes::default()));
        super::debris(&debris, &mirrored, Vec3::ZERO, &mut out);
        assert_eq!(out.len(), 2);
        // Each mirror rides its body: posed with it, still a flat mirror
        // on the brick's side, at full strength while solid.
        let poses: Vec<_> = debris.instances().map(|(_, t)| t.transform).collect();
        for mirror in &out {
            assert!(mirror.plane().is_some());
            assert_eq!(mirror.strength, 1.0);
            assert_eq!(mirror.tint, [0.9; 3]);
            let on = poses.iter().any(|pose| {
                mirror
                    .corners
                    .iter()
                    .zip(quad)
                    .all(|(c, q)| c.abs_diff_eq(pose.transform_point3(q), 1e-4))
            });
            assert!(on, "{mirror:?} rides no body");
        }
        // Nothing else keeps a mirror in debris: other bricks' debris
        // gives none.
        let mut none = Vec::new();
        let other = shapes_of(BTreeMap::from([(
            "other".to_string(),
            mirrored.mirrors["brick"].clone(),
        )]));
        super::debris(&debris, &other, Vec3::ZERO, &mut none);
        assert!(none.is_empty());
        // The hammered brick fades within a second or two; its mirror
        // fades with it, then goes.
        debris_tests_run(&mut debris, &building, 0.8);
        out.clear();
        super::debris(&debris, &mirrored, Vec3::ZERO, &mut out);
        assert!(out.iter().any(|m| m.strength < 1.0 && m.strength > 0.0));
        debris_tests_run(&mut debris, &building, 6.0);
        out.clear();
        super::debris(&debris, &mirrored, Vec3::ZERO, &mut out);
        assert!(out.is_empty() && !debris_reflects(&debris, &mirrored));
    }

    #[test]
    fn a_chain_kill_of_mirror_bricks_keeps_only_the_nearest_debris_mirrors() {
        use crate::brick_debris::{BrickDebris, tests as debris_tests};
        let bricks: Vec<_> = (0..200u64).map(|i| (i + 1, [i as f32, 0.3, 0.0])).collect();
        let (building, _) = debris_tests::building(&bricks);
        let shape = MirrorShape {
            quads: vec![[
                Vec3::new(-0.5, -0.3, -0.501),
                Vec3::new(0.5, -0.3, -0.501),
                Vec3::new(0.5, 0.3, -0.501),
                Vec3::new(-0.5, 0.3, -0.501),
            ]],
            tint: [1.0; 3],
            strength: 1.0,
        };
        let mirrored = shapes_of(BTreeMap::from([("brick".to_string(), shape)]));
        let mut debris = BrickDebris::new();
        let cues: Vec<_> = bricks
            .iter()
            .map(|(id, at)| debris_tests::tool_kill(*id, *id, *at))
            .collect();
        debris.cues(&cues, &building).unwrap();
        let mut out = Vec::new();
        super::debris(&debris, &mirrored, Vec3::ZERO, &mut out);
        assert_eq!(out.len(), MAX_DEBRIS_MIRRORS);
        // The nearest to the eye at x = 0.
        let far = out
            .iter()
            .map(|m| m.corners[0].x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(far < MAX_DEBRIS_MIRRORS as f32 + 1.0, "{far}");
    }

    /// Portals' windows come from the openings bodies pass through: a
    /// linked brick not drawn still shows the view its opening leads to,
    /// and only drawn unlinked bricks show glass.
    #[test]
    fn windows_follow_the_openings_bodies_pass_through() {
        use bri_content::brick::{Frame, Link};
        let mut mesh = crate::world_chunks::tests::meshes()
            .remove("definition/a")
            .unwrap();
        mesh.footprint_studs = [4, 1];
        mesh.height_plates = 15;
        let link = Link {
            faces: vec![Face::North, Face::South],
            depth: 0.5,
            inset: 0.0,
            tint: [1.0; 3],
            idle: [0.5; 3],
            pass: true,
            frame: Frame {
                sides: 0.05,
                top: 0.05,
                bottom: 0.2,
            },
            name: "Portal".into(),
        };
        let collision = bri_content::collision::CollisionBody {
            id: "definition/a".into(),
            parts: vec![bri_content::collision::Part::Box {
                center: [0.0; 3],
                size: [2.0, 3.0, 0.5],
            }],
        };
        let shape = bri_physics::content::collider(&collision)
            .unwrap()
            .build()
            .shared_shape()
            .clone();
        let definitions = Definitions {
            entries: BTreeMap::from([(
                "definition/a".to_string(),
                bri_sim::definitions::Definition {
                    mesh,
                    collision,
                    shape,
                    indestructible: false,
                    special: Default::default(),
                    reflection: None,
                    link: Some(link),
                    glass: [0.6, 0.7, 0.8, 0.5],
                },
            )]),
        };
        let shapes = super::shapes(&definitions);
        let named = |x: f32, name: Option<&str>, visible: bool| {
            let mut b = brick([x, 1.5, 0.0], 0);
            b.name = name.map(Into::into);
            b.visible = visible;
            b
        };
        // A pair, one of them not drawn; and two unpaired, one not drawn.
        let world = world(vec![
            (1, named(0.0, Some("a"), true)),
            (2, named(10.0, Some("a"), false)),
            (3, named(20.0, None, true)),
            (4, named(30.0, None, false)),
        ]);
        let mut links = Links::default();
        links.reset(&world.bricks, &definitions);
        let mut index = MirrorIndex::default();
        index.sync(&world, None, &shapes);
        index.link(&links, &shapes);
        let windows = index.mirrors(|_| false, Vec3::new(0.0, 1.5, 50.0));
        let through = windows
            .iter()
            .filter(|m| matches!(m.looks, Looks::Through(_)))
            .count();
        let glass = windows.iter().filter(|m| m.looks == Looks::Plain).count();
        // Both sides of both portals of the pair look through; only the
        // drawn unpaired one shows glass.
        assert_eq!((through, glass), (4, 2), "{windows:?}");
        // Each looking side is an opening bodies pass through.
        assert_eq!(links.passages().list.len(), 4);
        // Drawn again, the unpaired one shows its glass too.
        let mut shown = (*world).clone();
        shown.bricks.get_mut(&4).unwrap().visible = true;
        let shown = Arc::new(shown);
        index.sync(
            &shown,
            Some(&crate::network::WorldChanges {
                bricks: [4].into(),
                palette: false,
            }),
            &shapes,
        );
        let glass = index
            .mirrors(|_| false, Vec3::new(0.0, 1.5, 50.0))
            .iter()
            .filter(|m| m.looks == Looks::Plain)
            .count();
        assert_eq!(glass, 4);
    }

    fn shapes_of(mirrors: BTreeMap<String, MirrorShape>) -> MirrorShapes {
        MirrorShapes {
            mirrors,
            ..Default::default()
        }
    }

    fn debris_tests_run(
        debris: &mut crate::brick_debris::BrickDebris,
        building: &crate::building::Building,
        seconds: f32,
    ) {
        for _ in 0..(seconds * 60.0) as usize {
            debris.advance(1.0 / 60.0, building).unwrap();
        }
    }
}
