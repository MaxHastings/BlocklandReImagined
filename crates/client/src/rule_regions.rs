//! Creator outlines derived from replicated brick state, never gameplay truth.
use bri_content::brick::Brick as Mesh;
use bri_render::lines::{LineVertex, box_edges};
use bri_world::{Brick, BrickId, Bricks, regions};
use glam::Vec3;
use std::collections::BTreeMap;

pub const REGION_COLOR: [f32; 3] = [0.15, 0.9, 1.0];
pub const PREVIEW_COLOR: [f32; 3] = [0.6, 1.0, 1.0];
pub const INACTIVE_COLOR: [f32; 3] = [0.5, 0.6, 0.7];
/// Orange marks a defined region beyond the observer budget, not detection.
pub const OVER_BUDGET_COLOR: [f32; 3] = [1.0, 0.45, 0.1];
pub type Preview = Option<(BrickId, Option<[f32; 3]>)>;

#[derive(Default)]
pub struct Outlines {
    bricks: Option<Bricks>,
    shown: bool,
    preview: Preview,
    candidates: BTreeMap<BrickId, Brick>,
}

impl Outlines {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    /// None means the immutable brick snapshot and editor draft are unchanged.
    pub fn update<'a>(
        &mut self,
        bricks: &Bricks,
        shown: bool,
        preview: Preview,
        mesh: impl Fn(&Brick) -> Option<&'a Mesh>,
    ) -> Option<Vec<LineVertex>> {
        if self.shown == shown
            && self.preview == preview
            && self.bricks.as_ref().is_some_and(|old| old.ptr_eq(bricks))
        {
            return None;
        }
        let candidate = |b: &Brick| regions::has_region_input(b) || b.rule_region.is_some();
        if let Some(old) = &self.bricks {
            for change in old.diff(bricks) {
                match change {
                    imbl::ordmap::DiffItem::Remove(id, _) => {
                        self.candidates.remove(id);
                    }
                    imbl::ordmap::DiffItem::Add(id, b)
                    | imbl::ordmap::DiffItem::Update { new: (id, b), .. } => {
                        if candidate(b) {
                            self.candidates.insert(*id, b.clone());
                        } else {
                            self.candidates.remove(id);
                        }
                    }
                }
            }
        } else {
            self.candidates = bricks
                .iter()
                .filter(|(_, b)| candidate(b))
                .map(|(&id, b)| (id, b.clone()))
                .collect();
        }
        self.bricks = Some(bricks.clone());
        self.shown = shown;
        self.preview = preview;
        let mut vertices = Vec::new();
        if !shown {
            return Some(vertices);
        }
        let mut observed = 0;
        let mut drawn = 0;
        // A draft can introduce a region before its first authoritative Send.
        let mut candidates = self.candidates.clone();
        if let Some((id, _)) = preview
            && let Some(brick) = bricks.get(&id)
        {
            candidates.insert(id, brick.clone());
        }
        for (&id, brick) in &candidates {
            let listens = regions::has_region_input(brick);
            let selected = preview.is_some_and(|(selected, _)| selected == id);
            let within_budget = !listens || observed < regions::MAX_OBSERVED_REGIONS;
            observed += usize::from(listens);
            let size = if selected {
                preview.unwrap().1
            } else {
                brick.rule_region
            };
            if (!listens && size.is_none()) || (drawn >= 512 && !selected) {
                continue;
            }
            let Some(mesh) = mesh(brick) else {
                continue;
            };
            let (lo, hi) = regions::bounds(size, bri_sim::definitions::brick_box(brick, mesh));
            let color = if !within_budget {
                OVER_BUDGET_COLOR
            } else if selected || !listens {
                PREVIEW_COLOR
            } else if brick.events.iter().any(|row| {
                row.enabled
                    && row.preserved.is_none()
                    && regions::REGION_INPUTS
                        .iter()
                        .any(|input| row.input.eq_ignore_ascii_case(input))
            }) {
                REGION_COLOR
            } else {
                INACTIVE_COLOR
            };
            box_edges(lo, hi, color, &mut vertices);
            if selected {
                // A small cross ties an otherwise floating region to its brick.
                let center = Vec3::from(brick.position);
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    for p in [center - axis * 0.3, center + axis * 0.3] {
                        vertices.push(LineVertex {
                            position: p.to_array(),
                            color,
                        });
                    }
                }
            }
            drawn += 1;
        }
        Some(vertices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_world::{ContentRef, EventRow, EventTarget};
    fn row() -> EventRow {
        EventRow {
            conditions: vec![],
            preserved: None,
            enabled: true,
            input: "onRegionEnter".into(),
            delay_ms: 0,
            target: EventTarget::Slot(bri_events::Slot::SelfBrick),
            output: "setColor".into(),
            params: vec![bri_events::Value::Color(0)],
        }
    }
    fn mesh() -> Mesh {
        bri_content::testing::bricks::block("test", [2, 8], 1, |face| {
            (bri_content::testing::bricks::default_surface(face), None)
        })
    }
    fn extent(vertices: &[LineVertex]) -> Vec3 {
        let lo = vertices.iter().fold(Vec3::splat(f32::INFINITY), |a, v| {
            a.min(Vec3::from(v.position))
        });
        let hi = vertices
            .iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), |a, v| {
                a.max(Vec3::from(v.position))
            });
        hi - lo
    }
    #[test]
    fn region_outlines_follow_edits_visibility_and_replica_lifecycle() {
        let mesh = mesh();
        let mut bricks = Bricks::new();
        let mut brick = Brick::new(ContentRef::Resolved("test".into()), [12.0, 6.0, -3.0], 1);
        brick.events.push(row());
        bricks.insert(1, brick);
        let mut outlines = Outlines::default();
        let lines = outlines
            .update(&bricks, true, None, |_| Some(&mesh))
            .unwrap();
        assert_eq!(extent(&lines), Vec3::new(1.0, 4.0, 4.0));
        assert!(lines.iter().all(|v| v.color == REGION_COLOR));
        assert!(
            outlines
                .update(&bricks, true, None, |_| Some(&mesh))
                .is_none()
        );
        bricks.get_mut(&1).unwrap().quarter_turns = 1;
        assert_eq!(
            extent(
                &outlines
                    .update(&bricks, true, None, |_| Some(&mesh))
                    .unwrap()
            ),
            Vec3::new(4.0, 4.0, 1.0)
        );
        bricks.get_mut(&1).unwrap().rule_region = Some([8.0, 5.0, 8.0]);
        assert_eq!(
            extent(
                &outlines
                    .update(&bricks, true, None, |_| Some(&mesh))
                    .unwrap()
            ),
            Vec3::new(8.0, 5.0, 8.0)
        );
        let preview = Some((1, Some([3.0, 6.0, 7.0])));
        assert_eq!(
            extent(
                &outlines
                    .update(&bricks, true, preview, |_| Some(&mesh))
                    .unwrap()
            ),
            Vec3::new(3.0, 6.0, 7.0)
        );
        assert_eq!(
            bricks[&1].rule_region,
            Some([8.0, 5.0, 8.0]),
            "draft must not mutate replica"
        );
        assert!(
            outlines
                .update(&bricks, false, None, |_| Some(&mesh))
                .unwrap()
                .is_empty()
        );
        bricks.get_mut(&1).unwrap().events[0].enabled = false;
        assert!(
            outlines
                .update(&bricks, true, None, |_| Some(&mesh))
                .unwrap()
                .iter()
                .all(|v| v.color == INACTIVE_COLOR)
        );
        bricks.remove(&1);
        assert!(
            outlines
                .update(&bricks, true, None, |_| Some(&mesh))
                .unwrap()
                .is_empty()
        );
        outlines.clear();
        assert!(
            outlines
                .update(&Bricks::new(), true, None, |_| Some(&mesh))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn preserved_rows_are_not_sensors_and_over_budget_regions_are_marked() {
        let mesh = mesh();
        let mut bricks = Bricks::new();
        let mut brick = Brick::new(ContentRef::Resolved("test".into()), [0.0; 3], 1);
        let mut unsupported = row();
        unsupported.preserved = Some(bri_events::PreservedRow {
            original: "future".into(),
            diagnostic: "unsupported".into(),
        });
        brick.events.push(unsupported);
        bricks.insert(1, brick.clone());
        let mut outlines = Outlines::default();
        assert!(
            outlines
                .update(&bricks, true, None, |_| Some(&mesh))
                .unwrap()
                .is_empty()
        );
        brick.events = vec![row()];
        for id in 1..=257 {
            bricks.insert(id, brick.clone());
        }
        let lines = outlines
            .update(&bricks, true, None, |_| Some(&mesh))
            .unwrap();
        assert_eq!(lines.len(), 257 * 24);
        assert_eq!(lines[256 * 24].color, OVER_BUDGET_COLOR);
        assert!(lines[..256 * 24].iter().all(|v| v.color == REGION_COLOR));
    }
}