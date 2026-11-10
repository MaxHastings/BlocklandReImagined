//! One authoring/distribution folder, with separately loaded content and host rules.
//! This layout never changes package identity, hashing, execution or trust.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE: &str = "addon.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub schema_version: u32,
    /// A direct child folder containing the content package's package.json.
    pub content: String,
    /// Optional direct child folder containing a server-only rules package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
}

impl Layout {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 4096, "addon.json exceeds 4096 bytes");
        let layout: Self = serde_json::from_slice(bytes).context("Reading addon.json")?;
        ensure!(layout.schema_version == 1, "Unsupported addon.json schema");
        for dir in std::iter::once(&layout.content).chain(layout.server.iter()) {
            ensure!(
                crate::path::problem(dir).is_none() && !dir.contains('/') && !dir.starts_with('.'),
                "Component must be a plain child folder: {dir}"
            );
        }
        ensure!(
            layout
                .server
                .as_ref()
                .is_none_or(|s| !s.eq_ignore_ascii_case(&layout.content)),
            "Content and server components must differ"
        );
        Ok(layout)
    }

    pub fn load(root: &Path) -> Result<Self> {
        let path = crate::path::inside(root, FILE).map_err(anyhow::Error::msg)?;
        ensure!(
            std::fs::metadata(&path)?.len() <= 4096,
            "addon.json exceeds 4096 bytes"
        );
        let layout = Self::parse(&std::fs::read(path)?)?;
        let content = layout.manifest(root, &layout.content)?;
        let server = layout
            .server
            .as_ref()
            .map(|s| layout.manifest(root, s))
            .transpose()?;
        layout.validate_manifests(&content, server.as_deref())?;
        Ok(layout)
    }

    fn manifest(&self, root: &Path, dir: &str) -> Result<Vec<u8>> {
        let path = crate::path::inside(root, &format!("{dir}/package.json"))
            .map_err(anyhow::Error::msg)?;
        ensure!(
            std::fs::metadata(&path)?.len() <= crate::library::MAX_MANIFEST_BYTES,
            "Component manifest exceeds size limit"
        );
        Ok(std::fs::read(path)?)
    }

    /// Check relationships without executing or broadening either component.
    pub fn validate_manifests(&self, content: &[u8], server: Option<&[u8]>) -> Result<()> {
        use crate::library::PackageInfo;
        let content: PackageInfo = serde_json::from_slice(content)?;
        ensure!(
            crate::id::namespace_problem(&content.id).is_none()
                && !crate::id::is_reserved(&content.id),
            "Invalid content package id"
        );
        ensure!(
            content
                .side()
                .is_some_and(|s| s != crate::packages::Side::Server),
            "Content component must be client or shared data"
        );
        ensure!(
            server.is_some() == self.server.is_some(),
            "Missing server component manifest"
        );
        if let Some(server) = server {
            let server: PackageInfo = serde_json::from_slice(server)?;
            ensure!(
                crate::id::namespace_problem(&server.id).is_none()
                    && !crate::id::is_reserved(&server.id)
                    && server.id != content.id,
                "Invalid server package id"
            );
            ensure!(
                server.side() == Some(crate::packages::Side::Server),
                "Rules component must be server-only"
            );
            ensure!(
                content.companions.contains(&server.id),
                "Content must name its server component in companions"
            );
            ensure!(
                server.dependencies.contains_key(&content.id),
                "Server component must depend on its content package"
            );
        }
        Ok(())
    }

    pub fn components<'a>(&'a self, out: &Path) -> Vec<(&'a str, PathBuf)> {
        let mut components = vec![(self.content.as_str(), out.to_path_buf())];
        if let Some(server) = &self.server {
            let mut path = out.as_os_str().to_owned();
            path.push("-rules");
            components.push((server.as_str(), PathBuf::from(path)));
        }
        components
    }
}

/// Copy a co-located authoring folder into the normal transactional install stage.
/// Links and Windows-special paths are refused before any package is activated.
pub fn stage(root: &Path, out: &Path) -> Result<()> {
    let layout = Layout::load(root)?;
    let mut count = 0;
    let mut total = 0_u64;
    for (component, target) in layout.components(out) {
        let source = crate::path::inside(root, component).map_err(anyhow::Error::msg)?;
        let mut pending = vec![(source, String::new())];
        while let Some((dir, prefix)) = pending.pop() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Non-UTF-8 component path"))?;
                count += 1;
                ensure!(count <= 20_000, "Add-On exceeds install entry budget");
                let relative = format!("{prefix}{name}");
                let destination =
                    crate::path::inside(&target, &relative).map_err(anyhow::Error::msg)?;
                let kind = entry.file_type()?;
                ensure!(
                    !kind.is_symlink(),
                    "Add-On components may not contain links"
                );
                if kind.is_dir() {
                    pending.push((entry.path(), format!("{relative}/")));
                } else {
                    ensure!(kind.is_file(), "Component entry is not a regular file");
                    total = total
                        .checked_add(entry.metadata()?.len())
                        .context("Component size overflow")?;
                    ensure!(
                        count <= 20_000 && total <= 1 << 30,
                        "Add-On exceeds install budget"
                    );
                    std::fs::create_dir_all(
                        destination.parent().context("Missing destination parent")?,
                    )?;
                    std::fs::copy(entry.path(), destination)?;
                }
            }
        }
    }
    Ok(())
}
