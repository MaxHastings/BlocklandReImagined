//! v20 `serverDirectSaveFileLoad` / `ServerLoadSaveFile_Tick`: a loaded save
//! is announced (`MsgUploadStart`), appears a few bricks at a time while the
//! game keeps running, and ends with `MsgProcessComplete`. The whole save is
//! validated and its owners and colors resolved when the load starts; each
//! batch is then published against the world as it is at that moment.
use super::*;

/// How much of each tick a load may take. A load publishes bricks until
/// the budget is spent, so it goes as fast as the host can place bricks while
/// every tick stays well inside its 8.3 ms. v20 planted a few bricks a tick
/// and took minutes over a big save.
const TICK_BUDGET: std::time::Duration = std::time::Duration::from_millis(4);
/// Bricks placed between budget checks, and the least a tick places, so a
/// load always moves forward however slow the host is.
const SLICE: usize = 256;

/// How fast a load goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadPace {
    /// As many bricks as fit in [`TICK_BUDGET`] each tick.
    Budget,
    /// Exactly this many bricks each tick, whatever the machine: tests use
    /// it to see a load unfold the same way every run.
    Bricks(usize),
}

pub(super) struct Loading {
    loader: OwnerId,
    started: u64,
    palette: Vec<[f32; 4]>,
    bricks: VecDeque<Brick>,
    total: usize,
    created: usize,
}

/// v20 `getTimeString(mFloor(seconds * 100) / 100)`: `0:03.27`, `1:05`.
fn time_string(hundredths: u64) -> String {
    let whole = hundredths / 100;
    let fraction = hundredths % 100;
    let seconds = if fraction == 0 {
        format!("{:02}", whole % 60)
    } else {
        format!(
            "{:02}.{}",
            whole % 60,
            format!("{fraction:02}").trim_end_matches('0')
        )
    };
    match whole {
        s if s >= 3600 => format!("{}:{:02}:{seconds}", s / 3600, s % 3600 / 60),
        s => format!("{}:{seconds}", s / 60),
    }
}

impl Session {
    /// Start loading a save; the bricks arrive over the next ticks.
    pub(super) fn start_build_load(
        &mut self,
        owner: OwnerId,
        build: bri_world::build::SavedBuild,
        ownership: bool,
    ) -> Result<usize> {
        ensure!(self.loading.is_none(), "There is another load in progress.");
        let mut plan = bri_world::build::LoadPlan::prepare(
            self.simulation.state(),
            build,
            owner,
            ownership,
            self.next_owner,
        )?;
        self.simulation.preflight_load(&plan)?;
        self.item_spawners
            .validate_append(self.simulation.state(), plan.bricks())?;
        // The saved builders' numbers are claimed now; their bricks follow
        // in batches.
        for (number, record) in plan.take_owners() {
            self.simulation.claim_owner(number, record)?;
        }
        self.next_owner = plan.next_owner;
        let (palette, bricks) = plan.into_parts();
        // Bricks without a definition here are kept with the world, not
        // placed, and the rest of the save still loads.
        let (bricks, unloaded) = self.simulation.split_placeable(bricks);
        let skipped = crate::simulation::unloaded_summary(&unloaded);
        if !unloaded.is_empty() {
            self.simulation.keep_unloaded(&palette, unloaded)?;
        }
        let total = bricks.len();
        let tick = self.simulation.state().tick;
        self.loading = Some(Box::new(Loading {
            loader: owner,
            started: tick,
            palette,
            bricks: bricks.into(),
            total,
            created: 0,
        }));
        self.system_message(
            Some(MessageTag::UploadStart),
            "Loading bricks. Please wait.".into(),
        );
        if let Some(skipped) = skipped {
            self.system_chat(skipped);
        }
        Ok(total)
    }

    /// Choose how fast loads go ([`LoadPace::Budget`] unless set).
    pub fn set_load_pace(&mut self, pace: LoadPace) {
        self.load_pace = pace;
    }

    /// Publish this tick's bricks, a slice at a time until the tick's
    /// budget is spent.
    pub(super) fn step_build_load(&mut self) -> Result<()> {
        if self.loading.is_none() {
            return Ok(());
        }
        let started = std::time::Instant::now();
        let mut published = 0;
        while self.loading.as_deref().is_some_and(|l| !l.bricks.is_empty()) {
            let slice = match self.load_pace {
                LoadPace::Budget => SLICE,
                LoadPace::Bricks(count) => (count - published).min(SLICE),
            };
            self.publish_load_slice(slice);
            published += slice;
            let done = match self.load_pace {
                LoadPace::Budget => started.elapsed() >= TICK_BUDGET,
                LoadPace::Bricks(count) => published >= count,
            };
            if done {
                break;
            }
        }
        // One collision refresh for the whole tick's bricks.
        self.simulation.refresh_collisions();
        if self.loading.as_deref().is_some_and(|l| l.bricks.is_empty()) {
            self.end_build_load();
        }
        Ok(())
    }

    /// Place the next `count` bricks of the load.
    fn publish_load_slice(&mut self, count: usize) {
        let Some(loading) = self.loading.as_deref_mut() else {
            return;
        };
        let count = count.min(loading.bricks.len());
        let bricks: Vec<Brick> = loading.bricks.drain(..count).collect();
        let loader = loading.loader;
        let palette = std::mem::take(&mut loading.palette);
        // The loader may have left; the host's authority carries on, as
        // v20's load keeps running for its brick group.
        let actor = Actor {
            owner: loader,
            administrator: true,
            ..Default::default()
        };
        // Bricks overlapping what is already built are skipped, as v20's
        // load deletes a brick whose plant() reports an overlap; they count
        // against the "created / total" line.
        let result = self
            .simulation
            .drop_overlapping(bricks)
            .and_then(|bricks| {
                if bricks.is_empty() {
                    return Ok(Vec::new());
                }
                let plan = bri_world::build::LoadPlan::batch(
                    self.simulation.state(),
                    &palette,
                    bricks,
                    self.next_owner,
                )?;
                // The item capacity was checked for the whole save when the
                // load started; a player's own item bricks meet it as they
                // are reconciled, as they always do.
                self.simulation.load_build_unrefreshed(&actor, plan)
            });
        let loading = self.loading.as_deref_mut().expect("load in progress");
        loading.palette = palette;
        match result {
            Ok(ids) => {
                loading.created += ids.len();
                self.dirty.extend(ids);
            }
            Err(error) => {
                // v20 stops at the brick limit with a message; any other
                // failure ends the load the same way.
                loading.bricks.clear();
                self.system_chat(format!("{error:#}"));
            }
        }
    }

    /// `ServerLoadSaveFile_End`.
    fn end_build_load(&mut self) {
        let Some(loading) = self.loading.take() else {
            return;
        };
        let ticks = self.simulation.state().tick - loading.started;
        let text = format!(
            "{} / {} bricks created in {}",
            loading.created,
            loading.total,
            time_string(ticks * 100 / 120)
        );
        self.system_message(Some(MessageTag::ProcessComplete), text);
    }

    /// Whether a save is still being loaded.
    pub fn build_loading(&self) -> bool {
        self.loading.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::time_string;
    #[test]
    fn v20_time_strings() {
        assert_eq!(time_string(327), "0:03.27");
        assert_eq!(time_string(300), "0:03");
        assert_eq!(time_string(1250), "0:12.5");
        assert_eq!(time_string(6500), "1:05");
        assert_eq!(time_string(366_001), "1:01:00.01");
    }
}
