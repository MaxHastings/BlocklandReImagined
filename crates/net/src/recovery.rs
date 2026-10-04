//! A host's crash-recovery snapshot: one file per hosted world, in the
//! host's own state folder (never a folder of saves the player browses),
//! replaced in place while the world changes and deleted when the host
//! stops cleanly. A file that is still there when the next game starts
//! means the last one ended abnormally (a crash, a host fault, a killed
//! server), and only then is it offered back.
//!
//! v20 never saved on its own, and a timer that kept adding save files was
//! removed for filling the save folder (`df90261`): this is one hidden slot,
//! not a save history.
use anyhow::Result;
use bri_sim::session::Session;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Where a host keeps its recovery snapshot, and how often it looks.
#[derive(Debug, Clone)]
pub struct Recovery {
    pub path: PathBuf,
    pub every: Duration,
}
impl Recovery {
    /// A crash loses at most about this much building. Writing happens off
    /// the host's tick and only when something changed.
    pub const EVERY: Duration = Duration::from_secs(60);
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            every: Self::EVERY,
        }
    }
}

/// The world and mini-game a snapshot holds (`Session::recovery_snapshot`).
pub type Snapshot = (bri_world::World, Option<serde_json::Value>);

/// Write `snapshot` to `path` as a build (with events, ownership and its
/// mini-game), replacing what was there in one atomic step.
pub fn write(path: &Path, snapshot: Snapshot) -> Result<()> {
    let (world, minigame) = snapshot;
    let mut build = bri_world::build::SavedBuild::capture(&world, true, true)?;
    build.minigame = minigame;
    build.validate()?;
    bri_files::replace(path, &bri_world::build::encode(&build)?)?;
    Ok(())
}

/// What a snapshot is compared by: the world's revision and the mini-game.
type Fingerprint = (u64, Option<serde_json::Value>);

/// When to take a snapshot. The clock is passed in, so tests drive it.
pub(crate) struct Schedule {
    every: Duration,
    next: Instant,
    /// What the slot (or, before the first write, the starting world) holds.
    kept: Fingerprint,
}
impl Schedule {
    pub(crate) fn new(every: Duration, now: Instant, start: &Snapshot) -> Self {
        Self {
            every,
            next: now + every,
            kept: (start.0.revision, start.1.clone()),
        }
    }
    /// Whether to look at the world now. Cheap: called every tick.
    pub(crate) fn due(&self, now: Instant) -> bool {
        now >= self.next
    }
    /// After a look at `now`: the snapshot to write, when it differs from
    /// what is kept.
    pub(crate) fn take(&mut self, now: Instant, snapshot: Snapshot) -> Option<Snapshot> {
        self.next = now + self.every;
        let fingerprint = (snapshot.0.revision, snapshot.1.clone());
        (fingerprint != self.kept).then(|| {
            self.kept = fingerprint;
            snapshot
        })
    }
}

/// A running host's recovery slot.
pub(crate) struct Keeper {
    path: PathBuf,
    schedule: Schedule,
    writing: Option<tokio::task::JoinHandle<()>>,
}
impl Keeper {
    pub(crate) fn new(recovery: Recovery, session: &Session) -> Self {
        Self {
            schedule: Schedule::new(recovery.every, Instant::now(), &session.recovery_snapshot()),
            path: recovery.path,
            writing: None,
        }
    }
    /// Called each tick: on schedule, hands a changed snapshot to a
    /// blocking thread to encode and write. Taking it copies no brick.
    pub(crate) fn tick(&mut self, now: Instant, session: &Session) {
        if !self.schedule.due(now) || self.writing.as_ref().is_some_and(|w| !w.is_finished()) {
            return;
        }
        if let Some(snapshot) = self.schedule.take(now, session.recovery_snapshot()) {
            let path = self.path.clone();
            self.writing = Some(tokio::task::spawn_blocking(move || {
                if let Err(error) = write(&path, snapshot) {
                    eprintln!("Could not keep the recovery snapshot: {error:#}");
                }
            }));
        }
    }
    /// The host stopped. Cleanly: the slot is deleted (nothing to
    /// recover). After a failure: the final world is written to it, and its
    /// path returned.
    pub(crate) fn finish(
        self,
        failed: bool,
        session: &Session,
    ) -> impl std::future::Future<Output = Option<PathBuf>> + Send + use<> {
        // A session a panic left half-changed may panic again; the last
        // scheduled snapshot then stays.
        let last = failed.then(|| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session.recovery_snapshot()))
        });
        self.finish_with(last)
    }
    async fn finish_with(mut self, last: Option<std::thread::Result<Snapshot>>) -> Option<PathBuf> {
        if let Some(writing) = self.writing.take() {
            let _ = writing.await;
        }
        let Some(last) = last else {
            match std::fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => eprintln!(
                    "Could not remove the recovery snapshot {}: {error}",
                    self.path.display()
                ),
            }
            return None;
        };
        let path = self.path.clone();
        let written = match last {
            Ok(snapshot) => tokio::task::spawn_blocking(move || write(&path, snapshot))
                .await
                .map_err(anyhow::Error::from)
                .and_then(|r| r),
            Err(_) => Err(anyhow::anyhow!("the world could not be read")),
        };
        match written {
            Ok(()) => Some(self.path),
            Err(error) => {
                eprintln!("Could not keep the final world for recovery: {error:#}");
                self.path.is_file().then_some(self.path)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world(revision: u64) -> bri_world::World {
        let mut world = bri_world::World::new("w".into(), "map".into(), vec![[1.0; 4]]);
        world.revision = revision;
        world
    }
    #[test]
    fn a_snapshot_is_taken_on_schedule_and_only_when_the_world_changed() {
        let start = Instant::now();
        let every = Duration::from_secs(60);
        let mut schedule = Schedule::new(every, start, &(world(5), None));
        assert!(!schedule.due(start + Duration::from_secs(59)));
        let at = start + every;
        assert!(schedule.due(at));
        // Nobody built: nothing is written, and the next look is a full
        // interval later.
        assert!(schedule.take(at, (world(5), None)).is_none());
        assert!(!schedule.due(at + Duration::from_secs(30)));
        let at = at + every;
        assert!(schedule.take(at, (world(6), None)).is_some());
        // The same world again writes nothing.
        let at = at + every;
        assert!(schedule.take(at, (world(6), None)).is_none());
        // A mini-game change alone (teams, settings) is kept too.
        let at = at + every;
        let game = Some(serde_json::json!({"teams":[{"name":"Red"}]}));
        assert!(schedule.take(at, (world(6), game.clone())).is_some());
        assert!(schedule.take(at + every, (world(6), game)).is_none());
    }
}
