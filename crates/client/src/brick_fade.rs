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
//! colour to ease from and appears at once. Turning a brick's rendering off
//! or on eases its alpha to 0 or back the same way (`shown_color`).
//!
//! The replicated chunks leave an easing brick out, and this module draws it
//! alone with its current colour, like v20 taking a changed brick out of its
//! static batch. Once settled, the chunk is rebuilt with the brick in it
//! before this drawing stops.
use anyhow::Result;
use bri_console::Clamp;
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

/// The colour a brick painted `color` eases toward: v20's client unpack
/// sets a non-rendering brick's target alpha to 0 (`setRendering`, 0x539965),
/// so turning rendering off fades the brick out on the same curve as a
/// repaint, and turning it back on fades it in.
pub fn shown_color(color: [f32; 4], rendering: bool) -> [f32; 4] {
    if rendering {
        color
    } else {
        [color[0], color[1], color[2], 0.0]
    }
}

/// v20 stops drawing a planted brick's mesh below this alpha
/// (`fxDTSBrick::renderObject`, 0x533bf0).
pub const MIN_DRAWN_ALPHA: f32 = 0.03;
/// Below this alpha, v20 outlines a brick while a building tool is out.
pub const OUTLINE_ALPHA: f32 = 0.1;

/// One frame of v20's easing. Returns the new drawn colour and whether it
/// has reached the target.
pub fn ease(drawn: [f32; 4], target: [f32; 4], dt: f32) -> ([f32; 4], bool) {
    let k = RATE * dt.clamped(MIN_DT, MAX_DT);
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
    /// Every brick here and whether it is drawn faint enough for v20 to
    /// outline it while a building tool is out.
    pub fn outlined(&self) -> Vec<(u64, bool)> {
        self.fades
            .iter()
            .map(|(id, f)| (*id, f.drawn[3] < OUTLINE_ALPHA))
            .collect()
    }
    /// Stop easing every brick. Each is drawn here at its target until the
    /// chunks take it back, so nothing blinks out meanwhile.
    pub fn settle_all(&mut self) {
        for fade in self.fades.values_mut() {
            fade.drawn = fade.target;
            fade.settled = true;
        }
    }
    /// Stop easing `id`: it is drawn at its target (a knocked-out brick:
    /// gone at once, its debris takes its place) until the chunks take it.
    pub fn settle(&mut self, id: u64) {
        if let Some(fade) = self.fades.get_mut(&id) {
            fade.drawn = fade.target;
            fade.settled = true;
        }
    }
    /// Start easing the `changed` bricks whose paint or rendering differs
    /// between the drawn world `from` and the new world `to`. A brick already
    /// easing carries on from where it is drawn now. Bricks that moved or
    /// vanished stop easing.
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
                from.palette
                    .get(usize::from(old.color))
                    .map(|c| shown_color(*c, old.visible)),
                to.palette
                    .get(usize::from(new.color))
                    .map(|c| shown_color(*c, new.visible)),
            ) else {
                self.settle(id);
                continue;
            };
            let before = &before;
            let target = &target;
            // A hidden brick repainted stays hidden: nothing to see ease.
            if !(old.visible || new.visible)
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

/// What a frame asks of the GPU for one easing brick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FadeWork {
    /// A new mesh, or one whose layout changed: upload its geometry.
    Upload,
    /// Same layout, new colour: rewrite its vertices.
    Vertices,
}

/// Work counted across frames, for tests and diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FadeDiagnostics {
    /// Geometry uploads (new GPU buffers).
    pub uploads: u64,
    /// Vertex rewrites of an uploaded mesh.
    pub vertex_updates: u64,
    /// Images those uploads carried. The shared brick palette holds every
    /// brick texture, so this stays 0: easing never uploads a texture.
    pub images_uploaded: u64,
}

/// CPU meshes of easing bricks, one per brick, built against the shared
/// brick material palette like chunks and debris: geometry only, no images.
/// A colour change rebuilds a brick's geometry in place.
#[derive(Default)]
pub struct FadeMeshes {
    meshes: BTreeMap<u64, FadeMesh>,
    pub diagnostics: FadeDiagnostics,
}
struct FadeMesh {
    data: SceneData,
    drawn: [f32; 4],
}
/// What must match for a colour change to be a vertex update only.
#[derive(PartialEq)]
struct Layout {
    vertices: usize,
    indices: usize,
    batches: Vec<usize>,
}
impl Layout {
    fn of(data: &SceneData) -> Self {
        Self {
            vertices: data.vertices.len(),
            indices: data.indices.len(),
            batches: data.batches.iter().map(|b| b.material).collect(),
        }
    }
}

impl FadeMeshes {
    pub fn clear(&mut self) {
        self.meshes.clear();
    }
    pub fn len(&self) -> usize {
        self.meshes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }
    pub fn contains(&self, id: u64) -> bool {
        self.meshes.contains_key(&id)
    }
    pub fn data(&self, id: u64) -> Option<&SceneData> {
        self.meshes.get(&id).map(|m| &m.data)
    }
    /// Bring the meshes of the bricks `fades` draws this frame up to date
    /// from the drawn `world`'s bricks, and say what each changed one needs.
    pub fn update(
        &mut self,
        fades: &BrickFades,
        left_out: &BTreeSet<u64>,
        world: &PublicWorld,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
        palette: &crate::world_chunks::BrickPalette,
        materials: &crate::materials::BrickMaterials,
    ) -> Result<Vec<(u64, FadeWork)>> {
        let shown: BTreeMap<u64, [f32; 4]> = fades.shown(left_out).collect();
        self.meshes.retain(|id, _| shown.contains_key(id));
        let mut work = Vec::new();
        for (id, drawn) in shown {
            if self.meshes.get(&id).is_some_and(|m| m.drawn == drawn) {
                continue;
            }
            let Some((brick, colors)) = fade_brick(world, id, drawn) else {
                self.meshes.remove(&id);
                continue;
            };
            let same_palette = self
                .meshes
                .get(&id)
                .is_some_and(|m| m.data.materials.len() == palette.scene.materials.len());
            let job = match self.meshes.get_mut(&id).filter(|_| same_palette) {
                Some(mesh) => {
                    let before = Layout::of(&mesh.data);
                    crate::world_chunks::rebuild_brick(
                        &mut mesh.data,
                        &brick,
                        &colors,
                        meshes,
                        palette,
                        Some(materials),
                    )?;
                    mesh.drawn = drawn;
                    if Layout::of(&mesh.data) == before {
                        FadeWork::Vertices
                    } else {
                        FadeWork::Upload
                    }
                }
                None => {
                    let data = crate::world_chunks::build_brick(
                        &brick,
                        &colors,
                        meshes,
                        palette,
                        Some(materials),
                    )?;
                    self.meshes.insert(id, FadeMesh { data, drawn });
                    FadeWork::Upload
                }
            };
            let data = &self.meshes[&id].data;
            if data.indices.is_empty() {
                self.meshes.remove(&id);
                continue;
            }
            match job {
                FadeWork::Upload => {
                    self.diagnostics.uploads += 1;
                    self.diagnostics.images_uploaded += data.images.len() as u64;
                }
                FadeWork::Vertices => self.diagnostics.vertex_updates += 1,
            }
            work.push((id, job));
        }
        Ok(work)
    }
}

/// GPU meshes of easing bricks, one per brick. They bind the shared brick
/// palette's textures, so a brick starting to ease uploads two small
/// buffers and a colour change rewrites its vertices.
#[derive(Default)]
pub struct FadeModels {
    cpu: FadeMeshes,
    models: BTreeMap<u64, GpuScene>,
}

impl FadeModels {
    pub fn clear(&mut self) {
        self.cpu.clear();
        self.models.clear();
    }
    pub fn diagnostics(&self) -> &FadeDiagnostics {
        &self.cpu.diagnostics
    }
    /// Bring the meshes of the bricks `fades` draws this frame up to date
    /// from the drawn `world`'s bricks, against `gpu_palette` (the uploaded
    /// `palette`).
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
        palette: &crate::world_chunks::BrickPalette,
        gpu_palette: &GpuScene,
        materials: &crate::materials::BrickMaterials,
    ) -> Result<()> {
        let work = self
            .cpu
            .update(fades, left_out, world, meshes, palette, materials)?;
        let cpu = &self.cpu;
        self.models.retain(|id, _| cpu.contains(*id));
        for (id, job) in work {
            let Some(data) = self.cpu.data(id) else {
                continue;
            };
            match (job, self.models.get_mut(&id)) {
                (FadeWork::Vertices, Some(gpu)) => {
                    let centers: Vec<_> = data.batches.iter().map(|b| b.center).collect();
                    gpu.update_vertices(queue, &data.vertices, &centers)?;
                }
                _ => {
                    let gpu = renderer.upload_palette_model(device, data, gpu_palette)?;
                    self.models.insert(id, gpu);
                }
            }
        }
        Ok(())
    }
    pub fn scenes(&self) -> impl Iterator<Item = &GpuScene> {
        self.models.values()
    }
}

/// `id` as it stands in `world`, recoloured to index a one-colour palette
/// painted `drawn`, or `None` when it is gone or drawn too faint to show.
fn fade_brick(
    world: &PublicWorld,
    id: u64,
    drawn: [f32; 4],
) -> Option<(bri_world::Brick, [[f32; 4]; 1])> {
    let brick = world.bricks.get(&id)?;
    if drawn[3] < MIN_DRAWN_ALPHA {
        return None;
    }
    // Every colour the brick names, its events' included, indexes the
    // one-colour palette it is drawn with.
    let mut brick = brick.clone();
    brick.recolor(|_| 0);
    // A brick fading out has already stopped rendering.
    brick.visible = true;
    Some((brick, [drawn.map(|v| v.clamped(0.0, 1.0))]))
}

/// One brick where it stands in `world`, painted `drawn`, against the
/// shared brick palette.
#[cfg(test)]
fn brick_scene(
    world: &PublicWorld,
    id: u64,
    drawn: [f32; 4],
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
    palette: &crate::world_chunks::BrickPalette,
    materials: &crate::materials::BrickMaterials,
) -> Result<Option<SceneData>> {
    let Some((brick, colors)) = fade_brick(world, id, drawn) else {
        return Ok(None);
    };
    let data = crate::world_chunks::build_brick(&brick, &colors, meshes, palette, Some(materials))?;
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
        let palette = crate::world_chunks::BrickPalette::new(&materials).unwrap();
        let mut world = world(1, true);
        let brick = world.bricks.get_mut(&7).unwrap();
        brick.events = vec![crate::world_scene::tests::set_color(2)];
        let data = brick_scene(&world, 7, WHITE, &meshes, &palette, &materials)
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
        fades.observe(&world(0, false), &world(1, false), [7]);
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

    /// v20 fades a brick whose rendering is turned off on the colour curve,
    /// its alpha easing to 0: the outline shows from alpha 0.1 (about 0.58 s
    /// at 60 fps), the mesh stops below 0.03, and it settles at 0.01. Turning
    /// rendering back on fades it in.
    #[test]
    fn rendering_off_fades_out_and_back_in() {
        let dt = 1.0 / 60.0;
        let mut fades = BrickFades::default();
        fades.observe(&world(1, true), &world(1, false), [7]);
        let out = fades.left_out();
        assert_eq!(out, BTreeSet::from([7]));
        fades.chunks_applied(&out);
        assert_eq!(fades.drawn(7), Some(WHITE));
        let mut outline_frame = None;
        let mut frames = 0;
        while !fades.left_out().is_empty() {
            fades.advance(dt, &out);
            frames += 1;
            let drawn = fades.drawn(7).unwrap();
            assert_eq!(drawn[..3], WHITE[..3]);
            if drawn[3] < OUTLINE_ALPHA && outline_frame.is_none() {
                outline_frame = Some(frames);
            }
            assert!(frames < 200, "never settles");
        }
        // (1 - 4/60)^n < 0.1 first holds at n = 34.
        assert_eq!(outline_frame, Some(34));
        assert_eq!(frames, 68);
        assert_eq!(fades.drawn(7), Some([1.0, 1.0, 1.0, 0.0]));
        fades.chunks_applied(&BTreeSet::new());
        assert!(fades.is_empty());
        // Back on: eases in from nothing.
        fades.observe(&world(1, false), &world(1, true), [7]);
        let out = fades.left_out();
        fades.chunks_applied(&out);
        fades.advance(dt, &out);
        assert!((fades.drawn(7).unwrap()[3] - RATE * dt).abs() < 1e-6);
    }

    /// `count` bricks in a row, all painted white, shown or not.
    fn row(count: u64, visible: bool) -> PublicWorld {
        let mut bricks = bri_world::Bricks::default();
        for id in 1..=count {
            let mut brick = Brick::new(ContentRef::Resolved("a".into()), [id as f32, 0.0, 0.0], 1);
            brick.color = 1;
            brick.visible = visible;
            bricks.insert(id, brick);
        }
        PublicWorld {
            name: "Test".into(),
            map_id: "map/test".into(),
            palette: vec![BLACK, WHITE],
            bricks,
        }
    }

    /// Max (v0.2.3): a few dozen bricks blown up in a minigame lag the game
    /// when they come back, until they have all faded in. Each returning
    /// brick eased in through a scene of its own that copied every brick
    /// surface image, rebuilt every frame of the ~68-frame ease, and its
    /// first upload made textures and mip chains per brick. Now an easing
    /// brick binds the shared brick palette: no image is ever uploaded,
    /// each brick uploads geometry once when it starts (and once more when
    /// it turns opaque at the end), and every other frame only rewrites its
    /// vertices.
    #[test]
    fn returning_bricks_fade_in_without_textures_or_per_frame_uploads() {
        const RETURNING: u64 = 48;
        let meshes = BTreeMap::from([("a".to_string(), crate::world_scene::tests::mesh())]);
        let materials = crate::materials::BrickMaterials::in_memory();
        let palette = crate::world_chunks::BrickPalette::new(&materials).unwrap();
        let (dead, back) = (row(RETURNING, false), row(RETURNING, true));
        let mut fades = BrickFades::default();
        fades.observe(&dead, &back, 1..=RETURNING);
        let out = fades.left_out();
        assert_eq!(out.len(), RETURNING as usize);
        fades.chunks_applied(&out);
        let mut cpu = FadeMeshes::default();
        let mut frames = 0;
        let mut uploading_frames = 0;
        loop {
            fades.advance(1.0 / 60.0, &out);
            let work = cpu
                .update(&fades, &out, &back, &meshes, &palette, &materials)
                .unwrap();
            frames += 1;
            assert!(work.len() <= RETURNING as usize);
            let uploads = work.iter().filter(|(_, w)| *w == FadeWork::Upload).count();
            if uploads > 0 {
                uploading_frames += 1;
            }
            // In place, the mesh is what a fresh build would be.
            for id in [1, RETURNING] {
                let drawn = fades.drawn(id).unwrap();
                let fresh = brick_scene(&back, id, drawn, &meshes, &palette, &materials)
                    .unwrap()
                    .unwrap();
                let kept = cpu.data(id).unwrap();
                let vertices = |d: &SceneData| {
                    d.vertices
                        .iter()
                        .map(|v| (v.position, v.normal, v.uv, v.lightmap_uv, v.color, v.fx))
                        .collect::<Vec<_>>()
                };
                assert_eq!(vertices(kept), vertices(&fresh));
                assert_eq!(kept.indices, fresh.indices);
                let batches = |d: &SceneData| {
                    d.batches
                        .iter()
                        .map(|b| (b.indices.clone(), b.material, b.center))
                        .collect::<Vec<_>>()
                };
                assert_eq!(batches(kept), batches(&fresh));
                assert!(kept.images.is_empty());
            }
            if fades.left_out().is_empty() {
                break;
            }
            assert!(frames < 200, "never settles");
        }
        assert_eq!(frames, 68);
        let d = &cpu.diagnostics;
        eprintln!("{frames} frames, {uploading_frames} uploading: {d:?}");
        assert_eq!(d.images_uploaded, 0, "{d:?}");
        assert!(d.uploads <= 2 * RETURNING, "{d:?}");
        assert!(
            uploading_frames <= 2,
            "uploads on {uploading_frames} frames"
        );
        assert_eq!(d.uploads + d.vertex_updates, RETURNING * frames, "{d:?}");
        // The chunks take them back.
        fades.chunks_applied(&BTreeSet::new());
        cpu.update(
            &fades,
            &BTreeSet::new(),
            &back,
            &meshes,
            &palette,
            &materials,
        )
        .unwrap();
        assert!(fades.is_empty() && cpu.is_empty());
    }

    #[test]
    fn a_faded_out_brick_draws_no_mesh() {
        let meshes = BTreeMap::from([("a".to_string(), crate::world_scene::tests::mesh())]);
        let materials = crate::materials::BrickMaterials::in_memory();
        let palette = crate::world_chunks::BrickPalette::new(&materials).unwrap();
        let world = world(1, false);
        let half = [1.0, 1.0, 1.0, 0.5];
        let data = brick_scene(&world, 7, half, &meshes, &palette, &materials).unwrap();
        assert!(data.is_some_and(|d| d.vertices.iter().all(|v| v.color == half)));
        let faint = [1.0, 1.0, 1.0, 0.02];
        assert!(
            brick_scene(&world, 7, faint, &meshes, &palette, &materials)
                .unwrap()
                .is_none()
        );
    }
}
