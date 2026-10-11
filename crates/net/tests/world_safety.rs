//! The world survives an abrupt end. A hosted world is kept on disk as one
//! recovery snapshot, replaced in place (`bri_net::recovery`); this file
//! checks what is true of it however the host ends:
//!
//! * a writer killed outright (SIGKILL / TerminateProcess, in a child
//!   process, at many different moments) leaves the slot holding one whole
//!   snapshot, never half of one and never nothing once one was written;
//! * a slot cut short at any length is refused whole, not loaded partly;
//! * a host that is still running writes its builders' bricks to the slot
//!   within its interval, a builder whose connection simply vanishes costs
//!   the world nothing, and a host started from what the slot held gives
//!   the same bricks to whoever joins next.
//!
//! The first needs no network and no game content; the last runs a real
//! host and clients over loopback QUIC.
mod common;

use anyhow::{Context, Result, bail, ensure};
use bri_net::{client::Client, recovery, server};
use bri_sim::session::{Command, Reply};
use bri_world::{Brick, ContentRef, World, build};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command as Process, Stdio},
    time::{Duration, Instant},
};

/// The child's path to write to; unset in an ordinary run.
const CHILD: &str = "BRI_WORLD_SAFETY_WRITER";
/// Bricks a revision adds, so a snapshot's size says which revision it is.
const STEP: u64 = 40;

/// The world the writer keeps on writing: `revision` times `STEP` plates.
fn world_of(revision: u64) -> World {
    let mut world = World::new(
        format!("rev {revision}"),
        "fixture".into(),
        vec![[1.0; 4], [0.0; 4]],
    );
    for id in 1..=revision * STEP {
        let (x, z) = ((id % 40) as f32, (id / 40) as f32);
        let mut brick = Brick::new(
            ContentRef::Resolved("plate".into()),
            [-20.0 + x, 0.1, 5.25 + z * 0.5],
            1,
        );
        brick.color = (id % 2) as u8;
        world.bricks.insert(id, brick);
    }
    world.next_brick_id = revision * STEP + 1;
    world.revision = revision;
    world
}

/// Not a test of its own: the process the tests below kill. It writes
/// snapshots of growing worlds forever and says each revision once written.
#[test]
fn writer_child() {
    let Some(path) = std::env::var_os(CHILD) else {
        return;
    };
    for revision in 1.. {
        recovery::write(Path::new(&path), (world_of(revision), None)).expect("write");
        println!("rev {revision}");
    }
}

/// Run the writer, let it announce `announcements` revisions and run on
/// for `extra`, then kill it. Returns the last revision it announced.
fn kill_a_writer(slot: &Path, announcements: u64, extra: Duration) -> Result<u64> {
    let mut child = Process::new(std::env::current_exe()?)
        .args(["--exact", "writer_child", "--nocapture", "--test-threads=1"])
        .env(CHILD, slot)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("no stdout")?).lines();
    let mut last = 0;
    let started = Instant::now();
    while last < announcements {
        ensure!(
            started.elapsed() < Duration::from_secs(60),
            "the writer never got to revision {announcements}"
        );
        let Some(line) = lines.next() else {
            bail!("the writer ended by itself at revision {last}");
        };
        if let Some(n) = line?.strip_prefix("rev ") {
            last = n.parse()?;
        }
    }
    std::thread::sleep(extra);
    child.kill()?;
    child.wait()?;
    Ok(last)
}

/// The revision a slot holds, after checking it is a whole, valid build.
fn revision_in(slot: &Path) -> Result<u64> {
    let bytes = std::fs::read(slot).context("the slot is gone")?;
    let saved = build::decode(&bytes).context("the slot does not decode")?;
    saved.validate()?;
    let revision: u64 = saved
        .world
        .name
        .strip_prefix("rev ")
        .context("not a snapshot of the writer")?
        .parse()?;
    ensure!(
        saved.world.bricks.len() as u64 == revision * STEP,
        "rev {revision} holds {} bricks: half a snapshot",
        saved.world.bricks.len()
    );
    ensure!(
        saved.world.revision == revision,
        "the snapshot's own revision disagrees with its name"
    );
    Ok(revision)
}

#[test]
fn a_writer_killed_at_any_moment_leaves_one_whole_snapshot() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let slot = dir.path().join("recovery").join("hosted-world.build");
    // How many revisions to see, and how much longer to run, before each kill.
    let moments = [
        (1, 0),
        (1, 1500),
        (2, 300),
        (3, 4000),
        (5, 0),
        (5, 2500),
        (8, 700),
        (13, 100),
        (21, 6100),
    ];
    let mut newest = 0;
    for (announced, extra_us) in moments {
        let last = kill_a_writer(&slot, announced, Duration::from_micros(extra_us))?;
        let found = revision_in(&slot)
            .with_context(|| format!("after a kill at rev {last} (+{extra_us} us)"))?;
        // Announced revisions were written; the writer runs on between the
        // announcement and the kill, so the slot may be well ahead of it.
        ensure!(
            found >= last,
            "announced rev {last}, the slot holds rev {found}"
        );
        newest = found;
    }
    // After all that, the next host to write the slot replaces it as usual.
    recovery::write(&slot, (world_of(newest + 1), None))?;
    ensure!(revision_in(&slot)? == newest + 1);
    // A killed writer can leave its staging file behind, one at most per
    // kill, named for the slot and ending `.tmp`; nothing else.
    let strays: Vec<_> = std::fs::read_dir(slot.parent().unwrap())?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<_>>()?;
    let others: Vec<_> = strays
        .iter()
        .filter(|n| *n != "hosted-world.build" && !n.ends_with(".tmp"))
        .collect();
    ensure!(others.is_empty(), "unexpected files: {others:?}");
    ensure!(
        strays.len() <= 1 + moments.len(),
        "{} files after {} kills",
        strays.len(),
        moments.len()
    );
    Ok(())
}

/// A snapshot cut short is refused, whatever the cut. (An atomic replace
/// cannot produce one; this is the guard against a disk, a copy or an
/// older writer that does.)
#[test]
fn a_snapshot_cut_short_anywhere_is_refused_whole() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let slot = dir.path().join("slot.build");
    recovery::write(&slot, (world_of(5), None))?;
    let whole = std::fs::read(&slot)?;
    ensure!(build::decode(&whole)?.world.bricks.len() as u64 == 5 * STEP);
    let stride = (whole.len() / 400).max(1);
    let mut cuts: Vec<usize> = (0..whole.len().min(300)).collect();
    cuts.extend((300..whole.len()).step_by(stride));
    cuts.push(whole.len() - 1);
    for cut in cuts {
        let part = &whole[..cut];
        if let Ok(partial) = build::decode(part) {
            bail!(
                "a snapshot cut at {cut} of {} bytes loaded {} bricks",
                whole.len(),
                partial.world.bricks.len()
            );
        }
        // The list a save screen draws from only reads the head: it may
        // show a cut file's head, but never a count that is not the file's.
        if let Ok((_, bricks)) = build::decode_header(part) {
            ensure!(bricks == 5 * STEP, "a cut at {cut} claims {bricks} bricks");
        }
    }
    Ok(())
}

// ------------------------------------------------ a live host and clients

async fn plant(client: &mut Client, x: f32) -> Result<u64> {
    let Reply::Planted(id) = client
        .command(Command::Plant {
            definition: "plate".into(),
            position: [x, 0.1, -3.25],
            quarter_turns: 0,
            color: 0,
        })
        .await?
    else {
        bail!("expected a plant reply");
    };
    Ok(id)
}

async fn join(address: std::net::SocketAddr, certificate: &[u8], name: &str) -> Result<Client> {
    let mut client = Client::connect(address, certificate, name.into(), Vec::new(), None).await?;
    tokio::time::timeout(Duration::from_secs(60), client.await_world()).await??;
    Ok(client)
}

/// What the slot holds, once it holds `bricks` bricks, within `limit`.
async fn slot_with(slot: &Path, bricks: usize, limit: Duration) -> Result<World> {
    let until = Instant::now() + limit;
    loop {
        if let Ok(bytes) = std::fs::read(slot)
            && let Ok(saved) = build::decode(&bytes)
            && saved.world.bricks.len() == bricks
        {
            return Ok(saved.world);
        }
        ensure!(
            Instant::now() < until,
            "the slot never held {bricks} bricks"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn replicated(client: &Client) -> std::collections::BTreeMap<u64, [f32; 3]> {
    client
        .replica
        .world
        .bricks
        .iter()
        .map(|(id, brick)| (*id, brick.position))
        .collect()
}

/// A builder's bricks reach the slot while the host runs; the builder's
/// connection vanishing costs the world nothing; a host started from the
/// slot gives the same bricks to the next joiner.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hosts_slot_keeps_what_players_built_and_restarts_from_it() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let slot: PathBuf = dir.path().join("state").join("recovery.json");
    let host = server::start(common::session(), common::options())?;
    host.keep_recovery(recovery::Recovery {
        path: slot.clone(),
        every: Duration::from_millis(200),
    })?;
    let mut builder = join(host.address, &host.certificate, "Builder").await?;
    let mut ids = Vec::new();
    for n in 0..6 {
        ids.push(plant(&mut builder, -47.0 + n as f32 * 1.5).await?);
    }
    let kept = slot_with(&slot, ids.len(), Duration::from_secs(30)).await?;
    for id in &ids {
        ensure!(
            kept.bricks.contains_key(id),
            "brick {id} is not in the slot"
        );
    }
    // The slot as it is on disk now is what a kill of the host leaves.
    let survived = std::fs::read(&slot)?;
    // The builder's connection ends without a word: the endpoint is dropped
    // under the host, which keeps running with the world as it was.
    let before: std::collections::BTreeMap<u64, [f32; 3]> = kept
        .bricks
        .iter()
        .map(|(id, brick)| (*id, brick.position))
        .collect();
    drop(builder);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let watcher = join(host.address, &host.certificate, "Watcher").await?;
    ensure!(
        replicated(&watcher) == before,
        "the world changed when its builder vanished: {:?} then {:?}",
        before,
        replicated(&watcher)
    );
    // Nothing is left half-written beside the slot.
    watcher.close();
    host.stop().await?;
    ensure!(!slot.exists(), "a clean stop left its recovery snapshot");

    // The host that never got to stop: start a new one from what survived.
    let restored = build::decode(&survived)?;
    restored.validate()?;
    ensure!(restored.world.bricks.len() == ids.len());
    let host = server::start(common::session_with(restored.world), common::options())?;
    let joiner = join(host.address, &host.certificate, "Joiner").await?;
    ensure!(
        replicated(&joiner) == before,
        "a host restarted from the slot has other bricks"
    );
    host.stop().await?;
    Ok(())
}
