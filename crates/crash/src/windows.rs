//! Windows: tee stderr into the session log and write a minidump on a native
//! crash (an unhandled SEH exception).
use std::{
    fs::File,
    io::{self, Read, Write},
    mem::ManuallyDrop,
    os::windows::io::{FromRawHandle, RawHandle},
    time::SystemTime,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL},
    System::{
        Console::{GetStdHandle, STD_ERROR_HANDLE, SetStdHandle},
        Diagnostics::Debug::{
            EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, MINIDUMP_EXCEPTION_INFORMATION,
            MiniDumpScanMemory, MiniDumpWithIndirectlyReferencedMemory, MiniDumpWriteDump,
            SetUnhandledExceptionFilter,
        },
        Pipes::CreatePipe,
        Threading::{GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId},
    },
};

pub(crate) fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: no preconditions; fails harmlessly without a parent console
    // or when this process already has one.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

/// The stderr tee, kept so `finish` can drain it before the process exits.
/// Handles are stored as integers: raw handles are not `Send`.
struct Tee {
    write: usize,
    original: usize,
    reader: std::thread::JoinHandle<File>,
}

static TEE: std::sync::Mutex<Option<Tee>> = std::sync::Mutex::new(None);

/// Put the original stderr back, close the pipe and wait for the reader to
/// write everything already sent; returns the session log. Without this,
/// bytes still in the pipe when the process exits (a startup error, a panic
/// message) are lost from both the log and the terminal whenever the reader
/// thread has not run yet.
fn drain() -> Option<File> {
    let tee = TEE.lock().unwrap_or_else(|e| e.into_inner()).take()?;
    // SAFETY: restores the handle this process had before the tee, then
    // closes the pipe's write end, which the tee owns and nothing else uses.
    unsafe {
        SetStdHandle(STD_ERROR_HANDLE, tee.original as HANDLE);
        CloseHandle(tee.write as HANDLE);
    }
    tee.reader.join().ok()
}

/// Deliver everything written so far and stop teeing (the program's end).
pub(crate) fn finish() {
    drain();
}

/// Deliver everything written so far, then keep teeing into the same log.
pub(crate) fn flush() {
    if let Some(log) = drain() {
        let _ = tee_stderr(log);
    }
}

pub(crate) fn install(log: File) -> io::Result<()> {
    tee_stderr(log)?;
    // SAFETY: registers a process-wide filter with a matching signature.
    unsafe { SetUnhandledExceptionFilter(Some(on_native_crash)) };
    Ok(())
}

/// Route stderr through a pipe whose reader thread appends every byte to the
/// session log and forwards it to the original stderr (a terminal in dev,
/// nothing for a windowed release build). Rust's stderr looks up the process
/// handle on every write, so `eprintln!` from any thread follows the switch.
fn tee_stderr(mut log: File) -> io::Result<()> {
    let mut read: HANDLE = std::ptr::null_mut();
    let mut write: HANDLE = std::ptr::null_mut();
    // SAFETY: out-pointers to locals; no security attributes; default size.
    if unsafe { CreatePipe(&mut read, &mut write, std::ptr::null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: querying a standard handle has no preconditions.
    let original = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    // SAFETY: `read` is a fresh pipe handle this function now owns.
    let mut reader = unsafe { File::from_raw_handle(read as RawHandle) };
    // The original stderr is borrowed, never closed.
    let echo = (!original.is_null() && original != INVALID_HANDLE_VALUE)
        // SAFETY: a valid standard handle; ManuallyDrop keeps it open.
        .then(|| ManuallyDrop::new(unsafe { File::from_raw_handle(original as RawHandle) }));
    let reader = std::thread::Builder::new()
        .name("bri-log".into())
        .spawn(move || {
            let mut echo = echo;
            let mut buffer = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                let _ = log.write_all(&buffer[..n]);
                if let Some(echo) = echo.as_mut() {
                    let _ = echo.write_all(&buffer[..n]);
                }
            }
            log
        })?;
    // SAFETY: `write` is the pipe's write end, kept open until `finish`.
    if unsafe { SetStdHandle(STD_ERROR_HANDLE, write) } == 0 {
        return Err(io::Error::last_os_error());
    }
    *TEE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Tee {
        write: write as usize,
        original: original as usize,
        reader,
    });
    Ok(())
}

unsafe extern "system" fn on_native_crash(info: *const EXCEPTION_POINTERS) -> i32 {
    if let Some(state) = crate::STATE.get() {
        let _guard = state.writing.lock().unwrap_or_else(|e| e.into_inner());
        let stamp = crate::timestamp(SystemTime::now());
        let dump = crate::unique(&state.directory, &format!("crash-{stamp}"), "dmp");
        // SAFETY: `info` is the exception record the OS passed this filter.
        let code = unsafe { info.as_ref() }
            .and_then(|i| unsafe { i.ExceptionRecord.as_ref() })
            .map(|r| (r.ExceptionCode as u32, r.ExceptionAddress as usize));
        let written = unsafe { write_minidump(&dump, info) };
        let report = dump.with_extension("txt");
        if let Ok(mut file) = File::create(&report) {
            let _ = writeln!(
                file,
                "{} {} crashed (native exception)",
                state.program,
                env!("CARGO_PKG_VERSION")
            );
            let _ = writeln!(file, "time: {stamp}");
            if let Some((code, address)) = code {
                let _ = writeln!(file, "exception: 0x{code:08X} at 0x{address:016X}");
            }
            let _ = match written {
                Ok(()) => writeln!(file, "minidump: {}", dump.display()),
                Err(e) => writeln!(file, "minidump failed: {e}"),
            };
            let _ = state.append_log_tail(&mut file);
            let _ = file.sync_all();
        }
    }
    // Let Windows Error Reporting run as it would have.
    EXCEPTION_CONTINUE_SEARCH
}

/// # Safety
/// `info` must be the pointers handed to an exception filter (or null).
unsafe fn write_minidump(path: &std::path::Path, info: *const EXCEPTION_POINTERS) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: NUL-terminated path; a new file we close below.
    let file = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE,
            0,
            std::ptr::null(),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if file == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: takes ownership so the handle closes on every path.
    let owned = unsafe { File::from_raw_handle(file as RawHandle) };
    let exception = MINIDUMP_EXCEPTION_INFORMATION {
        // SAFETY: no preconditions.
        ThreadId: unsafe { GetCurrentThreadId() },
        ExceptionPointers: info as *mut EXCEPTION_POINTERS,
        ClientPointers: 0,
    };
    // SAFETY: this process, an open writable file, and the filter's exception.
    let ok = unsafe {
        MiniDumpWriteDump(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            file,
            MiniDumpWithIndirectlyReferencedMemory | MiniDumpScanMemory,
            if info.is_null() { std::ptr::null() } else { &exception },
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    let result = if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    };
    drop(owned);
    result
}
