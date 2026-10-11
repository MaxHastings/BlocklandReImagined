//! Two players on a modded host, the host killed, and both back in the
//! world it left: the whole journey, headless, over loopback QUIC.
//!
//! 1. A host with shared Add-Ons runs in a child process and keeps its
//!    recovery snapshot. Two clients with empty caches join; each
//!    downloads the server's Add-Ons, verifies them and builds.
//! 2. The host process is killed outright (SIGKILL / TerminateProcess)
//!    once its snapshot holds both players' bricks.
//! 3. One file of one client's cached Add-On is damaged on disk meanwhile,
//!    both the installed copy and the copy the cache keeps (a crash
//!    mid-write, a bad sector): the fault the rejoin must survive.
//! 4. A new host starts from the snapshot the dead one left. Both players
//!    rejoin: the intact cache downloads nothing, the damaged file is
//!    thrown away and only it is fetched again, both run exactly the server's
//!    Add-Ons, and both see every brick under the owner number they had.
mod common;

use anyhow::{Context, Result, bail, ensure};
use bri_net::{
    client::{Client, HostPin},
    packages::{Fetched, PackageShelf},
    recovery,
    server::{self, ServerOptions},
};
use bri_package::{
    environment::{Environment, PackageRef, hash_dir},
    packages::PackageSet,
    sync::Cache,
};
use bri_progress::Progress;
use bri_sim::session::{Command, Reply};
use bri_world::{OwnerId, build};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command as Process, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

/// The child's content root and recovery slot; unset in an ordinary run.
const CHILD_ROOT: &str = "BRI_JOURNEY_HOST_ROOT";
const CHILD_SLOT: &str = "BRI_JOURNEY_HOST_SLOT";

/// The server's Add-Ons, written into `root`: two shared packages a joiner
/// must download (one with a large texture) and a server-only one it must
/// never be offered.
fn content(root: &Path) -> Result<(PackageSet, Environment)> {
    let files: &[(&str, &[u8])] = &[
        ("castle/package.json", br#"{"id":"castle"}"#),
        ("castle/models/tower.glb", &[3; 200_000]),
        ("castle/textures/stone.png", &[5; 900_000]),
        ("banners/package.json", br#"{"id":"banners"}"#),
        ("banners/textures/flag.png", &[8; 40_000]),
        ("rules/rules.rhai", b"// server-only behaviour"),
    ];
    for (path, bytes) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, bytes)?;
    }
    let set = PackageSet::parse(
        br#"{"schema_version":1,"packages":[
            {"id":"castle","version":"1.0.0","side":"shared","dir":"castle"},
            {"id":"banners","version":"1.0.0","side":"shared","dir":"banners"},
            {"id":"rules","version":"1.0.0","side":"server","dir":"rules"}
        ]}"#,
    )?;
    let environment = Environment::load(root, &set)?;
    Ok((set, environment))
}

fn options(root: &Path) -> Result<(ServerOptions, Environment)> {
    let (set, environment) = content(root)?;
    let shelf = PackageShelf::new(root, &set, &environment)?;
    Ok((
        ServerOptions {
            environment: environment.clone(),
            packages: Some(Arc::new(shelf)),
            ..common::options()
        },
        environment,
    ))
}

/// Not a test of its own: the host process the journey kills. It hosts an
/// empty world with the Add-Ons, keeps its recovery slot every 100 ms and
/// says where to find it, then runs until it is killed.
#[test]
fn journey_host_child() {
    let (Some(root), Some(slot)) = (std::env::var_os(CHILD_ROOT), std::env::var_os(CHILD_SLOT))
    else {
        return;
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let (options, _) = options(Path::new(&root)).unwrap();
        let host = server::start(common::session(), options).unwrap();
        host.keep_recovery(recovery::Recovery {
            path: slot.into(),
            every: Duration::from_millis(100),
        })
        .unwrap();
        println!("\njourney-host {} {}", host.address, hex(&host.certificate));
        std::future::pending::<()>().await;
    });
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(text: &str) -> Result<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| Ok(u8::from_str_radix(&text[i..i + 2], 16)?))
        .collect()
}

/// One player: who they are, and the Add-On cache they keep between games.
struct Player {
    name: &'static str,
    identity: bri_identity::ClientIdentity,
    cache: Cache,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
}
impl Player {
    fn new(name: &'static str) -> Result<Self> {
        let (id_dir, cache_dir) = (tempfile::tempdir()?, tempfile::tempdir()?);
        Ok(Self {
            name,
            identity: bri_identity::ClientIdentity::load_or_create(
                id_dir.path().join("client.identity"),
            )?,
            cache: Cache::open(cache_dir.path())?,
            _dirs: (id_dir, cache_dir),
        })
    }
    fn cache_root(&self) -> &Path {
        self._dirs.1.path()
    }
    /// Join as a game does: download what the server runs and lacks here,
    /// then join with exactly the server's Add-Ons.
    async fn join(
        &self,
        address: std::net::SocketAddr,
        certificate: &[u8],
    ) -> Result<(Client, Vec<Fetched>, Vec<PackageRef>)> {
        let mut ran = Vec::new();
        let connect = Client::connect_fetching(
            address,
            HostPin::from(certificate),
            self.name.into(),
            Vec::new(),
            None,
            &self.identity,
            &self.cache,
            Progress::default(),
            |fetched, dropped| {
                ensure!(dropped.is_empty(), "the server's Add-Ons were dropped");
                ran = fetched.iter().map(|f| f.package.clone()).collect();
                Ok(ran.clone())
            },
        );
        let (mut client, fetched, _) = tokio::time::timeout(Duration::from_secs(60), connect)
            .await
            .with_context(|| format!("{} could not join", self.name))??;
        tokio::time::timeout(Duration::from_secs(60), client.await_world()).await??;
        Ok((client, fetched, ran))
    }
}

async fn plant(client: &mut Client, x: f32, z: f32) -> Result<u64> {
    match client
        .command(Command::Plant {
            definition: "plate".into(),
            position: [x, 0.1, z],
            quarter_turns: 0,
            color: 0,
        })
        .await?
    {
        Reply::Planted(id) => Ok(id),
        other => bail!("expected a plant, got {other:?}"),
    }
}

/// Bricks by id, with where they stand and who owns them.
fn bricks(client: &Client) -> BTreeMap<u64, ([i32; 3], OwnerId)> {
    client
        .replica
        .world
        .bricks
        .iter()
        .map(|(id, b)| {
            (
                *id,
                (b.position.map(|v| (v * 1000.0).round() as i32), b.owner),
            )
        })
        .collect()
}

/// Receive until `client` sees `count` bricks.
async fn seen(client: &mut Client, count: usize) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(30), async {
        while client.replica.world.bricks.len() != count {
            client.receive().await?;
        }
        Result::<()>::Ok(())
    })
    .await
    .with_context(|| format!("{} never saw {count} bricks", client.owner))?
}

fn sorted(mut refs: Vec<PackageRef>) -> Vec<PackageRef> {
    refs.sort_by(|a, b| a.id.cmp(&b.id));
    refs
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_players_rejoin_a_killed_modded_host_with_their_builds_and_add_ons() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let slot = state.path().join("recovery").join("hosted-world.build");
    let (_, environment) = content(root.path())?;
    let served = sorted(environment.client_packages());
    ensure!(served.len() == 2, "{served:?}");

    // 1. The host, in a process of its own.
    let mut child = Process::new(std::env::current_exe()?)
        .args([
            "--exact",
            "journey_host_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ROOT, root.path())
        .env(CHILD_SLOT, &slot)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("no stdout")?).lines();
    let (address, certificate) = loop {
        let line = lines.next().context("the host ended before it was up")??;
        // libtest prints `test journey_host_child ... ` ahead of it.
        if let Some((_, rest)) = line.split_once("journey-host ") {
            let (address, certificate) = rest.split_once(' ').context("bad host line")?;
            break (address.parse()?, unhex(certificate)?);
        }
    };
    let kill = scopeguard(&mut child);

    let ann = Player::new("Ann")?;
    let ben = Player::new("Ben")?;
    let (mut a, fetched_a, ran_a) = ann.join(address, &certificate).await?;
    let (mut b, fetched_b, ran_b) = ben.join(address, &certificate).await?;
    for (who, fetched, ran) in [("Ann", &fetched_a, &ran_a), ("Ben", &fetched_b, &ran_b)] {
        ensure!(sorted(ran.clone()) == served, "{who} runs {ran:?}");
        for f in fetched {
            ensure!(f.downloaded > 0, "{who} had {} already", f.package.id);
            ensure!(
                hash_dir(&f.dir)?.0 == f.package.hash,
                "{who}: {} unverified",
                f.package.id
            );
        }
    }
    let (owner_a, owner_b) = (a.owner, b.owner);
    ensure!(owner_a != owner_b);
    for n in 0..4 {
        plant(&mut a, -40.0 + 1.5 * n as f32, -3.25).await?;
    }
    for n in 0..2 {
        plant(&mut b, -40.0 + 1.5 * n as f32, -6.25).await?;
    }
    // Both see the same six bricks before the crash.
    seen(&mut a, 6).await?;
    seen(&mut b, 6).await?;
    let built = bricks(&a);
    ensure!(bricks(&b) == built, "the players disagree on the build");

    // 2. Once the snapshot holds all of it, the host process is killed.
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(bytes) = std::fs::read(&slot)
            && let Ok(saved) = build::decode(&bytes)
            && saved.world.bricks.len() == 6
        {
            break;
        }
        ensure!(Instant::now() < until, "the snapshot never held the build");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    drop(kill);
    drop((a, b));

    // 3. Ben's cached castle is damaged while nobody plays.
    let castle = fetched_b
        .iter()
        .find(|f| f.package.id == "castle")
        .context("Ben has no castle")?;
    let texture = castle.dir.join("textures").join("stone.png");
    let mut bytes = std::fs::read(&texture)?;
    let original = bytes[1000];
    let stored = std::fs::read_dir(ben.cache_root().join("objects"))?
        .map(|e| Ok(e?.path()))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .find(|p| std::fs::read(p).is_ok_and(|b| b == bytes))
        .context("Ben's cache keeps no copy of the texture")?;
    bytes[1000] ^= 0xff;
    std::fs::write(&texture, &bytes)?;
    std::fs::write(&stored, &bytes)?;

    // 4. A new host from what the dead one left, with the same Add-Ons.
    let left = build::decode(&std::fs::read(&slot)?)?;
    left.validate()?;
    let (options, _) = options(root.path())?;
    let host = server::start(common::session_with(left.world), options)?;
    let (mut a, fetched_a, ran_a) = ann.join(host.address, &host.certificate).await?;
    let (mut b, fetched_b, ran_b) = ben.join(host.address, &host.certificate).await?;
    seen(&mut a, 6).await?;
    seen(&mut b, 6).await?;
    ensure!(sorted(ran_a.clone()) == served, "Ann runs {ran_a:?}");
    ensure!(sorted(ran_b.clone()) == served, "Ben runs {ran_b:?}, fetched {fetched_b:?}");
    ensure!(
        fetched_a.iter().all(|f| f.downloaded == 0),
        "Ann's intact cache was downloaded again"
    );
    let again = fetched_b
        .iter()
        .find(|f| f.package.id == "castle")
        .context("Ben did not get the castle again")?;
    // Both of Ben's copies of the texture were bad: it alone comes again.
    ensure!(
        again.downloaded == bytes.len() as u64,
        "Ben downloaded {} bytes of the castle again, not the {}-byte texture",
        again.downloaded,
        bytes.len()
    );
    ensure!(
        hash_dir(&again.dir)?.0 == again.package.hash,
        "Ben ran a damaged castle"
    );
    ensure!(
        std::fs::read(again.dir.join("textures").join("stone.png"))?[1000] == original,
        "Ben's castle texture was not repaired"
    );
    ensure!(
        fetched_b
            .iter()
            .filter(|f| f.package.id != "castle")
            .all(|f| f.downloaded == 0),
        "Ben's intact banners were downloaded again"
    );
    ensure!(
        (a.owner, b.owner) == (owner_a, owner_b),
        "owner numbers changed: {:?} then {:?}",
        (owner_a, owner_b),
        (a.owner, b.owner)
    );
    ensure!(bricks(&a) == built, "Ann sees another build");
    ensure!(bricks(&b) == built, "Ben sees another build");
    ensure!(
        built.values().filter(|(_, o)| *o == owner_a).count() == 4
            && built.values().filter(|(_, o)| *o == owner_b).count() == 2,
        "the bricks' owners: {built:?}"
    );
    a.close();
    b.close();
    host.stop().await?;
    Ok(())
}

/// Kills the child when dropped, so a failing test never leaves a host up.
struct Kill<'a>(&'a mut std::process::Child);
impl Drop for Kill<'_> {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn scopeguard(child: &mut std::process::Child) -> Kill<'_> {
    Kill(child)
}
