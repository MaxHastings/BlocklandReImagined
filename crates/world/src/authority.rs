use crate::*;
use anyhow::{Context, Result, ensure};

/// v20 `$TrustLevel` values between a player and a brick group.
pub mod trust {
    pub const NONE: u8 = 0;
    /// Build on their bricks, use the wrench on them, ride their vehicles.
    pub const BUILD: u8 = 1;
    /// Also paint, print, hammer, undo and edit events.
    pub const FULL: u8 = 2;
    /// The same brick group.
    pub const YOU: u8 = 3;
}

/// Which brick groups trust an actor (v20 `getTrustLevel`).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Trust {
    /// Only the actor's own bricks and public bricks.
    #[default]
    OwnerOnly,
    /// `$Server::LAN`: everyone is trusted as if the bricks were their own.
    Everyone,
    /// Mutual trust levels with other owners; the actor's other owner IDs
    /// (earlier sessions of the same identity) are listed as `YOU`.
    Levels(std::sync::Arc<std::collections::BTreeMap<OwnerId, u8>>),
}

/// This context is constructed by the server, never trusted from a command packet.
#[derive(Debug, Clone, Default)]
pub struct Actor {
    pub owner: OwnerId,
    pub administrator: bool,
    pub trust: Trust,
}
impl Actor {
    /// v20 `getTrustLevel(actor, brick group)`.
    pub fn trust_level(&self, group: OwnerId) -> u8 {
        if group == self.owner && group != 0 {
            return trust::YOU;
        }
        match &self.trust {
            Trust::Everyone => trust::YOU,
            // Public-domain bricks are fully trusted.
            _ if group == 0 => trust::FULL,
            Trust::OwnerOnly => trust::NONE,
            Trust::Levels(levels) => levels.get(&group).copied().unwrap_or(trust::NONE),
        }
    }
    /// Administrators may always edit; others need this much trust.
    pub fn trusted(&self, group: OwnerId, level: u8) -> bool {
        self.administrator || self.trust_level(group) >= level
    }
}
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Edit {
    Color(u8),
    ColorEffect(u8),
    ShapeEffect(u8),
    Print(Option<ContentRef>),
    Name(Option<String>),
    Events(Vec<EventRow>),
    Properties(WrenchProperties),
}
/// Native, atomic subset of the ordinary brick wrench. Asset IDs are resolved
/// against the server's tool catalog before reaching this authority boundary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WrenchProperties {
    pub name: Option<String>,
    pub light: Option<String>,
    pub emitter: Option<String>,
    pub emitter_direction: u8,
    #[serde(default)]
    pub item_spawn: ItemSpawn,
    /// Music/sound brick loop id.
    #[serde(default)]
    pub sound: Option<String>,
    /// Vehicle spawn brick vehicle id and recolor flag.
    #[serde(default)]
    pub vehicle: Option<String>,
    #[serde(default)]
    pub recolor_vehicle: bool,
    pub raycast: bool,
    pub colliding: bool,
    pub visible: bool,
}
pub struct Authority {
    world: World,
}
impl Authority {
    pub fn new(world: World) -> Result<Self> {
        world.validate()?;
        Ok(Self { world })
    }
    pub fn state(&self) -> &World {
        &self.world
    }
    /// Record who an owner number belongs to, or refresh their last known
    /// name. A principal keeps one number per world.
    pub fn claim_owner(&mut self, owner: OwnerId, record: OwnerRecord) -> Result<()> {
        record.validate()?;
        ensure!(owner != 0, "Owner zero is world-owned");
        match self.world.owner_of(&record.principal) {
            Some(existing) => ensure!(existing == owner, "Principal already owns another number"),
            None => ensure!(
                !self.world.owners.contains_key(&owner) && self.world.owners.len() < MAX_OWNERS,
                "Owner number is taken or the owner table is full"
            ),
        }
        self.world.owners.insert(owner, record);
        Ok(())
    }
    /// Keep bricks this server cannot place (no definition) with the world,
    /// so saving writes them back. `palette` is the load's merged colorset,
    /// which may extend the world's.
    pub fn keep_unloaded(&mut self, palette: &[[f32; 4]], bricks: Vec<Brick>) -> Result<()> {
        ensure!(
            palette.starts_with(&self.world.palette) && palette.len() <= 256,
            "The colorset changed while loading"
        );
        ensure!(
            self.world.bricks.len() + self.world.unloaded.len() + bricks.len() <= MAX_BRICKS,
            "Loaded build exceeds world brick limit"
        );
        for brick in &bricks {
            brick.validate(palette.len())?;
        }
        self.world.palette = palette.to_vec();
        self.world.unloaded.extend(bricks);
        Ok(())
    }
    /// Commit only a plan validated against the same world revision. Physics
    /// adapters must preflight all geometry before this infallible publication.
    pub fn load_build(
        &mut self,
        actor: &Actor,
        plan: crate::build::LoadPlan,
    ) -> Result<Vec<BrickId>> {
        ensure!(
            actor.administrator,
            "Only the host/administrator may load builds"
        );
        ensure!(
            plan.base_revision == self.world.revision && plan.first_id == self.world.next_brick_id,
            "World changed while preparing build load"
        );
        let revision = self
            .world
            .revision
            .checked_add(1)
            .context("Revision exhausted")?;
        ensure!(
            self.world.owners.len() + plan.owners.len() <= MAX_OWNERS
                && plan.owners.iter().all(|(owner, record)| {
                    !self.world.owners.contains_key(owner)
                        && self.world.owner_of(&record.principal).is_none()
                }),
            "Loaded owners conflict with this world's owners"
        );
        let ids = plan.bricks.keys().copied().collect();
        self.world.owners.extend(plan.owners);
        self.world.bricks.extend(plan.bricks);
        self.world.palette = plan.palette;
        self.world.next_brick_id = plan.next_id;
        self.world.revision = revision;
        Ok(ids)
    }
    /// The supplied server validator must check catalog availability, reach,
    /// build grid, overlap/support and map permissions against the current world.
    /// It is required even for administrator placement.
    pub fn plant(
        &mut self,
        actor: &Actor,
        mut brick: Brick,
        validate: impl FnOnce(&World, &Brick) -> Result<()>,
    ) -> Result<BrickId> {
        ensure!(
            actor.owner != 0 || actor.administrator,
            "No authenticated builder"
        );
        ensure!(
            self.world.bricks.len() < MAX_BRICKS,
            "World brick limit reached"
        );
        ensure!(
            matches!(brick.definition, ContentRef::Resolved(_)),
            "Cannot plant unresolved brick content"
        );
        brick.owner = actor.owner;
        brick.source_records.clear();
        brick.validate(self.world.palette.len())?;
        validate(&self.world, &brick)?;
        let id = self.world.next_brick_id;
        let next = id.checked_add(1).context("Brick IDs exhausted")?;
        let revision = self
            .world
            .revision
            .checked_add(1)
            .context("Revision exhausted")?;
        self.world.bricks.insert(id, brick);
        self.world.next_brick_id = next;
        self.world.revision = revision;
        Ok(id)
    }
    fn permission(actor: &Actor, brick: &Brick, level: u8) -> Result<()> {
        ensure!(
            actor.administrator
                || (actor.owner != 0 && actor.trust_level(brick.owner) >= level),
            "Brick edit denied"
        );
        Ok(())
    }
    /// Trust an edit needs: the wrench's basic settings need build trust,
    /// rendering, collision, raycasting, events and paint need full trust.
    fn edit_level(old: &Brick, edit: &Edit) -> u8 {
        match edit {
            Edit::Name(_) => trust::BUILD,
            Edit::Properties(p)
                if p.raycast == old.raycast
                    && p.colliding == old.colliding
                    && p.visible == old.visible =>
            {
                trust::BUILD
            }
            _ => trust::FULL,
        }
    }
    pub fn edit(&mut self, actor: &Actor, id: BrickId, edit: Edit) -> Result<()> {
        let old = self.world.bricks.get(&id).context("Unknown brick")?;
        Self::permission(actor, old, Self::edit_level(old, &edit))?;
        let mut next = old.clone();
        match edit {
            Edit::Color(c) => next.color = c,
            Edit::ColorEffect(c) => next.color_effect = c,
            Edit::Print(p) => next.print = p,
            Edit::Name(n) => next.name = n,
            Edit::Events(e) => next.events = e,
            Edit::ShapeEffect(effect) => next.shape_effect = effect,
            Edit::Properties(properties) => {
                next.name = properties.name;
                next.light = properties.light.map(|id| Light {
                    asset: ContentRef::Resolved(id),
                    enabled: true,
                });
                next.emitter = Some(Emitter {
                    asset: properties.emitter.map(ContentRef::Resolved),
                    direction: properties.emitter_direction,
                });
                next.item_spawn = properties.item_spawn;
                next.sound = properties.sound.map(ContentRef::Resolved);
                next.vehicle = properties.vehicle.map(|id| crate::VehicleSpawn {
                    vehicle: ContentRef::Resolved(id),
                    recolor: properties.recolor_vehicle,
                });
                next.raycast = properties.raycast;
                next.colliding = properties.colliding;
                next.visible = properties.visible;
            }
        }
        next.validate(self.world.palette.len())?;
        let revision = self
            .world
            .revision
            .checked_add(1)
            .context("Revision exhausted")?;
        self.world.bricks.insert(id, next);
        self.world.revision = revision;
        Ok(())
    }
    pub fn remove(&mut self, actor: &Actor, id: BrickId) -> Result<()> {
        Self::permission(
            actor,
            self.world.bricks.get(&id).context("Unknown brick")?,
            trust::FULL,
        )?;
        let revision = self
            .world
            .revision
            .checked_add(1)
            .context("Revision exhausted")?;
        self.world.bricks.remove(&id);
        self.world.revision = revision;
        Ok(())
    }
    /// Trusted server mutation from the event engine or game rules. The
    /// server has already decided the change is permitted.
    pub fn mutate(&mut self, id: BrickId, change: impl FnOnce(&mut Brick)) -> Result<()> {
        let mut next = self.world.bricks.get(&id).context("Unknown brick")?.clone();
        change(&mut next);
        next.validate(self.world.palette.len())?;
        let revision = self
            .world
            .revision
            .checked_add(1)
            .context("Revision exhausted")?;
        self.world.bricks.insert(id, next);
        self.world.revision = revision;
        Ok(())
    }
    /// Continue an earlier world's clock (the host changed maps).
    pub fn set_tick(&mut self, tick: u64) {
        self.world.tick = tick;
    }
    /// Advance the world clock one fixed tick.
    pub fn step(&mut self) -> Result<()> {
        self.world.tick = self.world.tick.checked_add(1).context("Tick exhausted")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> World {
        let mut world = World::new("events".into(), "map/test".into(), vec![[1.0; 4]; 8]);
        for (id, owner) in [(1, 7), (2, 7), (3, 8)] {
            let b = Brick::new(
                ContentRef::Resolved("brick/test".into()),
                [id as f32, 0.0, 0.0],
                owner,
            );
            world.bricks.insert(id, b);
        }
        world.next_brick_id = 4;
        world
    }
    fn row(output: &str, color: u8) -> EventRow {
        EventRow {
            preserved: None,
            enabled: true,
            input: "onActivate".into(),
            delay_ms: 0,
            target: EventTarget::Slot(bri_events::Slot::SelfBrick),
            output: output.into(),
            params: vec![EventValue::Color(color)],
        }
    }
    #[test]
    fn event_rows_round_trip_and_are_bounded() {
        let mut world = fixture();
        world.bricks.get_mut(&1).unwrap().events = (0..MAX_EVENTS_PER_BRICK)
            .map(|i| row("setColor", (i % 4) as u8))
            .collect();
        let bytes = serde_json::to_vec(&world).unwrap();
        let restored = crate::persistence::decode(&bytes).unwrap();
        assert_eq!(restored, world);
        let mut server = Authority::new(restored).unwrap();
        let mut too_many = server.state().bricks[&1].events.clone();
        too_many.push(too_many[0].clone());
        let before = server.state().clone();
        let actor = Actor {
            owner: 7,
            administrator: false,
            ..Default::default()
        };
        assert!(server.edit(&actor, 1, Edit::Events(too_many)).is_err());
        assert!(
            server
                .edit(&actor, 1, Edit::Events(vec![row("setColor", 200)]))
                .is_err()
        );
        assert_eq!(server.state(), &before);
    }
    #[test]
    fn ownership_failed_edits_and_id_allocation_are_atomic() {
        let mut server = Authority::new(fixture()).unwrap();
        let actor = Actor {
            owner: 7,
            administrator: false,
            ..Default::default()
        };
        let original = server.state().clone();
        assert!(server.edit(&actor, 3, Edit::Color(2)).is_err());
        assert!(server.edit(&actor, 1, Edit::Color(200)).is_err());
        assert_eq!(*server.state(), original);
        let draft = Brick::new(ContentRef::Resolved("brick/test".into()), [0.0; 3], 999);
        assert!(
            server
                .plant(&actor, draft.clone(), |_, _| anyhow::bail!("Overlap"))
                .is_err()
        );
        assert_eq!(*server.state(), original);
        let id = server.plant(&actor, draft.clone(), |_, _| Ok(())).unwrap();
        assert_eq!(server.state().bricks[&id].owner, 7);
        server.remove(&actor, id).unwrap();
        let next = server.plant(&actor, draft, |_, _| Ok(())).unwrap();
        assert!(next > id);
        assert!(
            server
                .remove(
                    &Actor {
                        owner: 0,
                        administrator: false,
                        ..Default::default()
                    },
                    next
                )
                .is_err()
        );
    }
    #[test]
    fn trust_levels_gate_edits_like_v20() {
        let mut server = Authority::new(fixture()).unwrap();
        let owner = server.state().bricks[&1].owner;
        let levels = |level| Trust::Levels(std::sync::Arc::new([(owner, level)].into()));
        let actor = |trust| Actor {
            owner: 99,
            administrator: false,
            trust,
        };
        assert_eq!(actor(Trust::OwnerOnly).trust_level(0), trust::FULL);
        assert_eq!(actor(Trust::Everyone).trust_level(owner), trust::YOU);
        assert!(server.edit(&actor(Trust::OwnerOnly), 1, Edit::Name(None)).is_err());
        let build = actor(levels(trust::BUILD));
        server.edit(&build, 1, Edit::Name(Some("door".into()))).unwrap();
        assert!(server.edit(&build, 1, Edit::Color(1)).is_err());
        assert!(server.remove(&build, 1).is_err());
        let full = actor(levels(trust::FULL));
        server.edit(&full, 1, Edit::Color(1)).unwrap();
        server.remove(&full, 1).unwrap();
    }
    #[test]
    fn trusted_mutation_validates_before_publishing() {
        let mut server = Authority::new(fixture()).unwrap();
        server.mutate(1, |b| b.visible = false).unwrap();
        assert!(!server.state().bricks[&1].visible);
        let before = server.state().clone();
        assert!(server.mutate(1, |b| b.color = 99).is_err());
        assert_eq!(server.state(), &before);
    }
}
