//! A fixed corpus of known-tricky `.bls` saves from Maxwell's saves folder,
//! hosted as the game hosts them (see `bri_client::save_host`). It replaces
//! the random sample of saves once checked by hand before a release; the gate
//! runs it whenever a change touches saving, loading, the `.bls` converter or
//! brick and print data (`SAVE_CORPUS_PATHS` in tools/gate.py).
//!
//! The saves are never in the repository: `save-corpus.json` lists only their
//! paths relative to the saves folder (`BRI_SAVES`, else
//! `%LOCALAPPDATA%/BlocklandReImagined/saves`). Content comes from
//! `BRI_CONTENT`, else `content/`. Without either the test says so and passes.
//! The chosen saves are copied into a temporary saves folder; the originals
//! are only read.
use anyhow::{Context, Result, ensure};
use bri_client::{
    content::ClientContent,
    old_saves::{Converter, OldSaves},
    save_host::SaveHost,
    saves::Store,
};
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
struct Corpus {
    save: Vec<Save>,
}
#[derive(serde::Deserialize)]
struct Save {
    /// Relative to the saves folder, `/`-separated.
    path: String,
    #[allow(dead_code)]
    reason: String,
    /// Bricks the host places, when stable.
    bricks: Option<usize>,
    /// Hosting must fail with an error containing this text.
    error: Option<String>,
}

fn saves_folder() -> Option<PathBuf> {
    std::env::var_os("BRI_SAVES").map(PathBuf::from).or_else(|| {
        std::env::var_os("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join("BlocklandReImagined").join("saves"))
    })
}

fn content_root() -> PathBuf {
    std::env::var_os("BRI_CONTENT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

#[test]
#[ignore = "Maxwell's saves folder (BRI_SAVES) and generated content (BRI_CONTENT or content/); no window"]
fn the_fixed_save_corpus_hosts_like_the_game() -> Result<()> {
    let corpus: Corpus = serde_json::from_str(include_str!("save-corpus.json"))?;
    let root = content_root();
    let Some(saves) = saves_folder().filter(|s| s.is_dir()) else {
        eprintln!("skipped: no saves folder; set BRI_SAVES to Maxwell's saves folder");
        return Ok(());
    };
    if !root.is_dir() {
        eprintln!("skipped: no generated content at {}; set BRI_CONTENT", root.display());
        return Ok(());
    }
    let state = std::env::temp_dir().join(format!("bri-save-corpus-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let result = run(&corpus, &root, &saves, &state);
    let _ = std::fs::remove_dir_all(&state);
    result
}

fn run(corpus: &Corpus, root: &Path, saves: &Path, state: &Path) -> Result<()> {
    let folder = state.join("saves");
    let mut present = vec![];
    for save in &corpus.save {
        let source = saves.join(&save.path);
        if !source.is_file() {
            eprintln!("missing from the saves folder, not checked: {}", save.path);
            continue;
        }
        let target = folder.join(&save.path);
        std::fs::create_dir_all(target.parent().context("save folder")?)?;
        std::fs::copy(&source, &target)?;
        present.push(save);
    }
    ensure!(
        !present.is_empty(),
        "none of the corpus saves is in {}",
        saves.display()
    );
    let started = std::time::Instant::now();
    let content = ClientContent::load(root)?;
    let old = OldSaves::new(folder.clone(), state.join("converted-saves"));
    old.set_converter(Converter::new(&content)?);
    old.start();
    while old.busy() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    eprintln!("converted {} saves in {:.0?}", present.len(), started.elapsed());
    let store = Store::new(state, &content, Some(old.clone()));
    let entries = store.list()?;
    drop(content);

    // One failure message per save, or None. Saves are hosted a few at a
    // time, each worker with its own content, and reported in manifest order.
    let check = |host: &SaveHost, save: &Save| -> Option<String> {
        let Ok(source) = folder.join(&save.path).canonicalize() else {
            return Some(format!("{}: its copy went missing", save.path));
        };
        let entry = entries.iter().find(|e| {
            e.source
                .as_ref()
                .and_then(|s| s.canonicalize().ok())
                .is_some_and(|s| s == source)
        });
        let Some(entry) = entry else {
            return Some(format!("{}: the game did not list it after converting", save.path));
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host.host(entry)))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("panicked")));
        match (&outcome, &save.error, save.bricks) {
            (Ok(placed), None, Some(want)) if *placed != want => Some(format!(
                "{}: placed {placed} bricks, expected {want} (update save-corpus.json if the change is intended)",
                save.path
            )),
            (Ok(placed), None, _) => {
                eprintln!("ok {}: {placed} bricks", save.path);
                None
            }
            (Ok(placed), Some(want), _) => Some(format!(
                "{}: placed {placed} bricks, expected the error {want:?}",
                save.path
            )),
            (Err(error), Some(want), _) if format!("{error:#}").contains(want.as_str()) => {
                eprintln!("ok {}: refused as expected ({error:#})", save.path);
                None
            }
            (Err(error), _, _) => Some(format!("{}: {error:#}", save.path)),
        }
    };
    // Largest first, so the big saves overlap instead of finishing last.
    let mut order: Vec<usize> = (0..present.len()).collect();
    order.sort_by_key(|&i| {
        std::cmp::Reverse(std::fs::metadata(folder.join(&present[i].path)).map_or(0, |m| m.len()))
    });
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(vec![None; present.len()]);
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    std::thread::scope(|scope| -> Result<()> {
        let mut handles = vec![];
        for _ in 0..workers {
            let handle = scope.spawn(|| -> Result<()> {
                let host = SaveHost::new(ClientContent::load(root)?)?;
                loop {
                    let n = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(&i) = order.get(n) else {
                        return Ok(());
                    };
                    let save = present[i];
                    let failure = check(&host, save);
                    results.lock().unwrap_or_else(|e| e.into_inner())[i] = failure;
                }
            });
            handles.push(handle);
        }
        for handle in handles {
            handle.join().map_err(|_| anyhow::anyhow!("a corpus worker panicked"))??;
        }
        Ok(())
    })?;
    eprintln!("hosted {} saves in {:.0?}", present.len(), started.elapsed());
    let failures: Vec<String> = results
        .into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .flatten()
        .collect();
    ensure!(
        failures.is_empty(),
        "{} of {} corpus saves failed:\n{}",
        failures.len(),
        present.len(),
        failures.join("\n")
    );
    Ok(())
}
