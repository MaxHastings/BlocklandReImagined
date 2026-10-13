//! `bri-addon-check <add-on folder> [--json] [--package [--out FILE.zip]]`:
//! check an Add-On the way the game will load it and say what it is. Exits 1
//! when it cannot load. With `--package`, an Add-On that passes is also
//! written as the zip the game installs and players download, so its files
//! arrive exactly as they are on disk.
use std::path::PathBuf;

const USAGE: &str = "Usage: bri-addon-check ADD_ON_FOLDER [--json] [--package [--out FILE.zip]]";

fn main() {
    let mut json = false;
    let mut package = false;
    let mut out = None;
    let mut folder = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--package" => package = true,
            "--out" => match args.next() {
                Some(path) => out = Some(PathBuf::from(path)),
                None => {
                    eprintln!("--out needs a file name; see --help");
                    std::process::exit(2);
                }
            },
            "-h" | "--help" => {
                println!(
                    "{USAGE}\n\n\
                     Checks one Add-On (and the Add-Ons it needs, found beside it) the way\n\
                     the game loads it: manifest, files, sounds, HUD bindings, scripts. Runs\n\
                     nothing.\n\n\
                     --package  when the check passes, write the Add-On (with its\n\
                     \x20          <folder>-rules companion beside it, if any) as one zip,\n\
                     \x20          read it back and confirm every file is byte-for-byte\n\
                     \x20          the same. Drop the zip in the game's Add-Ons folder to\n\
                     \x20          install it. Hidden files and build output (target/) are\n\
                     \x20          left out.\n\
                     --out      where the zip goes (default: <folder name>.zip here)."
                );
                return;
            }
            _ if folder.is_none() => folder = Some(PathBuf::from(arg)),
            _ => {
                eprintln!("unexpected argument `{arg}`; see --help");
                std::process::exit(2);
            }
        }
    }
    let Some(folder) = folder else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    if out.is_some() && !package {
        eprintln!("--out is for --package; see --help");
        std::process::exit(2);
    }
    let report = bri_package_runtime::check::check(&folder);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("reports serialize")
        );
    } else {
        println!("{report}");
    }
    if !report.ok {
        if package {
            eprintln!("Not packaged: fix the errors above first.");
        }
        std::process::exit(1);
    }
    if package {
        let name = folder
            .canonicalize()
            .ok()
            .and_then(|f| f.file_name().map(|n| n.to_owned()))
            .unwrap_or_else(|| "add-on".into());
        let out = out.unwrap_or_else(|| {
            let mut file = name;
            file.push(".zip");
            PathBuf::from(file)
        });
        match bri_package::classic::pack_native_zip(&folder, &out) {
            Ok(packed) => {
                // The report went to stdout as JSON; keep it parseable.
                let say = |line: String| {
                    if json {
                        eprintln!("{line}");
                    } else {
                        println!("{line}");
                    }
                };
                say(format!(
                    "\nPackaged {} ({} files, {} KiB) as {}\n  folders: {}\n  sha256: {}\n  every file read back byte-for-byte identical",
                    packed.folders[0],
                    packed.files,
                    packed.bytes.div_ceil(1024),
                    out.display(),
                    packed.folders.join(", "),
                    packed.sha256
                ));
                if !packed.skipped.is_empty() {
                    say(format!("  left out: {}", packed.skipped.join(", ")));
                }
            }
            Err(error) => {
                eprintln!("Not packaged: {error:#}");
                std::process::exit(1);
            }
        }
    }
}
