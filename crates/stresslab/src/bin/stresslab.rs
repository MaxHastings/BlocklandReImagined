//! Agent workflow for mod packages, exercised on the Stress Lab.
//!
//!   stresslab check [<root>] [--list packages.json]
//!       Load every mod package the list names (default: the Stress Lab in
//!       this checkout), compile its scripts, and print every problem as
//!       JSON diagnostics with stable codes. Exit 1 when there are errors.
//!   stresslab test
//!       Run the Stress Lab headless for a scripted minute: generate the
//!       world, mine, sell, spawn a creeper and wait for it to explode.
//!       Prints a JSON report; exit 1 when a step fails.
use anyhow::{Context, Result, bail};
use bri_package::packages::PackageSet;
use bri_package_runtime::{Catalog, script::Runtime};
use bri_sim::session::{ActionAim, Command, PackageArg, PackageCommand, Session};
use serde_json::json;
use std::path::PathBuf;

fn check(args: &[String]) -> Result<bool> {
    let root = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(bri_stresslab::packages_root);
    let set = match args.iter().position(|a| a == "--list") {
        Some(i) => PackageSet::load(&PathBuf::from(
            args.get(i + 1).context("--list needs a path")?,
        ))?,
        None if root.join("packages.json").is_file() => {
            PackageSet::load(&root.join("packages.json"))?
        }
        None => bri_stresslab::fixture_set(),
    };
    let mut diagnostics: Vec<bri_package::diag::Diagnostic> = set.validate().0;
    let catalog = match Catalog::load(&root, &set, true) {
        Ok(catalog) => Some(catalog),
        Err(problems) => {
            diagnostics.extend(problems);
            None
        }
    };
    let mut compiled = false;
    if let Some(catalog) = &catalog {
        match Runtime::compile(catalog) {
            Ok(_) => compiled = true,
            Err(problems) => diagnostics.extend(problems),
        }
    }
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity == bri_package::diag::Severity::Error)
        .count();
    let packages: Vec<_> = catalog
        .iter()
        .flat_map(|c| c.packages.values())
        .map(|p| {
            json!({
                "id": p.id(),
                "version": p.manifest.version,
                "side": p.side,
                "capabilities": p.manifest.capabilities,
                "provides": p.manifest.provides.iter().map(|x| &x.id).collect::<Vec<_>>(),
                "commands": p.behaviour.as_ref().map(|b| b.commands.iter().map(|c| &c.name).collect::<Vec<_>>()),
            })
        })
        .collect();
    let report = json!({ "ok": errors == 0, "compiled": compiled, "packages": packages, "diagnostics": diagnostics });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(errors == 0)
}

fn command(
    s: &mut Session,
    owner: u64,
    sequence: u64,
    package: &str,
    name: &str,
    args: Vec<PackageArg>,
    aim: Option<ActionAim>,
) -> Result<()> {
    s.command_with_aim(
        owner,
        sequence,
        Command::Package(PackageCommand {
            package: package.into(),
            command: name.into(),
            args,
        }),
        aim,
    )
    .map(|_| ())
}
fn test() -> Result<bool> {
    let started = std::time::Instant::now();
    let (mut s, spawns) = bri_stresslab::fixture_session(None)?;
    let generated = s.package_stats();
    let player = s.join("Tester".into(), spawns[0], true)?;
    let down = Some(ActionAim {
        yaw: 0.0,
        pitch: -1.5,
    });
    let mut sequence = 0;
    let mut steps = Vec::new();
    for _ in 0..30 {
        s.step()?;
    }
    let value = |s: &Session, key: &str| {
        s.package_value("stresslab-economy", player, key)
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
    };
    for _ in 0..12 {
        sequence += 1;
        let _ = command(
            &mut s,
            player,
            sequence,
            "stresslab-economy",
            "mine",
            vec![],
            down,
        );
        for _ in 0..20 {
            s.step()?;
        }
    }
    steps
        .push(json!({ "step": "mine", "ok": value(&s, "mined") > 0, "mined": value(&s, "mined") }));
    sequence += 1;
    command(
        &mut s,
        player,
        sequence,
        "stresslab-economy",
        "sell_all",
        vec![],
        None,
    )?;
    steps.push(json!({ "step": "sell", "ok": true, "bits": value(&s, "bits") }));
    sequence += 1;
    let forged = command(
        &mut s,
        player,
        sequence,
        "stresslab-economy",
        "give_bits",
        vec![PackageArg::Int(99)],
        None,
    )
    .is_err();
    steps.push(json!({ "step": "forged command refused", "ok": forged }));
    for _ in 0..300 {
        s.step()?;
    }
    sequence += 1;
    command(
        &mut s,
        player,
        sequence,
        "stresslab-creeper",
        "spawn",
        vec![],
        None,
    )?;
    let spawned = s.package_entities().len() == 1;
    let explosions = |s: &Session| {
        s.package_state()
            .packages
            .get("stresslab-creeper")
            .and_then(|n| n.global.get("explosions"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
    };
    let mut exploded = false;
    for _ in 0..2400 {
        s.step()?;
        if explosions(&s) > 0 {
            exploded = true;
            break;
        }
    }
    let health = s.vitals().get(&player).map_or(0.0, |v| v.health);
    steps.push(json!({ "step": "creeper spawns", "ok": spawned }));
    steps.push(json!({ "step": "creeper explodes and hurts the player", "ok": exploded && health < 100.0, "health": health }));
    let ok = steps.iter().all(|s| s["ok"] == true);
    let report = json!({
        "ok": ok,
        "generated": generated,
        "after": s.package_stats(),
        "steps": steps,
        "diagnostics": s.package_diagnostics(),
        "seconds": started.elapsed().as_secs_f32(),
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(ok)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ok = match args.first().map(String::as_str) {
        Some("check") => check(&args[1..])?,
        Some("test") => test()?,
        _ => bail!("usage: stresslab check [<root>] [--list packages.json] | stresslab test"),
    };
    if !ok {
        std::process::exit(1);
    }
    Ok(())
}
