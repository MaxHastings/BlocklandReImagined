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

#[cfg(windows)]
mod windows;

pub const KEEP_SESSIONS: usize = 20;
pub const KEEP_CRASHES: usize = 10;
/// Lines of the session log copied into a crash report.
const LOG_TAIL: usize = 200;

/// Where this run's files live. Held for the life of the program.
#[derive(Debug)]
pub struct Capture {
    pub directory: PathBuf,
    pub session_log: PathBuf,
}

static STATE: OnceLock<State> = OnceLock::new();
struct State {
    directory: PathBuf,
    session_log: PathBuf,
    program: String,
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
    rotate(&directory, "session-", KEEP_SESSIONS.saturating_sub(1))?;
    rotate(&directory, "crash-", KEEP_CRASHES)?;
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
        program: program.into(),
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
        if let Some(state) = STATE.get() {
            let _ = state.write_panic(info);
        }
        previous(info);
    }));
    Ok(Capture {
        directory,
        session_log,
    })
}

/// Candidate log directories for a game: next to the executable first (so
/// the files sit with the game), then under the per-user state directory.
pub fn default_directories(state: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("logs")))
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
        writeln!(file, "{} {} crashed (Rust panic)", self.program, env!("CARGO_PKG_VERSION"))?;
        writeln!(file, "time: {stamp}")?;
        writeln!(file, "thread: {}", thread.name().unwrap_or("unnamed"))?;
        if let Some(location) = info.location() {
            writeln!(file, "at: {}:{}:{}", location.file(), location.line(), location.column())?;
        }
        writeln!(file, "message: {message}\n")?;
        writeln!(file, "backtrace:\n{}", std::backtrace::Backtrace::force_capture())?;
        self.append_log_tail(&mut file)?;
        file.sync_all()?;
        Ok(path)
    }
    fn append_log_tail(&self, out: &mut impl Write) -> io::Result<()> {
        let log = fs::read(&self.session_log).unwrap_or_default();
        let text = String::from_utf8_lossy(&log);
        let lines: Vec<_> = text.lines().collect();
        writeln!(out, "\nlast {LOG_TAIL} lines of {}:", self.session_log.display())?;
        for line in &lines[lines.len().saturating_sub(LOG_TAIL)..] {
            writeln!(out, "{line}")?;
        }
        Ok(())
    }
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
            File::create(dir.path().join(format!("session-2026010{}-0000{n:02}.log", n % 3))).unwrap();
        }
        File::create(dir.path().join("crash-20260101-000000.txt")).unwrap();
        rotate(dir.path(), "session-", 20).unwrap();
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.iter().filter(|n| n.starts_with("session-")).count(), 20);
        assert!(names.iter().any(|n| n.starts_with("crash-")), "other kinds untouched");
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
