//! Where a host keeps what Add-Ons' rules save between games and restarts
//! (`set_host_data`): a roleplay server's money and ranks, Slayer's saved
//! configs. One small JSON file per Add-On in a folder of the host's own
//! (`<state>/addon-data/<add-on>.json`); rules never see a path, and each
//! Add-On reaches only its own file through the engine's keys and limits
//! (64 keys, 256 KiB a value).
//!
//! Saving happens off the game's thread: the newest data of each Add-On
//! waits for a writer thread, which replaces the file whole (written beside
//! it, then renamed over it), so a crash leaves the last complete save.
use bri_sim::session::AddOnData;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

type Data = BTreeMap<String, serde_json::Value>;

#[derive(Default)]
struct Queue {
    /// The newest data of each Add-On not yet on disk.
    waiting: BTreeMap<String, Data>,
    /// The writer is writing this Add-On's data now.
    writing: Option<(String, Data)>,
    closed: bool,
}

struct Shared {
    dir: PathBuf,
    queue: Mutex<Queue>,
    wake: Condvar,
}

/// Add-On data kept in files under one folder.
pub struct AddOnDataFiles {
    shared: Arc<Shared>,
    writer: Option<std::thread::JoinHandle<()>>,
}

/// The file `package`'s data is kept in, or `None` for an id that is not a
/// plain Add-On id (never a path).
fn file(dir: &Path, package: &str) -> Option<PathBuf> {
    bri_package::id::namespace_problem(package)
        .is_none()
        .then(|| dir.join(format!("{package}.json")))
}

impl AddOnDataFiles {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let shared = Arc::new(Shared {
            dir: dir.into(),
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        let writer = std::thread::Builder::new()
            .name("add-on data".into())
            .spawn(move || write_loop(&worker))
            .ok();
        Self { shared, writer }
    }

    /// Wait until everything saved so far is on disk.
    pub fn flush(&self) {
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        while !queue.waiting.is_empty() || queue.writing.is_some() {
            if self.writer.is_none() {
                // No writer thread: write here.
                let waiting = std::mem::take(&mut queue.waiting);
                drop(queue);
                for (package, data) in waiting {
                    write(&self.shared.dir, &package, &data);
                }
                return;
            }
            queue = self
                .shared
                .wake
                .wait(queue)
                .unwrap_or_else(|e| e.into_inner());
        }
    }
}

impl Drop for AddOnDataFiles {
    fn drop(&mut self) {
        self.flush();
        self.shared
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .closed = true;
        self.shared.wake.notify_all();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

fn write_loop(shared: &Shared) {
    let mut queue = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if let Some(package) = queue.waiting.keys().next().cloned() {
            let data = queue.waiting.remove(&package).unwrap_or_default();
            queue.writing = Some((package.clone(), data.clone()));
            drop(queue);
            write(&shared.dir, &package, &data);
            queue = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            queue.writing = None;
            shared.wake.notify_all();
            continue;
        }
        if queue.closed {
            return;
        }
        queue = shared.wake.wait(queue).unwrap_or_else(|e| e.into_inner());
    }
}

fn write(dir: &Path, package: &str, data: &Data) {
    let Some(path) = file(dir, package) else {
        return;
    };
    let result = (|| -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        if data.is_empty() {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => return Ok(()),
            }
        }
        let partial = path.with_extension("json.partial");
        std::fs::write(&partial, serde_json::to_vec_pretty(data)?)?;
        std::fs::rename(&partial, &path)?;
        Ok(())
    })();
    if let Err(error) = result {
        bri_console::warn(format!(
            "Could not save {package}'s Add-On data to {}: {error:#}",
            path.display()
        ));
    }
}

impl AddOnData for AddOnDataFiles {
    fn load(&self, package: &str) -> Data {
        {
            let queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(data) = queue.waiting.get(package) {
                return data.clone();
            }
            if let Some((_, data)) = queue.writing.as_ref().filter(|(p, _)| p == package) {
                return data.clone();
            }
        }
        let Some(path) = file(&self.shared.dir, package) else {
            return Data::new();
        };
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                bri_console::warn(format!(
                    "{} is damaged and was not read: {error}",
                    path.display()
                ));
                Data::new()
            }),
            Err(_) => Data::new(),
        }
    }

    fn save(&self, package: &str, data: &Data) {
        if file(&self.shared.dir, package).is_none() {
            return;
        }
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.waiting.insert(package.to_owned(), data.clone());
        drop(queue);
        self.shared.wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn what_an_add_on_saves_is_there_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let files = AddOnDataFiles::new(dir.path());
            let data = Data::from([("money".into(), json!({ "1234": 250 }))]);
            files.save("roleplay", &data);
            // Read back before it reaches the disk.
            assert_eq!(files.load("roleplay"), data);
        }
        let again = AddOnDataFiles::new(dir.path());
        assert_eq!(again.load("roleplay")["money"], json!({ "1234": 250 }));
        assert!(again.load("economy").is_empty());
    }

    #[test]
    fn an_id_that_is_not_an_add_on_id_reaches_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let files = AddOnDataFiles::new(dir.path().join("data"));
        files.save("../escape", &Data::from([("k".into(), json!(1))]));
        files.flush();
        assert!(!dir.path().join("escape.json").exists());
        assert!(files.load("../escape").is_empty());
    }

    #[test]
    fn forgetting_everything_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let files = AddOnDataFiles::new(dir.path());
        files.save("ranks", &Data::from([("k".into(), json!(1))]));
        files.flush();
        assert!(dir.path().join("ranks.json").exists());
        files.save("ranks", &Data::new());
        files.flush();
        assert!(!dir.path().join("ranks.json").exists());
    }
}
