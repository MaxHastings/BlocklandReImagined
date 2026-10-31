//! Connection identity and authorized conversation precede a physical body.
use super::*;
use bri_minigames::GameId;
use bri_package_runtime::script::ConnectionView;

impl Session {
    /// Retained membership was issued by this world's authoritative game rules.
    /// Owner reuse is admitted only through verified identity/resume proof.
    pub(super) fn retained_connection_game(&self, owner: OwnerId) -> Option<GameId> {
        self.last_membership
            .get(&owner)
            .copied()
            .flatten()
            .filter(|game| self.minigames.game(*game).is_ok())
    }
    fn pending_connection_game(&self, owner: OwnerId) -> Option<GameId> {
        self.retained_connection_game(owner)
            .or_else(|| self.minigames.server_game())
            .or_else(|| self.minigames.default_game())
    }
    /// No body, team slot, readiness or gameplay permission is created here.
    pub fn connection(&self, owner: OwnerId) -> Option<ConnectionView> {
        if let Some(peer) = self.peers.get(&owner) {
            if self.bots.is_bot(owner) {
                return None;
            }
            return Some(ConnectionView {
                id: owner,
                name: peer.name.clone(),
                loading: false,
                minigame: self.game_of(owner).map(|g| g.0),
                team: self.minigames.team_of(peer.combat.player).map(|t| t.0),
                alive: Some(peer.combat.alive),
                position: self.voice_origin(owner),
            });
        }
        let arrival = self.arrivals.get(&owner)?;
        Some(ConnectionView {
            id: owner,
            name: arrival.name.clone(),
            loading: true,
            minigame: self.pending_connection_game(owner).map(|g| g.0),
            team: None,
            alive: None,
            position: None,
        })
    }
    pub(super) fn conversation_recipients(&self, game: GameId) -> Vec<OwnerId> {
        let mut recipients: BTreeSet<_> = self
            .minigames
            .game(game)
            .map(|g| g.members.iter().filter_map(|m| self.owner_of(*m)).collect())
            .unwrap_or_default();
        recipients.extend(
            self.arrivals
                .keys()
                .copied()
                .filter(|owner| self.pending_connection_game(*owner) == Some(game)),
        );
        recipients.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    use bri_package_runtime::Catalog;

    fn session() -> Result<Session> {
        let mut s = Session::new(crate::simulation::Simulation::new(
            bri_world::World::new("Connections".into(), "test".into(), vec![[1.; 4]; 2]),
            crate::testing::definitions(),
            vec![],
        )?);
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        let catalog = Catalog::load(
            &root,
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: "voice_chat".into(),
                    version: "1.0.0".into(),
                    dir: "voice_chat".into(),
                    side: Side::Server,
                    role: None,
                }],
            },
            true,
        )
        .map_err(|diagnostics| anyhow::anyhow!("{diagnostics:?}"))?;
        s.install_packages(Arc::new(catalog), None)?;
        Ok(s)
    }
    fn create(s: &mut Session, owner: OwnerId, color: u8, private: bool) -> Result<GameId> {
        let actor = s.peers[&owner].combat.player;
        let effects = s.minigames.execute(bri_minigames::Command::Create {
            actor,
            color,
            settings: bri_minigames::Settings {
                invite_only: private,
                ..Default::default()
            },
        })?;
        s.apply_minigame_effects(effects)?;
        Ok(s.game_of(owner).unwrap())
    }
    fn default(s: &mut Session, game: Option<GameId>) -> Result<()> {
        let effects = s.minigames.set_default_game(game)?;
        s.apply_minigame_effects(effects)
    }
    fn addressed(s: &mut Session, owner: OwnerId, text: &str) -> bool {
        s.take_private_notices()
            .iter()
            .any(|(to, n)| *to == owner && matches!(n, Notice::Chat(line) if line == text))
    }

    #[test]
    fn accepted_destination_has_no_body_or_team_and_only_receives_its_game() -> Result<()> {
        let mut s = session()?;
        let a = s.join("A".into(), Vec3::new(0., 10., 0.), true)?;
        let b = s.join("B".into(), Vec3::new(5., 10., 0.), false)?;
        let first = create(&mut s, a, 0, false)?;
        let private = create(&mut s, b, 1, true)?;
        default(&mut s, Some(first))?;
        s.chat_game(Some(first), None, "before admission".into());
        let loading = s.arrive("Loading".into(), &Clan::default(), false, None)?;
        assert!(!addressed(&mut s, loading, "before admission"));
        let view = s.connection(loading).unwrap();
        assert!(view.loading);
        assert_eq!(view.minigame, Some(first.0));
        assert_eq!((view.alive, view.team, view.position), (None, None, None));
        assert!(!s.peers.contains_key(&loading));
        assert!(!s.vitals().contains_key(&loading));
        assert_eq!(s.minigames.players().count(), 2);
        assert_eq!(s.minigames.game(first)?.members.len(), 1);
        assert!(s.command(loading, 1, Command::Suicide).is_err());
        let routes = s.voice_routes(&[a, b, loading]);
        assert!(!routes[&loading][&a].spatial);
        assert!(!routes[&a][&loading].spatial);
        assert!(!routes.get(&loading).is_some_and(|r| r.contains_key(&b)));
        s.chat_game(Some(first), None, "in first".into());
        assert!(addressed(&mut s, loading, "in first"));
        s.chat_game(Some(private), None, "private unrelated".into());
        assert!(!addressed(&mut s, loading, "private unrelated"));
        // A dead private-channel speaker cannot use loading as a spectator backdoor.
        s.kill(a, Some(a), super::super::combat::DamageKind::Suicide, None)?;
        assert!(
            !s.voice_routes(&[a, loading])
                .get(&loading)
                .is_some_and(|r| r.contains_key(&a))
        );
        s.command(
            a,
            1,
            Command::MiniGame(MiniGameRequest::AddOnSettings {
                game: first.0,
                settings: vec![SettingEdit {
                    key: "voice_chat:mode".into(),
                    value: Some(bri_package::setting::SettingValue::Text("global".into())),
                }],
                teams: None,
                quiet: true,
                reset: false,
            }),
        )?;
        assert!(s.voice_routes(&[a, loading])[&loading].contains_key(&a));
        // Host-selected default is an explicit auto-entry grant, including a
        // private default; arbitrary private games above remain inaccessible.
        default(&mut s, Some(private))?;
        s.chat_game(Some(first), None, "old destination".into());
        assert!(!addressed(&mut s, loading, "old destination"));
        s.chat_game(Some(private), None, "new destination".into());
        assert!(addressed(&mut s, loading, "new destination"));
        s.enter_arrival(loading, Vec3::new(10., 10., 0.))?;
        assert_eq!(s.game_of(loading), Some(private));
        let next = s.arrive("Next".into(), &Clan::default(), false, None)?;
        default(&mut s, None)?;
        assert_eq!(s.connection(next).unwrap().minigame, None);
        s.chat_game(Some(private), None, "after revocation".into());
        assert!(!addressed(&mut s, next, "after revocation"));
        Ok(())
    }

    #[test]
    fn retained_entry_requires_owner_proof_and_does_not_cross_map_identity() -> Result<()> {
        let mut s = session()?;
        let a = s.join("Owner".into(), Vec3::new(0., 10., 0.), true)?;
        let principal = bri_admin::Principal([19; 32]);
        let b = s.join_verified(
            "Member".into(),
            Vec3::new(5., 10., 0.),
            false,
            Some(principal),
        )?;
        let game = create(&mut s, a, 0, true)?;
        let player = s.peers[&b].combat.player;
        let effects = s.minigames.host_place(player, Some(game))?;
        s.apply_minigame_effects(effects)?;
        s.disconnect(b)?;
        let anonymous = s.arrive("Member".into(), &Clan::default(), false, None)?;
        assert_ne!(anonymous, b);
        assert_eq!(s.connection(anonymous).unwrap().minigame, None);
        let returning = s.arrive("Member".into(), &Clan::default(), false, Some(principal))?;
        assert_eq!(returning, b);
        assert_eq!(s.connection(b).unwrap().minigame, Some(game.0));
        assert_eq!(s.minigames.game(game)?.members.len(), 1);
        s.enter_arrival(b, Vec3::new(5., 10., 0.))?;
        assert_eq!(s.game_of(b), Some(game));
        // An explicit game kick revokes the retained association.
        let actor = s.peers[&a].combat.player;
        let target = s.peers[&b].combat.player;
        let effects = s
            .minigames
            .execute(bri_minigames::Command::Kick { actor, target })?;
        s.apply_minigame_effects(effects)?;
        s.disconnect(b)?;
        s.arrive("Member".into(), &Clan::default(), false, Some(principal))?;
        assert_eq!(s.connection(b).unwrap().minigame, None);
        s.enter_arrival(b, Vec3::new(5., 10., 0.))?;
        // Issue a fresh legitimate membership before checking world-ID reuse.
        let effects = s
            .minigames
            .host_place(s.peers[&b].combat.player, Some(game))?;
        s.apply_minigame_effects(effects)?;
        s.disconnect(b)?;
        let mut next = session()?;
        let reused = next.minigames.host_create_shared(
            0,
            bri_minigames::Settings {
                invite_only: true,
                ..Default::default()
            },
        )?;
        assert_eq!(reused, game);
        next.adopt(s, a)?;
        next.arrive("Member".into(), &Clan::default(), false, Some(principal))?;
        assert_eq!(
            next.connection(b).unwrap().minigame,
            None,
            "a new world's reused game ID is not an old membership grant"
        );
        Ok(())
    }
}
