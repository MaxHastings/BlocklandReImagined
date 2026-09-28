//! Administrator world commands from the Admin menu and chat
//! (`ServerCmdHilightBrickGroup`, `RealBrickCount`, `CancelAllEvents`,
//! `ClearBots`).
use super::*;

/// `setColorFX(3)`: Glow.
const GLOW: u8 = 3;

pub(super) struct Highlight {
    until: u64,
    /// Brick and the color/effect it had before.
    bricks: Vec<(BrickId, u8, u8)>,
}

impl Session {
    /// Every brick of the owner flashes the palette's brightest color with
    /// Glow, then returns to its own color and effect.
    pub(super) fn highlight_brick_group(&mut self, group: OwnerId) -> Result<()> {
        if self.highlights.contains_key(&group) {
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
        let bricks: Vec<_> = world
            .bricks
            .iter()
            .filter(|(_, b)| b.owner == group)
            .map(|(id, b)| (*id, b.color, b.color_effect))
            .collect();
        ensure!(!bricks.is_empty(), "Unknown brick group");
        let ms: u64 = match bricks.len() {
            n if n > 10_000 => 3000,
            n if n > 4000 => 2000,
            n if n > 2000 => 1500,
            _ => 1000,
        };
        for (id, _, _) in &bricks {
            self.simulation.mutate(*id, |b| {
                b.color = color;
                b.color_effect = GLOW;
            })?;
            self.dirty.insert(*id);
        }
        let until = self.simulation.state().tick + ms * 120 / 1000;
        self.highlights.insert(group, Highlight { until, bricks });
        Ok(())
    }
    pub(super) fn step_highlights(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let done: Vec<OwnerId> = self
            .highlights
            .iter()
            .filter(|(_, h)| h.until <= tick)
            .map(|(g, _)| *g)
            .collect();
        for group in done {
            let highlight = self.highlights.remove(&group).unwrap();
            for (id, color, effect) in highlight.bricks {
                if self.simulation.state().bricks.contains_key(&id) {
                    self.simulation.mutate(id, |b| {
                        b.color = color;
                        b.color_effect = effect;
                    })?;
                    self.dirty.insert(id);
                }
            }
        }
        Ok(())
    }
    /// `/brickCount` (anyone) and `/realBrickCount` (admins): the server's
    /// bricks, told to whoever asked.
    pub(super) fn brick_count(&mut self, asker: OwnerId) {
        let count = self.simulation.state().bricks.len();
        let text = if count == 1 {
            "1 brick".to_string()
        } else {
            format!("{count} bricks")
        };
        self.notify(asker, Notice::Chat(text));
    }
    /// `/cancelAllEvents`: drop every scheduled event row.
    pub(super) fn admin_cancel_all_events(&mut self, admin: OwnerId) {
        let name = self.peers.get(&admin).map_or_else(String::new, |p| p.name.clone());
        self.system_chat(format!("\u{E003}{name}\u{E000} canceled all events."));
        self.cancel_all_events();
    }
    /// `/clearBots`: remove every bot; its spawn brick keeps the setting and
    /// brings it back when respawned.
    pub(super) fn admin_clear_bots(&mut self, admin: OwnerId) -> Result<()> {
        let bricks = self.bot_bricks();
        for brick in &bricks {
            self.reconcile_bot_brick(*brick, None)?;
        }
        let name = self.peers.get(&admin).map_or_else(String::new, |p| p.name.clone());
        self.system_chat(format!(
            "\u{E003}{name}\u{E000} cleared all bots ({}).",
            bricks.len()
        ));
        Ok(())
    }
}
