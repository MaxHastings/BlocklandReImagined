//! Crash capture. One call at startup gives every run a session log and turns
//! any crash into files a player can send as they are:
//!
//! - `logs/session-<time>.log`: everything the game wrote to stderr (every
//!   console line, warning and error), still echoed to a terminal if any.
//! - `logs/crash-<time>.txt`: a Rust panic's message, location, thread and
//!   backtrace, followed by the end of the session log.
//! - `logs/crash-<time>.dmp` (Windows): a minidump of a native crash (access
//!   violation, stack overflow in foreign code, GPU driver fault) that
//!   WinDbg or Visual Studio opens against the shipped symbols.
//!
//! Old files rotate out: the newest [`KEEP_SESSIONS`] session logs and
//! [`KEEP_CRASHES`] crash reports are kept.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

mod dialog;
#[cfg(windows)]
mod windows;
pub use dialog::{alert, open, summarize};
#[cfg(windows)]
pub use windows::STALL_DUMP_VAR;
/// The dialog title players see.
pub const PRODUCT: &str = "Blockland ReImagined";

pub const KEEP_SESSIONS: usize = 20;
pub const KEEP_CRASHES: usize = 10;
/// The longest a native crash's capture waits on other threads: for its
/// minidump, then for the session log. A crashing process still running past
/// it is hung.
#[cfg(windows)]
pub const NATIVE_CAPTURE_WAIT: std::time::Duration =
    windows::DUMP_WAIT.saturating_add(windows::DRAIN_WAIT);
/// Lines of the session log copied into a crash report.
const LOG_TAIL: usize = 200;

/// Where this run's files live. Held for the life of the program.
#[derive(Debug)]
pub struct Capture {
    pub directory: PathBuf,
    pub session_log: PathBuf,
    /// A crash report from the previous run nobody has been shown yet.
    pub previous_crash: Option<PathBuf>,
}

static STATE: OnceLock<State> = OnceLock::new();
struct State {
    directory: PathBuf,
    session_log: PathBuf,
    previous_crash: Option<PathBuf>,
    program: String,
    /// Show a dialog when the main thread panics (a desktop game, not a
    /// server or test).
    dialogs: std::sync::atomic::AtomicBool,
    /// Serializes crash reports from panics on several threads at once.
    writing: Mutex<()>,
}

/// Install crash capture in the first writable directory of `candidates`
/// (normally `<game>/logs`, then a per-user fallback for read-only installs).
/// Idempotent: later calls return the first installation's paths.
pub fn install(program: &str, candidates: &[PathBuf]) -> io::Result<Capture> {
    if let Some(state) = STATE.get() {
        return Ok(state.capture());
    }
    let directory = candidates
        .iter()
        .find(|dir| writable(dir))
        .cloned()
        .ok_or_else(|| io::Error::other("No writable log directory"))?;
    let previous_crash = unseen_crash(&directory);
    rotate(&directory, "session-", KEEP_SESSIONS.saturating_sub(1))?;
    rotate(&directory, "crash-", KEEP_CRASHES)?;
    rotate(&directory, "seen-", KEEP_CRASHES)?;
    let stamp = timestamp(SystemTime::now());
    let session_log = unique(&directory, &format!("session-{stamp}"), "log");
    let mut log = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&session_log)?;
    writeln!(
        log,
        "{program} {} ({}), session {stamp}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    )?;
    let state = State {
        directory: directory.clone(),
        session_log: session_log.clone(),
        previous_crash: previous_crash.clone(),
        program: program.into(),
        dialogs: std::sync::atomic::AtomicBool::new(false),
        writing: Mutex::new(()),
    };
    if STATE.set(state).is_err() {
        return Ok(STATE.get().expect("installed").capture());
    }
    #[cfg(windows)]
    windows::install(log)?;
    #[cfg(not(windows))]
    drop(log);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(state) = STATE.get()
            && let Ok(report) = state.write_panic(info)
            && std::thread::current().name() == Some("main")
            && state.dialogs.load(std::sync::atomic::Ordering::Relaxed)
        {
            // The main thread is going down with the game: say so now.
            acknowledge(&report);
            alert(
                &format!("{PRODUCT} stopped unexpectedly"),
                &format!(
                    "{PRODUCT} ran into a problem and has to close.\n\nA crash report was saved as {}.",
                    report.file_name().unwrap_or_default().to_string_lossy()
                ),
                Some(&state.directory),
            );
        }
        previous(info);
        // The process may end right after this: deliver the message now.
        #[cfg(windows)]
        windows::flush();
    }));
    Ok(Capture {
        directory,
        session_log,
        previous_crash,
    })
}

/// Deliver everything written to stderr so far to the session log and the
/// terminal, and stop teeing. Call it as the program's last step; the panic
/// hook calls it when the main thread panics.
pub fn finish() {
    #[cfg(windows)]
    windows::finish();
}

/// A windowed (GUI subsystem) build has no console. Started from a
/// terminal, write to that terminal; double-clicked, do nothing.
pub fn attach_parent_console() {
    #[cfg(windows)]
    windows::attach_parent_console();
}

/// Desktop games tell the player about a crash with a dialog; servers and
/// tools leave it to the report.
pub fn enable_dialogs() {
    if let Some(state) = STATE.get() {
        state
            .dialogs
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Mark a crash report as shown, so the next launch does not show it again.
pub fn acknowledge(report: &Path) {
    if let (Some(dir), Some(stem)) = (report.parent(), report.file_stem()) {
        let _ = File::create(dir.join(format!("seen-{}", stem.to_string_lossy())));
    }
}

/// What to tell a player about a crash report from their last run: the
/// report and, after a native crash, its minidump, by name.
pub fn previous_crash_message(report: &Path) -> String {
    let mut files = vec![report.to_path_buf()];
    let dump = report.with_extension("dmp");
    if dump.is_file() {
        files.push(dump);
    }
    let names: Vec<String> = files
        .iter()
        .map(|f| {
            format!(
                "    {}",
                f.file_name().unwrap_or_default().to_string_lossy()
            )
        })
        .collect();
    format!(
        "{PRODUCT} closed unexpectedly last time it ran. It saved what happened in:\n\n{}\n\nPlease send {} to whoever gave you the game; it helps them fix the problem. Nothing is sent automatically.",
        names.join("\n"),
        if files.len() == 1 {
            "that file"
        } else {
            "both files"
        },
    )
}

/// The newest crash report written during the latest session that has not
/// been shown to the player. Session and report names embed sortable UTC
/// times, so "during" is a string comparison.
fn unseen_crash(dir: &Path) -> Option<PathBuf> {
    let names: Vec<String> = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let started = names
        .iter()
        .filter_map(|n| n.strip_prefix("session-"))
        .map(|n| n.trim_end_matches(".log").to_string())
        .max()?;
    names
        .iter()
        .filter_map(|n| n.strip_prefix("crash-").map(|stamp| (n, stamp)))
        .filter(|(n, stamp)| {
            n.ends_with(".txt")
                && stamp.trim_end_matches(".txt") >= started.as_str()
                && !names.contains(&format!("seen-{}", n.trim_end_matches(".txt")))
        })
        .map(|(n, _)| n)
        .max()
        .map(|n| dir.join(n))
}

/// Whether the executable runs from `<name>.app/Contents/MacOS`.
#[cfg(target_os = "macos")]
fn inside_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok())
        .and_then(|exe| exe.parent().map(|dir| dir.ends_with("Contents/MacOS")))
        .unwrap_or(false)
}
#[cfg(not(target_os = "macos"))]
fn inside_app_bundle() -> bool {
    false
}
/// Candidate log directories for a game: next to the executable first (so
/// the files sit with the game), then under the per-user state directory.
pub fn default_directories(state: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("logs")))
        // A macOS app bundle must stay unchanged for its signature.
        .filter(|_| !inside_app_bundle())
    {
        dirs.push(dir);
    }
    dirs.push(state.join("logs"));
    dirs
}

impl State {
    fn capture(&self) -> Capture {
        Capture {
            directory: self.directory.clone(),
            session_log: self.session_log.clone(),
            previous_crash: self.previous_crash.clone(),
        }
    }
    fn write_panic(&self, info: &std::panic::PanicHookInfo<'_>) -> io::Result<PathBuf> {
        let _guard = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        let stamp = timestamp(SystemTime::now());
        let path = unique(&self.directory, &format!("crash-{stamp}"), "txt");
        let mut file = File::create(&path)?;
        let thread = std::thread::current();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(non-text panic payload)".into());
        writeln!(
            file,
            "{} {} crashed (Rust panic)",
            self.program,
            env!("CARGO_PKG_VERSION")
        )?;
        if let Ok(executable) = std::env::current_exe() {
            writeln!(file, "executable: {}", executable.display())?;
        }
        writeln!(file, "time: {stamp}")?;
        writeln!(file, "thread: {}", thread.name().unwrap_or("unnamed"))?;
        if let Some(location) = info.location() {
            writeln!(
                file,
                "at: {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )?;
        }
        writeln!(file, "message: {message}\n")?;
        let backtrace = std::backtrace::Backtrace::force_capture();
        writeln!(file, "backtrace status: {:?}", backtrace.status())?;
        writeln!(
            file,
            "symbolication: use native module offsets with matching build symbols; native IPs are return addresses"
        )?;
        write_native_backtrace(&mut file)?;
        writeln!(file, "backtrace:\n{backtrace}")?;
        self.append_log_tail(&mut file)?;
        file.sync_all()?;
        Ok(path)
    }
    fn append_log_tail(&self, out: &mut impl Write) -> io::Result<()> {
        let log = fs::read(&self.session_log).unwrap_or_default();
        let text = String::from_utf8_lossy(&log);
        let lines: Vec<_> = text.lines().collect();
        writeln!(
            out,
            "\nlast {LOG_TAIL} lines of {}:",
            self.session_log.display()
        )?;
        for line in &lines[lines.len().saturating_sub(LOG_TAIL)..] {
            writeln!(out, "{line}")?;
        }
        Ok(())
    }
}

#[cfg(windows)]
fn write_native_backtrace(out: &mut impl Write) -> io::Result<()> {
    windows::write_native_backtrace(out)
}

/// Independent of Rust's symbol resolver: retain addresses even when the
/// executable was stripped. `dladdr` identifies the loaded image and its slide.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn write_native_backtrace(out: &mut impl Write) -> io::Result<()> {
    use std::ffi::{CStr, c_char, c_int, c_void};
    #[repr(C)]
    struct DlInfo {
        filename: *const c_char,
        base: *mut c_void,
        symbol: *const c_char,
        symbol_address: *mut c_void,
    }
    #[cfg_attr(target_os = "linux", link(name = "dl"))]
    unsafe extern "C" {
        fn backtrace(buffer: *mut *mut c_void, size: c_int) -> c_int;
        fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
    }
    let mut frames = [std::ptr::null_mut(); 128];
    // SAFETY: the fixed buffer has space for exactly the requested frames.
    let count = unsafe { backtrace(frames.as_mut_ptr(), frames.len() as c_int) };
    writeln!(out, "native frames (up to 128):")?;
    for (index, &frame) in frames.iter().take(count.max(0) as usize).enumerate() {
        let ip = frame as usize;
        let mut info = DlInfo {
            filename: std::ptr::null(),
            base: std::ptr::null_mut(),
            symbol: std::ptr::null(),
            symbol_address: std::ptr::null_mut(),
        };
        // SAFETY: address is only queried, and the output is a valid Dl_info.
        if unsafe { dladdr(frame, &mut info) } != 0 && !info.filename.is_null() {
            // SAFETY: successful dladdr supplies a NUL-terminated image name.
            let module = unsafe { CStr::from_ptr(info.filename) }.to_string_lossy();
            let base = info.base as usize;
            if let Some(offset) = ip.checked_sub(base) {
                writeln!(
                    out,
                    "  {index}: ip=0x{ip:016x} module={module:?} base=0x{base:016x} offset=0x{offset:x}"
                )?;
                continue;
            }
        }
        writeln!(out, "  {index}: ip=0x{ip:016x} module=<unknown>")?;
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn write_native_backtrace(out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "native frames: unsupported on this platform")
}

fn writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".write-probe-{}", std::process::id()));
    let ok = File::create(&probe).is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

/// Keep the newest `keep` files whose names start with `prefix`.
fn rotate(dir: &Path, prefix: &str, keep: usize) -> io::Result<()> {
    let mut files: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .collect();
    // Names embed a sortable UTC timestamp, so name order is age order.
    files.sort();
    let excess = files.len().saturating_sub(keep);
    for old in &files[..excess] {
        let _ = fs::remove_file(old);
    }
    Ok(())
}

/// `<stem>.<ext>`, or `<stem>-2.<ext>` and so on if that name is taken.
fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}.{ext}")))
        .find(|p| !p.exists())
        .expect("unbounded")
}

/// Sortable UTC `YYYYMMDD-HHMMSS`.
pub fn timestamp(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn timestamps_are_sortable_utc() {
        assert_eq!(timestamp(UNIX_EPOCH), "19700101-000000");
        let t = UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        assert_eq!(timestamp(t), "20260921-141320");
        let leap = UNIX_EPOCH + Duration::from_secs(951_782_400); // 2000-02-29
        assert_eq!(timestamp(leap), "20000229-000000");
    }

    #[test]
    fn rotation_keeps_the_newest_files_of_each_kind() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..25 {
            File::create(
                dir.path()
                    .join(format!("session-2026010{}-0000{n:02}.log", n % 3)),
            )
            .unwrap();
        }
        File::create(dir.path().join("crash-20260101-000000.txt")).unwrap();
        rotate(dir.path(), "session-", 20).unwrap();
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names.iter().filter(|n| n.starts_with("session-")).count(),
            20
        );
        assert!(
            names.iter().any(|n| n.starts_with("crash-")),
            "other kinds untouched"
        );
    }

    #[test]
    fn only_an_unseen_crash_from_the_last_session_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let touch = |name: &str| File::create(dir.path().join(name)).unwrap();
        touch("session-20260101-000000.log");
        touch("crash-20251231-235959.txt"); // before the last session
        assert_eq!(unseen_crash(dir.path()), None);
        touch("crash-20260101-000500.txt");
        touch("crash-20260101-000500.dmp");
        assert_eq!(
            unseen_crash(dir.path()),
            Some(dir.path().join("crash-20260101-000500.txt"))
        );
        acknowledge(&dir.path().join("crash-20260101-000500.txt"));
        assert_eq!(unseen_crash(dir.path()), None, "shown once");
        touch("session-20260102-000000.log");
        touch("crash-20260101-000600.txt"); // an older session's crash
        assert_eq!(unseen_crash(dir.path()), None);
    }

    #[test]
    fn the_crash_message_names_the_report_and_its_dump() {
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("crash-20260101-000500.txt");
        File::create(&report).unwrap();
        let text = previous_crash_message(&report);
        assert!(text.contains("crash-20260101-000500.txt") && text.contains("that file"));
        assert!(!text.contains(".dmp"));
        File::create(dir.path().join("crash-20260101-000500.dmp")).unwrap();
        let text = previous_crash_message(&report);
        assert!(text.contains("crash-20260101-000500.dmp") && text.contains("both files"));
    }

    #[test]
    fn unique_names_never_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let a = unique(dir.path(), "crash-x", "txt");
        File::create(&a).unwrap();
        let b = unique(dir.path(), "crash-x", "txt");
        assert_ne!(a, b);
        assert!(b.ends_with("crash-x-2.txt"));
    }
}
