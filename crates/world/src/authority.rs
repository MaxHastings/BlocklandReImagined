use crate::*;
use anyhow::{Context, Result, ensure};

/// This context is constructed by the server, never trusted from a command packet.
pub struct Actor {
    pub owner: OwnerId,
    pub administrator: bool,
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
        let ids = plan.bricks.keys().copied().collect();
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
    fn permission(actor: &Actor, brick: &Brick) -> Result<()> {
        ensure!(
            actor.administrator || (actor.owner != 0 && actor.owner == brick.owner),
            "Brick edit denied"
        );
        Ok(())
    }
    pub fn edit(&mut self, actor: &Actor, id: BrickId, edit: Edit) -> Result<()> {
        let old = self.world.bricks.get(&id).context("Unknown brick")?;
        Self::permission(actor, old)?;
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
        Self::permission(actor, self.world.bricks.get(&id).context("Unknown brick")?)?;
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
                        administrator: false
                    },
                    next
                )
                .is_err()
        );
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
