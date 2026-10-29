//! Interpreter selection for the release bundle's Python tools.
use std::process::Command;

/// Match the supported toolchain: a successful `--version` alone can select
/// an older Python that lacks `hashlib.file_digest`, used by the bundler.
pub fn supported() -> &'static str {
    ["python3", "python"]
        .into_iter()
        .find(|python| {
            Command::new(python)
                .args(["-c", "import sys; sys.exit(sys.version_info < (3, 11))"])
                .output()
                .is_ok_and(|out| out.status.success())
        })
        .expect("Python 3.11 or newer on PATH (python3 or python) runs tools/addon_bundle.py")
}
