//! `/tune`: the host changes an Add-On body's numbers while the game runs
//! ([`crate::tuning`]). The body is rebuilt from its archetype file's JSON
//! with the change, by the same code that loaded it, and replaces the old
//! one in the table: players of it carry on with the new numbers, and every
//! client gets the tuned body ([`Session::tuned_archetypes`]).
use super::*;
use crate::archetype::{Archetype, ArchetypeId};

/// How the host's private answer to a refused `/tune` change begins, so
/// the host's Tune window can say it where the change was made.
pub const NOT_TUNED: &str = "Not tuned:";

/// The tuned bodies: each one's archetype file as tuned so far, and a count
/// that rises with every change, for the host loop to send them.
#[derive(Default)]
pub(super) struct Tuned {
    defs: BTreeMap<String, serde_json::Value>,
    revision: u64,
}

impl Session {
    /// Apply `changes` (field path, value) to the Add-On body `id`. Only the
    /// host may: the Add-On's files are on the host's computer, and the
    /// Tune window saves them there. All the changes apply, or none.
    pub fn tune_archetype(
        &mut self,
        owner: OwnerId,
        id: &str,
        changes: &[(String, f32)],
    ) -> Result<()> {
        ensure!(
            self.admin.rank(owner).1,
            "Only the host tunes bodies: the Add-On's files are on the host's computer"
        );
        ensure!(
            !changes.is_empty() && changes.len() <= crate::tuning::MAX_CHANGES,
            "A tune changes 1 to {} numbers",
            crate::tuning::MAX_CHANGES
        );
        let index = self
            .archetypes
            .find(id)
            .with_context(|| format!("No body is called {id}"))?;
        let catalog = self
            .packages
            .as_ref()
            .map(|p| p.catalog.clone())
            .context("Only an Add-On's own bodies can be tuned, and this game runs none")?;
        let id = self.archetypes.resolve(index).id.clone();
        let (_, def) = catalog
            .archetypes()
            .find(|(i, _)| **i == id)
            .with_context(|| format!("{id} is not an Add-On's own body"))?;
        ensure!(
            def.adjusts.is_none(),
            "{id} changes a v20 player type in place; tune it in its file"
        );
        let old = self.archetypes.resolve(index).clone();
        let mut json = match self.tuned.defs.get(&id) {
            Some(json) => json.clone(),
            None => serde_json::to_value(def).context("Reading the archetype")?,
        };
        for (path, value) in changes {
            crate::tuning::set(&mut json, &old, path, *value)?;
        }
        let def: bri_package_runtime::content::ArchetypeDef =
            serde_json::from_value(json.clone()).context("The tuned archetype")?;
        def.validate()?;
        let mut tuned = packages::archetype(&self.archetypes, &id, &def, &catalog)?;
        // The same module: keep the controller already bound to it.
        tuned.movement.controller = old.movement.controller.clone();
        self.archetypes.replace(index, tuned)?;
        if let Err(error) = self
            .fill_body_mount_points()
            .and_then(|()| self.retune_players(index))
        {
            self.archetypes.replace(index, old)?;
            self.retune_players(index)?;
            return Err(error);
        }
        self.tuned.defs.insert(id, json);
        self.tuned.revision += 1;
        Ok(())
    }
    /// Give every living player of body `index` its numbers as the table
    /// has them now: a player keeps the motor constants it spawned with
    /// ([`Self::set_player_archetype`]), so without this a tuned speed or
    /// size would wait for their next spawn.
    fn retune_players(&mut self, index: ArchetypeId) -> Result<()> {
        let players: Vec<OwnerId> = self
            .peers
            .iter()
            .filter(|(_, p)| p.player.state().archetype == index)
            .map(|(owner, _)| *owner)
            .collect();
        for owner in players {
            self.set_player_archetype(owner, index)?;
        }
        Ok(())
    }
    /// The motor numbers `owner` moves by now (their body's, at their scale).
    pub fn player_tuning(&self, owner: OwnerId) -> Option<&crate::player::PlayerTuning> {
        self.peers.get(&owner).map(|p| p.player.tuning())
    }
    /// The archetype file of each tuned body, as tuned so far.
    pub fn tuned_archetype_files(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.tuned.defs
    }
    /// Rises with every tune, so the host loop sends tuned bodies only when
    /// one changed.
    pub fn tuned_revision(&self) -> u64 {
        self.tuned.revision
    }
    /// The tuned bodies, for clients: each without its controller's module,
    /// which clients already have and keep ([`apply_tuned`]).
    pub fn tuned_archetypes(&self) -> Vec<(ArchetypeId, Archetype)> {
        self.tuned
            .defs
            .keys()
            .filter_map(|id| {
                let index = self.archetypes.find(id)?;
                let mut a = self.archetypes.resolve(index).clone();
                if let Some(c) = &mut a.controller {
                    c.module.clear();
                }
                Some((index, a))
            })
            .collect()
    }
    /// `/tune <body> <path> <value> [<path> <value>...]`.
    pub(super) fn tune_command(&mut self, owner: OwnerId, words: &[String]) {
        let text = match words {
            [] | [_] => {
                "Type /tune in chat to open the Tune window, or /tune <body> <field> <value>."
                    .to_string()
            }
            [body, rest @ ..] if rest.len() % 2 == 0 => {
                let changes: Result<Vec<(String, f32)>> = rest
                    .chunks(2)
                    .map(|pair| {
                        let value: f32 = pair[1]
                            .parse()
                            .with_context(|| format!("{} is not a number", pair[1]))?;
                        Ok((pair[0].clone(), value))
                    })
                    .collect();
                match changes.and_then(|c| self.tune_archetype(owner, body, &c)) {
                    Ok(()) => return,
                    Err(error) => format!("{NOT_TUNED} {error:#}"),
                }
            }
            _ => "Each field needs a value: /tune <body> <field> <value>.".to_string(),
        };
        self.private_chat(owner, text);
    }
}

/// Put the host's tuned bodies (`Session::tuned_archetypes`) into a client's
/// table, keeping each body's controller module and its bound controller.
pub fn apply_tuned(
    table: &mut crate::archetype::Archetypes,
    tuned: Vec<(ArchetypeId, Archetype)>,
) -> Result<()> {
    for (index, mut a) in tuned {
        let old = table
            .get(index)
            .with_context(|| format!("No archetype {}", index.0))?;
        ensure!(
            old.id == a.id,
            "Archetype {} cannot become {}",
            old.id,
            a.id
        );
        a.controller.clone_from(&old.controller);
        a.movement.controller = old.movement.controller.clone();
        table.replace(index, a)?;
    }
    Ok(())
}
