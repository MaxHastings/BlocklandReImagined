//! Render an Add-On's client code offscreen, headless, to PNG frames.
//!
//! `bri-addon-preview <add-on folder> <output folder> [frames]`
use anyhow::{Context, Result, bail};
use bri_client_sandbox::{AddOnCode, Budgets, Sandbox, TrustLevel, gpu};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, out, rest @ ..] = args.as_slice() else {
        bail!("usage: bri-addon-preview <add-on folder> <output folder> [frames]");
    };
    let frames: usize = rest.first().map(|f| f.parse()).transpose()?.unwrap_or(4);
    let code = match AddOnCode::load(&PathBuf::from(dir)) {
        Ok(Some(code)) => code,
        Ok(None) => bail!("{dir} has no client code"),
        Err(problems) => {
            for p in &problems {
                eprintln!(
                    "{} {}: {}",
                    p.code,
                    p.location.as_deref().unwrap_or(""),
                    p.message
                );
            }
            bail!("{} problems", problems.len());
        }
    };
    let sandbox = Sandbox::new()?;
    let mut addon = sandbox
        .start(&code, Budgets::default(), TrustLevel::Sandboxed)
        .map_err(|e| anyhow::anyhow!("{} stopped: {e}", code.name))?;
    let times: Vec<f32> = (0..frames).map(|i| i as f32 * 0.5).collect();
    let (adapter, images) = gpu::render_offscreen(&mut addon, 512, 384, &times)?;
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out)?;
    for (i, image) in images.iter().enumerate() {
        let path = out.join(format!("{}-{i:02}.png", code.id));
        gpu::write_png(image, &path).with_context(|| path.display().to_string())?;
        println!("{}", path.display());
    }
    println!(
        "{} frames of {} on {adapter}, code {}",
        images.len(),
        code.name,
        &code.code_hash[..12]
    );
    Ok(())
}
