//! The offscreen device every renderer test in a binary draws on, one test
//! at a time (as `bri-render`'s `persistent_scene` and `mirrors` do). A
//! machine without a GPU draws on a software adapter whose every frame keeps
//! all its cores busy; tests drawing side by side, each on its own device,
//! starve one another until their frames miss their waits. Taking turns on
//! one device gives each frame the whole machine.
#![allow(dead_code)]

use anyhow::Result;
use bri_ui::gpu::Headless;
use std::sync::{Mutex, MutexGuard, PoisonError};

static GPU: Mutex<Option<Headless>> = Mutex::new(None);

/// One test's turn on the shared device; the next test waits for it.
pub struct Turn(MutexGuard<'static, Option<Headless>>);
impl std::ops::Deref for Turn {
    type Target = Headless;
    fn deref(&self) -> &Headless {
        self.0
            .as_ref()
            .expect("the device is made before a turn starts")
    }
}

/// Wait for this test's turn on the shared device, making it first.
pub fn turn() -> Result<Turn> {
    // A test that failed on its turn leaves the device as good as ever.
    let mut gpu = GPU.lock().unwrap_or_else(PoisonError::into_inner);
    if gpu.is_none() {
        *gpu = Some(Headless::new()?);
    }
    Ok(Turn(gpu))
}

/// How long one wait may keep the GPU busy before it counts as hung. Only
/// a hang reaches it: a software adapter (WARP on the Windows CI runner,
/// lavapipe on Linux) can spend well over half a minute on one frame, and
/// a frame that finishes ends the wait at once.
pub const HANG: std::time::Duration = std::time::Duration::from_secs(300);

/// Wait until the GPU has finished everything submitted so far, saying so
/// every half minute it is still at it, and fail only after [`HANG`].
pub fn wait(device: &wgpu::Device, what: &str) -> Result<()> {
    const SLICE: std::time::Duration = std::time::Duration::from_secs(30);
    let start = std::time::Instant::now();
    loop {
        match device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(SLICE),
        }) {
            Ok(_) => {
                if start.elapsed() >= SLICE {
                    eprintln!("{what}: the GPU took {:.1?}", start.elapsed());
                }
                return Ok(());
            }
            Err(wgpu::PollError::Timeout) if start.elapsed() < HANG => {
                eprintln!(
                    "{what}: the GPU is still drawing after {:.0?}",
                    start.elapsed()
                );
            }
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "{what}: the GPU did not finish within {:.0?}: {error}",
                    start.elapsed()
                ));
            }
        }
    }
}
