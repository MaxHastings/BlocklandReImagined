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

/// A native-crash child, killed if it is still running once its capture
/// can no longer be waiting for anything ([`bri_crash::NATIVE_CAPTURE_WAIT`]):
/// a hung capture fails the test instead of hanging it. Returns stderr, or
/// None when it hung.
#[cfg(windows)]
fn crashing_child(case: &str, dir: &Path) -> Option<String> {
    use std::io::Read;
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(CASE, case)
        .env(DIR, dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // Read stderr as it comes, so a full pipe never stalls the child.
    let mut pipe = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = pipe.read_to_string(&mut text);
        text
    });
    let deadline = std::time::Instant::now() + bri_crash::NATIVE_CAPTURE_WAIT;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(!status.success());
            return Some(reader.join().unwrap());
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::yield_now();
    }
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
    bri_crash::install("capture-test", std::slice::from_ref(&dir)).unwrap();
    eprintln!("line before the crash");
    match case.as_str() {
        "panic" => panic!("deliberate test panic"),
        #[cfg(windows)]
        "native" => crash(),
        #[cfg(windows)]
        "loader_busy" => {
            loader::keep_busy(&dir);
            crash();
        }
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
    assert!(
        report.contains("message: deliberate test panic"),
        "{report}"
    );
    assert!(report.contains("executable: "), "{report}");
    assert!(report.contains("backtrace status: Captured"), "{report}");
    assert!(
        report.contains("symbolication: use native module offsets with matching build symbols"),
        "{report}"
    );
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    assert_native_frames(&report);
    assert!(report.contains("backtrace:"));
    assert!(report.contains("capture.rs"), "panic location recorded");
    let sessions = files(dir.path(), "session-", ".log");
    assert_eq!(sessions.len(), 1);
    let log = fs::read_to_string(&sessions[0]).unwrap();
    assert!(log.contains("capture-test"), "session header");
    // The tee still echoes to the original stderr.
    assert!(String::from_utf8_lossy(&out.stderr).contains("line before the crash"));
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn assert_native_frames(report: &str) {
    let native = report
        .split_once("native frames (up to 128):\n")
        .expect("native addresses retained")
        .1
        .split_once("backtrace:\n")
        .expect("symbolic backtrace retained")
        .0;
    let frames: Vec<_> = native
        .lines()
        .filter(|line| line.contains("ip=0x"))
        .collect();
    assert!(frames.len() >= 3, "caller chain retained: {report}");
    let mut modules = 0;
    for frame in frames {
        let hex = |label: &str| {
            let token = frame
                .split_once(label)
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap();
            u64::from_str_radix(token.trim_start_matches("0x"), 16).unwrap()
        };
        let ip = hex("ip=");
        assert_ne!(ip, 0, "{frame}");
        if frame.contains("base=") {
            modules += 1;
            let base = hex("base=");
            let offset = hex("offset=");
            assert_ne!(base, 0, "{frame}");
            assert_eq!(base + offset, ip, "ASLR-independent offset: {frame}");
        }
    }
    assert!(modules >= 3, "module mappings retained: {report}");
    let executable = std::env::current_exe().unwrap();
    let filename = executable.file_name().unwrap().to_string_lossy();
    assert!(
        native.contains(filename.as_ref()),
        "test executable mapped: {report}"
    );
}

/// A real access violation, not a Rust panic, after naming the thread it
/// happens on.
#[cfg(windows)]
fn crash() {
    // SAFETY: no preconditions.
    let thread = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    eprintln!("{CRASHING_THREAD}{thread}");
    // SAFETY: deliberately writes through null.
    unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u32>(), 1) };
}
#[cfg(windows)]
const CRASHING_THREAD: &str = "crashing thread: ";

/// The child's crash, its minidump and its report: the dump holds the
/// exception, raised on the crashing thread, and that thread.
#[cfg(windows)]
fn assert_captured(dir: &Path, stderr: &str) {
    let thread: u32 = stderr
        .lines()
        .find_map(|l| l.strip_prefix(CRASHING_THREAD))
        .expect("the child named its crashing thread")
        .trim()
        .parse()
        .unwrap();
    let dumps = files(dir, "crash-", ".dmp");
    assert_eq!(dumps.len(), 1, "{stderr}");
    let report = fs::read_to_string(dumps[0].with_extension("txt")).unwrap();
    assert!(report.contains("exception: 0xC0000005"), "{report}");
    assert!(report.contains("minidump: "), "{report}");
    assert!(
        report.contains("line before the crash"),
        "session tail included"
    );
    let dump = minidump::Dump::read(&dumps[0]);
    assert_eq!(
        dump.exception(),
        Some((thread, ACCESS_VIOLATION)),
        "the dump holds the access violation on the crashing thread"
    );
    assert!(
        dump.threads().contains(&thread),
        "the crashing thread dumped"
    );
}
/// `EXCEPTION_ACCESS_VIOLATION`.
#[cfg(windows)]
const ACCESS_VIOLATION: u32 = 0xC000_0005;

#[cfg(windows)]
#[test]
fn a_native_crash_leaves_a_minidump_and_a_report() {
    let dir = tempfile::tempdir().unwrap();
    let stderr = crashing_child("native", dir.path()).expect("the crash capture hung");
    assert_captured(dir.path(), &stderr);
}

/// A dump that never finishes: the crash stops waiting at its `DUMP_WAIT`
/// and the report says so, and the dump's snapshot clone (a process of its
/// own holding the crashed one's memory) is ended with it, not left
/// running.
#[cfg(windows)]
#[test]
fn a_dump_that_never_finishes_leaves_no_snapshot_behind() {
    use std::io::Read;
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(CASE, "native")
        .env(DIR, dir.path())
        .env(bri_crash::STALL_DUMP_VAR, "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut pipe = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = pipe.read_to_string(&mut text);
        text
    });
    // The snapshot is taken: its clone runs as the child's child.
    let deadline = std::time::Instant::now() + bri_crash::NATIVE_CAPTURE_WAIT;
    while processes::children(child.id()).is_empty() {
        assert!(std::time::Instant::now() < deadline, "no snapshot taken");
        std::thread::yield_now();
    }
    // The crash gives up on the dump, frees the snapshot and writes its
    // report.
    let reports = loop {
        let reports = files(dir.path(), "crash-", ".txt");
        if reports
            .first()
            .and_then(|r| fs::read_to_string(r).ok())
            .is_some_and(|r| r.contains("minidump failed"))
        {
            break reports;
        }
        assert!(std::time::Instant::now() < deadline, "no report written");
        std::thread::yield_now();
    };
    // Freeing the snapshot ends its clone; Windows tears the process down
    // after, later on a loaded machine. It must go while the crashed process
    // is still there: a clone left to go only when that process does is the
    // copy of its memory this guards against.
    let released = std::time::Instant::now() + bri_crash::NATIVE_CAPTURE_WAIT;
    while !processes::children(child.id()).is_empty() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "the snapshot's clone outlived the crashed process"
        );
        assert!(
            std::time::Instant::now() < released,
            "the snapshot's clone was never released"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let report = fs::read_to_string(&reports[0]).unwrap();
    assert!(report.contains("did not finish in time"), "{report}");
    assert!(!child.wait().unwrap().success());
    let _ = reader.join();
}

/// Running copies of this executable started by another: a snapshot's
/// clone runs the same image as the process it copies (Windows Error
/// Reporting's own processes do not).
#[cfg(windows)]
mod processes {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    /// The ids of processes running this executable whose parent is
    /// `parent`.
    pub fn children(parent: u32) -> Vec<u32> {
        let exe = std::env::current_exe().unwrap();
        let name: Vec<u16> = exe.file_name().unwrap().encode_wide().collect();
        // SAFETY: a snapshot of every process, closed below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        assert_ne!(snapshot, INVALID_HANDLE_VALUE);
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = Vec::new();
        // SAFETY: a valid snapshot and an entry whose size is set.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            let image = &entry.szExeFile;
            let image = &image[..image.iter().position(|c| *c == 0).unwrap_or(image.len())];
            if entry.th32ParentProcessID == parent && image == name.as_slice() {
                found.push(entry.th32ProcessID);
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        // SAFETY: the snapshot opened above.
        unsafe { CloseHandle(snapshot) };
        found
    }
}

/// The original hang: a crash while the loader was busy on other threads
/// (libraries loading and unloading, threads starting). A minidump of the
/// live process suspended those threads mid-load, then loaded a library
/// itself and waited for them forever, its crashing thread suspended too.
#[cfg(windows)]
#[test]
fn a_native_crash_while_other_threads_load_libraries_still_dumps() {
    let dir = tempfile::tempdir().unwrap();
    let stderr = crashing_child("loader_busy", dir.path()).expect("the crash capture hung");
    assert_captured(dir.path(), &stderr);
}

/// Copy just the executable, as a player distribution does. Raw native frames
/// must survive without a PDB adjacent to this copy. Its embedded original PDB
/// path may still resolve on the build machine; this does not assert otherwise.
#[cfg(windows)]
#[test]
fn a_copied_executable_retains_native_frames_without_adjacent_symbols() {
    let dir = tempfile::tempdir().unwrap();
    let original = std::env::current_exe().unwrap();
    let copied = dir.path().join(original.file_name().unwrap());
    fs::copy(&original, &copied).unwrap();
    assert!(files(dir.path(), "", ".pdb").is_empty());
    let out = Command::new(&copied)
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(CASE, "panic")
        .env(DIR, dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let reports = files(dir.path(), "crash-", ".txt");
    assert_eq!(reports.len(), 1, "{}", String::from_utf8_lossy(&out.stderr));
    let report = fs::read_to_string(&reports[0]).unwrap();
    assert_native_frames(&report);
    let module = format!("module={:?}", copied.to_string_lossy());
    assert!(
        report.contains(&module),
        "copied executable module mapped: {report}"
    );
    assert!(files(dir.path(), "", ".pdb").is_empty());
}

/// Other threads keeping the loader busy until the crash's report exists.
#[cfg(windows)]
mod loader {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }
    /// System libraries a test executable does not load itself.
    const LIBRARIES: [&str; 2] = ["version.dll", "winmm.dll"];

    /// One thread per library, each loading and unloading it and starting a
    /// thread (whose start-up runs under the loader too) over and over;
    /// returns once each has been round once.
    pub fn keep_busy(dir: &std::path::Path) {
        let (started, running) = std::sync::mpsc::channel();
        for name in LIBRARIES {
            let dir = dir.to_path_buf();
            let started = started.clone();
            let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
            std::thread::spawn(move || {
                loop {
                    // SAFETY: a NUL-terminated name; freed below.
                    let module = unsafe { LoadLibraryW(wide.as_ptr()) };
                    let _ = std::thread::spawn(|| {}).join();
                    if !module.is_null() {
                        // SAFETY: loaded above by this thread.
                        unsafe { FreeLibrary(module) };
                    }
                    let _ = started.send(());
                    if !super::files(&dir, "crash-", ".txt").is_empty() {
                        break;
                    }
                }
            });
        }
        for _ in LIBRARIES {
            running.recv().unwrap();
        }
    }
}

/// Just enough of the minidump format to find the exception and threads.
#[cfg(windows)]
mod minidump {
    /// `MINIDUMP_STREAM_TYPE`s.
    const THREAD_LIST: u32 = 3;
    const EXCEPTION: u32 = 6;
    /// "MDMP", little-endian.
    const SIGNATURE: u32 = 0x504D_444D;
    /// `MINIDUMP_THREAD`'s size.
    const THREAD_SIZE: usize = 48;
    /// `MINIDUMP_DIRECTORY`'s size: stream type, data size, data offset.
    const DIRECTORY_ENTRY: usize = 12;

    pub struct Dump(Vec<u8>);
    impl Dump {
        pub fn read(path: &std::path::Path) -> Self {
            let dump = Self(std::fs::read(path).unwrap());
            assert_eq!(dump.u32(0), SIGNATURE, "a minidump");
            dump
        }
        fn u32(&self, at: usize) -> u32 {
            u32::from_le_bytes(self.0[at..at + 4].try_into().unwrap())
        }
        /// Each stream's (type, offset): `MINIDUMP_HEADER` gives their count
        /// at 8 and the directory's offset at 12.
        fn streams(&self) -> impl Iterator<Item = (u32, usize)> + '_ {
            let (count, directory) = (self.u32(8) as usize, self.u32(12) as usize);
            (0..count).map(move |i| {
                let entry = directory + i * DIRECTORY_ENTRY;
                (self.u32(entry), self.u32(entry + 8) as usize)
            })
        }
        fn stream(&self, kind: u32) -> Option<usize> {
            self.streams().find(|(k, _)| *k == kind).map(|(_, at)| at)
        }
        /// `MINIDUMP_EXCEPTION_STREAM`: the thread id, then (after 4 bytes
        /// of alignment) the record, whose first field is the code.
        pub fn exception(&self) -> Option<(u32, u32)> {
            let at = self.stream(EXCEPTION)?;
            Some((self.u32(at), self.u32(at + 8)))
        }
        /// `MINIDUMP_THREAD_LIST`: a count, then each thread, id first.
        pub fn threads(&self) -> Vec<u32> {
            let Some(at) = self.stream(THREAD_LIST) else {
                return Vec::new();
            };
            (0..self.u32(at) as usize)
                .map(|i| self.u32(at + 4 + i * THREAD_SIZE))
                .collect()
        }
    }
}
