//! The box outlines v20 draws round non-rendering bricks while a building
//! tool is out (`showBricks`, `fxDTSBrick::renderObject`), in their paint
//! colour, and round bricks fading in or out under alpha 0.1
//! (`brick_fade::OUTLINE_ALPHA`).
//!
//! The hidden bricks are kept from each replica revision's changes
//! (`Bricks::diff`, which skips the shared tree), as `rule_regions` keeps
//! its candidates, and the outlines are rebuilt only when what they draw
//! changed: a world change costs its changed bricks, not a pass over every
//! brick, and a revision that touches no outlined brick rebuilds nothing.
use bri_render::lines::LineVertex;
use bri_world::{Brick, BrickId, Bricks};
use glam::Vec3;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct HiddenOutlines {
    bricks: Option<Bricks>,
    /// Bricks that do not render.
    hidden: BTreeSet<BrickId>,
    /// What the last outlines drew: shown, palette, bricks.
    drawn: Option<(bool, Vec<[f32; 4]>, Vec<BrickId>)>,
    /// An outlined brick changed since the last outlines.
    dirty: bool,
    pub diagnostics: HiddenOutlineDiagnostics,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct HiddenOutlineDiagnostics {
    /// Passes over a whole world: the first replica, or one after `clear`.
    pub full_scans: u64,
    /// Bricks examined, in passes and in changes.
    pub visited: u64,
    /// Outlines built.
    pub rebuilds: u64,
}

impl HiddenOutlines {
    /// Forget the world (a new session or device).
    pub fn clear(&mut self) {
        let diagnostics = self.diagnostics;
        *self = Self {
            diagnostics,
            ..Self::default()
        };
    }
    /// The outlines to draw, or None when the drawn ones still hold.
    /// `fading` is `BrickFades::outlined` (faint: under alpha 0.1), `dead`
    /// a brick its debris stands in for, `outline` a brick's box (None for
    /// one without a mesh). `force` rebuilds whatever changed (new lines
    /// on the device).
    #[allow(clippy::too_many_arguments)] // the replica plus the frame's state
    pub fn update(
        &mut self,
        bricks: &Bricks,
        palette: &[[f32; 4]],
        show: bool,
        fading: &[(u64, bool)],
        dead: impl Fn(BrickId) -> bool,
        outline: impl Fn(&Brick) -> Option<(Vec3, Vec3)>,
        force: bool,
    ) -> Option<Vec<LineVertex>> {
        self.follow(bricks);
        // Hidden bricks, and any fading in or out drawn under alpha 0.1.
        let ids: Vec<BrickId> = if show {
            let easing: BTreeSet<u64> = fading.iter().map(|(id, _)| *id).collect();
            let faint = fading
                .iter()
                .filter(|(id, faint)| *faint && bricks.contains_key(id))
                .map(|(id, _)| *id);
            let mut ids: Vec<_> = self
                .hidden
                .iter()
                .copied()
                .filter(|id| !easing.contains(id))
                .chain(faint)
                .filter(|id| !dead(*id))
                .collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        } else {
            Vec::new()
        };
        if !force
            && !self.dirty
            && self
                .drawn
                .as_ref()
                .is_some_and(|(s, p, d)| *s == show && p == palette && *d == ids)
        {
            return None;
        }
        self.dirty = false;
        self.diagnostics.rebuilds += 1;
        let mut vertices = vec![];
        for id in &ids {
            let Some(brick) = bricks.get(id) else {
                continue;
            };
            let (Some((low, high)), Some(color)) =
                (outline(brick), palette.get(usize::from(brick.color)))
            else {
                continue;
            };
            bri_render::lines::box_edges(low, high, [color[0], color[1], color[2]], &mut vertices);
        }
        self.drawn = Some((show, palette.to_vec(), ids));
        Some(vertices)
    }
    /// Bring the hidden bricks up to `bricks`.
    fn follow(&mut self, bricks: &Bricks) {
        if self.bricks.as_ref().is_some_and(|old| old.ptr_eq(bricks)) {
            return;
        }
        match &self.bricks {
            Some(old) => {
                let drawn = self.drawn.as_ref().map(|(_, _, ids)| ids);
                for change in old.diff(bricks) {
                    self.diagnostics.visited += 1;
                    let (id, hidden) = match change {
                        imbl::ordmap::DiffItem::Remove(id, _) => (*id, false),
                        imbl::ordmap::DiffItem::Add(id, b)
                        | imbl::ordmap::DiffItem::Update { new: (id, b), .. } => (*id, !b.visible),
                    };
                    if hidden {
                        self.hidden.insert(id);
                    } else {
                        self.hidden.remove(&id);
                    }
                    // An outlined brick changed (moved, repainted, shown):
                    // its box does too. Bricks newly hidden change which
                    // are drawn, which `update` compares.
                    if drawn.is_some_and(|d| d.binary_search(&id).is_ok()) {
                        self.dirty = true;
                    }
                }
            }
            None => {
                self.diagnostics.full_scans += 1;
                self.diagnostics.visited += bricks.len() as u64;
                self.hidden = bricks
                    .iter()
                    .filter(|(_, b)| !b.visible)
                    .map(|(id, _)| *id)
                    .collect();
                self.dirty = true;
            }
        }
        self.bricks = Some(bricks.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brick(x: f32, color: u8) -> Brick {
        let mut b = Brick::new(
            bri_world::ContentRef::Resolved("brick".into()),
            [x, 0.0, 0.0],
            1,
        );
        b.color = color;
        b
    }
    fn outline(b: &Brick) -> Option<(Vec3, Vec3)> {
        let at = Vec3::from(b.position);
        Some((at - Vec3::splat(0.25), at + Vec3::splat(0.25)))
    }

    /// Bricks knocked out and back far from any hidden brick rebuild
    /// nothing and read only what changed; hiding, repainting and showing
    /// a brick rebuilds the outlines, and only the first replica is read
    /// whole.
    #[test]
    fn hidden_outlines_follow_world_changes_without_rereading_the_world() {
        let palette = [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]];
        let mut bricks: Bricks = (0..5000u64).map(|id| (id, brick(id as f32, 0))).collect();
        let mut hidden = bricks[&3].clone();
        hidden.visible = false;
        bricks.insert(3, hidden);
        let mut lines = HiddenOutlines::default();
        let update = |lines: &mut HiddenOutlines, bricks: &Bricks, dead: Option<u64>| {
            lines.update(
                bricks,
                &palette,
                true,
                &[],
                |id| Some(id) == dead,
                outline,
                false,
            )
        };
        let first = update(&mut lines, &bricks, None).expect("first outlines");
        assert_eq!(first.len(), 24, "one box");
        assert!(update(&mut lines, &bricks, None).is_none());
        let visited = lines.diagnostics.visited;
        // A blast's bricks go and come back, but they are dead (their
        // debris stands in for them) while gone.
        for id in 100..140 {
            let mut b = bricks[&id].clone();
            b.visible = false;
            bricks.insert(id, b);
            assert!(
                update(&mut lines, &bricks, Some(id)).is_none(),
                "brick {id} out"
            );
            let mut b = bricks[&id].clone();
            b.visible = true;
            bricks.insert(id, b);
            assert!(
                update(&mut lines, &bricks, None).is_none(),
                "brick {id} back"
            );
        }
        // A visible brick repainted far away: nothing to redraw.
        let mut b = bricks[&4000].clone();
        b.color = 1;
        bricks.insert(4000, b);
        assert!(update(&mut lines, &bricks, None).is_none());
        // The hidden brick repainted, then shown.
        let mut b = bricks[&3].clone();
        b.color = 1;
        bricks.insert(3, b);
        let repainted = update(&mut lines, &bricks, None).expect("repainted");
        assert_eq!(repainted[0].color[..3], [0.0, 1.0, 0.0]);
        let mut b = bricks[&3].clone();
        b.visible = true;
        bricks.insert(3, b);
        assert_eq!(update(&mut lines, &bricks, None), Some(vec![]));
        assert_eq!(lines.diagnostics.full_scans, 1);
        assert!(
            lines.diagnostics.visited - visited < 200,
            "{} bricks examined for 83 changes",
            lines.diagnostics.visited - visited
        );
        // Put away, the tool draws nothing; taken out again, the boxes.
        let hide = lines.update(&bricks, &palette, false, &[], |_| false, outline, false);
        assert_eq!(hide, Some(vec![]));
    }
}
