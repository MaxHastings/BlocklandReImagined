//! v20 eases a brick into a new paint colour instead of snapping to it.
//!
//! `blocklandv20.exe` keeps two colours per brick. `fxDTSBrick::unpackUpdate`
//! (0x541540) writes the new colour ID's palette colour to the target
//! (+0x378). The colour the brick is drawn with (+0x388) follows it once per
//! rendered frame (0x53cc90): with `dt` the seconds since the brick was last
//! drawn, clamped to 0.001..0.1, `k = 4 * dt`; `k >= 1` snaps, otherwise
//! `drawn += (target - drawn) * k` on all four channels, and `drawn` snaps once
//! every channel is within 0.01. A brick not drawn for over 0.3 s snaps.
//!
//! Every colour change reaches the client the same way, as a new colour ID,
//! so paint cans, undo, the wrench and `setColor` events all ease. It takes
//! about a second from black to white. A newly planted brick has no
//! colour to ease from and appears at once.
//!
//! The replicated chunks leave an easing brick out, and this module draws it
//! alone with its current colour, like v20 taking a changed brick out of its
//! static batch. Once settled, the chunk is rebuilt with the brick in it
//! before this drawing stops.
use anyhow::Result;
use bri_net::protocol::PublicWorld;
use bri_render::scene::{GpuScene, SceneData, SceneRenderer};
use std::collections::{BTreeMap, BTreeSet};

/// `k = RATE * dt` per frame.
pub const RATE: f32 = 4.0;
/// Frame times are clamped to this range before easing.
pub const MIN_DT: f32 = 0.001;
pub const MAX_DT: f32 = 0.1;
/// Longer than this since the last frame snaps to the target.
pub const SNAP_DT: f32 = 0.3;
/// Channels this close to the target snap.
pub const SNAP_DISTANCE: f32 = 0.01;
/// Bricks easing at once; further repaints snap, like an unrendered brick.
pub const MAX_FADES: usize = 512;

/// One frame of v20's easing. Returns the new drawn colour and whether it
/// has reached the target.
pub fn ease(drawn: [f32; 4], target: [f32; 4], dt: f32) -> ([f32; 4], bool) {
    let k = RATE * dt.clamp(MIN_DT, MAX_DT);
    let close = (0..4).all(|i| (drawn[i] - target[i]).abs() < SNAP_DISTANCE);
    if dt > SNAP_DT || k >= 1.0 || close {
        return (target, true);
    }
    (
        std::array::from_fn(|i| drawn[i] + (target[i] - drawn[i]) * k),
        false,
    )
}

#[derive(Clone, Debug, PartialEq)]
struct Fade {
    drawn: [f32; 4],
    target: [f32; 4],
    settled: bool,
}

/// Bricks easing between paint colours.
#[derive(Default)]
pub struct BrickFades {
    fades: BTreeMap<u64, Fade>,
}

impl BrickFades {
    pub fn clear(&mut self) {
        self.fades.clear();
    }
    pub fn len(&self) -> usize {
        self.fades.len()
    }
    pub fn is_empty(&self) -> bool {
        self.fades.is_empty()
    }
    /// The colour `brick` is drawn with while it eases.
    pub fn drawn(&self, brick: u64) -> Option<[f32; 4]> {
        self.fades.get(&brick).map(|f| f.drawn)
    }
    /// Stop easing every brick. Each is drawn here at its target until the
    /// chunks take it back, so nothing blinks out meanwhile.
    pub fn settle_all(&mut self) {
        for fade in self.fades.values_mut() {
            fade.drawn = fade.target;
            fade.settled = true;
        }
    }
    fn settle(&mut self, id: u64) {
        if let Some(fade) = self.fades.get_mut(&id) {
            fade.drawn = fade.target;
            fade.settled = true;
        }
    }
    /// Start easing the `changed` bricks whose paint differs between the
    /// drawn world `from` and the new world `to`. A brick already easing
    /// carries on from where it is drawn now. Bricks that moved, vanished
    /// or were hidden stop easing.
    pub fn observe(
        &mut self,
        from: &PublicWorld,
        to: &PublicWorld,
        changed: impl IntoIterator<Item = u64>,
    ) {
        for id in changed {
            let (Some(old), Some(new)) = (from.bricks.get(&id), to.bricks.get(&id)) else {
                self.settle(id);
                continue;
            };
            let (Some(before), Some(target)) = (
                from.palette.get(usize::from(old.color)),
                to.palette.get(usize::from(new.color)),
            ) else {
                self.settle(id);
                continue;
            };
            if !(old.visible && new.visible)
                || old.position != new.position
                || old.quarter_turns != new.quarter_turns
                || old.definition != new.definition
            {
                self.settle(id);
                continue;
            }
            let room = self.fades.len() < MAX_FADES;
            match self.fades.get_mut(&id) {
                Some(fade) => {
                    if fade.target != *target {
                        fade.target = *target;
                        fade.settled = false;
                    }
                }
                None if before != target && room => {
                    self.fades.insert(
                        id,
                        Fade {
                            drawn: *before,
                            target: *target,
                            settled: false,
                        },
                    );
                }
                None => {}
            }
        }
    }
    /// Bricks the chunks should leave out: every brick still easing.
    pub fn left_out(&self) -> BTreeSet<u64> {
        self.fades
            .iter()
            .filter(|(_, f)| !f.settled)
            .map(|(id, _)| *id)
            .collect()
    }
    /// Advance the bricks drawn here (`shown`, the set the applied chunks
    /// leave out) by one frame of `dt` seconds.
    pub fn advance(&mut self, dt: f32, shown: &BTreeSet<u64>) {
        for (id, fade) in &mut self.fades {
            if fade.settled || !shown.contains(id) {
                continue;
            }
            (fade.drawn, fade.settled) = ease(fade.drawn, fade.target, dt);
        }
    }
    /// The chunks now leave out `left_out`: settled bricks back in their
    /// chunks stop being drawn here.
    pub fn chunks_applied(&mut self, left_out: &BTreeSet<u64>) {
        self.fades
            .retain(|id, fade| !fade.settled || left_out.contains(id));
    }
    /// Whether the chunks leave out a different set than they should.
    pub fn needs_rebuild(&self, left_out: &BTreeSet<u64>) -> bool {
        self.left_out() != *left_out
    }
    /// Easing bricks the chunks leave out, with their drawn colours.
    pub fn shown<'a>(
        &'a self,
        left_out: &'a BTreeSet<u64>,
    ) -> impl Iterator<Item = (u64, [f32; 4])> + 'a {
        self.fades
            .iter()
            .filter(|(id, _)| left_out.contains(id))
            .map(|(id, f)| (*id, f.drawn))
    }
}

/// GPU meshes of easing bricks, one per brick, rebuilt as their colour moves.
#[derive(Default)]
pub struct FadeModels {
    models: BTreeMap<u64, FadeModel>,
}
struct FadeModel {
    gpu: GpuScene,
    drawn: [f32; 4],
    layout: Layout,
}
/// What must match for a colour change to be a vertex update only.
#[derive(PartialEq)]
struct Layout {
    vertices: usize,
    indices: usize,
    batches: Vec<usize>,
    materials: Vec<bri_render::scene::Material>,
}
impl Layout {
    fn of(data: &SceneData) -> Self {
        Self {
            vertices: data.vertices.len(),
            indices: data.indices.len(),
            batches: data.batches.iter().map(|b| b.material).collect(),
            materials: data.materials.clone(),
        }
    }
}

impl FadeModels {
    pub fn clear(&mut self) {
        self.models.clear();
    }
    /// Rebuild the meshes of the bricks `fades` draws this frame from the
    /// drawn `world`'s bricks.
    #[allow(clippy::too_many_arguments)] // GPU context plus the brick catalogs
    pub fn upload(
        &mut self,
        fades: &BrickFades,
        left_out: &BTreeSet<u64>,
        world: &PublicWorld,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
        materials: &crate::materials::BrickMaterials,
    ) -> Result<()> {
        let shown: BTreeMap<u64, [f32; 4]> = fades.shown(left_out).collect();
        self.models.retain(|id, _| shown.contains_key(id));
        for (id, drawn) in shown {
            if self.models.get(&id).is_some_and(|m| m.drawn == drawn) {
                continue;
            }
            let Some(data) = brick_scene(world, id, drawn, meshes, materials)? else {
                self.models.remove(&id);
                continue;
            };
            let layout = Layout::of(&data);
            match self.models.get_mut(&id) {
                Some(model) if model.layout == layout => {
                    let centers: Vec<_> = data.batches.iter().map(|b| b.center).collect();
                    model.gpu.update_vertices(queue, &data.vertices, &centers)?;
                    model.drawn = drawn;
                }
                _ => {
                    let gpu = renderer.upload(device, queue, &data)?;
                    self.models.insert(id, FadeModel { gpu, drawn, layout });
                }
            }
        }
        Ok(())
    }
    pub fn scenes(&self) -> impl Iterator<Item = &GpuScene> {
        self.models.values().map(|m| &m.gpu)
    }
}

/// One brick where it stands in `world`, painted `drawn`.
fn brick_scene(
    world: &PublicWorld,
    id: u64,
    drawn: [f32; 4],
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
    materials: &crate::materials::BrickMaterials,
) -> Result<Option<SceneData>> {
    let Some(brick) = world.bricks.get(&id) else {
        return Ok(None);
    };
    // Every colour the brick names, its events' included, indexes the
    // one-colour palette it is drawn with.
    let mut brick = brick.clone();
    brick.recolor(|_| 0);
    let one = PublicWorld {
        name: "Easing brick".into(),
        map_id: world.map_id.clone(),
        palette: vec![drawn.map(|v| v.clamp(0.0, 1.0))],
        bricks: bri_world::Bricks::unit(id, brick),
    };
    let data =
        crate::world_scene::build_world_scene_materials(&one, meshes, 200_000, Some(materials))?;
    Ok((!data.indices.is_empty()).then_some(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_world::{Brick, ContentRef};

    const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

    /// v20's curve at 60 fps: each frame closes 4/60 of the gap, so black to
    /// white is about two thirds of the way after a quarter second and
    /// settles, snapping the last 0.01, a little over a second in.
    #[test]
    fn colour_eases_like_v20_over_time() {
        let dt = 1.0 / 60.0;
        let mut drawn = BLACK;
        let mut frames = 0;
        let mut quarter = None;
        loop {
            let settled;
            (drawn, settled) = ease(drawn, WHITE, dt);
            frames += 1;
            if frames == 15 {
                quarter = Some(drawn[0]);
            }
            if settled {
                break;
            }
            assert!(frames < 200, "never settles");
        }
        let expected = 1.0 - (1.0 - RATE * dt).powi(15);
        assert!((quarter.unwrap() - expected).abs() < 1e-5);
        assert!((0.64..0.65).contains(&expected));
        assert_eq!(drawn, WHITE);
        // (1 - 4/60)^n < 0.01 first holds at n = 67; the snap is the frame after.
        assert_eq!(frames, 68);
        // Alpha eases too (painting a transparent colour).
        let glass = [1.0, 1.0, 1.0, 0.5];
        assert_eq!(ease(WHITE, glass, dt).0[3], 1.0 - 0.5 * RATE * dt);
    }

    #[test]
    fn frame_time_is_clamped_and_long_gaps_snap() {
        // A slow frame moves at most 0.4 of the way.
        let (slow, settled) = ease(BLACK, WHITE, 0.2);
        assert!(!settled && (slow[0] - 0.4).abs() < 1e-6);
        // A stalled brick snaps.
        assert_eq!(ease(BLACK, WHITE, 0.31), (WHITE, true));
        // A zero-length frame still moves a little.
        assert!((ease(BLACK, WHITE, 0.0).0[0] - 0.004).abs() < 1e-6);
    }

    fn world(color: u8, visible: bool) -> PublicWorld {
        let mut brick = Brick::new(ContentRef::Resolved("a".into()), [1.0; 3], 1);
        brick.color = color;
        brick.visible = visible;
        PublicWorld {
            name: "Test".into(),
            map_id: "map/test".into(),
            palette: vec![BLACK, WHITE, [1.0, 0.0, 0.0, 1.0]],
            bricks: bri_world::Bricks::unit(7, brick),
        }
    }

    /// Reported crash ("Invalid replicated brick 1590: Event color outside
    /// palette"): a repainted brick with a `setColor` event eased in a
    /// one-colour palette that its event colour indexed past.
    #[test]
    fn a_brick_with_event_colours_eases() {
        let meshes = BTreeMap::from([("a".to_string(), crate::world_scene::tests::mesh())]);
        let materials = crate::materials::BrickMaterials::in_memory();
        let mut world = world(1, true);
        let brick = world.bricks.get_mut(&7).unwrap();
        brick.events = vec![crate::world_scene::tests::set_color(2)];
        let data = brick_scene(&world, 7, WHITE, &meshes, &materials)
            .unwrap()
            .unwrap();
        assert!(data.vertices.iter().all(|v| v.color == WHITE));
    }

    #[test]
    fn repaints_ease_then_return_to_the_chunks() {
        let mut fades = BrickFades::default();
        // A repaint (paint can, undo, event) eases from the old colour.
        fades.observe(&world(0, true), &world(1, true), [7]);
        let out = fades.left_out();
        assert_eq!(out, BTreeSet::from([7]));
        assert!(fades.needs_rebuild(&BTreeSet::new()));
        // Nothing moves until the chunks leave the brick out.
        fades.advance(0.05, &BTreeSet::new());
        assert_eq!(fades.drawn(7), Some(BLACK));
        fades.chunks_applied(&out);
        fades.advance(0.05, &out);
        assert!((fades.drawn(7).unwrap()[0] - 0.2).abs() < 1e-6);
        // Repainted again mid-way: it carries on from where it is drawn.
        fades.observe(&world(1, true), &world(2, true), [7]);
        assert!((fades.drawn(7).unwrap()[0] - 0.2).abs() < 1e-6);
        for _ in 0..200 {
            fades.advance(1.0 / 60.0, &out);
        }
        assert_eq!(fades.drawn(7), Some([1.0, 0.0, 0.0, 1.0]));
        // Settled: the chunks take it back, and only then does it leave here.
        assert!(fades.left_out().is_empty());
        assert!(fades.needs_rebuild(&out));
        assert_eq!(fades.shown(&out).count(), 1);
        fades.chunks_applied(&BTreeSet::new());
        assert!(fades.is_empty());
    }

    #[test]
    fn unchanged_hidden_or_new_bricks_do_not_ease() {
        let mut fades = BrickFades::default();
        fades.observe(&world(1, true), &world(1, true), [7]);
        fades.observe(&world(0, true), &world(1, false), [7]);
        let mut empty = world(0, true);
        empty.bricks = Default::default();
        fades.observe(&empty, &world(1, true), [7]);
        assert!(fades.is_empty());
        // Removed mid-ease: drawn at its target until the chunks catch up.
        fades.observe(&world(0, true), &world(1, true), [7]);
        let out = fades.left_out();
        fades.chunks_applied(&out);
        fades.observe(&world(1, true), &empty, [7]);
        assert!(fades.left_out().is_empty());
        assert_eq!(fades.shown(&out).collect::<Vec<_>>(), [(7, WHITE)]);
        fades.chunks_applied(&BTreeSet::new());
        assert!(fades.is_empty());
    }
}
