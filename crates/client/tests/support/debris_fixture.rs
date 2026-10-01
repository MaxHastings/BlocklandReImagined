//! The weapon debris pack (the stock gun's ejected casing): the generated
//! v20 pack, or `bri_client::testing::weapon_debris`' made-up one, which
//! `WeaponDebrisAssets::load` reads through the same checks.
#![allow(dead_code)]

use super::files::{repo_root, scratch};
use anyhow::Result;
use std::path::PathBuf;

pub struct DebrisFixture {
    /// The folder holding `pack.json`.
    pub dir: PathBuf,
    pub content: bool,
    /// Where regenerated evidence goes.
    pub out: PathBuf,
    _scratch: Option<tempfile::TempDir>,
}

impl DebrisFixture {
    pub fn content() -> Result<Self> {
        let root = repo_root();
        Ok(Self {
            dir: root.join("content/weapon-debris-pack-001"),
            content: true,
            out: root.join("artifacts/native-weapon-debris"),
            _scratch: None,
        })
    }

    pub fn synthetic() -> Result<Self> {
        let dir = scratch("weapon-debris-")?;
        bri_client::testing::weapon_debris::write(dir.path())?;
        Ok(Self {
            dir: dir.path().to_path_buf(),
            content: false,
            out: PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("native-weapon-debris-synthetic"),
            _scratch: Some(dir),
        })
    }
}
