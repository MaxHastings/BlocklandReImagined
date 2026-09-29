//! Try an Add-On's game rules headless, without the game or any Rust:
//!
//!   bri-addon-run <folder> [--seconds N] [--wait N] [--send "<command> [args...]"]...
//!
//! Checks the Add-On as `bri-addon-check` does, then runs it with the Add-Ons
//! it needs (found beside it) in an empty world with two players, `Host`
//! (an administrator) and `Guest`. Each `--send` is a command the Host sends,
//! one second after the one before (the first at one second); `--wait N`
//! waits N more seconds before the next. `--send "Guest: gift 1"` sends it
//! as the Guest, and `other-add-on:command` names another Add-On's command.
//! The run lasts `--seconds` (10 by default) or until the last command has
//! had a second to answer, whichever is longer. Prints every chat line as it happens, then the state players
//! receive and any problem the scripts hit. Exit 1 on a problem.
use anyhow::{Context, Result, bail};
use bri_package::packages::{PackageEntry, PackageSet};
use bri_package_runtime::{Catalog, check, content::ArgType};
use bri_sim::{
    definitions::Definitions,
    session::{Command, Notice, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use std::{path::PathBuf, sync::Arc};

const TICKS_PER_SECOND: u64 = 120;
const USAGE: &str =
    "usage: bri-addon-run <folder> [--seconds N] [--wait N] [--send \"<command> [args...]\"]...";

struct Send {
    guest: bool,
    package: String,
    command: String,
    args: Vec<String>,
}

fn parse(text: &str, target: &str) -> Result<Send> {
    let (guest, rest) = match text.split_once(':') {
        Some((who, rest)) if who.eq_ignore_ascii_case("guest") => (true, rest),
        Some((who, rest)) if who.eq_ignore_ascii_case("host") => (false, rest),
        _ => (false, text),
    };
    let mut words = rest.split_whitespace();
    let first = words.next().context("--send needs a command")?;
    let (package, command) = match first.split_once(':') {
        Some((package, command)) => (package.to_owned(), command.to_owned()),
        None => (target.to_owned(), first.to_owned()),
    };
    Ok(Send {
        guest,
        package,
        command,
        args: words.map(str::to_owned).collect(),
    })
}

fn argument(text: &str, kind: ArgType) -> Result<PackageArg> {
    Ok(match kind {
        ArgType::Int => PackageArg::Int(
            text.parse()
                .with_context(|| format!("`{text}` is not a whole number"))?,
        ),
        ArgType::Float => PackageArg::Float(
            text.parse()
                .with_context(|| format!("`{text}` is not a number"))?,
        ),
        ArgType::Bool => PackageArg::Bool(
            text.parse()
                .with_context(|| format!("`{text}` is not true or false"))?,
        ),
        ArgType::String => PackageArg::String(text.to_owned()),
    })
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(2);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut folder = None;
    let mut seconds = 10u64;
    let mut sends = Vec::new();
    let mut at = 0u64;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--seconds" => {
                seconds = it
                    .next()
                    .context(USAGE)?
                    .parse()
                    .context("--seconds needs a whole number")?
            }
            "--send" => {
                at += TICKS_PER_SECOND;
                sends.push((at, it.next().context(USAGE)?.clone()));
            }
            "--wait" => {
                let wait: u64 = it
                    .next()
                    .context(USAGE)?
                    .parse()
                    .context("--wait needs a whole number")?;
                at += wait * TICKS_PER_SECOND;
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if folder.is_none() && !a.starts_with("--") => folder = Some(PathBuf::from(a)),
            _ => bail!("{USAGE}"),
        }
    }
    let folder = folder.context(USAGE)?;
    let report = check::check(&folder);
    if !report.ok {
        println!("{report}");
        std::process::exit(1);
    }
    let target = report.add_ons[0].id.clone();
    let parent = match folder.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let set = PackageSet {
        schema_version: 1,
        packages: report
            .add_ons
            .iter()
            .map(|a| PackageEntry {
                id: a.id.clone(),
                version: a.version.clone(),
                side: a.side,
                dir: a.folder.clone(),
                role: None,
            })
            .collect(),
    };
    let catalog = Catalog::load(&parent, &set, true).map_err(|problems| {
        anyhow::anyhow!(
            problems
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;
    // Commands in the order they go out, with typed arguments.
    let mut plan = Vec::new();
    for (at, text) in &sends {
        let send = parse(text, &target)?;
        let def = catalog
            .packages
            .get(&send.package)
            .and_then(|p| p.behaviour.as_ref())
            .and_then(|b| b.commands.iter().find(|c| c.name == send.command))
            .with_context(|| format!("`{}` has no command `{}`", send.package, send.command))?;
        if def.args.len() != send.args.len() {
            bail!(
                "`{}` takes {} argument(s), {} given",
                send.command,
                def.args.len(),
                send.args.len()
            );
        }
        let args = send
            .args
            .iter()
            .zip(&def.args)
            .map(|(text, kind)| argument(text, *kind))
            .collect::<Result<Vec<_>>>()?;
        plan.push((*at, send, args));
    }
    let world = World::new("Add-On test".into(), "addon-test".into(), vec![[1.0; 4]]);
    let mut session = Session::new(Simulation::new(world, Definitions::default(), vec![])?);
    session.install_packages(Arc::new(catalog), None)?;
    let host = session.join("Host".into(), Vec3::new(0.0, 1.0, 0.0), true)?;
    let guest = session.join("Guest".into(), Vec3::new(4.0, 1.0, 0.0), false)?;
    let name = |owner| if owner == host { "Host" } else { "Guest" };
    let mut printed = 0u64;
    let mut sequence = [0u64; 2];
    let end = (seconds * TICKS_PER_SECOND).max(at + TICKS_PER_SECOND);
    let mut next = plan.iter().peekable();
    for tick in 0..=end {
        while let Some((_, send, args)) = next.next_if(|(at, ..)| *at == tick) {
            let owner = if send.guest { guest } else { host };
            let seq = &mut sequence[send.guest as usize];
            *seq += 1;
            println!(
                "{:>6.2}s  {} sends {}",
                tick as f64 / TICKS_PER_SECOND as f64,
                name(owner),
                [send.command.clone()]
                    .iter()
                    .chain(&send.args)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            if let Err(e) = session.command(
                owner,
                *seq,
                Command::Package(PackageCommand {
                    package: send.package.clone(),
                    command: send.command.clone(),
                    args: args.clone(),
                }),
            ) {
                println!("         refused: {e:#}");
            }
        }
        if tick > 0 {
            session.step()?;
        }
        let at = tick as f64 / TICKS_PER_SECOND as f64;
        let seen = printed;
        for line in session.chat().into_iter().filter(|l| l.id >= seen) {
            printed = line.id + 1;
            println!("{at:>6.2}s  [everyone] {}", line.text);
        }
        for (owner, notice) in session.take_private_notices() {
            if let Notice::Chat(text) = notice
                && !text.starts_with('\u{E001}')
            {
                println!("{at:>6.2}s  [to {}] {text}", name(owner));
            }
        }
    }
    println!(
        "\nState players receive after {} s:",
        end / TICKS_PER_SECOND
    );
    for (package, ns) in &session.package_state().packages {
        for (key, value) in &ns.global {
            println!("  {package} global {key} = {value}");
        }
        for (owner, keys) in &ns.players {
            for (key, value) in keys {
                println!("  {package} {} {key} = {value}", name(*owner));
            }
        }
    }
    let problems = session.package_diagnostics();
    for d in &problems {
        println!("{d}");
    }
    if problems.is_empty() {
        println!("OK: no script problems.");
        Ok(())
    } else {
        std::process::exit(1)
    }
}
