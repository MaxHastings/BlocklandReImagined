//! GPU objects whose pipelines take seconds to compile, built on a worker
//! thread so the window shows the menu and keeps responding meanwhile.
//!
//! Shader compilation is the slow part of opening the game: the scene
//! shader is large, and every pipeline variant (blend, culling, sample
//! count) compiles it again on Vulkan and once per format on DX12. Nothing
//! the menus draw needs those pipelines, so they compile in the background
//! and the first frame goes up without them. Code that draws the world waits
//! for them ([`Building::wait`]); code that can skip a frame polls
//! ([`Building::ready`]).
use std::thread::JoinHandle;

/// Opens when the thread holding its [`GateOpener`] finishes, however it ends; a thread that should wait its turn waits on it.
#[derive(Clone, Default)]
pub struct Gate(std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>);

/// Opens its [`Gate`] when dropped.
pub struct GateOpener(Gate);

impl Gate {
    pub fn opener(&self) -> GateOpener {
        GateOpener(self.clone())
    }
    /// Blocks until the gate has opened.
    pub fn wait(&self) {
        let (lock, condition) = &*self.0;
        let mut open = lock.lock().unwrap_or_else(|e| e.into_inner());
        while !*open {
            open = condition.wait(open).unwrap_or_else(|e| e.into_inner());
        }
    }
}

impl Drop for GateOpener {
    fn drop(&mut self) {
        let (lock, condition) = &*self.0.0;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        condition.notify_all();
    }
}

pub struct Building<T: Send + 'static> {
    working: Option<JoinHandle<T>>,
    done: Option<T>,
}

impl<T: Send + 'static> Building<T> {
    /// Build `make` on a new thread named `name`; the log records how long
    /// it took, so a slow start on a player's PC names its cause.
    pub fn spawn(name: &str, make: impl FnOnce() -> T + Send + 'static) -> Self {
        let label = name.to_string();
        let handle = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let start = std::time::Instant::now();
                let value = make();
                bri_console::echo(format!(
                    "Compiled {label} in {} ms",
                    start.elapsed().as_millis()
                ));
                value
            })
            .expect("starting a GPU build thread");
        Self {
            working: Some(handle),
            done: None,
        }
    }
    /// The object, if its build has finished. Never blocks.
    pub fn ready(&mut self) -> Option<&mut T> {
        if self.working.as_ref().is_some_and(|h| !h.is_finished()) {
            return None;
        }
        Some(self.wait())
    }
    /// The object itself, if its build has finished. Never blocks.
    pub fn into_ready(mut self) -> Option<T> {
        self.ready()?;
        self.done.take()
    }
    /// The object, if it was already collected by `ready` or `wait`.
    pub fn finished(&self) -> Option<&T> {
        self.done.as_ref()
    }
    /// The object, waiting for its build if it is still compiling. A panic
    /// on the worker carries on here, as if it had been built on this thread.
    pub fn wait(&mut self) -> &mut T {
        if let Some(handle) = self.working.take() {
            let value = handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            self.done = Some(value);
        }
        self.done.as_mut().expect("a finished GPU build")
    }
}
