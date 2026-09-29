//! Millisecond timer resolution while a session runs.
//!
//! Windows wakes sleeping threads on its system timer, every 15.6 ms unless
//! a process asks for finer. Tokio's timers sleep on that clock, so without
//! this the host's 120 Hz tick woke in bursts of two, 40 Hz poses left 15.6
//! or 31 ms apart instead of an even 25 ms, and a guest's movement datagrams
//! waited up to a whole timer period. Games commonly request 1 ms for as long
//! as they simulate; since Windows 10 2004 the request only affects this
//! process, and it ends with the session, so menus idle at the normal rate.

/// Holds 1 ms timer resolution until dropped. Requests nest, so a listen
/// host's server and its own client may each hold one.
pub struct Guard(());

impl Guard {
    pub fn acquire() -> Self {
        #[cfg(windows)]
        // SAFETY: plain Win32 call with no pointers; paired with
        // timeEndPeriod in Drop.
        unsafe {
            windows_sys::Win32::Media::timeBeginPeriod(1);
        }
        Self(())
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(windows)]
        // SAFETY: ends the request `acquire` made with the same period.
        unsafe {
            windows_sys::Win32::Media::timeEndPeriod(1);
        }
    }
}
