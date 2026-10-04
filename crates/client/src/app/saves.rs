//! Saves, save pictures, old saves and colour sets.
use super::*;

/// Saves and their pictures, file jobs, old saves and colour-set loads.
pub(super) struct Saves {
    /// Each listed save's own file, whose picture Load Bricks previews.
    pub(super) save_pictures: HashMap<crate::save_picture::Key, PathBuf>,
    pub(super) save_previews: crate::save_picture::Previews,
    /// The save picture to take with the next scene drawn.
    pub(super) save_picture: Option<PathBuf>,
    /// Save pictures being read back and written.
    pub(super) save_shots: crate::platform::Screenshots,
    pub(super) saves: crate::saves::Store,
    pub(super) file_jobs: crate::saves::Jobs,
    /// v20 `.bls` saves players brought over; converting starts with the
    /// first frame, once startup has settled which Add-Ons are on.
    pub(super) old_saves: std::sync::Arc<crate::old_saves::OldSaves>,
    pub(super) old_saves_started: bool,
    /// Whether the build a crashed hosted game left was offered back this
    /// run (`crate::recovery`).
    pub(super) recovery_offered: bool,
    /// A save list read because converted saves arrived while a save
    /// dialog was open.
    pub(super) save_refresh:
        Option<std::sync::mpsc::Receiver<Result<Vec<crate::saves::Entry>, String>>>,
    /// A read save waiting on `LoadBricksColorGui`'s choice.
    pub(super) color_load: Option<(crate::saves::Request, Box<bri_world::build::SavedBuild>)>,
}

impl App {
    pub(super) fn poll_files(&mut self) {
        let Some((request, result)) = self.files.file_jobs.poll(&self.files.saves, &self.runtime)
        else {
            return;
        };
        let result = match result {
            Ok(crate::saves::Outcome::Listed(entries)) => {
                self.show_save_files(entries);
                Ok(())
            }
            Ok(crate::saves::Outcome::Saved(path, entries)) => {
                // What the host had when it took the build is saved under a
                // name; bricks placed while the file was written are not.
                if let Some(a) = self
                    .net
                    .attempt
                    .as_mut()
                    .filter(|a| a.local && request.session == Some(a.id))
                    && request.revision.is_some()
                {
                    a.saved_revision = request.revision;
                }
                // v20's save picture: the next scene drawn, without the interface.
                self.files.save_picture = crate::save_picture::path_for(&path);
                self.show_save_files(entries);
                Ok(())
            }
            Ok(crate::saves::Outcome::Loaded(build)) => {
                if self
                    .net
                    .attempt
                    .as_ref()
                    .filter(|a| a.entered)
                    .map(|a| a.id)
                    != request.session
                    || self.ui.session_request() != request.session
                {
                    Err(anyhow::anyhow!(
                        "Connection changed while reading the build; load canceled"
                    ))
                } else if matches!(request.action, UiAction::LoadBricks { .. }) {
                    // `LoadBricks_ColorCheck`: differing colours ask first.
                    let differs = self
                        .scene
                        .query_source
                        .as_ref()
                        .and_then(|w| crate::saves::color_difference(&w.palette, &build));
                    if let Some(append) = differs {
                        self.ui.apply(UiUpdate::ColorWarning { append });
                        self.files.color_load = Some((request, build));
                        return;
                    }
                    match self.send_load(request.id, build, request.action) {
                        Ok(()) => return, // Complete only after authoritative acceptance.
                        Err(error) => Err(error),
                    }
                } else {
                    Err(anyhow::anyhow!("Unexpected loaded build"))
                }
            }
            Err(error) => Err(anyhow::anyhow!(error)),
        };
        self.answer(request.id, result);
    }
    /// Draw the scene once more into a texture of its own, without the
    /// interface, and write it as the save picture at `path` (v20's
    /// `screenShot` after `Canvas.setContent(noHudGui)`). Waits for a frame
    /// with a scene to draw.
    pub(super) fn take_save_picture(
        &mut self,
        frame: &mut RenderContext<'_>,
        path: PathBuf,
    ) -> Result<()> {
        let texture = frame.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Save picture frame"),
            size: wgpu::Extent3d {
                width: frame.size.0,
                height: frame.size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: frame.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let drawn = self.render_scene(&mut RenderContext {
            device: frame.device,
            queue: frame.queue,
            encoder: frame.encoder,
            target: &view,
            format: frame.format,
            size: frame.size,
            ui_renderer: frame.ui_renderer,
        })?;
        if !drawn {
            self.files.save_picture = Some(path);
            return Ok(());
        }
        let capture =
            crate::platform::capture_copy(frame.device, frame.encoder, &texture, frame.format)?;
        self.files.save_shots.copied(
            crate::platform::Shot {
                path,
                fit: Some(crate::save_picture::FIT),
            },
            capture,
        );
        Ok(())
    }
    pub(super) fn show_save_files(&mut self, entries: Vec<crate::saves::Entry>) {
        self.files.save_pictures = entries
            .iter()
            .filter_map(|e| Some(((e.info.map.clone(), e.info.name.clone()), e.picture()?)))
            .collect();
        let maps = entries
            .iter()
            .map(|e| e.info.map.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        self.ui.apply(UiUpdate::SaveFiles {
            maps,
            files: entries.into_iter().map(|e| e.info).collect(),
        });
    }
    /// Convert `.bls` saves against the content now loaded.
    pub(super) fn start_old_saves(&mut self) {
        self.files.old_saves_started = true;
        match crate::old_saves::Converter::new(&self.content) {
            Ok(converter) => {
                self.files.old_saves.set_converter(converter);
                self.files.old_saves.start();
            }
            Err(error) => bri_console::warn(format!("Old saves can't be converted: {error:#}")),
        }
    }
    /// Start converting on the first frame, and put newly converted saves in
    /// an open save dialog as they arrive.
    pub(super) fn poll_old_saves(&mut self) {
        if !self.files.old_saves_started {
            self.start_old_saves();
        }
        self.show_damaged_files();
        // A hosted game that ended abnormally last time left its build:
        // offer it back once, on the first frame.
        if !self.files.recovery_offered {
            self.files.recovery_offered = true;
            if let Some(left) = crate::recovery::left(&self.state_dir, &self.files.saves) {
                self.ui
                    .apply(UiUpdate::Question(crate::recovery::question(&left, None)));
            }
        }
        if let Some(rx) = &self.files.save_refresh {
            match rx.try_recv() {
                Ok(Ok(entries)) => {
                    self.files.save_refresh = None;
                    self.show_save_files(entries);
                }
                Ok(Err(error)) => {
                    self.files.save_refresh = None;
                    bri_console::warn(format!("Could not list saves: {error}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.files.save_refresh = None,
            }
        }
        let open = self.ui.is_open(ScreenId::LoadBricks) || self.ui.is_open(ScreenId::SaveBricks);
        // A closed dialog lists afresh when it opens.
        if self.files.old_saves.take_changed() && open {
            let store = self.files.saves.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            self.files.save_refresh = Some(rx);
            self.runtime.spawn_blocking(move || {
                let _ = tx.send(store.list().map_err(|e| format!("{e:#}")));
            });
        }
    }
    pub(super) fn send_load(
        &mut self,
        id: RequestId,
        build: Box<bri_world::build::SavedBuild>,
        action: UiAction,
    ) -> Result<()> {
        let UiAction::LoadBricks { ownership, .. } = action else {
            anyhow::bail!("Unexpected loaded build");
        };
        self.command(id, Command::LoadBuild { build, ownership }, action)?;
        // The loaded build arrives in batches; it matches its file.
        if let Some(a) = self.net.attempt.as_mut() {
            a.settling = Some(std::time::Instant::now() + SETTLE);
        }
        Ok(())
    }
    /// `ColorWarning_Click*`: load the waiting save as chosen, or leave Load
    /// Bricks open.
    pub(super) fn choose_color_load(&mut self, choice: bri_ui::api::ColorLoad) {
        use bri_ui::api::ColorLoad;
        let Some((request, mut build)) = self.files.color_load.take() else {
            return;
        };
        let result = if choice == ColorLoad::Cancel {
            Err(anyhow::anyhow!(bri_ui::api::LOAD_CANCELED))
        } else if self
            .net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .map(|a| a.id)
            != request.session
        {
            Err(anyhow::anyhow!(
                "Connection changed while reading the build; load canceled"
            ))
        } else {
            if choice == ColorLoad::Match
                && let Some(world) = &self.scene.query_source
            {
                crate::saves::match_colors(&world.palette, &mut build);
            }
            self.send_load(request.id, build, request.action)
        };
        if let Err(error) = result {
            self.answer(request.id, Err(error));
        }
    }
    /// Paint divisions for a world palette: the content's named divisions
    /// when the world matches a locally installed colorset, numbered ones otherwise.
    pub(super) fn colorset(&self, palette: &[[f32; 4]]) -> Vec<PaintDivision> {
        let default_colors: Vec<_> = self
            .content
            .paint
            .iter()
            .flat_map(|d| d.colors.iter().copied())
            .collect();
        if palette == default_colors.as_slice() {
            self.content.paint.clone()
        } else if let Some(set) = self.ui.core.host_colorsets.iter().find(|set| {
            set.divisions
                .iter()
                .flat_map(|d| d.colors.iter().copied())
                .eq(palette.iter().copied())
        }) {
            set.divisions.clone()
        } else {
            palette
                .chunks(9)
                .enumerate()
                .map(|(i, c)| PaintDivision {
                    name: format!("World {}", i + 1),
                    colors: c.to_vec(),
                })
                .collect()
        }
    }
    /// Tell the menus whether leaving would drop changes the host has not
    /// saved under a name.
    pub(super) fn track_unsaved(&mut self, a: &mut Attempt) {
        let now = std::time::Instant::now();
        let unsaved = match a.view.as_ref().map(|v| v.world_revision) {
            Some(revision) if a.local && a.entered => {
                match a.settling {
                    Some(until) if now < until => {
                        // Still rebuilding: follow it, and wait for it to go quiet.
                        if a.saved_revision != Some(revision) {
                            a.settling = Some(now + SETTLE);
                        }
                        a.saved_revision = Some(revision);
                    }
                    Some(_) => a.settling = None,
                    None => {}
                }
                *a.saved_revision.get_or_insert(revision) != revision
            }
            _ => false,
        };
        if unsaved != self.ui.core.unsaved_changes {
            self.ui
                .apply_session(a.id, UiUpdate::UnsavedChanges(unsaved));
        }
    }
}
