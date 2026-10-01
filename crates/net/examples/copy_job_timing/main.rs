//! How a million-brick copy runs as copy jobs, a slice each tick.
//!
//! A deterministic build of 1,000,000 one-stud plates (1000 by 1000, the
//! world's brick limit) is selected, cut, planted again, both undone, then
//! supercut and that undone, each as the multi-tick copy job a duplicator
//! starts. Every job reports its ticks, its total time and its worst tick,
//! against the tick's copy work (`DEFAULT_COPY_WORK`, 10,000 units of a
//! quarter microsecond: 2.5 ms). The last line is the same as JSON.
//!
//! cargo run --release -p bri-net --example copy_job_timing
use anyhow::{Context, Result, bail};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_sim::{
    session::{Command, DEFAULT_COPY_WORK, PackageCommand, Reply, Session, ToolAction},
    simulation::Simulation,
    testing,
};
use bri_world::{Brick, ContentRef, OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// As the game and the server run.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Plates along each side of the build: 1000 (`COPY_TIMING_SIDE` sets a
/// smaller one for a quick look).
fn side() -> usize {
    std::env::var("COPY_TIMING_SIDE")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| (1..=1000).contains(&n))
        .unwrap_or(1000)
}
/// A quarter microsecond: one unit of copy work.
const UNIT: Duration = Duration::from_nanos(250);

struct Timing {
    job: &'static str,
    ticks: usize,
    total: Duration,
    /// The command that started it, which works on it at once.
    start: Duration,
    worst: Duration,
    bricks: usize,
}

struct Probe {
    s: Session,
    host: OwnerId,
    seq: u64,
}

impl Probe {
    fn command(&mut self, command: Command) -> Result<Reply> {
        self.seq += 1;
        self.s.command(self.host, self.seq, command)
    }
    fn typed(&mut self, command: &str) -> Result<()> {
        self.command(Command::Package(PackageCommand {
            package: "copy-timing".into(),
            command: command.into(),
            args: vec![],
        }))?;
        Ok(())
    }
    /// Start a job with `start`, then step until it is done.
    fn time(
        &mut self,
        job: &'static str,
        start: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<Timing> {
        // Commands wait out their cooldowns and the plant wait.
        for _ in 0..121 {
            self.s.step()?;
        }
        let begun = Instant::now();
        start(self)?;
        let start = begun.elapsed();
        let (mut ticks, mut worst) = (0, start);
        while self.s.copy_working(self.host) {
            let tick = Instant::now();
            self.s.step()?;
            worst = worst.max(tick.elapsed());
            ticks += 1;
        }
        let timing = Timing {
            job,
            ticks,
            total: begun.elapsed(),
            start,
            worst,
            bricks: self.s.simulation().state().bricks.len(),
        };
        let budget = UNIT * DEFAULT_COPY_WORK;
        println!(
            "{:<14} {:>6} ticks  total {:>8.1} ms  first {:>6.2} ms  worst tick {:>6.2} ms ({:.2}x the {:.1} ms copy budget)  bricks after {}",
            timing.job,
            timing.ticks,
            ms(timing.total),
            ms(timing.start),
            ms(timing.worst),
            timing.worst.as_secs_f64() / budget.as_secs_f64(),
            ms(budget),
            timing.bricks,
        );
        Ok(timing)
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn packages(sim: &Path, here: &Path, root: &Path) -> Result<Arc<bri_package_runtime::Catalog>> {
    // The Advanced Duplicator's tool from the sim tests, and this probe's
    // own package beside it, read from one folder each.
    let tool = sim.join("tests/fixtures/duplicators");
    let _ = std::fs::remove_dir_all(root);
    for (from, to) in [
        (tool.join("advanced-duplicator-tool"), root.join("advanced-duplicator-tool")),
        (here.join("copy-timing"), root.join("copy-timing")),
    ] {
        copy_dir(&from, &to)?;
    }
    let entry = |id: &str, side| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        dir: id.into(),
        role: None,
    };
    let set = PackageSet {
        schema_version: 1,
        packages: vec![
            entry("advanced-duplicator-tool", Side::Shared),
            entry("copy-timing", Side::Server),
        ],
    };
    let catalog = bri_package_runtime::Catalog::load(root, &set, true)
        .map_err(|e| anyhow::anyhow!("{e:#?}"))?;
    Ok(Arc::new(catalog))
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from).with_context(|| from.display().to_string())? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sim = manifest.join("../sim");
    let here = manifest.join("examples/copy_job_timing");

    let side = side();
    let root = std::env::temp_dir().join(format!("bri-copy-timing-{}", std::process::id()));
    let result = run(side, &sim, &here, &root);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn run(side: usize, sim: &Path, here: &Path, root: &Path) -> Result<()> {
    let built = Instant::now();
    let palette = vec![[1.0; 4], [0.2, 0.4, 1.0, 1.0]];
    let mut world = World::new("Copy timing".into(), "copy-timing".into(), palette);
    let mut id = 1;
    for x in 0..side {
        for z in 0..side {
            let position = [0.25 + x as f32 * 0.5, 0.1, 0.25 + z as f32 * 0.5];
            let brick = Brick::new(ContentRef::Resolved(testing::PLATE.into()), position, 0);
            world.bricks.insert(id, brick);
            id += 1;
        }
    }
    world.next_brick_id = id;
    let floor = ColliderBuilder::cuboid(1000.0, 0.5, 1000.0).translation(Vector::new(0.0, -0.5, 0.0));
    let mut s = Session::new(Simulation::new(world, testing::definitions(), vec![floor])?);
    s.set_spawn_points(vec![Vec3::new(-4.0, 0.05, -4.0)])?;
    let tool = sim.join("tests/fixtures/duplicators/advanced-duplicator-tool/assets/weapons.json");
    s.set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(tool)?)?)?;
    s.install_packages(packages(sim, here, root)?, None)?;
    let host = s.join("Host".into(), Vec3::new(-4.0, 0.05, -4.0), true)?;
    let mut probe = Probe { s, host, seq: 0 };
    let settings = bri_admin::ServerSettings {
        brick_limit: bri_world::MAX_BRICKS as u32,
        ..bri_admin::ServerSettings::default()
    };
    match probe.command(Command::Admin(bri_admin::Request::new(
        bri_admin::Action::HostConfigure { settings },
    )))? {
        Reply::Admin(_) => {}
        other => bail!("configuring the host: {other:?}"),
    }
    println!(
        "{} bricks built and loaded in {:.1} ms",
        probe.s.simulation().state().bricks.len(),
        ms(built.elapsed())
    );

    let undo = |p: &mut Probe| -> Result<()> {
        p.command(Command::Tool(ToolAction::UndoBrick))?;
        Ok(())
    };
    let mut timings = vec![
        probe.time("select", |p| p.typed("sel"))?,
        probe.time("cut", |p| p.typed("cut"))?,
        probe.time("plant", |p| {
            p.command(Command::PlaceBlueprint {
                position: [side as f32 * 0.25, 0.0, side as f32 * 0.25],
                quarter_turns: 0,
                mirrored: false,
                flipped: false,
            })?;
            Ok(())
        })?,
        probe.time("undo plant", undo)?,
        probe.time("undo cut", undo)?,
    ];
    if probe.s.simulation().state().bricks.len() != side * side {
        bail!("the build did not come back whole");
    }
    timings.push(probe.time("supercut", |p| p.typed("supercut"))?);
    timings.push(probe.time("undo supercut", undo)?);
    if probe.s.simulation().state().bricks.len() != side * side {
        bail!("the supercut's undo did not bring the build back whole");
    }

    let budget = UNIT * DEFAULT_COPY_WORK;
    let worst = timings.iter().map(|t| t.worst).max().unwrap_or_default();
    let json = serde_json::json!({
        "bricks": side * side,
        "copy_work_units": DEFAULT_COPY_WORK,
        "copy_budget_ms": ms(budget),
        "worst_tick_ms": ms(worst),
        "worst_over_budget": worst.as_secs_f64() / budget.as_secs_f64(),
        "jobs": timings.iter().map(|t| serde_json::json!({
            "job": t.job,
            "ticks": t.ticks,
            "total_ms": ms(t.total),
            "first_ms": ms(t.start),
            "worst_tick_ms": ms(t.worst),
        })).collect::<Vec<_>>(),
    });
    println!("{json}");
    Ok(())
}
