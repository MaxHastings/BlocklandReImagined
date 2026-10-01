//! Bricks lit up for a moment: an administrator's Highlight Brick Group
//! and the bricks a duplicator selected. As in v20 the bricks themselves
//! take the colour and the Glow effect (`setColor`, `setColorFX(3)`), so
//! everyone sees it, and get their own back when it ends.
//!
//! A brick lit twice keeps the colours it had before the first, and stays
//! lit until the later end. Whatever changes a lit brick meanwhile (a
//! spray can) wins: only a colour or effect still as the highlight left it
//! is put back. Copies and cuts take a lit brick as it is underneath
//! ([`Session::unlit`]).
use super::*;

/// `setColorFX(3)`: Glow.
pub(super) const GLOW: u8 = 3;
/// Simulation ticks per second.
const SECOND: u64 = 120;

/// A lit brick: its own colour and effect, and what it shows until `until`.
struct Lit {
    color: u8,
    effect: u8,
    shown: (u8, u8),
    until: u64,
}

#[derive(Default)]
pub(super) struct Highlights {
    bricks: BTreeMap<BrickId, Lit>,
    /// Brick groups an administrator lit, until when (`isChainBlinking`).
    groups: BTreeMap<OwnerId, u64>,
}

impl Session {
    /// Show `ids` in palette colour `color` (or each in its own) with
    /// `effect` for `seconds`.
    pub(super) fn light_bricks(
        &mut self,
        ids: &[BrickId],
        color: Option<u8>,
        effect: u8,
        seconds: f32,
    ) -> Result<()> {
        let until = self.simulation.state().tick + (seconds.max(0.0) * SECOND as f32) as u64;
        for &id in ids {
            let Some(brick) = self.simulation.state().bricks.get(&id) else {
                continue;
            };
            let own = (brick.color, brick.color_effect);
            let color = color.unwrap_or(match self.highlights.bricks.get(&id) {
                // Lit already: its colour underneath.
                Some(lit) if brick.color == lit.shown.0 => lit.color,
                _ => brick.color,
            });
            let lit = self.highlights.bricks.entry(id).or_insert(Lit {
                color: own.0,
                effect: own.1,
                shown: (color, effect),
                until,
            });
            lit.shown = (color, effect);
            lit.until = lit.until.max(until);
            if own != (color, effect) {
                self.simulation.mutate(id, |b| {
                    b.color = color;
                    b.color_effect = effect;
                })?;
                self.dirty.insert(id);
            }
        }
        Ok(())
    }

    /// The palette entry nearest `rgba` in colour whose opacity is within
    /// 0.3 of it (Space Guy's `getClosestPaintColor`, which v20's
    /// duplicators used to pick their highlight); entry 0 when none is.
    pub(super) fn closest_paint(&self, rgba: [f32; 4]) -> u8 {
        let palette = &self.simulation.state().palette;
        palette
            .iter()
            .enumerate()
            .take(usize::from(u8::MAX) + 1)
            .filter(|(_, c)| (rgba[3] - c[3]).abs() < 0.3)
            .min_by(|(_, a), (_, b)| {
                let distance = |c: &[f32; 4]| (0..3).map(|i| (c[i] - rgba[i]).powi(2)).sum::<f32>();
                distance(a).total_cmp(&distance(b))
            })
            .map_or(0, |(i, _)| i as u8)
    }

    /// `brick` (standing as `id`) with its own colours, if it is lit.
    pub(super) fn unlit(&self, id: BrickId, brick: &Brick) -> Brick {
        let mut brick = brick.clone();
        if let Some(lit) = self.highlights.bricks.get(&id) {
            if brick.color == lit.shown.0 {
                brick.color = lit.color;
            }
            if brick.color_effect == lit.shown.1 {
                brick.color_effect = lit.effect;
            }
        }
        brick
    }

    /// Every brick of the owner flashes the palette's brightest colour
    /// with Glow, then returns to its own colour and effect.
    pub(super) fn highlight_brick_group(&mut self, group: OwnerId) -> Result<()> {
        if self.highlights.groups.contains_key(&group) {
            return Ok(()); // `isChainBlinking`
        }
        let world = self.simulation.state();
        // Brightest opaque entry: r + g + b + 10a.
        let color = world
            .palette
            .iter()
            .enumerate()
            .max_by(|a, b| {
                let score = |c: &[f32; 4]| c[0] + c[1] + c[2] + 10.0 * c[3];
                score(a.1).total_cmp(&score(b.1))
            })
            .map_or(0, |(i, _)| i as u8);
        let bricks: Vec<BrickId> = world
            .bricks
            .iter()
            .filter(|(_, b)| b.owner == group)
            .map(|(id, _)| *id)
            .collect();
        ensure!(!bricks.is_empty(), "Unknown brick group");
        let seconds = match bricks.len() {
            n if n > 10_000 => 3.0,
            n if n > 4000 => 2.0,
            n if n > 2000 => 1.5,
            _ => 1.0,
        };
        self.light_bricks(&bricks, Some(color), GLOW, seconds)?;
        let until = self.simulation.state().tick + (seconds * SECOND as f32) as u64;
        self.highlights.groups.insert(group, until);
        Ok(())
    }

    pub(super) fn step_highlights(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        self.highlights.groups.retain(|_, until| *until > tick);
        let done: Vec<BrickId> = self
            .highlights
            .bricks
            .iter()
            .filter(|(_, lit)| lit.until <= tick)
            .map(|(id, _)| *id)
            .collect();
        for id in done {
            let lit = self.highlights.bricks.remove(&id).expect("listed above");
            let Some(brick) = self.simulation.state().bricks.get(&id) else {
                continue;
            };
            let color = if brick.color == lit.shown.0 {
                lit.color
            } else {
                brick.color
            };
            let effect = if brick.color_effect == lit.shown.1 {
                lit.effect
            } else {
                brick.color_effect
            };
            if (color, effect) != (brick.color, brick.color_effect) {
                self.simulation.mutate(id, |b| {
                    b.color = color;
                    b.color_effect = effect;
                })?;
                self.dirty.insert(id);
            }
        }
        Ok(())
    }
}
