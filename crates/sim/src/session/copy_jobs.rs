//! Copy jobs: a duplicator's big work (selecting, planting, cutting,
//! painting, undoing, loading) done a slice each tick, so a copy of a
//! million bricks never holds a tick up. v20's New Duplicator worked the
//! same way (`ProcessPerTick`), behind a progress bar the player could
//! cancel.
//!
//! Every tick shares one budget of copy work ([`crate::simulation::work`])
//! among the players with a job, round robin. A command that starts a job
//! works on it at once with what is left of this tick's budget, so a job
//! that fits finishes within the command, as a small copy always did. A
//! player has one job at a time; while it runs, their duplicator's other
//! copy work is refused as busy. Its Add-On hears how far it has got
//! (`on_copy` with `working`) every [`REPORT_TICKS`], and the player may
//! cancel it (`cancel_copy`): what it did by then stays done, as one step
//! of their undo.
use super::*;

/// Copy work a tick allows by default: about 2.5 ms of a release build,
/// a third of a tick at 120 Hz.
pub const DEFAULT_COPY_WORK: u32 = 10_000;

/// The least copy work a job is given when it gets a turn: enough for
/// the dearest brick ([`crate::simulation::work::BREAK`]), so every job
/// moves.
const MIN_SHARE: u32 = 32;

/// How often a running job's Add-On hears how far it has got: four times
/// a second.
pub(super) const REPORT_TICKS: u64 = 30;

/// How a job stopped.
pub(super) enum Ending {
    Done,
    /// Its player cancelled it.
    Canceled,
    /// Something went wrong part way.
    Failed(anyhow::Error),
    /// Its player left.
    Left,
}

/// How far a job has got, for its Add-On's `on_copy`.
#[derive(Default)]
pub(super) struct Progress {
    /// The `on_copy` action it reports as when done.
    pub action: &'static str,
    /// Bricks done, of `total`.
    pub done: usize,
    pub total: usize,
    /// Bricks put in besides (a supercut's plain bricks).
    pub placed: usize,
    /// Bricks of `done` left as they were (no trust, or in the way).
    pub refused: usize,
    /// Bricks found and still to be looked around (a stack selection).
    pub queued: usize,
    /// How far through what it searches, in percent, while it searches
    /// (a box's buckets; a plant's later passes for bricks that now fit).
    pub searched: Option<usize>,
}

/// One kind of copy job.
pub(super) trait CopyWork: Send {
    fn progress(&self) -> Progress;
    /// Work as far as `budget` allows ([`crate::simulation::work`]); true
    /// once there is nothing left to do.
    fn step(&mut self, session: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<bool>;
    /// Wrap up: keep what was done, as one undo step, and report it.
    fn finish(self: Box<Self>, session: &mut Session, owner: OwnerId, ending: Ending);
}

struct Job {
    work: Box<dyn CopyWork>,
    /// The Add-On that started it, which hears its progress.
    package: Option<String>,
    /// The tick its progress was last reported.
    reported: u64,
}

pub(super) struct CopyJobs {
    jobs: BTreeMap<OwnerId, Job>,
    /// What is left of this tick's copy work.
    left: u32,
    per_tick: u32,
    /// Ticks the jobs have had turns: who goes first rotates, so with
    /// more jobs than a tick's work covers each still gets turns.
    turns: usize,
}
impl Default for CopyJobs {
    fn default() -> Self {
        Self {
            jobs: BTreeMap::new(),
            left: DEFAULT_COPY_WORK,
            per_tick: DEFAULT_COPY_WORK,
            turns: 0,
        }
    }
}

impl Session {
    /// Current authoritative admission for a mutating copy slice. A job never
    /// retains trust/administrator privileges from its starting tick.
    pub(super) fn live_copy_actor(
        &self,
        owner: OwnerId,
        action: Option<bri_minigames::BuildAction>,
    ) -> Result<Actor> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        if let Some(action) = action {
            combat::ensure_may_build(&peer.combat, &self.minigames, action)?;
        }
        Ok(peer.actor.clone())
    }
    /// Set how much copy work a tick allows (at least enough for one
    /// brick): the host's knob, and how tests make a small copy take many
    /// ticks.
    pub fn set_copy_work(&mut self, per_tick: u32) {
        self.copy_jobs.per_tick = per_tick.max(MIN_SHARE);
        self.copy_jobs.left = self.copy_jobs.left.min(self.copy_jobs.per_tick);
    }

    /// Whether `owner` has copy work running.
    pub fn copy_working(&self, owner: OwnerId) -> bool {
        self.copy_jobs.jobs.contains_key(&owner)
    }

    /// Refuse new copy work for `owner` while a job of theirs runs.
    pub(super) fn ensure_copy_idle(&self, owner: OwnerId) -> Result<()> {
        ensure!(
            !self.copy_working(owner),
            "Your duplicator is still working. Cancel it first."
        );
        Ok(())
    }

    /// Start `work` for `owner`: as far as this tick's copy work allows
    /// now, the rest over the next ticks. `package` hears its progress.
    pub(super) fn start_copy_job(
        &mut self,
        owner: OwnerId,
        package: Option<String>,
        mut work: Box<dyn CopyWork>,
    ) {
        debug_assert!(!self.copy_working(owner));
        let mut budget = self.copy_jobs.left;
        let stepped = work.step(self, owner, &mut budget);
        self.copy_jobs.left = budget;
        match stepped {
            Ok(true) => work.finish(self, owner, Ending::Done),
            Err(error) => work.finish(self, owner, Ending::Failed(error)),
            Ok(false) => self.keep_copy_job(owner, package, work),
        }
    }

    /// [`Self::start_copy_job`] for a caller that wraps up a job done at
    /// once itself: `work` back when it finished within this tick's copy
    /// work (not yet `finish`ed), else `None` and it goes on as a job.
    pub(super) fn begin_copy_job<W: CopyWork + 'static>(
        &mut self,
        owner: OwnerId,
        package: Option<String>,
        mut work: W,
    ) -> Result<Option<W>> {
        debug_assert!(!self.copy_working(owner));
        let mut budget = self.copy_jobs.left;
        let stepped = work.step(self, owner, &mut budget);
        self.copy_jobs.left = budget;
        match stepped {
            Ok(true) => Ok(Some(work)),
            Err(error) => {
                Box::new(work).finish(self, owner, Ending::Failed(error));
                Ok(None)
            }
            Ok(false) => {
                self.keep_copy_job(owner, package, Box::new(work));
                Ok(None)
            }
        }
    }

    fn keep_copy_job(&mut self, owner: OwnerId, package: Option<String>, work: Box<dyn CopyWork>) {
        let tick = self.simulation.state().tick;
        let progress = work.progress();
        self.copy_jobs.jobs.insert(
            owner,
            Job {
                work,
                package: package.clone(),
                reported: tick,
            },
        );
        if let Some(package) = package {
            self.report_progress(&package, owner, progress);
        }
    }

    /// Run `work` to the end now, whatever it costs: the engine's own
    /// callers and tests that want the whole result at once.
    pub(super) fn run_copy_work(&mut self, owner: OwnerId, work: &mut dyn CopyWork) -> Result<()> {
        let mut budget = u32::MAX;
        while !work.step(self, owner, &mut budget)? {
            budget = u32::MAX;
        }
        Ok(())
    }

    /// Stop `owner`'s running job, keeping what it did. False when none
    /// runs.
    pub fn cancel_copy(&mut self, owner: OwnerId) -> bool {
        match self.copy_jobs.jobs.remove(&owner) {
            Some(job) => {
                job.work.finish(self, owner, Ending::Canceled);
                true
            }
            None => false,
        }
    }

    /// A player who leaves stops their job.
    pub(super) fn forget_copy_job(&mut self, owner: OwnerId) {
        if let Some(job) = self.copy_jobs.jobs.remove(&owner) {
            job.work.finish(self, owner, Ending::Left);
        }
    }

    /// Each tick: a fresh budget of copy work, shared by the running jobs.
    pub(super) fn step_copy_jobs(&mut self) -> Result<()> {
        self.copy_jobs.left = self.copy_jobs.per_tick;
        // Lit selections wear off within the same budget, leaving the
        // jobs at least half.
        let share = if self.copy_jobs.jobs.is_empty() {
            self.copy_jobs.left
        } else {
            self.copy_jobs.left / 2
        };
        let mut budget = share;
        let lit = self.step_highlights(&mut budget);
        self.copy_jobs.left -= share - budget;
        lit?;
        let mut owners: Vec<OwnerId> = self.copy_jobs.jobs.keys().copied().collect();
        if !owners.is_empty() {
            let first = self.copy_jobs.turns % owners.len();
            owners.rotate_left(first);
            self.copy_jobs.turns = self.copy_jobs.turns.wrapping_add(1);
        }
        let mut waiting = owners.len() as u32;
        let tick = self.simulation.state().tick;
        for owner in owners {
            let Some(mut job) = self.copy_jobs.jobs.remove(&owner) else {
                continue;
            };
            // An even share of what is left (what one leaves goes on to
            // the next), and enough to move.
            let left = self.copy_jobs.left;
            let share = (left / waiting.max(1)).max(MIN_SHARE).min(left);
            waiting -= 1;
            let mut budget = share;
            let stepped = job.work.step(self, owner, &mut budget);
            self.copy_jobs.left -= share - budget;
            match stepped {
                Ok(true) => job.work.finish(self, owner, Ending::Done),
                Err(error) => job.work.finish(self, owner, Ending::Failed(error)),
                Ok(false) => {
                    if tick >= job.reported + REPORT_TICKS {
                        job.reported = tick;
                        if let Some(package) = &job.package {
                            let progress = job.work.progress();
                            self.report_progress(package, owner, progress);
                        }
                    }
                    self.copy_jobs.jobs.insert(owner, job);
                }
            }
        }
        Ok(())
    }

    /// Tell `package` how far `owner`'s job has got.
    fn report_progress(&mut self, package: &str, owner: OwnerId, progress: Progress) {
        let mut outcome = copy_store::CopyOutcome::about(progress.action, None, None);
        outcome.working = true;
        outcome.bricks = progress.done;
        outcome.total = progress.total;
        outcome.placed = progress.placed;
        outcome.refused = progress.refused;
        outcome.queued = progress.queued;
        outcome.searched = progress.searched;
        self.report_copy(package, owner, outcome);
    }
}

impl Session {
    /// What is left of this tick's copy work.
    pub(super) fn copy_jobs_left(&self) -> u32 {
        self.copy_jobs.left
    }
    pub(super) fn set_copy_jobs_left(&mut self, left: u32) {
        self.copy_jobs.left = left;
    }
}
