//! `bri-addon-check <add-on folder> [--json]`: check an Add-On the way the
//! game will load it and say what it is. Exits 1 when it cannot load.
use std::path::PathBuf;

fn main() {
    let mut json = false;
    let mut folder = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!(
                    "Usage: bri-addon-check ADD_ON_FOLDER [--json]\n\n\
                     Checks one Add-On (and the Add-Ons it needs, found beside it) the way\n\
                     the game loads it: manifest, files, HUD bindings, scripts. Runs nothing."
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
        eprintln!("Usage: bri-addon-check ADD_ON_FOLDER [--json]");
        std::process::exit(2);
    };
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
        std::process::exit(1);
    }
}
