//! Temporary content fix-up (throwaway tool).
//!
//! Writes the shared stock brick-surface pictures that exported bricks'
//! placeholder materials reference by relative URI: each of the five raw
//! surface masks from the `brick-materials` bundle, composited over white at
//! full opacity — exactly what a default-painted brick shows in game. They
//! are stored once, beside the bundle under `surfaces-composited/`, so every
//! brick's `.glb` can reference them by path instead of embedding a copy (an
//! embedded copy per file bloats the set past the shared geometry byte
//! budget). The raw masks themselves are never referenced: they are decal
//! alpha pictures and would show transparent or black in an editor, not
//! painted.
//!
//! Usage: `gen-brick-surface-pictures CONTENT_ROOT [--apply]`
//!
//! Dry-run by default; with `--apply` writes the five PNGs into
//! `<bundle>/surfaces-composited/`. Existing identical files are left alone;
//! a changed source rewrites them.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail, ensure};
use bri_addon_import::{composite_over_white, find_bundle_manifest};

struct Args {
    root: PathBuf,
    apply: bool,
}

const SURFACES: [&str; 5] = ["top", "side", "bottom_edge", "bottom_loop", "ramp"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(args) = parse_args(&args) else {
        eprintln!("usage: gen-brick-surface-pictures CONTENT_ROOT [--apply]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("gen-brick-surface-pictures: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Result<Args> {
    let mut root = None;
    let mut apply = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--apply" => apply = true,
            other if !other.starts_with('-') => {
                ensure!(root.is_none(), "unexpected argument: {other}");
                root = Some(PathBuf::from(other));
            }
            _ => bail!("unknown flag: {a}"),
        }
    }
    let Some(root) = root else {
        bail!("missing CONTENT_ROOT")
    };
    let root = root.canonicalize().context("content root does not exist")?;
    Ok(Args { root, apply })
}

fn run(args: &Args) -> Result<String> {
    let manifest = find_bundle_manifest(&args.root, 0)
        .with_context(|| "no brick-materials bundle found under the content root")?;
    let dir = manifest.parent().context("the bundle has no folder")?;

    // The raw mask each surface names in its own manifest.
    let bytes = fs::read(&manifest).with_context(|| format!("{} does not read", manifest.display()))?;
    let bundle: bri_content::brick_materials::Bundle =
        serde_json::from_slice(&bytes).context("the bundle manifest is unreadable")?;

    let out_dir = dir.join("surfaces-composited");
    if args.apply {
        fs::create_dir_all(&out_dir)
            .with_context(|| format!("cannot create {}", out_dir.display()))?;
    }

    let mut written = 0usize;
    let mut unchanged = 0usize;
    for name in SURFACES {
        let image = bundle
            .surfaces
            .get(name)
            .with_context(|| format!("the bundle names no {name} surface"))?;
        let raw = dir.join(&image.path);
        let rgba = image::load_from_memory_with_format(
            &fs::read(&raw).with_context(|| format!("{} does not read", raw.display()))?,
            image::ImageFormat::Png,
        )
        .with_context(|| format!("{} is not a PNG", raw.display()))?
        .into_rgba8();
        let (w, h, rgba) = composite_over_white(rgba);

        let target = out_dir.join(format!("{name}.png"));
        if !args.apply {
            println!(
                "would write {} ({}x{}){}",
                target.display(),
                w,
                h,
                if target.is_file() { " — exists" } else { "" }
            );
            continue;
        }
        let mut png = Vec::new();
        image::RgbaImage::from_raw(w, h, rgba)
            .expect("pixels")
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        if target.is_file() && fs::read(&target)? == png {
            unchanged += 1;
            continue; // Identical: leave the file (and its mtime) alone.
        }
        fs::write(&target, &png).with_context(|| format!("cannot write {}", target.display()))?;
        written += 1;
    }

    Ok(format!(
        "gen-brick-surface-pictures {}: {} picture(s) in {}, {} written, {} already current",
        if args.apply { "apply" } else { "dry-run" },
        SURFACES.len(),
        out_dir.display(),
        written,
        unchanged
    ))
}
