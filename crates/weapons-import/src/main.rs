use anyhow::{Context, Result};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        args.len() == 4,
        "Usage: bri-weapons-import REFERENCE_ROOT RECOVERED_CORE_SCRIPT RECOVERED_DAMAGE_TYPES FRESH_OUTPUT"
    );
    let pack = bri_weapons_import::convert(
        args[0].as_ref(),
        args[1].as_ref(),
        args[2].as_ref(),
        args[3].as_ref(),
    )
    .context("weapon conversion")?;
    println!(
        "{} items, {} images, {} projectiles, {} damage types, {} explosions, {} resources, {} diagnostics",
        pack.items.len(),
        pack.images.len(),
        pack.projectiles.len(),
        pack.damage_types.len(),
        pack.explosions.len(),
        pack.resources.len(),
        pack.diagnostics.len()
    );
    Ok(())
}
