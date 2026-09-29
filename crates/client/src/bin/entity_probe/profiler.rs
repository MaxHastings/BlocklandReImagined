//! A small sampling profiler for one thread of this process, so the probe
//! can say where the time goes without an external tool or administrator
//! rights. A second thread suspends the target about every millisecond,
//! walks its stack with the x64 unwind tables, resumes it, and the samples
//! are symbolized from the PDB at the end.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE},
    System::{
        Diagnostics::Debug::{
            CONTEXT, CONTEXT_FULL_AMD64, GetThreadContext, RtlLookupFunctionEntry,
            RtlVirtualUnwind, SYMBOL_INFOW, SYMOPT_DEFERRED_LOADS, SYMOPT_UNDNAME, SymFromAddrW,
            SymInitializeW, SymSetOptions, UNW_FLAG_NHANDLER,
        },
        Threading::{GetCurrentProcess, GetCurrentThread, ResumeThread, SuspendThread},
    },
};

const DEPTH: usize = 96;

#[repr(C, align(16))]
struct Aligned(CONTEXT);

struct Target(HANDLE);
// SAFETY: a duplicated thread handle may be used from any thread.
unsafe impl Send for Target {}

pub struct Profiler {
    stop: Arc<AtomicBool>,
    worker: JoinHandle<Vec<Vec<u64>>>,
}
impl Profiler {
    /// Start sampling the calling thread.
    pub fn start() -> Self {
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: duplicates the pseudo-handle into a real one we own.
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                GetCurrentThread(),
                GetCurrentProcess(),
                &mut handle,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            );
        }
        let target = Target(handle);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = std::thread::spawn(move || {
            let target = target;
            let mut samples = Vec::with_capacity(1 << 16);
            let mut frames = [0u64; DEPTH];
            while !stopping.load(Ordering::Relaxed) {
                // SAFETY: the target is suspended while its context and stack
                // are read; nothing here allocates or takes locks meanwhile.
                let n = unsafe { sample(target.0, &mut frames) };
                if n > 0 {
                    samples.push(frames[..n].to_vec());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            // SAFETY: the handle was duplicated above and is closed once.
            unsafe { CloseHandle(target.0) };
            samples
        });
        Self { stop, worker }
    }
    /// Stop and report the hottest functions, inclusive and self, with the
    /// share of samples each was on the stack for.
    pub fn finish(self, top: usize) -> serde_json::Value {
        self.stop.store(true, Ordering::Relaxed);
        let samples = self.worker.join().unwrap_or_default();
        let mut names = Symbols::new();
        let mut inclusive: BTreeMap<String, usize> = BTreeMap::new();
        let mut leaf: BTreeMap<String, usize> = BTreeMap::new();
        // Caller -> callee pairs, to read the path into the hot spots.
        let mut edges: BTreeMap<(String, String), usize> = BTreeMap::new();
        for stack in &samples {
            let named: Vec<String> = stack.iter().map(|pc| names.name(*pc)).collect();
            if let Some(first) = named.first() {
                *leaf.entry(first.clone()).or_default() += 1;
            }
            let unique: BTreeSet<&String> = named.iter().collect();
            for name in unique {
                *inclusive.entry(name.clone()).or_default() += 1;
            }
            let pairs: BTreeSet<(String, String)> = named
                .windows(2)
                .map(|w| (w[1].clone(), w[0].clone()))
                .filter(|(a, b)| a != b)
                .collect();
            for pair in pairs {
                *edges.entry(pair).or_default() += 1;
            }
        }
        let total = samples.len().max(1) as f64;
        let ranked = |map: BTreeMap<String, usize>| {
            let mut v: Vec<_> = map.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            v.into_iter()
                .take(top)
                .map(|(name, n)| format!("{:5.1}% {name}", n as f64 * 100.0 / total))
                .collect::<Vec<_>>()
        };
        let mut calls: Vec<_> = edges.into_iter().collect();
        calls.sort_by(|a, b| b.1.cmp(&a.1));
        serde_json::json!({
            "samples": samples.len(),
            "inclusive": ranked(inclusive),
            "self": ranked(leaf),
            "calls": calls
                .into_iter()
                .take(top * 2)
                .map(|((a, b), n)| format!("{:5.1}% {a} -> {b}", n as f64 * 100.0 / total))
                .collect::<Vec<_>>(),
        })
    }
}

unsafe fn sample(thread: HANDLE, frames: &mut [u64; DEPTH]) -> usize {
    // SAFETY: the caller passes a valid thread handle of this process.
    unsafe {
        if SuspendThread(thread) == u32::MAX {
            return 0;
        }
        let mut context: Aligned = std::mem::zeroed();
        context.0.ContextFlags = CONTEXT_FULL_AMD64;
        let mut n = 0;
        if GetThreadContext(thread, &mut context.0) != 0 {
            let c = &mut context.0;
            while n < DEPTH && c.Rip != 0 {
                frames[n] = c.Rip;
                n += 1;
                let mut base = 0u64;
                let entry = RtlLookupFunctionEntry(c.Rip, &mut base, std::ptr::null_mut());
                if entry.is_null() {
                    // A leaf function: the return address is on top of the stack.
                    if c.Rsp == 0 {
                        break;
                    }
                    c.Rip = *(c.Rsp as *const u64);
                    c.Rsp += 8;
                } else {
                    let mut data = std::ptr::null_mut();
                    let mut frame = 0u64;
                    RtlVirtualUnwind(
                        UNW_FLAG_NHANDLER,
                        base,
                        c.Rip,
                        entry,
                        c,
                        &mut data,
                        &mut frame,
                        std::ptr::null_mut(),
                    );
                }
            }
        }
        ResumeThread(thread);
        n
    }
}

struct Symbols {
    cache: BTreeMap<u64, String>,
}
impl Symbols {
    fn new() -> Self {
        static INIT: std::sync::Once = std::sync::Once::new();
        // SAFETY: initializes dbghelp for this process once.
        INIT.call_once(|| unsafe {
            SymSetOptions(SYMOPT_UNDNAME | SYMOPT_DEFERRED_LOADS);
            // The PDB sits beside the executable.
            let dir: Vec<u16> = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.as_os_str().to_owned()))
                .map(|d| d.to_string_lossy().encode_utf16().chain([0]).collect())
                .unwrap_or_else(|| vec![0]);
            if SymInitializeW(GetCurrentProcess(), dir.as_ptr(), 1) == 0 {
                eprintln!(
                    "SymInitializeW failed: {}",
                    windows_sys::Win32::Foundation::GetLastError()
                );
            }
        });
        Self {
            cache: BTreeMap::new(),
        }
    }
    fn name(&mut self, pc: u64) -> String {
        if let Some(n) = self.cache.get(&pc) {
            return n.clone();
        }
        #[repr(C, align(8))]
        struct Buffer {
            info: SYMBOL_INFOW,
            name: [u16; 512],
        }
        // SAFETY: a zeroed SYMBOL_INFOW followed by room for its name.
        let name = unsafe {
            let mut b: Buffer = std::mem::zeroed();
            b.info.SizeOfStruct = std::mem::size_of::<SYMBOL_INFOW>() as u32;
            b.info.MaxNameLen = 512;
            let mut displacement = 0u64;
            if SymFromAddrW(GetCurrentProcess(), pc, &mut displacement, &mut b.info) != 0 {
                let len = (b.info.NameLen as usize).min(512);
                let ptr = b.info.Name.as_ptr();
                let slice = std::slice::from_raw_parts(ptr, len);
                shorten(&String::from_utf16_lossy(slice))
            } else {
                if self.cache.is_empty() {
                    eprintln!(
                        "SymFromAddrW failed: {}",
                        windows_sys::Win32::Foundation::GetLastError()
                    );
                }
                format!("0x{pc:x}")
            }
        };
        self.cache.insert(pc, name.clone());
        name
    }
}
/// Drop generic arguments and hashes so one function reads as one line.
fn shorten(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in name.chars() {
        match ch {
            '<' => {
                depth += 1;
                if depth == 1 {
                    out.push_str("<..>");
                }
            }
            '>' => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}
