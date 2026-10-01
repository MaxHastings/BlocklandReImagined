//! Headless check of a whole saves folder, as the game hosts each save: the
//! game's own background converter turns every `.bls` into a native save,
//! Load Bricks reads it, a fresh host session on its map loads it to the
//! end, and the joined player's client builds its brick chunks, query
//! mirror and prediction mirror from the replicated world. No window, no
//! input; the saves folder is only read (conversions go to a temporary
//! folder).
//!
//! Usage: saves_host_probe <content-root> <saves-dir> <report.json>
//! Prints one line per failing save and a summary; the report lists every
//! save with its outcome and the prints this client does not have.
use anyhow::{Result, ensure};
use bri_client::{
    content::ClientContent,
    old_saves::{Converter, OldSaves},
    save_host::SaveHost,
    saves::Store,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 3,
        "Usage: saves_host_probe <content-root> <saves-dir> <report.json>"
    );
    let state = std::env::temp_dir().join(format!("saves_host_probe-{}", std::process::id()));
    std::fs::create_dir_all(&state)?;
    let result = run(&args, &state);
    let _ = std::fs::remove_dir_all(&state);
    result
}

fn run(args: &[PathBuf], state: &std::path::Path) -> Result<()> {
    let content = ClientContent::load(&args[0])?;
    let old = OldSaves::new(args[1].clone(), state.join("converted-saves"));
    old.set_converter(Converter::new(&content)?);
    old.start();
    while old.busy() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let store = Store::new(state, &content, Some(old.clone()));
    let setup = SaveHost::new(content)?;

    let entries: Vec<_> = store
        .list()?
        .into_iter()
        .filter(|e| e.path.starts_with(old.cache()))
        .collect();
    let mut saves = vec![];
    let (mut failed, mut with_unknown_prints) = (0, 0);
    // Every unknown print across the saves. The bundle holds every stock
    // v20 print, so an unknown name is never a stock print; this flags the
    // near misses: a stock image name under a class it is not in
    // (`2x2f/computer1`), which a wrong class mapping would also produce.
    let stock_names: std::collections::BTreeSet<String> = setup
        .materials
        .bundle
        .prints
        .iter()
        .map(|p| p.name.to_ascii_lowercase())
        .collect();
    let stock_name = |name: &str| {
        let alias = bri_content::brick_materials::legacy_print_alias(name);
        let name = alias.as_deref().unwrap_or(name);
        name.split_once('/')
            .is_some_and(|(_, stem)| stock_names.contains(&stem.to_ascii_lowercase()))
    };
    let mut all_unknown: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for entry in &entries {
        let label = format!("{}/{}", entry.info.map, entry.info.name);
        let unknown = Store::read(entry)
            .map(|b| setup.unknown_prints(&b))
            .unwrap_or_default();
        if !unknown.is_empty() {
            with_unknown_prints += 1;
        }
        for (name, count) in &unknown {
            let entry = all_unknown.entry(name.clone()).or_default();
            entry.0 += count;
            entry.1 += 1;
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| setup.host(entry)))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("panicked")));
        let result = match &outcome {
            Ok(placed) => json!({ "placed": placed }),
            Err(error) => {
                failed += 1;
                println!("FAILED {label}: {error:#}");
                json!({ "error": format!("{error:#}") })
            }
        };
        saves.push(json!({
            "save": label,
            "listed_bricks": entry.info.brick_count,
            "unknown_prints": unknown,
            "result": result,
        }));
    }
    let summary = json!({
        "saves": entries.len(),
        "failed": failed,
        "with_unknown_prints": with_unknown_prints,
        "unknown_print_bricks": all_unknown.values().map(|(b, _)| b).sum::<usize>(),
        "stock_name_other_class_bricks": all_unknown
            .iter()
            .filter(|(name, _)| stock_name(name))
            .map(|(_, (b, _))| b)
            .sum::<usize>(),
    });
    for (name, (bricks, saves)) in all_unknown.iter().filter(|(name, _)| stock_name(name)) {
        println!("STOCK NAME, OTHER CLASS {name}: {bricks} bricks in {saves} saves");
    }
    println!("{summary}");
    let unknown: BTreeMap<_, _> = all_unknown
        .iter()
        .map(|(name, (bricks, saves))| {
            (
                name,
                json!({ "bricks": bricks, "saves": saves, "stock_name_other_class": stock_name(name) }),
            )
        })
        .collect();
    let report: Value = json!({ "summary": summary, "unknown_prints": unknown, "saves": saves });
    std::fs::write(&args[2], serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
