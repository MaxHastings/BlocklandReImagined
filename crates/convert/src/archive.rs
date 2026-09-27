//! RAR fallback is offline tooling only. Extracts to a bounded pipe, never disk.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    path::Path,
    process::{Command, Stdio},
};
const MAX: u64 = 16 * 1024 * 1024;

pub fn is_rar(path: &Path) -> Result<bool> {
    let mut signature = [0_u8; 8];
    let n = std::fs::File::open(path)?.read(&mut signature)?;
    Ok(n >= 7 && signature[..6] == *b"Rar!\x1a\x07")
}
fn run(args: &[&OsStr]) -> Result<Vec<u8>> {
    let executable = std::env::var_os("BRI_7Z").unwrap_or_else(|| OsString::from("7z"));
    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("RAR requires 7z on PATH or BRI_7Z pointing to its executable")?;
    let mut data = Vec::new();
    let read = child
        .stdout
        .take()
        .context("No archive output pipe")?
        .take(MAX + 1)
        .read_to_end(&mut data);
    if read.is_err() || data.len() as u64 > MAX {
        let _ = child.kill();
        let _ = child.wait();
        read?;
        anyhow::bail!("Archive output exceeds {MAX} bytes");
    }
    let status = child.wait()?;
    ensure!(
        status.success(),
        "7z failed with {status}; no output accepted"
    );
    Ok(data)
}
pub fn rar_assets(path: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let listing = run(&[
        OsStr::new("l"),
        OsStr::new("-slt"),
        OsStr::new("-ba"),
        OsStr::new("-sccUTF-8"),
        OsStr::new("--"),
        path.as_os_str(),
    ])?;
    let text = std::str::from_utf8(&listing)?.replace("\r\n", "\n");
    let mut entries = Vec::new();
    for block in text.split("\n\n") {
        let values: std::collections::BTreeMap<_, _> = block
            .lines()
            .filter_map(|line| line.split_once(" = "))
            .collect();
        let Some(name) = values.get("Path") else {
            continue;
        };
        let lower = name.to_lowercase();
        if ![".ter", ".blb", ".dts", ".dsq", ".dif", ".mis"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        {
            continue;
        }
        ensure!(
            values.get("Folder") == Some(&"-") && values.get("Encrypted") == Some(&"-"),
            "Unsupported RAR member {name}"
        );
        let size: u64 = values
            .get("Size")
            .context("RAR member lacks size")?
            .parse()?;
        ensure!(size <= MAX, "RAR asset {name} exceeds size limit");
        let data = run(&[
            OsStr::new("e"),
            OsStr::new("-so"),
            OsStr::new("-y"),
            OsStr::new("-spd"),
            OsStr::new("--"),
            path.as_os_str(),
            OsStr::new(name),
        ])?;
        ensure!(data.len() as u64 == size, "RAR size mismatch for {name}");
        entries.push((name.to_string(), data));
    }
    Ok(entries)
}
