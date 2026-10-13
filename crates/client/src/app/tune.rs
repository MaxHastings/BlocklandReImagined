//! `/tune`: the host's Tune window over a body ([`crate::tune`]).
use super::*;

/// Why Save to Add-On is off on a computer whose files are not the game's.
const NOT_HOSTED_HERE: &str =
    "Save to Add-On is off: this game's Add-Ons are not this computer's own.";

/// The open Tune window and the table it last showed.
#[derive(Default)]
pub(super) struct Tuning {
    window: Option<crate::tune::TuneWindow>,
    shown: Option<Arc<bri_sim::archetype::Archetypes>>,
}

impl App {
    /// `/tune` or `/tune <body>`: open the Tune window, for the host only.
    /// Answered by the caller.
    pub(super) fn open_tune(&mut self, args: &[String]) -> Result<()> {
        let view = self
            .network_view()
            .context("Tune bodies in a game you host: type /tune in chat.")?;
        ensure!(
            view.admin_snapshot.as_ref().is_some_and(|s| s.local_host),
            "Only the host tunes bodies: the Add-On's files are on the host's computer."
        );
        let archetypes = view.archetypes.clone();
        let wanted = args.join(" ");
        let body = if wanted.is_empty() {
            let pose = view
                .poses
                .get(&view.owner)
                .context("Spawn first, then type /tune to tune the body you play.")?;
            archetypes.resolve(pose.player.archetype).clone()
        } else {
            let wanted = wanted.to_ascii_lowercase();
            archetypes
                .iter()
                .map(|(_, a)| a)
                .find(|a| {
                    a.id.to_ascii_lowercase() == wanted
                        || (!a.name.is_empty() && a.name.to_ascii_lowercase() == wanted)
                })
                .cloned()
                .with_context(|| format!("No body in this game is called {}.", args.join(" ")))?
        };
        ensure!(
            !body.id.starts_with("v20."),
            "{} is a v20 player type: only an Add-On's own bodies can be tuned.",
            if body.name.is_empty() {
                &body.id
            } else {
                &body.name
            }
        );
        self.tuning.window = Some(crate::tune::TuneWindow::open(&body));
        self.tuning.shown = None;
        self.show_tune();
        Ok(())
    }
    /// Why Save to Add-On is off, or empty when it is on.
    fn cannot_save(&self, body: &str) -> String {
        if !self.addons.packages_from_tools {
            return NOT_HOSTED_HERE.into();
        }
        let Some(catalog) = self.addons.server_packages.as_deref() else {
            return NOT_HOSTED_HERE.into();
        };
        match crate::tune::archetype_file(&self.content.paths.root, catalog, body) {
            Ok(_) => String::new(),
            Err(error) => format!("Save to Add-On is off: {error:#}."),
        }
    }
    /// Give the window the body as it plays now.
    fn show_tune(&mut self) {
        let Some(window) = &self.tuning.window else {
            return;
        };
        let Some(view) = self.network_view() else {
            self.tuning = Tuning::default();
            self.ui.apply(UiUpdate::Tune(None));
            return;
        };
        let archetypes = view.archetypes.clone();
        let cannot_save = self.cannot_save(&window.body);
        let Some(window) = &self.tuning.window else {
            return;
        };
        let shown = window.view(&archetypes, &cannot_save);
        if shown.is_none() {
            self.tuning = Tuning::default();
        } else {
            self.tuning.shown = Some(archetypes);
        }
        self.ui.apply(UiUpdate::Tune(shown));
    }
    /// Each frame: a tuned body arrived, or the game ended.
    pub(super) fn refresh_tune(&mut self) {
        if self.tuning.window.is_none() {
            return;
        }
        let current = self.network_view().map(|v| v.archetypes.clone());
        let same = match (&current, &self.tuning.shown) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if !same {
            if let Some(window) = &mut self.tuning.window {
                window.status.clear();
                if let Some(archetypes) = &current {
                    window.settle(archetypes);
                }
            }
            self.show_tune();
        }
    }
    /// The host refused a change (a private "Not tuned:" notice): said in
    /// the window, and what was waiting on it is not. False when no Tune
    /// window is open.
    pub(super) fn tune_refused(&mut self, why: &str) -> bool {
        let Some(window) = &mut self.tuning.window else {
            return false;
        };
        window.refused(why);
        self.tuning.shown = None;
        self.show_tune();
        true
    }
    /// The window closed itself (Done or Escape).
    pub(super) fn close_tune(&mut self) {
        self.tuning = Tuning::default();
    }
    fn tune_command(&mut self, id: RequestId, body: &str, changes: &[(String, f32)]) -> Result<()> {
        ensure!(!changes.is_empty(), "Nothing to change.");
        let mut args = vec![body.to_string()];
        for (path, value) in changes {
            args.push(path.clone());
            args.push(value.to_string());
        }
        let command = Command::Package(bri_sim::session::PackageCommand {
            package: String::new(),
            command: "tune".into(),
            args: args
                .into_iter()
                .map(bri_sim::session::PackageArg::String)
                .collect(),
        });
        self.command(
            id,
            command,
            UiAction::TuneSet {
                body: body.into(),
                changes: changes.to_vec(),
            },
        )
    }
    /// A Tune window request: answered here when it succeeds, by the
    /// caller when it fails.
    pub(super) fn tune_action(&mut self, id: RequestId, action: &UiAction) -> Result<()> {
        if self.tuning.window.is_none() {
            self.ui.apply(UiUpdate::Tune(None));
            self.answer(id, Ok(()));
            return Ok(());
        }
        let archetypes = self
            .network_view()
            .map(|v| v.archetypes.clone())
            .context("Not in a game")?;
        let Some(window) = &mut self.tuning.window else {
            return Ok(());
        };
        match action {
            UiAction::TuneSet { body, changes } if *body == window.body => {
                let changes = changes.clone();
                let body = body.clone();
                // Save straight after writes these too, before the host's
                // tuned body arrives.
                window.sending(&changes);
                self.tune_command(id, &body, &changes)
            }
            UiAction::TuneUndo { body } if *body == window.body => {
                let undo = window.undo(&archetypes);
                let body = body.clone();
                if undo.is_empty() {
                    self.answer(id, Ok(()));
                    return Ok(());
                }
                self.tune_command(id, &body, &undo)
            }
            UiAction::TuneSave { body } if *body == window.body => {
                let result = self.save_tune(&archetypes);
                if let Some(window) = &mut self.tuning.window {
                    window.status = match &result {
                        Ok(file) => format!("Saved to {file}."),
                        Err(error) => format!("Not saved: {error:#}"),
                    };
                }
                self.tuning.shown = None;
                self.show_tune();
                // The status line says how it went.
                self.answer(id, Ok(()));
                Ok(())
            }
            // A window over another body (it was reopened): nothing to do.
            _ => {
                self.answer(id, Ok(()));
                Ok(())
            }
        }
    }
    /// Write the window's changes into the body's file; its name for the
    /// status line.
    fn save_tune(&mut self, archetypes: &bri_sim::archetype::Archetypes) -> Result<String> {
        let window = self
            .tuning
            .window
            .as_ref()
            .context("The Tune window is closed")?;
        let cannot = self.cannot_save(&window.body);
        ensure!(cannot.is_empty(), "{cannot}");
        let catalog = self
            .addons
            .server_packages
            .as_deref()
            .context(NOT_HOSTED_HERE)?;
        let file = crate::tune::archetype_file(&self.content.paths.root, catalog, &window.body)?;
        let body = archetypes
            .find(&window.body)
            .map(|i| archetypes.resolve(i).clone())
            .context("The body left the game")?;
        crate::tune::save(&file, &body, &window.changes(archetypes))?;
        if let Some(window) = &mut self.tuning.window {
            window.saved(archetypes);
        }
        // The game already plays what was saved: not an edit to announce.
        self.addons.edits.restamp();
        let shown = file
            .strip_prefix(&self.content.paths.root)
            .unwrap_or(&file)
            .display()
            .to_string();
        Ok(shown)
    }
}
