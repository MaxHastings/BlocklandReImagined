//! Embeds `ports/` (the verified-ports list and each port's files) so the
//! importer the game ships applies ports with no files beside it.
use std::{fmt::Write, path::Path};

fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.path());
    for e in entries {
        let path = e.path();
        if path.is_dir() {
            walk(&path, root, out);
        } else {
            out.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn main() {
    let root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("ports");
    println!("cargo:rerun-if-changed=ports");
    let mut files = vec![];
    walk(&root, &root, &mut files);
    let mut src = String::from("pub static FILES: &[(&str, &[u8])] = &[\n");
    for f in &files {
        let abs = root.join(f);
        writeln!(
            src,
            "    ({f:?}, include_bytes!({:?})),",
            abs.to_string_lossy()
        )
        .unwrap();
    }
    src.push_str("];\n");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("ports.rs");
    std::fs::write(out, src).unwrap();
}
