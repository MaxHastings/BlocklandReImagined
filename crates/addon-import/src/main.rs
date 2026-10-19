use anyhow::{Context, Result, bail};
use bri_addon_import::{Options, import, porting};
use std::path::PathBuf;

const USAGE: &str = "Usage:
  bri-import-addon ADDON(.zip|folder) FRESH_OUTPUT_DIR [--reference V20_ROOT] [--core RECOVERED_SCRIPT.cs]... [--installed CONTENT_ROOT] [--version 1.0.0] [--json]
  bri-import-addon port ADDON(.zip|folder) FRESH_WORK_DIR [--reference V20_ROOT] [--core RECOVERED_SCRIPT.cs]... [--installed CONTENT_ROOT]
  bri-import-addon check-port WORK_DIR
  bri-import-addon gltf MODEL(.glb|.gltf) FRESH_OUTPUT_DIR [--kind body|item|prop|map|brick] [--id NAME] [--name MENU_NAME]
      [--height UNITS | --scale FACTOR] [--fps 30] [--right-hand NODE] [--left-hand NODE] [--eye NODE] [--loop CLIP]...
      [--lod TRIANGLES] [--lod-pixels 100] [--box-width UNITS] [--box-height UNITS] [--sky DIR] [--terrain DIR] [--scatter MODEL.glb]... [--backdrop MODEL.gltf] [--regions FILE.json] [--json]
  bri-import-addon export-map CONTENT_OR_BUNDLE_DIR MAP OUTPUT.glb
  bri-import-addon map MAP_ADDON(.zip|folder) FRESH_OUTPUT_DIR [--v20 V20_ROOT] [--mission NAME] [--id NAME] [--name MENU_NAME] [--version 1.0.0]

Converts one legacy Blockland Add-On into a native package directory and writes
import-report.json and IMPORT-REPORT.md into it. Never executes scripts.
`--installed` names the game's content folder: base datablocks the Add-On
inherits from or names (a brick's parent, a sound) are read from it.

`port` sets up a folder for porting the Add-On's scripts natively: the import,
the original scripts, a drafted port, checks of what v20 does, stubs and an
AGENT.md with instructions. `check-port` imports the Add-On again with that
port, runs the checks and prints the entry for the ports list.

`gltf` converts a Blender (glTF 2.0) model: a body becomes a package with an
archetype, an item or prop a `models/` folder for weapons.json or
vehicles.json, a map a map Add-On with its spawn points and lights, a brick
a brick Add-On for the brick menu. Names, scale and clips:
docs/modding/gltf-models.md; maps: docs/modding/maps.md.

`export-map` writes a converted map (`bedroom`, `slate`, or any map id) as a
.glb for Blender: its geometry and textures, `col_` collision, `spawn`
empties and its lights, ready to edit and import again with `gltf --kind map`.
Give it the game's `content` folder or a map bundle folder.

`map` converts an old Blockland map Add-On (`Map_*.zip`: a .mis mission, its
.dif interiors, .ter terrain, textures and sky) into a map Add-On, with its
lightmaps, through the converter that makes v20's own maps. `--v20` names a
Blockland v20 folder for the textures and models the map borrows from it.";

fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("gltf") {
        return gltf(std::env::args().skip(2));
    }
    if std::env::args().nth(1).as_deref() == Some("map") {
        return v20_map(std::env::args().skip(2));
    }
    if std::env::args().nth(1).as_deref() == Some("export-map") {
        return export_map(std::env::args().skip(2));
    }
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

fn gltf(mut args: impl Iterator<Item = String>) -> Result<()> {
    use bri_addon_import::gltf;
    let mut positional = vec![];
    let mut kind = gltf::Kind::Body;
    let mut opts = gltf::Options::new("", "", kind);
    let mut json = false;
    while let Some(a) = args.next() {
        let mut value = || args.next().context(USAGE);
        let number = |v: String| {
            v.parse::<f32>()
                .with_context(|| format!("{v} is not a number"))
        };
        match a.as_str() {
            "--kind" => kind = gltf::Kind::parse(&value()?)?,
            "--id" => opts.id = Some(value()?),
            "--name" => opts.name = Some(value()?),
            "--height" => opts.height = Some(number(value()?)?),
            "--scale" => opts.scale = Some(number(value()?)?),
            "--fps" => opts.fps = number(value()?)?,
            "--right-hand" => opts.right_hand = Some(value()?),
            "--left-hand" => opts.left_hand = Some(value()?),
            "--eye" => opts.eye = Some(value()?),
            "--loop" => opts.loops.push(value()?),
            "--lod" => {
                let v = value()?;
                opts.lod = Some(
                    v.parse()
                        .with_context(|| format!("{v} is not a triangle count"))?,
                )
            }
            "--lod-pixels" => opts.lod_pixels = number(value()?)?,
            "--box-width" => opts.box_width = Some(number(value()?)?),
            "--box-height" => opts.box_height = Some(number(value()?)?),
            "--sky" => opts.sky = Some(PathBuf::from(value()?)),
            "--terrain" => opts.terrain = Some(PathBuf::from(value()?)),
            "--scatter" => opts.scatter.push(PathBuf::from(value()?)),
            "--backdrop" => opts.backdrop = Some(PathBuf::from(value()?)),
            "--regions" => opts.regions = Some(PathBuf::from(value()?)),
            "--version" => opts.version = value()?,
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            flag if flag.starts_with("--") => bail!("unknown option {flag}\n{USAGE}"),
            _ => positional.push(PathBuf::from(a)),
        }
    }
    let [input, out] = <[PathBuf; 2]>::try_from(positional).map_err(|_| anyhow::anyhow!(USAGE))?;
    if bri_package::id::Version::parse(&opts.version).is_err() {
        bail!("--version must be major.minor.patch");
    }
    opts.input = input;
    opts.out = out.clone();
    opts.kind = kind;
    let report = gltf::import(&opts)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.markdown());
        println!("\nWritten to {}", out.display());
    }
    Ok(())
}

/// The map bundle folder in `dir` holding `map`: `dir` itself, or one of
/// its folders (the game's `content` folder holds the map bundle pack).
fn bundle_dir(dir: &std::path::Path, map: &str) -> Result<PathBuf> {
    use bri_addon_import::gltf::find_map;
    if dir.join("bundle.json").is_file() {
        return Ok(dir.to_path_buf());
    }
    let mut dirs: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("bundle.json").is_file())
        .collect();
    dirs.sort();
    dirs.into_iter()
        .find(|d| find_map(d, map).is_ok())
        .with_context(|| format!("no map bundle with map `{map}` in {}", dir.display()))
}

fn v20_map(args: impl Iterator<Item = String>) -> Result<()> {
    let mut positional = vec![];
    let mut opts = bri_addon_import::v20_map::Options {
        input: PathBuf::new(),
        out: PathBuf::new(),
        v20: None,
        mission: None,
        id: None,
        name: None,
        version: "1.0.0".into(),
    };
    let mut args = args;
    while let Some(a) = args.next() {
        let mut value = || args.next().context(USAGE);
        match a.as_str() {
            "--v20" => opts.v20 = Some(PathBuf::from(value()?)),
            "--mission" => opts.mission = Some(value()?),
            "--id" => opts.id = Some(value()?),
            "--name" => opts.name = Some(value()?),
            "--version" => opts.version = value()?,
            _ => positional.push(PathBuf::from(a)),
        }
    }
    let [input, out] = <[PathBuf; 2]>::try_from(positional).map_err(|_| anyhow::anyhow!(USAGE))?;
    opts.input = input;
    opts.out = out;
    let made = bri_addon_import::v20_map::import(&opts)?;
    for (mission, map) in made.missions.iter().zip(&made.map_ids) {
        println!("Imported {mission} as the map {map}");
    }
    println!(
        "{} files in {}. Check it with bri-addon-check.",
        made.files,
        opts.out.display()
    );
    Ok(())
}

fn export_map(args: impl Iterator<Item = String>) -> Result<()> {
    let args: Vec<String> = args.collect();
    let [dir, map, out] = <[String; 3]>::try_from(args).map_err(|_| anyhow::anyhow!(USAGE))?;
    let root = bundle_dir(std::path::Path::new(&dir), &map)?;
    let (glb, report) = bri_addon_import::gltf::export_map(&root, &map)?;
    std::fs::write(&out, glb).with_context(|| format!("writing {out}"))?;
    print!("{}", report.markdown());
    println!("\nWritten to {out}. Open it in Blender with File > Import > glTF 2.0.");
    Ok(())
}
