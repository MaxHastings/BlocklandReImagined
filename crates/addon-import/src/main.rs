use anyhow::{Context, Result, bail};
use bri_addon_import::{Options, import};
use std::path::PathBuf;

const USAGE: &str = "Usage: bri-import-addon ADDON(.zip|folder) FRESH_OUTPUT_DIR [--reference V20_ROOT] [--core RECOVERED_SCRIPT.cs]... [--version 1.0.0] [--json]

Converts one legacy Blockland Add-On into a native package directory and writes
import-report.json and IMPORT-REPORT.md into it. Never executes scripts.";

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut positional = vec![];
    let mut reference = None;
    let mut core = vec![];
    let mut version = "1.0.0".to_string();
    let mut json = false;
    while let Some(a) = args.next() {
        match a.to_str() {
            Some("--reference") => reference = Some(PathBuf::from(args.next().context(USAGE)?)),
            Some("--core") => core.push(PathBuf::from(args.next().context(USAGE)?)),
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
    let [input, out] = <[PathBuf; 2]>::try_from(positional).map_err(|_| anyhow::anyhow!(USAGE))?;
    if bri_package::id::Version::parse(&version).is_err() {
        bail!("--version must be major.minor.patch");
    }
    let report = import(&Options {
        input,
        out: out.clone(),
        reference,
        core,
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
