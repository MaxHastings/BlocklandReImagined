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
    Action(Action),
    Print(Option<ContentRef>),
    Name(Option<String>),
    Events(Vec<Event>),
    Properties(WrenchProperties),
    ShapeEffect(u8),
}
/// Native, atomic subset of the ordinary brick wrench. Asset IDs are resolved
/// against the server's tool catalog before reaching this authority boundary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrenchProperties {
    pub name: Option<String>,
    pub light: Option<String>,
    pub emitter: Option<String>,
    pub emitter_direction: u8,
    #[serde(default)]
    pub item_spawn: ItemSpawn,
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
            Edit::Action(a) => apply(&mut next, &a),
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
        self.world
            .pending
            .retain(|p| p.source != id && p.target != id);
        self.world.revision = revision;
        Ok(())
    }
    /// Called only after server-side activation distance or physical touch checks.
    /// Targets are captured now, scoped to the source's owner, and rechecked at execution.
    pub fn trigger(&mut self, source: BrickId, input: Input) -> Result<usize> {
        let brick = self
            .world
            .bricks
            .get(&source)
            .context("Unknown event source")?;
        let mut additions = Vec::new();
        let mut order = self.world.next_event_order;
        for e in brick
            .events
            .iter()
            .filter(|e| e.enabled && e.input == input)
        {
            let targets: Vec<_> = match &e.target {
                Target::ThisBrick => vec![source],
                Target::Named(n) => self
                    .world
                    .bricks
                    .iter()
                    .filter(|(_, b)| {
                        b.owner == brick.owner
                            && b.name.as_ref().is_some_and(|v| v.eq_ignore_ascii_case(n))
                    })
                    .map(|(id, _)| *id)
                    .collect(),
            };
            ensure!(
                additions.len() + targets.len() <= 4096,
                "Event trigger fanout exceeds limit"
            );
            let delay = (u64::from(e.delay_ms) * TICKS_PER_SECOND).div_ceil(1000);
            let due_tick = self
                .world
                .tick
                .checked_add(delay)
                .context("Event time overflow")?;
            for target in targets {
                additions.push(PendingAction {
                    due_tick,
                    order,
                    source,
                    source_owner: brick.owner,
                    target,
                    action: e.action.clone(),
                });
                order = order.checked_add(1).context("Event order exhausted")?;
            }
        }
        ensure!(
            self.world.pending.len() + additions.len() <= 20_000,
            "Event queue full"
        );
        let count = additions.len();
        if count == 0 {
            return Ok(0);
        }
        let revision = self
            .world
            .revision
            .checked_add(1)
            .context("Revision exhausted")?;
        self.world.pending.extend(additions);
        self.world.pending.sort_by_key(|p| (p.due_tick, p.order));
        self.world.next_event_order = order;
        self.world.revision = revision;
        Ok(count)
    }
    /// Execute due actions in stable order and advance exactly one fixed tick.
    pub fn step(&mut self) -> Result<Vec<BrickId>> {
        let tick = self.world.tick.checked_add(1).context("Tick exhausted")?;
        let revision = self
            .world
            .revision
            .checked_add(1)
            .context("Revision exhausted")?;
        let count = self
            .world
            .pending
            .partition_point(|p| p.due_tick <= self.world.tick);
        let due: Vec<_> = self.world.pending.drain(..count).collect();
        let mut changed = Vec::new();
        for p in due {
            if self
                .world
                .bricks
                .get(&p.source)
                .is_none_or(|s| s.owner != p.source_owner)
            {
                continue;
            }
            if let Some(target) = self
                .world
                .bricks
                .get_mut(&p.target)
                .filter(|b| b.owner == p.source_owner)
            {
                apply(target, &p.action);
                changed.push(p.target);
            }
        }
        self.world.tick = tick;
        self.world.revision = revision;
        changed.sort_unstable();
        changed.dedup();
        Ok(changed)
    }
}
fn apply(brick: &mut Brick, action: &Action) {
    match action {
        Action::Color(v) => brick.color = *v,
        Action::Visible(v) => brick.visible = *v,
        Action::Colliding(v) => brick.colliding = *v,
        Action::Raycast(v) => brick.raycast = *v,
        Action::ColorEffect(v) => brick.color_effect = *v,
        Action::Light(v) => {
            brick.light = v.as_ref().map(|asset| Light {
                asset: asset.clone(),
                enabled: true,
            })
        }
        Action::Emitter(v) => {
            let direction = brick.emitter.as_ref().map_or(0, |e| e.direction);
            brick.emitter = Some(Emitter {
                asset: v.clone(),
                direction,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> World {
        let mut world = World::new("events".into(), "map/test".into(), vec![[1.0; 4]; 8]);
        for (id, owner) in [(1, 7), (2, 7), (3, 8)] {
            let mut b = Brick::new(
                ContentRef::Resolved("brick/test".into()),
                [id as f32, 0.0, 0.0],
                owner,
            );
            b.name = Some("_lamp".into());
            world.bricks.insert(id, b);
        }
        world.next_brick_id = 4;
        world
    }
    #[test]
    fn large_zero_delay_list_roundtrips_and_executes_in_authored_order() {
        let mut world = fixture();
        world.bricks.get_mut(&1).unwrap().events = (0..MAX_EVENTS_PER_BRICK)
            .map(|i| Event {
                enabled: true,
                input: Input::Activate,
                delay_ms: 0,
                target: Target::ThisBrick,
                action: Action::Color((i % 4) as u8),
            })
            .collect();
        let bytes = serde_json::to_vec(&world).unwrap();
        let restored = crate::persistence::decode(&bytes).unwrap();
        assert_eq!(restored, world);
        let mut server = Authority::new(restored).unwrap();
        assert_eq!(
            server.trigger(1, Input::Activate).unwrap(),
            MAX_EVENTS_PER_BRICK
        );
        assert!(server.state().pending.iter().all(|p| p.due_tick == 0));
        assert!(
            server
                .state()
                .pending
                .windows(2)
                .all(|p| p[0].order < p[1].order)
        );
        assert_eq!(server.step().unwrap(), vec![1]);
        assert_eq!(server.state().bricks[&1].color, 3);
        assert!(server.state().pending.is_empty());
        let mut too_many = server.state().bricks[&1].events.clone();
        too_many.push(too_many[0].clone());
        let before = server.state().clone();
        assert!(
            server
                .edit(
                    &Actor {
                        owner: 7,
                        administrator: false
                    },
                    1,
                    Edit::Events(too_many)
                )
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
        assert!(
            server
                .edit(&actor, 3, Edit::Action(Action::Color(2)))
                .is_err()
        );
        assert!(
            server
                .edit(&actor, 1, Edit::Action(Action::Color(200)))
                .is_err()
        );
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
    fn delayed_named_events_survive_reload_and_respect_owner_scope() {
        let mut world = fixture();
        world.bricks.get_mut(&1).unwrap().events = vec![
            Event {
                enabled: true,
                input: Input::Activate,
                delay_ms: 10,
                target: Target::Named("_LAMP".into()),
                action: Action::Color(2),
            },
            Event {
                enabled: true,
                input: Input::Activate,
                delay_ms: 10,
                target: Target::ThisBrick,
                action: Action::Color(3),
            },
        ];
        let mut live = Authority::new(world).unwrap();
        assert_eq!(live.trigger(1, Input::Touch).unwrap(), 0);
        assert_eq!(live.trigger(1, Input::Activate).unwrap(), 3);
        // 10ms is rounded up to two 120Hz ticks; never execute it early.
        assert!(live.step().unwrap().is_empty());
        assert!(live.step().unwrap().is_empty());
        let bytes = serde_json::to_vec(live.state()).unwrap();
        let mut restored = Authority::new(crate::persistence::decode(&bytes).unwrap()).unwrap();
        assert_eq!(live.step().unwrap(), vec![1, 2]);
        restored.step().unwrap();
        assert_eq!(live.state(), restored.state());
        assert_eq!(live.state().bricks[&1].color, 3);
        assert_eq!(live.state().bricks[&2].color, 2);
        assert_eq!(live.state().bricks[&3].color, 0);
        assert!(live.state().pending.is_empty());
    }
    #[test]
    fn deleting_source_cancels_pending_actions_and_corrupt_saves_fail() {
        let mut world = fixture();
        world.bricks.get_mut(&1).unwrap().events.push(Event {
            enabled: true,
            input: Input::Activate,
            delay_ms: 100,
            target: Target::Named("_lamp".into()),
            action: Action::Visible(false),
        });
        let mut server = Authority::new(world).unwrap();
        server.trigger(1, Input::Activate).unwrap();
        server
            .remove(
                &Actor {
                    owner: 7,
                    administrator: false,
                },
                1,
            )
            .unwrap();
        for _ in 0..20 {
            server.step().unwrap();
        }
        assert!(server.state().bricks[&2].visible);
        let mut bad = server.state().clone();
        bad.next_brick_id = 2;
        assert!(crate::persistence::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
        bad = server.state().clone();
        bad.palette[0][0] = 2.0;
        assert!(bad.validate().is_err());
    }
}
