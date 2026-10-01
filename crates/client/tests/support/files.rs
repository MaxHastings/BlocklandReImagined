//! Writing fixture packs: files with their SHA-256, and PNGs drawn in code.
#![allow(dead_code)]

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Write `bytes` (making the folders) and return their SHA-256.
pub fn write(path: &Path, bytes: &[u8]) -> Result<String> {
    std::fs::create_dir_all(path.parent().context("fixture file has no folder")?)?;
    std::fs::write(path, bytes)?;
    Ok(sha256(bytes))
}

pub fn write_json(path: &Path, value: &serde_json::Value) -> Result<String> {
    write(path, &serde_json::to_vec_pretty(value)?)
}

/// A PNG of `width` x `height` whose pixel (x, y) is `pixel(x, y)`.
pub fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Result<Vec<u8>> {
    let image = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba(pixel(x, y)));
    let mut bytes = Vec::new();
    image.write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )?;
    Ok(bytes)
}

/// A fresh folder for one fixture under this target's test scratch space,
/// removed when the handle drops.
pub fn scratch(label: &str) -> Result<tempfile::TempDir> {
    let parent = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&parent)?;
    Ok(tempfile::Builder::new().prefix(label).tempdir_in(parent)?)
}

/// The repository root (where the generated `content/` packs live).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Where a variant writes its regenerated evidence: the content variant
/// keeps its historical folder under the repository, the synthetic one
/// writes beside the other test scratch files.
pub fn evidence_dir(content: bool, repo_relative: &str, synthetic_label: &str) -> PathBuf {
    if content {
        repo_root().join(repo_relative)
    } else {
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(synthetic_label)
    }
}
