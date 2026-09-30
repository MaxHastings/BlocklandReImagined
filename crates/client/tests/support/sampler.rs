//! A small in-process sampling profiler for benchmarks: another thread
//! suspends the measured thread about once a millisecond, unwinds its stack
//! with the x64 unwind tables and resumes it; symbols resolve afterwards
//! through dbghelp. Needs no administrator rights (unlike ETW). While the
//! thread is suspended the sampler never allocates, since the suspended
//! thread may hold the heap lock.
#![allow(dead_code)]

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

const DEPTH: usize = 96;

pub struct Sampler {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Vec<Vec<u64>>>>,
}

pub struct Profile {
    stacks: Vec<Vec<u64>>,
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE},
        System::{
            Diagnostics::Debug::{
                CONTEXT, CONTEXT_FULL_AMD64, GetThreadContext, RtlLookupFunctionEntry,
                RtlVirtualUnwind, SYMBOL_INFOW, SYMOPT_DEFERRED_LOADS, SYMOPT_UNDNAME,
                SymFromAddrW, SymInitializeW, SymSetOptions,
            },
            Threading::{GetCurrentProcess, GetCurrentThread, ResumeThread, SuspendThread},
        },
    };

    struct Handle(HANDLE);
    /// GetThreadContext needs a 16-byte aligned CONTEXT.
    #[repr(C, align(16))]
    struct Aligned(CONTEXT);
    unsafe impl Send for Handle {}

    /// Unwind `context` into `frames`; returns how many were written.
    unsafe fn walk(context: &mut CONTEXT, frames: &mut [u64; DEPTH]) -> usize {
        let mut n = 0;
        while n < DEPTH {
            let pc = context.Rip;
            if pc == 0 {
                break;
            }
            frames[n] = pc;
            n += 1;
            let mut base = 0u64;
            let entry = unsafe { RtlLookupFunctionEntry(pc, &mut base, std::ptr::null_mut()) };
            if entry.is_null() {
                // A leaf function: the return address is on top of the stack.
                if context.Rsp == 0 {
                    break;
                }
                context.Rip = unsafe { *(context.Rsp as *const u64) };
                context.Rsp += 8;
            } else {
                let mut data = std::ptr::null_mut();
                let mut frame = 0u64;
                unsafe {
                    RtlVirtualUnwind(
                        0,
                        base,
                        pc,
                        entry,
                        context,
                        &mut data,
                        &mut frame,
                        std::ptr::null_mut(),
                    )
                };
            }
        }
        n
    }

    pub fn start() -> Sampler {
        let mut target: HANDLE = std::ptr::null_mut();
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                GetCurrentThread(),
                GetCurrentProcess(),
                &mut target,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            );
        }
        let target = Handle(target);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = std::thread::spawn(move || {
            let target = target;
            let mut stacks = Vec::with_capacity(1 << 16);
            let mut frames = [0u64; DEPTH];
            while !stopping.load(Ordering::Relaxed) {
                let n = unsafe {
                    if SuspendThread(target.0) == u32::MAX {
                        break;
                    }
                    let mut context: Aligned = std::mem::zeroed();
                    context.0.ContextFlags = CONTEXT_FULL_AMD64;
                    let n = if GetThreadContext(target.0, &mut context.0) != 0 {
                        walk(&mut context.0, &mut frames)
                    } else {
                        0
                    };
                    ResumeThread(target.0);
                    n
                };
                if n > 0 {
                    stacks.push(frames[..n].to_vec());
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            stacks
        });
        Sampler {
            stop,
            worker: Some(worker),
        }
    }

    pub fn symbolize(addresses: &[u64]) -> HashMap<u64, String> {
        let process = unsafe { GetCurrentProcess() };
        unsafe {
            SymSetOptions(SYMOPT_UNDNAME | SYMOPT_DEFERRED_LOADS);
            let dir: Vec<u16> = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.as_os_str().to_owned()))
                .unwrap_or_default()
                .encode_wide()
                .chain([0])
                .collect();
            SymInitializeW(process, dir.as_ptr(), 1);
        }
        const NAME: usize = 512;
        let mut buffer = vec![0u64; (std::mem::size_of::<SYMBOL_INFOW>() + NAME * 2) / 8 + 1];
        addresses
            .iter()
            .map(|&address| {
                let info = buffer.as_mut_ptr().cast::<SYMBOL_INFOW>();
                let name = unsafe {
                    std::ptr::write_bytes(buffer.as_mut_ptr(), 0, buffer.len());
                    (*info).SizeOfStruct = std::mem::size_of::<SYMBOL_INFOW>() as u32;
                    (*info).MaxNameLen = NAME as u32;
                    let mut displacement = 0;
                    if SymFromAddrW(process, address, &mut displacement, info) != 0 {
                        let len = (*info).NameLen as usize;
                        let chars = std::slice::from_raw_parts((*info).Name.as_ptr(), len);
                        String::from_utf16_lossy(chars)
                    } else {
                        format!("{address:#x}")
                    }
                };
                (address, name)
            })
            .collect()
    }
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
mod imp {
    use super::*;
    pub fn start() -> Sampler {
        Sampler {
            stop: Arc::new(AtomicBool::new(true)),
            worker: None,
        }
    }
    pub fn symbolize(addresses: &[u64]) -> HashMap<u64, String> {
        addresses.iter().map(|a| (*a, format!("{a:#x}"))).collect()
    }
}

impl Sampler {
    /// Start sampling the calling thread.
    pub fn start() -> Self {
        imp::start()
    }
    pub fn finish(mut self) -> Profile {
        self.stop.store(true, Ordering::Relaxed);
        let stacks = self
            .worker
            .take()
            .and_then(|w| w.join().ok())
            .unwrap_or_default();
        Profile { stacks }
    }
}

/// Generic suffixes and hashes make one function many names; trim them.
fn tidy(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut depth = 0;
    for c in name.chars() {
        match c {
            '<' => {
                depth += 1;
                if depth == 1 {
                    out.push_str("<…>");
                }
            }
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

impl Profile {
    pub fn samples(&self) -> usize {
        self.stacks.len()
    }
    /// Stacks in the folded format flame graph tools read, root first.
    pub fn folded(&self) -> String {
        let mut unique: Vec<u64> = self.stacks.iter().flatten().copied().collect();
        unique.sort_unstable();
        unique.dedup();
        let names: HashMap<u64, String> = imp::symbolize(&unique)
            .into_iter()
            .map(|(a, n)| (a, tidy(&n).replace(';', ":")))
            .collect();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for stack in &self.stacks {
            let line: Vec<&str> = stack.iter().rev().map(|a| names[a].as_str()).collect();
            *counts.entry(line.join(";")).or_default() += 1;
        }
        let mut rows: Vec<_> = counts.into_iter().collect();
        rows.sort();
        rows.into_iter()
            .map(|(s, n)| {
                format!(
                    "{s} {n}
"
                )
            })
            .collect()
    }
    /// Top functions by inclusive and by self samples, as text.
    pub fn report(&self, top: usize) -> String {
        let mut unique: Vec<u64> = self.stacks.iter().flatten().copied().collect();
        unique.sort_unstable();
        unique.dedup();
        let names: HashMap<u64, String> = imp::symbolize(&unique)
            .into_iter()
            .map(|(a, n)| (a, tidy(&n)))
            .collect();
        let mut inclusive: HashMap<&str, usize> = HashMap::new();
        let mut own: HashMap<&str, usize> = HashMap::new();
        for stack in &self.stacks {
            let mut seen = std::collections::HashSet::new();
            for (i, address) in stack.iter().enumerate() {
                let name = names[address].as_str();
                if i == 0 {
                    *own.entry(name).or_default() += 1;
                }
                if seen.insert(name) {
                    *inclusive.entry(name).or_default() += 1;
                }
            }
        }
        let total = self.stacks.len().max(1) as f64;
        let table = |map: HashMap<&str, usize>| {
            let mut rows: Vec<_> = map.into_iter().collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1));
            rows.into_iter()
                .take(top)
                .map(|(name, n)| format!("{:6.2}%  {name}\n", n as f64 * 100.0 / total))
                .collect::<String>()
        };
        format!(
            "{} samples\n--- inclusive ---\n{}--- self ---\n{}",
            self.stacks.len(),
            table(inclusive),
            table(own)
        )
    }
}
