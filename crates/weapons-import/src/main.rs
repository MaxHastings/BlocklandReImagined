use anyhow::{Context, Result};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        args.len() == 3,
        "Usage: bri-weapons-import REFERENCE_ROOT RECOVERED_CORE_SCRIPT FRESH_OUTPUT"
    );
    let pack = bri_weapons_import::convert(args[0].as_ref(), args[1].as_ref(), args[2].as_ref())
        .context("weapon conversion")?;
    println!(
        "{} items, {} images, {} projectiles, {} resources, {} diagnostics",
        pack.items.len(),
        pack.images.len(),
        pack.projectiles.len(),
        pack.resources.len(),
        pack.diagnostics.len()
    );
    Ok(())
}
