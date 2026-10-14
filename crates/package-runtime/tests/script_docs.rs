//! Every function a rule script can call is in the modding guide
//! (docs/modding). Rhai is a small language with little help online, so
//! the guide is where modders learn what exists; a function added without
//! a line there is one nobody finds.
use std::{collections::BTreeSet, fs, path::Path};

/// Names registered on the engine that are not ours to document: Rhai's
/// own operators and conversions, given to views so they behave like maps.
const LANGUAGE: &[&str] = &["==", "!=", "contains", "to_string", "to_debug"];

fn rust_files(dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("source folder reads") {
        let path = entry.expect("source entry reads").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(fs::read_to_string(&path).expect("source file reads"));
        }
    }
}

/// The names in `register_fn("name", ...)`, however the call is wrapped.
fn registered(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("register_fn(") {
        rest = rest[at + "register_fn(".len()..].trim_start();
        if let Some(quoted) = rest.strip_prefix('"')
            && let Some(end) = quoted.find('"')
        {
            names.push(quoted[..end].to_owned());
        }
    }
    names
}

/// Whether `name` appears in `text` as a whole word.
fn mentions(text: &str, name: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(word)
            && !text[at + name.len()..].chars().next().is_some_and(word)
    })
}

#[test]
fn every_script_function_is_in_the_modding_guide() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources = Vec::new();
    rust_files(&crate_dir.join("src"), &mut sources);
    let functions: BTreeSet<String> = sources
        .iter()
        .flat_map(|s| registered(s))
        .filter(|name| !LANGUAGE.contains(&name.as_str()))
        .collect();
    assert!(
        functions.len() > 100,
        "found only {} script functions; has registration moved?",
        functions.len()
    );

    let guide_dir = crate_dir.join("../../docs/modding");
    let mut guide = String::new();
    for entry in fs::read_dir(&guide_dir).expect("docs/modding reads") {
        let path = entry.expect("guide entry reads").path();
        if path.extension().is_some_and(|e| e == "md") {
            guide += &fs::read_to_string(&path).expect("guide page reads");
        }
    }

    let missing: Vec<_> = functions
        .iter()
        .filter(|name| !mentions(&guide, name))
        .collect();
    assert!(
        missing.is_empty(),
        "script functions missing from docs/modding (add each to rules.md's \
         Script functions table or the section it belongs to): {missing:?}"
    );
}
