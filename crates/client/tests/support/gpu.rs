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

/// Switch `app` to Classic lighting (`$pref::Video::Lighting` 0) before its
/// GPU starts. Only for synthetic variants that crash lavapipe (Mesa
/// 25.2.8, the software adapter in CI containers) in the Unified fragment
/// path of `bri-render`'s scene shader: an open renderer follow-up. The
/// content variants keep the default (Unified) lighting, so it stays
/// covered.
pub fn pin_classic_lighting(app: &mut bri_client::app::App) -> Result<()> {
    use bri_ui::api::{UiAction, UiUpdate};
    app.ui.apply(UiUpdate::SetPrefs(vec![(
        bri_ui::screens::options::LIGHTING.into(),
        "0".into(),
    )]));
    let settings = app.ui.settings();
    app.ui
        .core
        .request(UiAction::SaveSettings(Box::new(settings)));
    anyhow::ensure!(
        bri_client::platform::PlatformApp::pump(app)?.is_empty(),
        "Unexpected native window command"
    );
    Ok(())
}
