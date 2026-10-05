//! Live dials: an administrator's `/botset`, `/botreload` and `/botsave`
//! (`docs/architecture/bots.md`, "Tuning"). A dial is any number a kind
//! has, by its path in `bots.json` (`crate::bot_kind::tuning`). A change
//! replaces the kind every brain of it plays by, so bots take it up at
//! their next decision. Overrides are kept in the user's data directory,
//! never the install folder, and the host applies them as it starts.
use super::*;
use crate::bot_kind::tuning::{self, Overrides};
use std::path::PathBuf;
use std::sync::Arc;

/// Reads the shipped kinds again (the enabled Add-Ons' `bots.json`).
pub type BotReload = Arc<dyn Fn() -> Result<Vec<BotKind>> + Send + Sync>;

/// Where a host's bot settings come from and go.
#[derive(Clone, Default)]
pub struct BotTuning {
    /// Reads the shipped kinds for `/botreload`; none keeps those given.
    pub reload: Option<BotReload>,
    /// The user-local override file (`bot-overrides.json`); none keeps
    /// changes for this game only.
    pub overrides: Option<PathBuf>,
}

/// What the session keeps for tuning.
#[derive(Default)]
pub(in crate::session) struct Tuning {
    source: BotTuning,
    /// The kinds before any override.
    shipped: Option<Vec<BotKind>>,
    /// Overrides in effect: the file's, then this game's `/botset`s.
    overrides: Overrides,
}

impl Session {
    /// Where bot settings reload from and overrides are kept. Applies the
    /// override file now, over the kinds given (`set_vehicle_pack`), and
    /// returns what in it no longer fits.
    pub fn set_bot_tuning(&mut self, source: BotTuning) -> Result<Vec<String>> {
        let shipped = self.bots.kinds.clone();
        let overrides = match &source.overrides {
            Some(path) => Overrides::load(path)?,
            None => Overrides::new(),
        };
        self.bots.tuning = Tuning {
            source,
            shipped: Some(shipped.clone()),
            overrides,
        };
        self.apply_bot_overrides(shipped)
    }

    fn apply_bot_overrides(&mut self, shipped: Vec<BotKind>) -> Result<Vec<String>> {
        let (kinds, problems) = self.bots.tuning.overrides.apply(shipped);
        self.set_bot_kinds(kinds)?;
        Ok(problems)
    }

    /// `/botset`, `/botreload` and `/botsave`, typed by `owner`; false
    /// when `command` is none of them.
    pub(in crate::session) fn bot_tuning_command(
        &mut self,
        owner: OwnerId,
        command: &str,
        args: &[PackageArg],
    ) -> bool {
        let command = command.to_ascii_lowercase();
        if !matches!(command.as_str(), "botset" | "botreload" | "botsave") {
            return false;
        }
        if !self.is_administrator(owner) {
            self.private_chat(owner, "Only an administrator can tune bots.".into());
            return true;
        }
        let words: Vec<String> = args
            .iter()
            .map(|a| match a {
                PackageArg::Int(n) => n.to_string(),
                PackageArg::Float(f) => f.to_string(),
                PackageArg::String(s) => s.clone(),
                PackageArg::Bool(b) => b.to_string(),
            })
            .filter(|w| !w.is_empty())
            .collect();
        let lines = match command.as_str() {
            "botset" => self.bot_set(&words),
            "botreload" => self.bot_reload(),
            _ => self.bot_save(),
        };
        let lines = lines.unwrap_or_else(|e| vec![format!("{e:#}")]);
        for line in lines {
            self.private_chat(owner, line);
        }
        true
    }

    /// `/botset <path> [value] [kind id]`: show or set one dial on every
    /// kind that has it (or the one named).
    fn bot_set(&mut self, words: &[String]) -> Result<Vec<String>> {
        let Some(path) = words.first() else {
            let mut out = vec![
                "/botset <dial> [value] [kind]: show or set a bot dial, e.g. /botset surprise.strength 0.6. /botreload reads bots.json and your overrides again; /botsave keeps the overrides.".to_string(),
            ];
            for (kind, dials) in &self.bots.tuning.overrides.kinds {
                for (path, value) in dials {
                    out.push(format!("  {kind} {path} = {value}"));
                }
            }
            return Ok(out);
        };
        let only = words.get(2);
        let ids: Vec<String> = self
            .bots
            .kinds
            .iter()
            .filter(|k| only.is_none_or(|id| &k.id == id))
            .filter(|k| tuning::dial(k, path).is_some() || only.is_some())
            .map(|k| k.id.clone())
            .collect();
        anyhow::ensure!(!ids.is_empty(), "No bot kind has a dial `{path}`.");
        let Some(value) = words.get(1) else {
            return Ok(ids
                .iter()
                .filter_map(|id| self.bots.kind(id))
                .map(|k| match tuning::dial(k, path) {
                    Some(v) => format!("{}: {path} = {}", k.name, short(v)),
                    None => format!("{}: {path} is not set", k.name),
                })
                .collect());
        };
        let value: f64 = value
            .parse()
            .ok()
            .filter(|v: &f64| v.is_finite())
            .with_context(|| format!("`{value}` is not a number."))?;
        let mut changed = Vec::new();
        let mut kinds = self.bots.kinds.clone();
        for id in &ids {
            let Some(kind) = kinds.iter_mut().find(|k| &k.id == id) else {
                continue;
            };
            let was = tuning::dial(kind, path);
            *kind = tuning::with_dial(kind, path, value)?;
            changed.push((id.clone(), kind.name.clone(), was));
        }
        self.set_bot_kinds(kinds)?;
        let mut out = Vec::new();
        for (id, name, was) in changed {
            self.bots.tuning.overrides.set(&id, path, value);
            let was = was.map_or("unset".to_string(), short);
            out.push(format!("{name}: {path} = {} (was {was})", short(value)));
        }
        anyhow::ensure!(
            self.bots.tuning.overrides.len() <= tuning::MAX_OVERRIDES,
            "Too many bot overrides"
        );
        Ok(out)
    }

    /// `/botreload`: the shipped kinds read again, then the override file.
    /// Changes not saved with `/botsave` are dropped.
    fn bot_reload(&mut self) -> Result<Vec<String>> {
        let shipped = match &self.bots.tuning.source.reload {
            Some(reload) => reload()?,
            None => self
                .bots
                .tuning
                .shipped
                .clone()
                .unwrap_or_else(|| self.bots.kinds.clone()),
        };
        for kind in &shipped {
            kind.validate()?;
        }
        self.bots.tuning.overrides = match &self.bots.tuning.source.overrides {
            Some(path) => Overrides::load(path)?,
            None => Overrides::new(),
        };
        self.bots.tuning.shipped = Some(shipped.clone());
        let problems = self.apply_bot_overrides(shipped)?;
        let mut out = vec![format!(
            "Bot settings reloaded: {} kinds, {} overrides.",
            self.bots.kinds.len(),
            self.bots.tuning.overrides.len()
        )];
        out.extend(problems.into_iter().map(|p| format!("  left out: {p}")));
        Ok(out)
    }

    /// `/botsave`: the overrides in effect, written to the user's file.
    fn bot_save(&mut self) -> Result<Vec<String>> {
        let path = self
            .bots
            .tuning
            .source
            .overrides
            .clone()
            .context("This host keeps no bot overrides.")?;
        self.bots.tuning.overrides.save(&path)?;
        Ok(vec![format!(
            "Saved {} bot overrides to {}.",
            self.bots.tuning.overrides.len(),
            path.display()
        )])
    }
}

/// A dial's value as typed: f32 settings without their float noise.
fn short(v: f64) -> String {
    let rounded = (v * 1e4).round() / 1e4;
    format!("{rounded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        use rapier3d::prelude::*;
        let simulation = crate::simulation::Simulation::new(
            bri_world::World::new("Tune".into(), "test".into(), vec![[1.0; 4]]),
            crate::definitions::Definitions::default(),
            vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        )
        .unwrap();
        Session::new(simulation)
    }
    fn kind(id: &str) -> BotKind {
        BotKind {
            id: id.into(),
            name: id.into(),
            ..Default::default()
        }
    }
    fn typed(s: &mut Session, who: OwnerId, seq: u64, line: &str) -> Vec<String> {
        let mut words = line.split_whitespace();
        let command = words.next().unwrap().to_string();
        s.command(
            who,
            seq,
            Command::Package(PackageCommand {
                package: String::new(),
                command,
                args: words.map(|w| PackageArg::String(w.into())).collect(),
            }),
        )
        .unwrap();
        s.private_notices
            .drain(..)
            .filter(|(o, _)| *o == who)
            .filter_map(|(_, n)| match n {
                Notice::Chat(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    /// `/botset` changes a dial on the kinds every brain plays by and says
    /// so; `/botsave` keeps it in the user file; `/botreload` reads the
    /// shipped kinds and that file again. Players cannot tune.
    #[test]
    fn an_administrator_sets_saves_and_reloads_a_dial() {
        let dir = std::env::temp_dir().join(format!("bri-botset-{}", std::process::id()));
        let file = dir.join(tuning::OVERRIDES_FILE);
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = session();
        s.set_bot_kinds(vec![kind("test:bot/a"), kind("test:bot/b")])
            .unwrap();
        // The shipped file, edited between reloads.
        let shipped = Arc::new(std::sync::Mutex::new(30.0f32));
        let source = shipped.clone();
        let problems = s
            .set_bot_tuning(BotTuning {
                reload: Some(Arc::new(move || {
                    let mut a = kind("test:bot/a");
                    a.sight = *source.lock().unwrap();
                    Ok(vec![a, kind("test:bot/b")])
                })),
                overrides: Some(file.clone()),
            })
            .unwrap();
        assert!(problems.is_empty());
        let admin = s
            .join("Admin".into(), Vec3::new(0., 0.05, 0.), true)
            .unwrap();
        let player = s
            .join("Player".into(), Vec3::new(4., 0.05, 0.), false)
            .unwrap();
        s.private_notices.clear();

        let said = typed(&mut s, player, 1, "botset surprise.strength 0.6");
        assert_eq!(said, ["Only an administrator can tune bots."]);
        assert_eq!(s.bots.kinds[0].surprise.strength, 0.5);

        let said = typed(&mut s, admin, 1, "botset surprise.strength 0.6");
        assert_eq!(
            said,
            [
                "test:bot/a: surprise.strength = 0.6 (was 0.5)",
                "test:bot/b: surprise.strength = 0.6 (was 0.5)"
            ]
        );
        assert!(
            s.bots
                .kinds
                .iter()
                .all(|k| (k.surprise.strength - 0.6).abs() < 1e-6)
        );
        let said = typed(&mut s, admin, 2, "botset sight 12 test:bot/b");
        assert_eq!(said, ["test:bot/b: sight = 12 (was 80)"]);
        assert_eq!(
            typed(&mut s, admin, 3, "botset sight"),
            ["test:bot/a: sight = 80", "test:bot/b: sight = 12"]
        );
        let said = typed(&mut s, admin, 4, "botset surprise.strength 3");
        assert!(said[0].contains("cannot be 3"), "{said:?}");
        let said = typed(&mut s, admin, 5, "botset surprise.nothing 1");
        assert_eq!(said, ["No bot kind has a dial `surprise.nothing`."]);

        // Not saved: a reload reads the shipped kinds and the (absent) file.
        *shipped.lock().unwrap() = 50.0;
        let said = typed(&mut s, admin, 6, "botreload");
        assert_eq!(said, ["Bot settings reloaded: 2 kinds, 0 overrides."]);
        assert_eq!(s.bots.kinds[0].sight, 50.0);
        assert_eq!(s.bots.kinds[0].surprise.strength, 0.5);

        typed(&mut s, admin, 7, "botset surprise.strength 0.25 test:bot/a");
        let said = typed(&mut s, admin, 8, "botsave");
        assert!(said[0].starts_with("Saved 1 bot overrides to "), "{said:?}");
        let said = typed(&mut s, admin, 9, "botreload");
        assert_eq!(said, ["Bot settings reloaded: 2 kinds, 1 overrides."]);
        assert!((s.bots.kinds[0].surprise.strength - 0.25).abs() < 1e-6);
        assert_eq!(s.bots.kinds[1].surprise.strength, 0.5);

        // A new host on the same user file starts with the saved value.
        let mut next = session();
        next.set_bot_kinds(vec![kind("test:bot/a")]).unwrap();
        next.set_bot_tuning(BotTuning {
            reload: None,
            overrides: Some(file.clone()),
        })
        .unwrap();
        assert!((next.bots.kinds[0].surprise.strength - 0.25).abs() < 1e-6);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
