//! The avatar package: the generated v20 one, or the made-up Blockhead-like
//! one `bri_client::testing::avatar` writes (same node names and sequence
//! aliases, invented geometry and motion).
#![allow(dead_code)]

use super::files::repo_root;
use anyhow::Result;
use bri_client::avatar::AvatarAssets;
use std::path::PathBuf;

pub struct AvatarFixture {
    pub assets: AvatarAssets,
    pub content: bool,
    /// Where regenerated evidence goes: `artifacts/<name>` for the content
    /// variant, a scratch folder for the synthetic one.
    out: PathBuf,
}

impl AvatarFixture {
    pub fn content() -> Result<Self> {
        let content = std::env::var_os("BRI_CONTENT")
            .map_or_else(|| repo_root().join("content"), PathBuf::from);
        Ok(Self {
            assets: AvatarAssets::load(&content.join("avatar-pack-002"))?,
            content: true,
            out: repo_root().join("artifacts"),
        })
    }

    pub fn synthetic() -> Result<Self> {
        Ok(Self {
            assets: bri_client::testing::avatar::assets()?,
            content: false,
            out: PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("synthetic-avatar"),
        })
    }

    /// The evidence folder `name`, made.
    pub fn out(&self, name: &str) -> Result<PathBuf> {
        let out = self.out.join(name);
        std::fs::create_dir_all(&out)?;
        Ok(out)
    }
}
