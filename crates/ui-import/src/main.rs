//! `bri-ui-import --v20 <install> --decompiled <dir> --stock-defaults <file> --out <dir>`
//!
//! `--decompiled` is a directory containing `client/ui/allClientGuis-Vanilla.gui`,
//! `client/scripts/allClientScripts-Vanilla.cs` and
//! `server/scripts/allGameScripts-Vanilla.cs` (for this repo: `.research/v20-dso`).
//! `--stock-defaults` is the stock v20 `client/defaults.cs`, not the B4v21 one
//! (for this repo: `.research/bl-decompiled/v20/client/defaults.cs`).

use anyhow::{Context, Result, bail};
use bri_ui_import::{Inputs, convert};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut v20, mut dec, mut defaults, mut out) = (None, None, None, None);
    let mut brick_catalog = None;
    while let Some(a) = args.next() {
        let v = args.next().with_context(|| format!("{a} needs a value"))?;
        match a.as_str() {
            "--v20" => v20 = Some(PathBuf::from(v)),
            "--decompiled" => dec = Some(PathBuf::from(v)),
            "--stock-defaults" => defaults = Some(PathBuf::from(v)),
            "--out" => out = Some(PathBuf::from(v)),
            "--brick-catalog" => brick_catalog = Some(PathBuf::from(v)),
            _ => bail!("unknown argument {a}"),
        }
    }
    let dec = dec.context("--decompiled required")?;
    let inputs = Inputs {
        v20_root: v20.context("--v20 required")?,
        client_gui: dec.join("client/ui/allClientGuis-Vanilla.gui"),
        client_scripts: dec.join("client/scripts/allClientScripts-Vanilla.cs"),
        server_scripts: dec.join("server/scripts/allGameScripts-Vanilla.cs"),
        stock_client_defaults: defaults.context("--stock-defaults required")?,
        brick_catalog,
    };
    let out = out.context("--out required")?;
    let r = convert(&inputs, &out)?;
    let p = &r.pack;
    println!(
        "wrote {} files ({} KiB) to {}",
        r.files_written,
        r.bytes_written / 1024,
        out.display()
    );
    println!(
        "images {} fonts {} skins {} styles {} layouts {} maps {} binds {} remap {} warnings {}",
        p.images.len(),
        p.fonts.len(),
        p.skins.len(),
        p.styles.len(),
        p.layouts.len(),
        p.maps.len(),
        p.data.default_binds.len(),
        p.data.remap.len(),
        p.warnings.len()
    );
    for w in &p.warnings {
        println!("warning: {w}");
    }
    Ok(())
}
