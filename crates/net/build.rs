//! Counts `protocol-changes/`: each wire change is one file there, and the
//! protocol version is the frozen base plus their number, so two branches'
//! changes add up instead of fighting over one number.
use std::{fmt::Write, path::Path};

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("protocol-changes");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("crates/net/protocol-changes exists")
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".md") && name != "README.md")
        .collect();
    names.sort();
    let mut out = String::from("pub const PROTOCOL_CHANGES: &[&str] = &[\n");
    for name in &names {
        println!("cargo:rerun-if-changed={}", dir.join(name).display());
        writeln!(out, "    {name:?},").unwrap();
    }
    out.push_str("];\n");
    let path = Path::new(&std::env::var("OUT_DIR").unwrap()).join("protocol_changes.rs");
    std::fs::write(path, out).unwrap();
}
