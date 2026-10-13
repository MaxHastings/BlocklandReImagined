//! Edits to Add-On files while a hosted game runs. A game keeps the copy of
//! each Add-On it started with, so a modder who edits a file and switches
//! back to the game would otherwise see nothing change and not know why.
//! Each enabled Add-On folder's [`Stamp`] is taken as the game starts and
//! again whenever the game window regains focus (the moment someone comes
//! back from an editor), on a worker thread, never per frame. An Add-On
//! whose stamp changed is named once in chat and marked in the Add-Ons
//! screen until the next game.
use bri_package::classic::{Stamp, stamp};
use bri_package::packages::PackageSet;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Each enabled Add-On's display name and folder stamp, by id.
type Stamps = BTreeMap<String, (String, Option<Stamp>)>;

/// The enabled Add-Ons under `root` (not the base game's parts) and their
/// folders' stamps right now.
pub fn stamps(root: &Path) -> Stamps {
    let Ok(set) = PackageSet::load_root(root) else {
        return Stamps::new();
    };
    set.packages
        .iter()
        .filter(|p| p.role.is_none())
        .map(|p| {
            let dir = root.join(&p.dir);
            let name = bri_package::library::add_on_label(&dir, &p.id);
            (p.id.clone(), (name, stamp(&dir)))
        })
        .collect()
}

#[derive(Default)]
pub struct AddOnEdits {
    /// The content root and the stamps the running game started with.
    started: Option<(PathBuf, Stamps)>,
    /// A stamping in progress: the first one (the game's start) or a check.
    job: Option<(bool, mpsc::Receiver<Stamps>)>,
    /// Ids changed since the game started, in the order noticed.
    changed: Vec<String>,
}

impl AddOnEdits {
    /// A hosted game started with the Add-Ons under `root` as they are now.
    pub fn game_started(&mut self, root: &Path) {
        self.changed.clear();
        self.started = Some((root.to_path_buf(), Stamps::new()));
        self.spawn(true);
    }
    /// The game ended: edits no longer need saying.
    pub fn game_ended(&mut self) {
        *self = Self::default();
    }
    /// This game's own save wrote an Add-On's file (the Tune window): take
    /// the stamps again, so the game, which already plays it, does not
    /// announce it as an edit. Edits already noticed stay noticed.
    pub fn restamp(&mut self) {
        if self.started.is_some() {
            self.spawn(true);
        }
    }
    /// Look again (the window regained focus).
    pub fn check(&mut self) {
        if self.started.is_some() && self.job.is_none() {
            self.spawn(false);
        }
    }
    fn spawn(&mut self, first: bool) {
        let Some((root, _)) = &self.started else {
            return;
        };
        let root = root.clone();
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("add-on-edits".into())
            .spawn(move || {
                let _ = tx.send(stamps(&root));
            });
        if spawned.is_ok() {
            self.job = Some((first, rx));
        }
    }
    /// Take a finished stamping: the names of Add-Ons newly seen changed.
    pub fn poll(&mut self) -> Vec<String> {
        let Some((first, rx)) = &self.job else {
            return Vec::new();
        };
        let first = *first;
        let now = match rx.try_recv() {
            Ok(now) => now,
            Err(mpsc::TryRecvError::Empty) => return Vec::new(),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.job = None;
                return Vec::new();
            }
        };
        self.job = None;
        let Some((_, started)) = &mut self.started else {
            return Vec::new();
        };
        if first {
            *started = now;
            return Vec::new();
        }
        let mut named = Vec::new();
        for (id, (name, stamp)) in &now {
            let edited = started.get(id).is_some_and(|(_, then)| then != stamp);
            if edited && !self.changed.contains(id) {
                self.changed.push(id.clone());
                named.push(name.clone());
            }
        }
        named
    }
    /// Ids of the Add-Ons edited since the game started.
    pub fn changed(&self) -> &[String] {
        &self.changed
    }
    /// What chat says when `names` were edited mid-game.
    pub fn notice(names: &[String]) -> String {
        let list = match names {
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
            [] => String::new(),
        };
        format!(
            "{list} changed on disk. This game keeps the copy it started with: host again to play your edits."
        )
    }
    /// Ids edited since the game started, as a set.
    pub fn changed_set(&self) -> BTreeSet<&str> {
        self.changed.iter().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wait(edits: &mut AddOnEdits) -> Vec<String> {
        for _ in 0..500 {
            if edits.job.is_none() {
                return Vec::new();
            }
            let named = edits.poll();
            if edits.job.is_none() {
                return named;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("stamping never finished");
    }

    #[test]
    fn an_add_on_edited_mid_game_is_named_once_and_cleared_by_the_next_game() {
        let root = std::env::temp_dir().join(format!("bri-add-on-edits-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("addons/runner");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("package.json"),
            json!({"schema_version": 1, "id": "runner", "version": "1.0.0", "api": 1,
                "name": "Runner", "license": "CC0-1.0", "provides": []})
            .to_string(),
        )
        .unwrap();
        std::fs::write(dir.join("runner.json"), b"{\"movement\":{\"forward\":9}}").unwrap();
        std::fs::write(
            root.join("packages.json"),
            json!({"schema_version": 1, "packages": [
                {"id": "runner", "version": "1.0.0", "side": "server", "dir": "addons/runner"}]})
            .to_string(),
        )
        .unwrap();
        let mut edits = AddOnEdits::default();
        edits.check();
        assert!(edits.job.is_none(), "nothing to check outside a game");
        edits.game_started(&root);
        assert!(wait(&mut edits).is_empty());
        edits.check();
        assert!(wait(&mut edits).is_empty(), "nothing edited yet");
        std::fs::write(dir.join("runner.json"), b"{\"movement\":{\"forward\":12}}").unwrap();
        edits.check();
        assert_eq!(wait(&mut edits), ["Runner"]);
        assert_eq!(edits.changed(), ["runner"]);
        edits.check();
        assert!(wait(&mut edits).is_empty(), "said once");
        assert!(AddOnEdits::notice(&["Runner".into()]).contains("host again"));
        edits.game_ended();
        assert!(edits.changed().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
