//! Helpers for tests that opt into generated content. Callers supply their
//! content root (including any `BRI_CONTENT` override); package roles, rather
//! than importer revision numbers, choose the directory.
use std::path::{Path, PathBuf};

pub fn pack_dir(root: &Path, role: &str) -> PathBuf {
    crate::packages::PackageSet::load_root(root)
        .and_then(|set| set.role_dir(root, role))
        .unwrap_or_else(|e| panic!("content role {role} under {}: {e:#}", root.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packages::{PackageEntry, PackageSet, Side};

    #[test]
    fn caller_root_and_declared_role_choose_the_pack_even_after_a_rename() {
        let root = std::env::temp_dir().join(format!("bri-role-fixture-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for name in ["first", "second"] {
            let content = root.join(name);
            let renamed = content.join(format!("{name}-authored-ui"));
            std::fs::create_dir_all(&renamed).unwrap();
            let set = PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: "fixture_ui".into(),
                    version: "1.0.0".into(),
                    side: Side::Client,
                    dir: format!("{name}-authored-ui"),
                    role: Some("ui_pack".into()),
                }],
            };
            std::fs::write(
                content.join("packages.json"),
                serde_json::to_vec(&set).unwrap(),
            )
            .unwrap();
            assert_eq!(
                pack_dir(&content, "ui_pack"),
                renamed.canonicalize().unwrap()
            );
            assert!(std::panic::catch_unwind(|| pack_dir(&content, "absent_role")).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
