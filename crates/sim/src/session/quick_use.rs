//! Quick use (`quick_use`): a carried item used once without switching to
//! it for good. It comes out, its trigger is pulled until its image leaves
//! the state it was pulled in (it fired, threw or began a charge, which
//! letting go then releases), and once that image is back where it may be
//! changed, or gone (the last grenade thrown), the item held before comes
//! back. An Add-On binds it to a key of its own (`binds.json`), as many
//! games throw a grenade or take a quick heal without changing weapon.
use super::*;
use bri_weapons::ActorId;

/// Ticks a quick use may take before the held item is put back anyway.
const QUICK_USE_TICKS: u64 = 10 * bri_world::TICKS_PER_SECOND;

#[derive(Debug, Clone)]
pub(super) struct QuickUse {
    /// The slot held before, put back after.
    back: Option<usize>,
    /// The item's image, once it is in hand.
    image: String,
    /// The image state the trigger was pulled in, once pulled.
    pulled: Option<String>,
    released: bool,
    started: u64,
}

impl Session {
    pub(super) fn quick_use(&mut self, owner: OwnerId, item: &str) -> Result<()> {
        ensure!(
            self.peers.get(&owner).is_some_and(|p| p.combat.alive),
            "Only living players use items"
        );
        ensure!(
            !self.quick_uses.contains_key(&owner),
            "Already using an item"
        );
        let actor = self
            .weapons
            .actor(ActorId(owner))
            .context("Missing inventory")?;
        let back = actor.selected;
        let slot = actor
            .inventory
            .iter()
            .position(|s| s.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(item)))
            .with_context(|| format!("They carry no {item}"))?;
        let image = self
            .weapons
            .pack
            .items
            .get(&actor.inventory[slot].clone().unwrap_or_default())
            .map(|i| i.image.clone())
            .context("Unknown item")?;
        self.equip_tool(owner, Some(slot))?;
        self.quick_uses.insert(
            owner,
            QuickUse {
                back: back.filter(|b| *b != slot),
                image,
                pulled: None,
                released: false,
                started: self.simulation.state().tick,
            },
        );
        Ok(())
    }

    /// Drive each quick use a step: pull, let go, put the old item back.
    pub(super) fn step_quick_uses(&mut self) -> Result<()> {
        if self.quick_uses.is_empty() {
            return Ok(());
        }
        let tick = self.simulation.state().tick;
        let owners: Vec<OwnerId> = self.quick_uses.keys().copied().collect();
        for owner in owners {
            let Some(peer) = self.peers.get(&owner) else {
                self.quick_uses.remove(&owner);
                continue;
            };
            if !peer.combat.alive {
                self.quick_uses.remove(&owner);
                continue;
            }
            let direction = peer.player.state().forward();
            let mut q = self.quick_uses.remove(&owner).unwrap();
            let held = self
                .weapons
                .image_state(ActorId(owner), 0)
                .filter(|(image, _)| image.id.eq_ignore_ascii_case(&q.image))
                .map(|(_, state)| (state.name.clone(), state.allow_change, state.down.is_some()));
            let late = tick.saturating_sub(q.started) > QUICK_USE_TICKS;
            match (&q.pulled, held) {
                // Out and ready: pull.
                (None, Some((state, _, true))) if !late => {
                    self.weapon_trigger(owner, true, direction, false)?;
                    q.pulled = Some(state);
                }
                // Not out, or not ready, yet.
                (None, _) if !late => {}
                // Pulled: let go once its image moved on.
                (Some(pulled), Some((state, allow, _))) if !late => {
                    if !q.released && state != *pulled {
                        self.weapon_trigger(owner, false, direction, false)?;
                        q.released = true;
                    }
                    if !(q.released && allow && state == *pulled) {
                        self.quick_uses.insert(owner, q);
                        continue;
                    }
                    self.equip_tool(owner, q.back)?;
                    continue;
                }
                // Gone from the hand (used up), too late, or never ready.
                _ => {
                    if q.pulled.is_some() && !q.released {
                        self.weapon_trigger(owner, false, direction, false)?;
                    }
                    self.equip_tool(owner, q.back)?;
                    continue;
                }
            }
            self.quick_uses.insert(owner, q);
        }
        Ok(())
    }
}
