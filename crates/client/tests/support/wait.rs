//! Waiting on a game in tests: on what it does, never on a wall-clock
//! deadline. A loaded machine (the gate builds and tests at once) slows the
//! hosted server, whose ticker skips missed ticks, and every client with it,
//! so a fixed wait in wall time fails a slow but healthy run.
//!
//! A wait ends when `ready` holds. It fails when:
//! - every app is in game and the server ticks they have all seen pass its
//!   budget of game time without `ready` (only forward steps count, so a
//!   rehost starting from tick 0 does not end it). The clock starts once
//!   every app's scene pipelines have compiled: a step that began with them
//!   compiling may block on the compile (a GPU opened after entering draws
//!   its first frame only once they are built), and the hosted server ticks
//!   on through it, so its ticks are not counted;
//! - an app's scene pipelines have been compiling for [`STALL`];
//! - an app that is loading or in game has not moved (no loading step, no
//!   new server tick) for [`STALL`]: it has stopped, not slowed;
//! - no app is loading or in game and none has changed for [`STALL`];
//! - an app's connection fails during the wait.
use anyhow::{Context, Result, bail};
use bri_client::app::App;
use bri_ui::api::ConnectionState;
use std::{
    thread,
    time::{Duration, Instant},
};

/// Server ticks per second of game time.
pub const TICK_HZ: u64 = 120;
/// How long an app may stand still before the wait counts it as stopped:
/// far past the longest single loading step under the gate's load.
pub const STALL: Duration = Duration::from_secs(300);

/// Server ticks in `time` of game time.
pub fn ticks(time: Duration) -> u64 {
    (time.as_secs_f64() * TICK_HZ as f64).ceil() as u64
}

/// Where an app has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    /// No host or join attempt.
    Idle,
    /// Loading, at this progress revision.
    Loading(u64),
    /// In game, at this server tick.
    InGame(u64),
}

fn mark(app: &App) -> Mark {
    match (app.network_view(), app.loading_revision()) {
        (Some(view), _) => Mark::InGame(view.tick),
        (None, Some(revision)) => Mark::Loading(revision),
        (None, None) => Mark::Idle,
    }
}

fn failed(app: &App) -> Option<&str> {
    match &app.ui.core.conn {
        ConnectionState::Failed { reason } => Some(reason),
        _ => None,
    }
}

/// Run `step` (every app, one frame of the wall time that passed) until
/// `ready` holds, with `game` of game time to spare once all are in game.
pub fn until(
    apps: &mut [&mut App],
    what: &str,
    game: Duration,
    mut step: impl FnMut(&mut [&mut App], Duration) -> Result<()>,
    mut ready: impl FnMut(&mut [&mut App]) -> Result<bool>,
) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    // A failure the wait started with is old news; a new one ends it.
    let failed_before: Vec<bool> = apps.iter().map(|a| failed(a).is_some()).collect();
    let mut marks: Vec<(Mark, Instant)> = apps.iter().map(|a| (mark(a), start)).collect();
    let mut changed = start;
    let mut tick: Option<u64> = None;
    let mut game_ticks = 0;
    // Since when an app's scene pipelines have been compiling.
    let mut compiling: Option<Instant> = None;
    loop {
        let now = Instant::now();
        let compiled = apps.iter_mut().all(|a| a.scene_pipelines_ready());
        step(apps, now.duration_since(previous))?;
        previous = now;
        if ready(apps)? {
            return Ok(());
        }
        let states = || {
            apps.iter()
                .map(|a| format!("{:?}", a.ui.core.conn))
                .collect::<Vec<_>>()
                .join("; ")
        };
        for (app, before) in apps.iter().zip(&failed_before) {
            if let Some(reason) = failed(app)
                && !before
            {
                bail!("{what}: connection failed: {reason}");
            }
        }
        for (app, (seen, since)) in apps.iter().zip(&mut marks) {
            let latest = mark(app);
            if latest != *seen {
                *seen = latest;
                *since = now;
                changed = now;
            }
        }
        let all_in_game: Option<Vec<u64>> = marks
            .iter()
            .map(|(m, _)| match m {
                Mark::InGame(t) => Some(*t),
                _ => None,
            })
            .collect();
        let latest = all_in_game.and_then(|t| t.into_iter().min());
        if let (Some(before), Some(after)) = (tick, latest)
            && compiled
        {
            game_ticks += after.saturating_sub(before);
        }
        tick = latest;
        compiling = if compiled {
            None
        } else {
            compiling.or(Some(now))
        };
        if let Some(since) = compiling
            && now.duration_since(since) >= STALL
        {
            bail!(
                "Timed out waiting for {what}: scene pipelines still compiling after {STALL:?}; {}",
                states()
            );
        }
        if game_ticks >= ticks(game) {
            bail!(
                "Timed out waiting for {what}: {game:?} of game time passed; {}",
                states()
            );
        }
        for (i, (seen, since)) in marks.iter().enumerate() {
            if *seen != Mark::Idle && now.duration_since(*since) >= STALL {
                bail!(
                    "Timed out waiting for {what}: app {i} stopped advancing ({seen:?} for {STALL:?}); {}",
                    states()
                );
            }
        }
        if marks.iter().all(|(m, _)| *m == Mark::Idle) && now.duration_since(changed) >= STALL {
            bail!(
                "Timed out waiting for {what}: nothing happened for {STALL:?}; {}",
                states()
            );
        }
        thread::sleep(Duration::from_millis(8));
    }
}

/// [`until`] for one app.
pub fn until_one(
    app: &mut App,
    what: &str,
    game: Duration,
    mut step: impl FnMut(&mut App, Duration) -> Result<()>,
    ready: impl Fn(&App) -> bool,
) -> Result<()> {
    until(
        &mut [app],
        what,
        game,
        |apps, elapsed| step(&mut *apps[0], elapsed),
        |apps| Ok(ready(&*apps[0])),
    )
}

/// Run `step` until `ready` holds, for work that does not run on the game
/// clock: a worker's bake or compile, which a loaded machine slows while the
/// hosted server ticks on, so game time says nothing about how far it has
/// got. No game-time budget: the wait fails only when the work is not done
/// after [`STALL`], or the app's connection fails.
pub fn until_done(
    app: &mut App,
    what: &str,
    mut step: impl FnMut(&mut App, Duration) -> Result<()>,
    ready: impl Fn(&App) -> bool,
) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    let failed_before = failed(app).is_some();
    loop {
        let now = Instant::now();
        step(app, now.duration_since(previous))?;
        previous = now;
        if ready(app) {
            return Ok(());
        }
        if let Some(reason) = failed(app)
            && !failed_before
        {
            bail!("{what}: connection failed: {reason}");
        }
        if now.duration_since(start) >= STALL {
            bail!(
                "Timed out waiting for {what}: not done after {STALL:?}; {:?}",
                app.ui.core.conn
            );
        }
        thread::sleep(Duration::from_millis(8));
    }
}

/// The server tick every app has seen, or `None` while one is out of game.
pub fn seen_tick(apps: &[&mut App]) -> Option<u64> {
    apps.iter()
        .map(|a| a.network_view().map(|v| v.tick))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .min()
}

/// Let `time` of game time pass (server ticks every app has seen), never
/// wall time: a loaded machine slows the game, and a test that holds a key
/// for a wall-clock while would move a slowed player less, or not at all.
/// Fails out of game.
pub fn run_for(
    apps: &mut [&mut App],
    time: Duration,
    step: impl FnMut(&mut [&mut App], Duration) -> Result<()>,
) -> Result<()> {
    let start = seen_tick(apps).context("waiting in game time out of game")?;
    let end = start + ticks(time);
    // The budget only catches a hang: the wait ends at `time`.
    until(
        apps,
        "game time to pass",
        time + Duration::from_secs(5),
        step,
        |a| Ok(seen_tick(a).is_some_and(|t| t >= end)),
    )
}

/// [`run_for`] for one app.
pub fn run_one_for(
    app: &mut App,
    time: Duration,
    mut step: impl FnMut(&mut App, Duration) -> Result<()>,
) -> Result<()> {
    run_for(&mut [app], time, |apps, elapsed| {
        step(&mut *apps[0], elapsed)
    })
}
