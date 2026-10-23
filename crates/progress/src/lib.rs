//! Engine-wide loading progress.
//!
//! Work that happens behind a loading screen (hosting, joining, a map change)
//! reports into one [`Progress`]: the stage it is in and how much of that
//! stage's work is done out of a known total. The loading screen polls a
//! [`Snapshot`] each frame and shows its [`Snapshot::status`] and
//! [`Snapshot::fraction`], like v20's `LoadingProgressTxt` and
//! `LoadingProgress`, which also restart from empty at each phase.
//!
//! Stages name what this engine actually does, not Torque's mission phases.
//! A stage whose total is unknown shows its name over an empty bar rather
//! than an invented fraction.
use bri_console::Clamp;
use std::sync::{Arc, Mutex, MutexGuard};

/// What the engine is doing. Loads visit the stages they need in their own
/// order: a host loads its map before serving, a joiner after connecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Stage {
    /// Nothing reported yet.
    #[default]
    Starting,
    /// Reading and hashing local content packs into the content identity the
    /// host and client must agree on.
    CheckingContent,
    /// Loading the map bundle: scene, terrain, collision, foliage.
    LoadingMap,
    /// Binding the host socket and opening the session.
    StartingServer,
    /// Opening the connection: QUIC, certificate check, identity handshake.
    Connecting,
    /// Fetching the server's packages this client lacks into its cache.
    DownloadingPackages,
    /// Checking the downloaded packages' files against the hashes the
    /// server promised, and installing them into the cache.
    VerifyingPackages,
    /// Loading the server's Add-Ons into this game: their bricks, weapons,
    /// vehicles and bots.
    LoadingAddOns,
    /// Joined; the host is preparing the world to send.
    WaitingForServer,
    /// Receiving the world checkpoint.
    ReceivingWorld,
    /// Meshing the received bricks into render chunks.
    BuildingBricks,
    /// Uploading textures, terrain and brick meshes to the graphics card.
    LoadingGraphics,
    /// Waiting for the graphics card's world shaders to finish compiling:
    /// seconds with DXC, minutes with FXC on a slow GPU, and again after
    /// the GPU drops out and the renderer restarts.
    CompilingShaders,
    /// The world is ready; waiting to enter it.
    Spawning,
}

impl Stage {
    /// Every stage, in the order a join visits them.
    pub const ALL: [Self; 14] = [
        Self::Starting,
        Self::CheckingContent,
        Self::LoadingMap,
        Self::StartingServer,
        Self::Connecting,
        Self::DownloadingPackages,
        Self::VerifyingPackages,
        Self::LoadingAddOns,
        Self::WaitingForServer,
        Self::ReceivingWorld,
        Self::BuildingBricks,
        Self::LoadingGraphics,
        Self::CompilingShaders,
        Self::Spawning,
    ];

    /// Whether this stage waits on the other end of the connection (or on
    /// the network itself), rather than on this computer's own work. A load
    /// that stops advancing in such a stage has a dead or stuck peer; one
    /// in a local stage is still working, however slow the machine.
    pub fn waits_on_peer(self) -> bool {
        match self {
            // Before the first report a joiner is resolving the address.
            Self::Starting
            | Self::Connecting
            | Self::DownloadingPackages
            | Self::WaitingForServer
            | Self::ReceivingWorld
            | Self::Spawning => true,
            Self::CheckingContent
            | Self::LoadingMap
            | Self::StartingServer
            | Self::VerifyingPackages
            | Self::LoadingAddOns
            | Self::BuildingBricks
            | Self::LoadingGraphics
            | Self::CompilingShaders => false,
        }
    }
}

/// What a stage's counts measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Unit {
    /// Discrete steps of a pipeline (files hashed, map parts loaded).
    #[default]
    Steps,
    Bytes,
    Bricks,
    /// Render chunks or GPU uploads.
    Chunks,
}

/// One consistent reading of a [`Progress`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub stage: Stage,
    pub unit: Unit,
    /// Current bounded task within a stage; cleared when the stage changes.
    pub detail: Option<&'static str>,
    /// Work done in this stage, never above `total`.
    pub done: u64,
    /// Work in this stage, when known.
    pub total: Option<u64>,
    /// Increases with every change, so pollers can skip unchanged readings.
    pub revision: u64,
}

impl Snapshot {
    /// This stage's completion, 0..=1. Zero while the total is unknown.
    pub fn fraction(&self) -> f32 {
        match self.total {
            Some(0) => 1.0,
            Some(total) => (self.done as f64 / total as f64).clamped(0.0, 1.0) as f32,
            None => 0.0,
        }
    }

    /// The loading bar text, upper case like v20's `LoadingProgressTxt`.
    pub fn status(&self) -> String {
        let label = match self.stage {
            Stage::Starting => "LOADING",
            Stage::CheckingContent => "CHECKING CONTENT",
            Stage::LoadingMap => "LOADING MAP",
            Stage::StartingServer => "STARTING SERVER",
            Stage::Connecting => "CONNECTING",
            Stage::DownloadingPackages => "DOWNLOADING ADD-ONS",
            Stage::VerifyingPackages => "VERIFYING ADD-ONS",
            Stage::LoadingAddOns => "LOADING ADD-ONS",
            Stage::WaitingForServer => "WAITING FOR SERVER",
            Stage::ReceivingWorld => "RECEIVING WORLD",
            Stage::BuildingBricks => "BUILDING BRICKS",
            Stage::LoadingGraphics => "LOADING GRAPHICS",
            Stage::CompilingShaders => "COMPILING SHADERS",
            Stage::Spawning => "SPAWNING",
        };
        let label = self
            .detail
            .map_or_else(|| label.to_owned(), |detail| format!("{label}: {detail}"));
        match (self.unit, self.total) {
            (_, None) | (_, Some(0)) | (Unit::Steps, _) => label,
            (Unit::Bytes, Some(total)) => {
                format!("{label}  {}", bytes_of(self.done, total))
            }
            (Unit::Bricks, Some(total)) => format!(
                "{label}  {} OF {} BRICKS",
                grouped(self.done),
                grouped(total)
            ),
            (Unit::Chunks, Some(total)) => format!("{label}  {} OF {}", self.done, total),
        }
    }
}

/// `done` of `total` bytes, in megabytes, or kilobytes when the total is
/// under one megabyte (many Add-Ons are a few kilobytes, which would read
/// "0.0 MB OF 0.0 MB").
fn bytes_of(done: u64, total: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if total < 1024 * 1024 {
        format!("{} KB OF {} KB", done.div_ceil(1024), total.div_ceil(1024))
    } else {
        format!("{:.1} MB OF {:.1} MB", done as f64 / MB, total as f64 / MB)
    }
}

fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A shared progress report. Clones report into the same state; any thread
/// may report or read. `Progress::default()` is a report nobody reads, for
/// callers without a loading screen.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    state: Arc<Mutex<Snapshot>>,
    subject: Arc<Mutex<Option<String>>>,
    trail: Arc<Mutex<Trail>>,
}

/// Finished stages, newest last, and how many have ever finished.
#[derive(Debug, Default)]
struct Trail {
    ended: std::collections::VecDeque<Snapshot>,
    count: u64,
}

/// Finished stages a [`Progress`] remembers; a load visits far fewer.
const TRAIL: usize = 64;

impl Progress {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, Snapshot> {
        // A reporter that panicked leaves a valid snapshot behind.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Enter `stage`, with nothing done yet out of `total` (None when not
    /// yet known).
    pub fn begin(&self, stage: Stage, unit: Unit, total: Option<u64>) {
        let mut s = self.state();
        if s.stage != Stage::Starting {
            let mut trail = self.trail.lock().unwrap_or_else(|e| e.into_inner());
            if trail.ended.len() == TRAIL {
                trail.ended.pop_front();
            }
            trail.ended.push_back(*s);
            trail.count += 1;
        }
        *s = Snapshot {
            stage,
            unit,
            detail: None,
            done: 0,
            total,
            revision: s.revision + 1,
        };
    }

    /// Report the actual work within a stage without restarting its counts.
    pub fn set_detail(&self, detail: Option<&'static str>) {
        let mut s = self.state();
        s.detail = detail;
        s.revision += 1;
    }

    /// Learn the current stage's total once known (a frame header arrives).
    pub fn set_total(&self, total: u64) {
        let mut s = self.state();
        s.total = Some(total);
        s.done = s.done.min(total);
        s.revision += 1;
    }

    /// Record `n` more units of work done in the current stage.
    pub fn advance(&self, n: u64) {
        let mut s = self.state();
        let done = s.done.saturating_add(n);
        s.done = s.total.map_or(done, |total| done.min(total));
        s.revision += 1;
    }

    /// Record the absolute work done in the current stage.
    pub fn set(&self, done: u64) {
        let mut s = self.state();
        s.done = s.total.map_or(done, |total| done.min(total));
        s.revision += 1;
    }

    /// Record the absolute work done only while still in `stage`, so a
    /// late report from a finished stage cannot overwrite the next one.
    pub fn set_in(&self, stage: Stage, done: u64) {
        let mut s = self.state();
        if s.stage == stage {
            s.done = s.total.map_or(done, |total| done.min(total));
            s.revision += 1;
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        *self.state()
    }

    /// The last reading of each stage this load has left, oldest first: a
    /// stage too quick for the loading screen to poll still shows here.
    pub fn trail(&self) -> Vec<Snapshot> {
        self.ended_since(0).0
    }

    /// The stages that finished after the first `seen` (as far as the
    /// trail still holds them), and how many have finished in all: pass
    /// that back as `seen` next time to get only newer ones.
    pub fn ended_since(&self, seen: u64) -> (Vec<Snapshot>, u64) {
        let trail = self.trail.lock().unwrap_or_else(|e| e.into_inner());
        let held = trail.count - trail.ended.len() as u64;
        let skip = seen.saturating_sub(held).min(trail.ended.len() as u64) as usize;
        (
            trail.ended.iter().skip(skip).copied().collect(),
            trail.count,
        )
    }

    /// Name what is loading once it is known (a joiner learns the map from
    /// the host's first message), so the loading screen can show it.
    pub fn set_subject(&self, subject: impl Into<String>) {
        *self.subject.lock().unwrap_or_else(|e| e.into_inner()) = Some(subject.into());
        self.state().revision += 1;
    }

    pub fn subject(&self) -> Option<String> {
        self.subject
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_restart_the_bar_and_ignore_late_reports() {
        let p = Progress::new();
        p.begin(Stage::ReceivingWorld, Unit::Bytes, None);
        assert_eq!(p.snapshot().fraction(), 0.0);
        assert_eq!(p.snapshot().status(), "RECEIVING WORLD");
        p.set_total(4 * 1024 * 1024);
        p.advance(1024 * 1024);
        assert_eq!(p.snapshot().fraction(), 0.25);
        assert_eq!(p.snapshot().status(), "RECEIVING WORLD  1.0 MB OF 4.0 MB");
        p.begin(Stage::BuildingBricks, Unit::Chunks, Some(8));
        p.set(2);
        assert_eq!(p.snapshot().fraction(), 0.25);
        p.set_in(Stage::ReceivingWorld, 7);
        assert_eq!(p.snapshot().done, 2);
    }

    #[test]
    fn task_details_reach_the_loading_screen_without_restarting_progress() {
        let p = Progress::new();
        p.begin(Stage::LoadingMap, Unit::Steps, Some(8));
        p.set(3);
        p.set_detail(Some("PREPARING COLLISION"));
        assert_eq!(p.snapshot().status(), "LOADING MAP: PREPARING COLLISION");
        assert_eq!((p.snapshot().done, p.snapshot().fraction()), (3, 0.375));
        p.begin(Stage::StartingServer, Unit::Steps, None);
        assert_eq!(p.snapshot().status(), "STARTING SERVER");
        assert_eq!(p.snapshot().detail, None);
    }

    #[test]
    fn every_stage_is_listed_once_and_names_itself() {
        // Adding a stage fails this match until it is listed in `ALL`.
        let position = |stage: Stage| match stage {
            Stage::Starting => 0,
            Stage::CheckingContent => 1,
            Stage::LoadingMap => 2,
            Stage::StartingServer => 3,
            Stage::Connecting => 4,
            Stage::DownloadingPackages => 5,
            Stage::VerifyingPackages => 6,
            Stage::LoadingAddOns => 7,
            Stage::WaitingForServer => 8,
            Stage::ReceivingWorld => 9,
            Stage::BuildingBricks => 10,
            Stage::LoadingGraphics => 11,
            Stage::CompilingShaders => 12,
            Stage::Spawning => 13,
        };
        let mut labels = std::collections::BTreeSet::new();
        for (i, stage) in Stage::ALL.into_iter().enumerate() {
            assert_eq!(position(stage), i, "{stage:?} out of place in Stage::ALL");
            let label = Snapshot {
                stage,
                ..Snapshot::default()
            }
            .status();
            assert!(!label.is_empty() && labels.insert(label), "{stage:?}");
        }
    }

    #[test]
    fn a_download_reads_as_add_ons_in_megabytes() {
        let p = Progress::new();
        p.begin(
            Stage::DownloadingPackages,
            Unit::Bytes,
            Some(30 * 1024 * 1024),
        );
        p.set_in(Stage::DownloadingPackages, 12 * 1024 * 1024 + 400 * 1024);
        assert_eq!(
            p.snapshot().status(),
            "DOWNLOADING ADD-ONS  12.4 MB OF 30.0 MB"
        );
        assert!((p.snapshot().fraction() - 0.413).abs() < 0.001);
        p.begin(
            Stage::DownloadingPackages,
            Unit::Bytes,
            Some(48 * 1024 + 10),
        );
        p.set(12 * 1024);
        assert_eq!(p.snapshot().status(), "DOWNLOADING ADD-ONS  12 KB OF 49 KB");
    }

    #[test]
    fn the_trail_keeps_each_finished_stage_as_it_ended() {
        let p = Progress::new();
        p.set_subject("Slate");
        p.begin(Stage::Connecting, Unit::Steps, None);
        p.begin(Stage::DownloadingPackages, Unit::Bytes, Some(10));
        p.set(10);
        p.begin(Stage::VerifyingPackages, Unit::Steps, Some(2));
        let trail = p.trail();
        let stages: Vec<_> = trail.iter().map(|s| s.stage).collect();
        assert_eq!(stages, [Stage::Connecting, Stage::DownloadingPackages]);
        assert_eq!((trail[1].done, trail[1].total), (10, Some(10)));
        let (newer, seen) = p.ended_since(1);
        assert_eq!(newer.len(), 1);
        assert_eq!((newer[0].stage, seen), (Stage::DownloadingPackages, 2));
        assert!(p.ended_since(seen).0.is_empty());
        for _ in 0..2 * TRAIL {
            p.begin(Stage::LoadingMap, Unit::Steps, None);
        }
        assert_eq!(p.trail().len(), TRAIL);
        let (newer, all) = p.ended_since(seen + TRAIL as u64);
        assert_eq!((newer.len(), all), (TRAIL, seen + 2 * TRAIL as u64));
        assert_eq!(p.ended_since(all - 1).0.len(), 1);
    }

    #[test]
    fn counts_never_pass_the_total_and_bricks_read_grouped() {
        let p = Progress::new();
        p.begin(Stage::ReceivingWorld, Unit::Bricks, Some(50_000));
        p.advance(12_345);
        assert_eq!(
            p.snapshot().status(),
            "RECEIVING WORLD  12,345 OF 50,000 BRICKS"
        );
        p.advance(u64::MAX);
        assert_eq!(p.snapshot().done, 50_000);
        assert_eq!(p.snapshot().fraction(), 1.0);
        let before = p.snapshot().revision;
        p.advance(0);
        assert!(p.snapshot().revision > before);
    }
}
