//! v20 `serverDirectSaveFileLoad` / `ServerLoadSaveFile_Tick`: a loaded save
//! is announced (`MsgUploadStart`), appears a few bricks at a time while the
//! game keeps running, and ends with `MsgProcessComplete`. The save's owners
//! and colours are resolved when the load starts; each slice of bricks is
//! then validated and published against the world as it is at that moment.
//! As in v20's `ServerLoadSaveFile_Tick`, a brick that cannot be planted is
//! skipped and counted against the "created / total" line.
use super::*;

/// How long a tick that is loading may take in all: placing bricks and the
/// systems that react to them. The
/// rest of the tick's 8.3 ms is left for the network and everything else.
const STEP_BUDGET: std::time::Duration = std::time::Duration::from_millis(7);
/// Bricks placed between budget checks, and the least a tick places, so a
/// load always moves forward however slow the host is.
const SLICE: usize = 256;

/// Where the time of a loading tick goes, so placing stops early enough
/// for what follows it to fit the tick's budget too.
#[derive(Debug)]
pub(super) struct LoadClock {
    step_started: std::time::Instant,
    placed_at: Option<std::time::Instant>,
    /// Last loading tick's work after placing (the systems reacting to
    /// the new bricks).
    tail: std::time::Duration,
}
impl Default for LoadClock {
    fn default() -> Self {
        Self {
            step_started: std::time::Instant::now(),
            placed_at: None,
            tail: Default::default(),
        }
    }
}
impl LoadClock {
    pub(super) fn start_step(&mut self) {
        self.step_started = std::time::Instant::now();
    }
    pub(super) fn end_step(&mut self) {
        if let Some(at) = self.placed_at.take() {
            self.tail = at.elapsed();
        }
    }
    /// Whether this tick has time left to place another slice.
    fn spare(&self) -> bool {
        self.step_started.elapsed() + self.tail < STEP_BUDGET
    }
}

/// How fast a load goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadPace {
    /// As many bricks as keep a tick inside [`STEP_BUDGET`].
    Budget,
    /// Exactly this many bricks each tick, whatever the machine: tests use
    /// it to see a load unfold the same way every run.
    Bricks(usize),
}

pub(super) struct Loading {
    loader: OwnerId,
    started: u64,
    /// Colours and owners of the save in this world; each brick is
    /// validated and mapped as its slice is placed.
    mapping: bri_world::build::LoadMapping,
    /// Saved bricks still to place, in save order.
    bricks: Queue,
    total: usize,
    created: usize,
    /// The build's mini-game, set up once its bricks are in.
    minigame: Option<serde_json::Value>,
}

/// Saved bricks still to place, in save order, copied out a slice at a time
/// so a big save costs nothing up front.
struct Queue {
    saved: bri_world::Bricks,
    /// The smallest saved id not taken yet.
    next: BrickId,
    /// Bricks the saving server could not place, which this one can.
    extra: VecDeque<Brick>,
    left: usize,
}
impl Queue {
    fn is_empty(&self) -> bool {
        self.left == 0
    }
    fn take(&mut self, count: usize) -> Vec<Brick> {
        let mut out = Vec::with_capacity(count);
        let mut last = None;
        for (id, brick) in self.saved.range(self.next..).take(count) {
            out.push(brick.clone());
            last = Some(*id);
        }
        if let Some(id) = last {
            self.next = id.saturating_add(1);
        }
        let more = count - out.len();
        out.extend(self.extra.drain(..more.min(self.extra.len())));
        self.left -= out.len();
        out
    }
    fn clear(&mut self) {
        self.saved = Default::default();
        self.extra.clear();
        self.left = 0;
    }
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
        // Only reads of the save happen here, on the authority's tick:
        // colours, owners, item capacity and which bricks this server can
        // place. Each brick is validated and mapped as it is placed.
        let mut mapping = bri_world::build::LoadMapping::new(
            self.simulation.state(),
            &build,
            owner,
            ownership,
            self.next_owner,
        )?;
        self.item_spawners.validate_append(
            self.simulation.state(),
            build.world.bricks.values().chain(&build.world.unloaded),
        )?;
        // Query the existing named-target index; no scan/copy of the live
        // world, and no automatic rename of deliberately connected builds.
        let shared_names = self.events.world.as_ref().is_some_and(|events| {
            build
                .world
                .bricks
                .values()
                .chain(&build.world.unloaded)
                .any(|brick| {
                    brick.name.as_deref().is_some_and(|name| {
                        mapping
                            .owner(brick.owner)
                            .is_ok_and(|scope| events.has_named_brick(scope, name))
                    })
                })
        });
        // The saved builders' numbers are claimed now; their bricks follow
        // in slices.
        for (number, record) in mapping.take_owners() {
            self.simulation.claim_owner(number, record)?;
        }
        self.next_owner = mapping.next_owner;
        // Bricks without a definition here are kept with the world, not
        // placed, and the rest of the save still loads.
        let minigame = build.minigame;
        let mut world = build.world;
        let mut known: std::collections::HashMap<String, bool> = Default::default();
        let mut placeable = |brick: &Brick| {
            let bri_world::ContentRef::Resolved(id) = &brick.definition else {
                return false;
            };
            if let Some(placeable) = known.get(id) {
                return *placeable;
            }
            let placeable = self.simulation.definitions.get(brick).is_ok();
            known.insert(id.clone(), placeable);
            placeable
        };
        let unplaceable: Vec<BrickId> = world
            .bricks
            .iter()
            .filter(|(_, brick)| !placeable(brick))
            .map(|(id, _)| *id)
            .collect();
        let mut unloaded: Vec<Brick> = unplaceable
            .iter()
            .filter_map(|id| world.bricks.remove(id))
            .collect();
        let (extra, still): (Vec<Brick>, Vec<Brick>) = std::mem::take(&mut world.unloaded)
            .into_iter()
            .partition(|b| placeable(b));
        unloaded.extend(still);
        let bricks = Queue {
            left: world.bricks.len() + extra.len(),
            next: 0,
            saved: world.bricks,
            extra: extra.into(),
        };
        let skipped = crate::simulation::unloaded_summary(&unloaded);
        if !unloaded.is_empty() {
            let unloaded = unloaded
                .into_iter()
                .map(|brick| mapping.brick(brick))
                .collect::<Result<Vec<_>>>()?;
            self.simulation.keep_unloaded(&mapping.palette, unloaded)?;
        }
        let total = bricks.left;
        let tick = self.simulation.state().tick;
        self.loading = Some(Box::new(Loading {
            loader: owner,
            started: tick,
            mapping,
            bricks,
            total,
            created: 0,
            minigame,
        }));
        self.system_message(
            Some(MessageTag::UploadStart),
            "Loading bricks. Please wait.".into(),
        );
        if shared_names {
            self.notify(
                owner,
                Notice::Center {
                    text: "Shared brick names: events can affect both builds.".into(),
                    seconds: 8.0,
                },
            );
        }
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
        let mut published = 0;
        while self
            .loading
            .as_deref()
            .is_some_and(|l| !l.bricks.is_empty())
        {
            let slice = match self.load_pace {
                LoadPace::Budget => SLICE,
                LoadPace::Bricks(count) => (count - published).min(SLICE),
            };
            self.publish_load_slice(slice);
            published += slice;
            let done = match self.load_pace {
                LoadPace::Budget => !self.load_clock.spare(),
                LoadPace::Bricks(count) => published >= count,
            };
            if done {
                break;
            }
        }
        // This tick's bricks are solid before anything else looks: joins
        // and spawn checks after the tick must find them, as they find a
        // planted brick. One refresh for all of the tick's slices.
        if published > 0 {
            self.simulation.refresh_collisions();
        }
        self.load_clock.placed_at = Some(std::time::Instant::now());
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
        let count = count.min(loading.bricks.left);
        // v20 `ServerLoadSaveFile_Tick` skips a line it cannot plant and
        // counts it as a failure; the load carries on.
        let mapping = &loading.mapping;
        let mapped: Vec<Brick> = loading
            .bricks
            .take(count)
            .into_iter()
            .filter_map(|brick| mapping.brick(brick).ok())
            .collect();
        let bricks: Result<Vec<Brick>> = Ok(mapped
            .into_iter()
            .filter(|brick| self.simulation.fits_grid(brick))
            .collect());
        let loader = loading.loader;
        let palette = std::mem::take(&mut loading.mapping.palette);
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
        let result = bricks
            .and_then(|bricks| self.simulation.drop_overlapping(bricks))
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
        loading.mapping.palette = palette;
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
        if let Some(minigame) = loading.minigame {
            self.restore_saved_minigame(loading.loader, minigame);
        }
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
