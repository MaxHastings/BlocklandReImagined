//! The whole content root loads: `ClientContent::load` and the real
//! `App::load` accept the made-up root `support::content_root` writes, as
//! they accept the generated v20 one. Never creates a window or OS input.
use anyhow::{Result, ensure};
use bri_client::{app::App, content::ClientContent, content::LOADABLE_MAPS};

#[macro_use]
mod support;
use support::content_root::ContentRoot;

fn the_client_content_loads(f: &ContentRoot) -> Result<()> {
    let content = ClientContent::load(&f.root)?;
    for map in LOADABLE_MAPS {
        ensure!(
            content.maps.iter().any(|m| m.id.eq_ignore_ascii_case(map)),
            "{map} is not loaded"
        );
    }
    Ok(())
}

fn the_app_loads(f: &ContentRoot) -> Result<()> {
    let state = support::files::scratch("content-root-state-")?;
    App::load(&f.root, state.path(), (960, 720))?;
    Ok(())
}

synthetic_and_content!(
    ContentRoot: the_client_content_loads,
    the_app_loads,
);
