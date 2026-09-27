//! Crash capture end to end, in child processes so the crash is real: the
//! test binary re-runs itself with an environment variable naming the case.
use std::{fs, path::Path, process::Command};

const CASE: &str = "BRI_CRASH_CASE";
const DIR: &str = "BRI_CRASH_DIR";

fn child(case: &str, dir: &Path) -> std::process::Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(CASE, case)
        .env(DIR, dir)
        .output()
        .unwrap()
}

fn files(dir: &Path, prefix: &str, ext: &str) -> Vec<std::path::PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with(prefix) && name.ends_with(ext)
        })
        .collect()
}

/// The body the child processes run. A no-op in the normal test run.
#[test]
fn child_entry() {
    let Ok(case) = std::env::var(CASE) else {
        return;
    };
    let dir = std::path::PathBuf::from(std::env::var(DIR).unwrap());
    bri_crash::install("capture-test", &[dir]).unwrap();
    eprintln!("line before the crash");
    match case.as_str() {
        "panic" => panic!("deliberate test panic"),
        #[cfg(windows)]
        "native" => unsafe {
            // A real access violation, not a Rust panic.
            std::ptr::write_volatile(std::ptr::null_mut::<u32>(), 1);
        },
        _ => {}
    }
}

#[test]
fn a_panic_leaves_a_report_with_backtrace_and_the_session_log() {
    let dir = tempfile::tempdir().unwrap();
    let out = child("panic", dir.path());
    assert!(!out.status.success());
    let reports = files(dir.path(), "crash-", ".txt");
    assert_eq!(reports.len(), 1, "{}", String::from_utf8_lossy(&out.stderr));
    let report = fs::read_to_string(&reports[0]).unwrap();
    assert!(report.contains("message: deliberate test panic"), "{report}");
    assert!(report.contains("backtrace:"));
    assert!(report.contains("capture.rs"), "panic location recorded");
    let sessions = files(dir.path(), "session-", ".log");
    assert_eq!(sessions.len(), 1);
    let log = fs::read_to_string(&sessions[0]).unwrap();
    assert!(log.contains("capture-test"), "session header");
    // The tee still echoes to the original stderr.
    assert!(String::from_utf8_lossy(&out.stderr).contains("line before the crash"));
}

#[cfg(windows)]
#[test]
fn a_native_crash_leaves_a_minidump_and_a_report() {
    let dir = tempfile::tempdir().unwrap();
    let out = child("native", dir.path());
    assert!(!out.status.success());
    let dumps = files(dir.path(), "crash-", ".dmp");
    assert_eq!(dumps.len(), 1, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(fs::metadata(&dumps[0]).unwrap().len() > 1024);
    let report = fs::read_to_string(dumps[0].with_extension("txt")).unwrap();
    assert!(report.contains("exception: 0xC0000005"), "{report}");
    assert!(report.contains("line before the crash"), "session tail included");
}
