//! The CPU time the calling thread has used: the work a stretch of code
//! cost, which a loaded machine does not stretch. Wall-clock time also
//! counts the time the OS gives other processes, so under the gate's
//! parallel test binaries it grows with no change in the work done.
//!
//! A diagnostic for performance bars, never game state.
use std::time::Duration;

/// CPU time (user and kernel) this thread has used since it started.
///
/// On Windows the OS charges whole clock ticks (about 15.6 ms) to the
/// thread running when each tick ends, so one short stretch reads zero or
/// a whole tick; summed over many stretches the total is the CPU time
/// used, within a few ticks.
#[cfg(windows)]
pub fn thread() -> Duration {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading};
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: the pseudo-handle of the current thread and four owned FILETIMEs.
    let ok = unsafe {
        Threading::GetThreadTimes(
            Threading::GetCurrentThread(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return Duration::ZERO;
    }
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    // FILETIME counts 100 ns intervals.
    Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}
/// CPU time (user and kernel) this thread has used since it started.
#[cfg(unix)]
pub fn thread() -> Duration {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: an owned timespec for the current thread's CPU clock.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    if ok != 0 {
        return Duration::ZERO;
    }
    Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
}
