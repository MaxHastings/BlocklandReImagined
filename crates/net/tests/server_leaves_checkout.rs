//! `bri-server` over a source checkout's content leaves it as it is unless
//! BRI_INSTALL_DEFAULT_ADD_ONS=1 asks: the push gate's smoke serves the
//! shared main checkout's content, which every worktree's tests read.
use anyhow::Result;
use std::path::Path;

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[test]
fn the_server_installs_a_checkouts_default_add_ons_only_when_asked() -> Result<()> {
    let checkout = tempfile::tempdir()?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    copy_dir(&repo.join("packages"), &checkout.path().join("packages"))?;
    let content = checkout.path().join("content");
    std::fs::create_dir_all(&content)?;
    let serve = |install: bool| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_bri-server"));
        command.env_remove("BRI_INSTALL_DEFAULT_ADD_ONS");
        if install {
            command.env("BRI_INSTALL_DEFAULT_ADD_ONS", "1");
        }
        // No base packs here, so the server stops at loading them.
        command
            .arg(&content)
            .arg("slate")
            .arg(checkout.path().join("state"))
            .arg("127.0.0.1:0")
            .arg("1")
            .output()
    };
    let out = serve(false)?;
    assert!(!out.status.success(), "served without base packs");
    assert!(
        std::fs::read_dir(&content)?.next().is_none(),
        "bri-server wrote into the checkout's content: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    serve(true)?;
    assert!(
        content.join("addons").is_dir(),
        "BRI_INSTALL_DEFAULT_ADD_ONS=1 installed nothing"
    );
    Ok(())
}
