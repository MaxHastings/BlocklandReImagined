use anyhow::{Context, Result, bail};
use bri_addon_import::{Options, import, porting};
use std::path::PathBuf;

const USAGE: &str = "Usage:
  bri-import-addon ADDON(.zip|folder) FRESH_OUTPUT_DIR [--reference V20_ROOT] [--core RECOVERED_SCRIPT.cs]... [--installed CONTENT_ROOT] [--version 1.0.0] [--json]
  bri-import-addon port ADDON(.zip|folder) FRESH_WORK_DIR [--reference V20_ROOT] [--core RECOVERED_SCRIPT.cs]... [--installed CONTENT_ROOT]
  bri-import-addon check-port WORK_DIR

Converts one legacy Blockland Add-On into a native package directory and writes
import-report.json and IMPORT-REPORT.md into it. Never executes scripts.
`--installed` names the game's content folder: base datablocks the Add-On
inherits from or names (a brick's parent, a sound) are read from it.

`port` sets up a folder for porting the Add-On's scripts natively: the import,
the original scripts, a drafted port, checks of what v20 does, stubs and an
AGENT.md with instructions. `check-port` imports the Add-On again with that
port, runs the checks and prints the entry for the ports list.";

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut positional = vec![];
    let mut reference = None;
    let mut core = vec![];
    let mut installed = None;
    let mut version = "1.0.0".to_string();
    let mut json = false;
    while let Some(a) = args.next() {
        match a.to_str() {
            Some("--reference") => reference = Some(PathBuf::from(args.next().context(USAGE)?)),
            Some("--core") => core.push(PathBuf::from(args.next().context(USAGE)?)),
            Some("--installed") => installed = Some(PathBuf::from(args.next().context(USAGE)?)),
            Some("--version") => {
                version = args.next().context(USAGE)?.to_string_lossy().into_owned()
            }
            Some("--json") => json = true,
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => positional.push(PathBuf::from(a)),
        }
    }
    match positional.first().and_then(|p| p.to_str()) {
        Some("port") if positional.len() == 3 => {
            let s = porting::scaffold(&positional[1], &positional[2], reference, core, installed)?;
            println!(
                "Port folder for {} set up in {}.",
                s.report.source.name,
                s.dir.display()
            );
            if let Some(l) = &s.listed {
                println!("The game already lists a port of it: {l}.");
            }
            for f in &s.drafted {
                println!("  drafted: {f}");
            }
            for f in &s.to_port {
                println!("  to port by hand: {f}");
            }
            println!(
                "\nRead AGENT.md there (or hand it to your agent), then run:\n  bri-import-addon check-port \"{}\"",
                s.dir.display()
            );
            return Ok(());
        }
        Some("check-port") if positional.len() == 2 => {
            let c = porting::check(&positional[1])?;
            if !c.applied {
                println!(
                    "The port was not applied: {}.",
                    c.reason.as_deref().unwrap_or("unknown")
                );
            }
            for (line, ok) in &c.results {
                println!("  {} {line}", if *ok { "PASS" } else { "FAIL" });
            }
            for f in &c.unported {
                println!("  still unported: {f}");
            }
            if !c.passed() {
                bail!("check-port failed; fix the port and run it again");
            }
            println!(
                "\nChecks pass. Entry ({}), also written to submit.json:\n{}",
                c.entry.status,
                serde_json::to_string_pretty(&c.entry)?
            );
            println!(
                "Submit it as crates/addon-import/ports/{}/: the port/ folder, with submit.json as entry.json.",
                c.entry.port
            );
            return Ok(());
        }
        Some("port" | "check-port") => bail!("{USAGE}"),
        _ => {}
    }
    let [input, out] = <[PathBuf; 2]>::try_from(positional).map_err(|_| anyhow::anyhow!(USAGE))?;
    if bri_package::id::Version::parse(&version).is_err() {
        bail!("--version must be major.minor.patch");
    }
    let report = import(&Options {
        input,
        out: out.clone(),
        reference,
        core,
        installed,
        version,
    })?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.markdown());
        println!("\nPackage written to {}", out.display());
    }
    Ok(())
}
