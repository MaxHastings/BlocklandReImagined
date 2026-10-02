//! Combat presentation: hugs, hidden bodies, hit feedback.
use super::*;

/// Client-side death, respawn and status presentation derived from vitals.
#[derive(Default)]
pub(super) struct CombatPresentation {
    pub(super) alive: Option<bool>,
    pub(super) health: f32,
    pub(super) countdown: Option<u64>,
    pub(super) died_at: std::collections::BTreeMap<bri_world::OwnerId, std::time::Instant>,
    pub(super) lights: std::collections::BTreeMap<bri_world::OwnerId, bool>,
    /// `/hug` and `/zombie`: `playThread(1, armReadyBoth)` holds until the
    /// arms change again (`Player::updateArm`, `fixArms`, unequip), kept
    /// with the held pose it replaced (`None` until the next frame sees it).
    pub(super) hugging:
        std::collections::BTreeMap<bri_world::OwnerId, Option<crate::avatar::HeldToolPose>>,
    pub(super) minigame_revision: u64,
    pub(super) minigame_state: Option<MiniGameUiState>,
    /// Energy bar fraction last shown, in hundredths.
    pub(super) energy: Option<u8>,
}
impl CombatPresentation {
    pub(super) fn hug_pose(
        &mut self,
        owner: bri_world::OwnerId,
        held: crate::avatar::HeldToolPose,
    ) -> crate::avatar::HeldToolPose {
        match self.hugging.get_mut(&owner) {
            Some(replaced @ None) => *replaced = Some(held),
            Some(Some(replaced)) if *replaced == held => {}
            Some(Some(_)) => {
                self.hugging.remove(&owner);
                return held;
            }
            None => return held,
        }
        crate::avatar::HeldToolPose::Both
    }
    /// Corpses disappear after `$CorpseTimeoutValue` (5 s).
    pub(super) fn hidden_bodies(
        &self,
        vitals: &std::collections::BTreeMap<bri_world::OwnerId, bri_sim::session::Vitals>,
    ) -> std::collections::BTreeSet<bri_world::OwnerId> {
        self.died_at
            .iter()
            .filter(|(owner, at)| {
                vitals.get(owner).is_some_and(|v| !v.alive)
                    && at.elapsed() >= Duration::from_secs(5)
            })
            .map(|(owner, _)| *owner)
            .collect()
    }
}
impl App {
    /// Death prompts, damage flash, light sounds, sit state and the
    /// Mini-Games dialog state, all derived from replicated vitals.
    pub(super) fn update_combat_presentation(&mut self) {
        let Some(a) = self.net.attempt.as_ref().filter(|a| a.entered) else {
            return;
        };
        let session = a.id;
        let Some(view) = a.view.as_ref() else {
            return;
        };
        let sun = self.scene.cpu_scene.as_ref().map(|s| s.sun_color);
        let auto_light = self.ui.core.prefs.bool_or("$pref::Input::AutoLight", true);
        let c = &mut self.combat;
        let mut updates = Vec::new();
        let mut light_on_spawn = false;
        // `showEnergyBar` datablocks show the predicted jet energy.
        let energy = self
            .motion
            .presented()
            .get(&view.owner)
            .filter(|p| view.archetypes.resolve(p.archetype).energy_bar)
            .map(|p| p.energy / view.archetypes.tuning(p.archetype, p.scale).max_energy);
        let shown = energy.map(|e| (e.clamp(0.0, 1.0) * 100.0).round() as u8);
        if shown != c.energy {
            c.energy = shown;
            updates.push(UiUpdate::Energy(energy));
        }
        for (owner, vitals) in &view.vitals {
            if !vitals.alive {
                c.died_at
                    .entry(*owner)
                    .or_insert_with(std::time::Instant::now);
            } else {
                c.died_at.remove(owner);
            }
            let previous = c.lights.insert(*owner, vitals.light);
            if previous.is_some_and(|old| old != vitals.light)
                && let Some(pose) = view.poses.get(owner)
            {
                self.audio.trigger(
                    if vitals.light {
                        "player.light_on"
                    } else {
                        "player.light_off"
                    },
                    bri_audio::Placement::World(pose.player.feet),
                );
            }
        }
        c.died_at.retain(|owner, _| view.vitals.contains_key(owner));
        c.lights.retain(|owner, _| view.vitals.contains_key(owner));
        if let Some(local) = view.vitals.get(&view.owner) {
            if local.alive {
                if c.alive == Some(false) {
                    updates.push(UiUpdate::ClearPrints);
                }
                if c.alive != Some(true) {
                    light_on_spawn = auto_light && sun.is_some_and(dark_sun);
                }
                if local.health < c.health && c.alive == Some(true) {
                    // Armor::onDamage: flash += delta / maxDamage * 2.
                    let max = view
                        .poses
                        .get(&view.owner)
                        .map_or(bri_sim::session::MAX_HEALTH, |p| {
                            view.archetypes.resolve(p.player.archetype).max_health
                        });
                    updates.push(UiUpdate::DamageFlash((c.health - local.health) / max * 2.0));
                }
                c.countdown = None;
            } else {
                if c.alive == Some(true) {
                    updates.push(UiUpdate::DamageFlash(0.75));
                }
                // handleYourDeath / respawnCountDownTick. A rule holding the
                // respawn (out of lives) prints its own message instead.
                let remaining = if local.respawn_held {
                    u64::MAX
                } else {
                    local.respawn_tick.saturating_sub(view.tick).div_ceil(120)
                };
                if remaining == u64::MAX {
                    c.countdown = Some(remaining);
                } else if c.countdown != Some(remaining) {
                    c.countdown = Some(remaining);
                    updates.push(UiUpdate::CenterPrint {
                        text: match remaining {
                            0 => "\u{E005}Click to respawn.".into(),
                            1 => "\u{E005}Respawning in 1 second...".into(),
                            n => format!("\u{E005}Respawning in {n} seconds..."),
                        },
                        seconds: if remaining == 0 { 300.0 } else { 2.0 },
                    });
                }
            }
            c.alive = Some(local.alive);
            c.health = local.health;
        }
        let state = crate::minigame_ui::state(
            view.owner,
            &view.minigames,
            &view.vitals,
            &view.names,
            &self.content.weapons.item_choices,
            &view.archetypes,
            c.minigame_revision,
        );
        let rank = crate::minigame_ui::Rank {
            admin: view.administrator,
            super_admin: view
                .admin_snapshot
                .as_ref()
                .is_some_and(|s| s.role == bri_admin::Role::SuperAdmin || s.local_host),
            host: view.admin_snapshot.as_ref().is_some_and(|s| s.local_host),
            trust: a
                .trust
                .iter()
                .map(|(owner, t)| {
                    let level = match t.level {
                        bri_sim::session::TrustLevel::You => 3,
                        bri_sim::session::TrustLevel::Full | bri_sim::session::TrustLevel::Lan => 2,
                        bri_sim::session::TrustLevel::Build => 1,
                        bri_sim::session::TrustLevel::None => 0,
                    };
                    (*owner, level)
                })
                .chain(std::iter::once((view.owner, 3)))
                .collect(),
        };
        let state = crate::minigame_ui::with_addon_settings(
            state,
            &view.minigames,
            &view.addon_settings,
            view.owner,
            &rank,
            view.addon_teams_shown_when.as_ref(),
            &view.world.palette,
        );
        let changed = c.minigame_state.as_ref().is_none_or(|old| {
            MiniGameUiState {
                revision: 0,
                ..old.clone()
            } != MiniGameUiState {
                revision: 0,
                ..state.clone()
            }
        });
        if changed {
            c.minigame_revision += 1;
            let state = MiniGameUiState {
                revision: c.minigame_revision,
                ..state
            };
            c.minigame_state = Some(state.clone());
            updates.push(UiUpdate::MiniGames(state));
        }
        for update in updates {
            self.ui.apply_session(session, update);
        }
        if light_on_spawn {
            self.ui.core.game(GameAction::UseLight);
        }
    }
}
