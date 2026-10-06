//! Who a bot counts as friend or foe, and whom it sees.
use super::*;

/// Rays one open-spot candidate may cast: the floor, the sky, and room
/// and sky each way round.
const OPEN_RAYS: usize = 18;

impl Session {
    /// Whether `bot` treats `other` as an enemy: anyone it may hurt, except
    /// bots of the same builder, who are on its side. A rules bot plays
    /// as a member, so its game alone says who is on its side (Slayer's
    /// `checkHoleBotTeams`).
    pub(super) fn bot_enemy(&self, bot: OwnerId, kind: &BotKind, other: OwnerId) -> bool {
        if other == bot
            || self.bot_allies(bot, other)
            || !self.peers.get(&other).is_some_and(|p| p.combat.alive)
        {
            return false;
        }
        if self.bots.is_brick_bot(bot)
            && self.bots.is_brick_bot(other)
            && self.bot_team_relation(bot, other).is_none()
            && (self.bot_allies(bot, other)
                || !kind.fights_bots
                    && kind.side.is_none()
                    && self
                        .bots
                        .brains
                        .get(&other)
                        .is_some_and(|b| b.kind.side.is_none()))
        {
            return false;
        }
        self.can_damage_player(bot, other, false)
    }
    /// Explicit same-game teams are the author's policy, including opposition.
    /// Unassigned creatures retain the original builder/species fallback.
    pub(super) fn bot_team_relation(&self, bot: OwnerId, other: OwnerId) -> Option<bool> {
        let a = self
            .minigames
            .player(self.peers.get(&bot)?.combat.player)
            .ok()?;
        let b = self
            .minigames
            .player(self.peers.get(&other)?.combat.player)
            .ok()?;
        (a.game.is_some() && a.game == b.game && a.team.is_some() && b.team.is_some())
            .then(|| self.minigames.allied(a.id, b.id))
    }
    /// Whether two brick bots are on one side: one side (Bot_Hole's
    /// `hType`) never fights itself and fights every other; bots of no side
    /// side with their builder.
    ///
    /// Each one's minigame player is looked up once (`bot_team_relation`
    /// and `game_of` read the same). Minigame teammates count only through
    /// that explicit relation: `minigames.allied` holds only for two players
    /// of one game who both have teams, which the relation has answered.
    pub(super) fn bot_allies(&self, bot: OwnerId, other: OwnerId) -> bool {
        let player = |o: OwnerId| {
            let peer = self.peers.get(&o)?;
            self.minigames.player(peer.combat.player).ok()
        };
        let (a, b) = (player(bot), player(other));
        if let (Some(a), Some(b)) = (a, b)
            && a.game.is_some()
            && a.game == b.game
            && a.team.is_some()
            && b.team.is_some()
        {
            return self.minigames.allied(a.id, b.id);
        }
        if !self.bots.is_brick_bot(bot) || !self.bots.is_brick_bot(other) {
            return false;
        }
        if a.and_then(|a| a.game) != b.and_then(|b| b.game) {
            return false;
        }
        let side = |o: OwnerId| {
            self.bots
                .brains
                .get(&o)
                .and_then(|b| b.kind.side.as_deref())
        };
        match (side(bot), side(other)) {
            (None, None) => self.bot_brick_owner(other) == self.bot_brick_owner(bot),
            (mine, theirs) => mine == theirs,
        }
    }
    pub(super) fn bot_sight(&self, bot: OwnerId, brain: &Brain, eye: Vec3) -> Sight {
        let kind = &brain.kind;
        let visible = |owner: OwnerId, urgency: SightUrgency| -> Option<Seen> {
            let p = self.peers.get(&owner)?;
            if !self.bot_enemy(bot, kind, owner) {
                return None;
            }
            let real = Vec3::from(p.player.state().feet);
            let way = self.bot_sees_player(bot, owner, eye, kind.sight, urgency)?;
            Some(Seen {
                owner,
                eye: way.aim,
                feet: way.seen(real),
                real,
                way,
            })
        };
        // Real injury takes priority over a previously visible bystander.
        // Resolve only through the same authoritative visibility test; when
        // the attacker is unseen during a retained objective, ordinary dated
        // hurt/search evidence must guide pursuit instead of a fresh passive
        // target. Never renew that evidence from an unseen live position.
        let tick = self.simulation.state().tick;
        let valid_threat = |k: &Knowledge| tick < k.expires && self.bot_enemy(bot, kind, k.subject);
        let threat = self
            .bots
            .hurt
            .get(&bot)
            .copied()
            .filter(valid_threat)
            .or(brain.objective_threat.filter(valid_threat));
        if let Some(threat) = threat {
            let target = visible(threat.subject, SightUrgency::Target);
            if target.is_some() || brain.objective.detail().is_some() {
                return Sight { target };
            }
        }
        // Keep fighting the same enemy while it stays in view.
        if let Some(seen) = brain.target.and_then(|t| visible(t, SightUrgency::Target)) {
            return Sight { target: Some(seen) };
        }
        // Through an opening, anyone may be in sight wherever they stand.
        let portals = !self.simulation.passages().list.is_empty();
        // Each enemy in view is its own option: the nearest, but each ally
        // already after one makes it farther (`team` overlap).
        let mut candidates: Vec<(f32, OwnerId)> = self
            .peers
            .iter()
            .filter(|(owner, p)| **owner != bot && p.combat.alive)
            .map(|(owner, p)| (p.player.eye().distance(eye), *owner))
            .filter(|(d, _)| portals || *d < kind.sight)
            .map(|(d, owner)| {
                (
                    d * (1.0 + kind.team.overlap() * self.team_crowd(bot, owner)),
                    owner,
                )
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Sight {
            target: candidates
                .into_iter()
                .find_map(|(_, owner)| visible(owner, SightUrgency::Ordinary)),
        }
    }
    /// Whether a body standing at `feet` has open sky above and room all
    /// round, to fling something: at most `OPEN_RAYS` rays with the floor
    /// looked for.
    pub(super) fn open_at(&self, feet: Vec3) -> bool {
        let clear = |from: Vec3, d: Vec3, length: f32| {
            matches!(self.simulation.target(from, d, length), Ok(None))
        };
        let chest = feet + Vec3::Y * 1.5;
        // Sky over it and over where its catch swings round it.
        clear(chest, Vec3::Y, OPEN_SKY)
            && (0..8).all(|i| {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let out = Vec3::new(a.sin(), 0.0, a.cos());
                clear(chest, out, OPEN_ROOM) && clear(chest + out * OPEN_SWING, Vec3::Y, OPEN_SKY)
            })
    }
    /// The nearest open place around `feet` to throw from, looked for from
    /// candidate `from` on (here first, then rings round it), each paid for
    /// from the shared ray budget: still looking where the budget ran out;
    /// found `None` when it is open here, or nowhere near.
    pub(super) fn bot_open_spot(&self, bot: OwnerId, feet: Vec3, from: usize) -> super::Spot {
        const RINGS: [f32; 4] = [6.0, 12.0, 18.0, 24.0];
        const AROUND: usize = 12;
        for at in from..1 + RINGS.len() * AROUND {
            if !self.bot_spend_rays(bot, OPEN_RAYS) {
                return super::Spot::Looking(at);
            }
            if at == 0 {
                if self.open_at(feet) {
                    return super::Spot::Found(None);
                }
                continue;
            }
            let (ring, i) = (RINGS[(at - 1) / AROUND], (at - 1) % AROUND);
            let a = i as f32 * std::f32::consts::TAU / AROUND as f32;
            let p = feet + Vec3::new(a.sin(), 0.0, a.cos()) * ring;
            // The floor there, looked for from waist height so a roof
            // overhead is not taken for it.
            let stand = match self.simulation.target(p + Vec3::Y * 2.0, -Vec3::Y, 8.0) {
                Ok(Some(hit)) if hit.normal.y > 0.7 => hit.position + Vec3::Y * 0.05,
                Ok(Some(_)) => continue,
                _ => p,
            };
            if self.open_at(stand) {
                return super::Spot::Found(Some(stand));
            }
        }
        super::Spot::Found(None)
    }
}
