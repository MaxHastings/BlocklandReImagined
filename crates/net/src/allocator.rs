//! Engine-wide allocator settings, applied once at process start by every
//! binary that installs mimalloc as its global allocator.
//!
//! mimalloc (3, as libmimalloc-sys builds it) purges (decommits) freed
//! memory back to the OS a second after it is freed, and does that work
//! inside whichever later `free` or `malloc` call notices the delay has
//! passed. A copy job that frees or rebuilds a few hundred megabytes of
//! brick maps therefore paid 5 to 30 ms of decommits inside single 4 KB
//! frees (copy_job_timing on Windows: worst ticks of 16 to 30 ms against
//! a 2.5 ms copy budget).
//! With purging off, freed pages stay committed and the next allocations
//! reuse them: the process keeps its peak working set instead of handing
//! memory back between big edits, the trade a game engine makes for flat
//! frame times.

use std::os::raw::{c_int, c_long};

// The mimalloc library itself, linked even where nothing else names it
// (this crate's own tests).
extern crate mimalloc;

/// `mi_option_purge_delay` in mimalloc 3's `mi_option_t`.
const MI_OPTION_PURGE_DELAY: c_int = 15;

unsafe extern "C" {
    fn mi_option_set(option: c_int, value: c_long);
    fn mi_option_get(option: c_int) -> c_long;
}

/// Turns mimalloc's delayed purge off for the whole process. Call first
/// thing in `main`; options set later still apply to later frees.
pub fn tune() {
    // SAFETY: mimalloc options are process-wide atomics; any thread may
    // set them at any time.
    unsafe { mi_option_set(MI_OPTION_PURGE_DELAY, -1) }
}

/// The purge delay now in force, in milliseconds (-1: never purged).
pub fn purge_delay() -> c_long {
    // SAFETY: as in `tune`.
    unsafe { mi_option_get(MI_OPTION_PURGE_DELAY) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_purge_option_index_is_mimallocs_purge_delay() {
        // mimalloc 3's default purge_delay is 1000 ms; a libmimalloc-sys
        // update that renumbered mi_option_t would read some other option
        // here.
        assert_eq!(super::purge_delay(), 1000);
        super::tune();
        assert_eq!(super::purge_delay(), -1);
    }
}
