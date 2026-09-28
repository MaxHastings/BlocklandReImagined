//! v20 `serverDirectSaveFileLoad` / `ServerLoadSaveFile_Tick`: a loaded save
//! is announced (`MsgUploadStart`), appears a few bricks at a time while the
//! game keeps running, and ends with `MsgProcessComplete`. The whole save is
//! validated and its owners and colors resolved when the load starts; each
//! batch is then published against the world as it is at that moment.
use super::*;

/// Ticks between published batches (100 ms at 120 ticks/s). Every batch
/// is one brick delta and one client mesh update.
const BATCH_TICKS: u64 = 12;
/// Smallest batch, so small saves visibly build up like v20's per-brick load.
const MIN_BATCH: usize = 25;
/// Large saves finish in about this many batches (8 s) instead of v20's
/// minutes.
const TARGET_BATCHES: usize = 80;

pub(super) struct Loading {
    loader: OwnerId,
    started: u64,
    next_batch: u64,
    batch: usize,
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
            next_batch: tick,
            batch: MIN_BATCH.max(total.div_ceil(TARGET_BATCHES)),
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

    /// Publish the next batch when it is due.
    pub(super) fn step_build_load(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let Some(loading) = self.loading.as_deref_mut() else {
            return Ok(());
        };
        if tick < loading.next_batch {
            return Ok(());
        }
        loading.next_batch = tick + BATCH_TICKS;
        let count = loading.batch.min(loading.bricks.len());
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
        let result = bri_world::build::LoadPlan::batch(
            self.simulation.state(),
            &palette,
            bricks,
            self.next_owner,
        )
        .and_then(|plan| {
            self.item_spawners
                .validate_append(self.simulation.state(), plan.bricks())?;
            self.simulation.load_build(&actor, plan)
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
        if self.loading.as_deref().is_some_and(|l| l.bricks.is_empty()) {
            self.end_build_load();
        }
        Ok(())
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
