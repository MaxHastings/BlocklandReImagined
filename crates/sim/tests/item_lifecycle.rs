//! A spawn brick's item beyond the respawn timer: the `reset` restore mode
//! keeps a taken item gone until the mini-game resets or a `restoreItem`
//! event brings it back, `hideItem` takes it away, and `onItemPickup` fires
//! on the brick once per item really taken, never for a touch that took
//! nothing.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_minigames::Settings;
use bri_sim::{
    definitions::{Definition, Definitions},
    item_spawners::NEVER,
    player::MoveInput,
    session::{Command, MiniGameRequest, Reply, Session, ToolCatalog},
    simulation::Simulation,
};
use bri_weapons::{CORE_TOOLS, ItemBounds};
use bri_world::{
    ContentRef, EventRow, EventTarget, EventValue, ItemRestore, ItemSpawn, OwnerId, World,
    authority::{Edit, WrenchProperties},
};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;

const WAND: &str = CORE_TOOLS[3];

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 2],
        height_plates: 1,
        attachment_rows: vec!["bb".into(), "bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.; 3],
            size: [1., 0.2, 1.],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: BTreeMap::from([(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]),
    }
}

struct Game {
    s: Session,
    seq: u64,
}
impl Game {
    fn new() -> Self {
        let world = World::new("Items".into(), "test".into(), vec![[1.; 4], [0.; 4]]);
        let simulation = Simulation::new(
            world,
            definitions(),
            vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        )
        .unwrap();
        let mut s = Session::new(simulation);
        s.set_item_bounds(
            CORE_TOOLS
                .into_iter()
                .map(|id| {
                    (
                        id.into(),
                        ItemBounds {
                            min: [-0.1; 3],
                            max: [0.1; 3],
                        },
                    )
                })
                .collect(),
        )
        .unwrap();
        s.set_tool_catalog(ToolCatalog {
            items: CORE_TOOLS.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        })
        .unwrap();
        s.set_event_catalog(bri_events::testing::catalog_extended(), Vec::new())
            .unwrap();
        s.set_spawn_points(vec![Vec3::new(-20.0, 0.05, -20.0)]).unwrap();
        Self { s, seq: 0 }
    }
    fn join(&mut self, name: &str, at: Vec3) -> OwnerId {
        self.s.join(name.into(), at, false).unwrap()
    }
    fn send(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<Reply> {
        self.seq += 1;
        self.s.command(owner, self.seq, command)
    }
    /// Step with every player standing still, so none is dropped as starved.
    fn steps(&mut self, n: u64) {
        let players: Vec<OwnerId> = self.s.tool_inventories().keys().copied().collect();
        for _ in 0..n {
            for &p in &players {
                let seq = 1_000_000 + self.s.simulation().state().tick;
                self.s.movement(p, seq, MoveInput::default()).unwrap();
            }
            self.s.step().unwrap();
        }
    }
    /// A brick of `owner` at `position` spawning the wand, with `restore`
    /// and these event rows.
    fn spawner(
        &mut self,
        owner: OwnerId,
        position: [f32; 3],
        restore: ItemRestore,
        rows: Vec<EventRow>,
    ) -> u64 {
        let Reply::Planted(id) = self
            .send(
                owner,
                Command::Plant {
                    definition: "plate".into(),
                    position,
                    quarter_turns: 0,
                    color: 0,
                },
            )
            .unwrap()
        else {
            panic!("planted");
        };
        self.s
            .edit_brick(
                owner,
                id,
                Edit::Properties(WrenchProperties {
                    item_spawn: ItemSpawn {
                        item: Some(ContentRef::Resolved(WAND.into())),
                        position: 0,
                        direction: 2,
                        respawn_ms: 1000,
                        restore,
                    },
                    raycast: true,
                    colliding: true,
                    visible: true,
                    ..Default::default()
                }),
            )
            .unwrap();
        if !rows.is_empty() {
            self.s.edit_brick(owner, id, Edit::Events(rows)).unwrap();
        }
        id
    }
    fn available_at(&self, brick: u64) -> u64 {
        self.s
            .weapon_view()
            .static_items
            .iter()
            .find(|i| i.brick == brick)
            .expect("the brick spawns an item")
            .available_at
    }
    fn wands(&self, owner: OwnerId) -> usize {
        self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .filter(|s| s.as_deref() == Some(WAND))
            .count()
    }
    fn color(&self, brick: u64) -> u8 {
        self.s.simulation().state().bricks[&brick].color
    }
}
fn row(input: &str, output: &str, params: Vec<EventValue>) -> EventRow {
    EventRow {
        conditions: vec![],
        preserved: None,
        enabled: true,
        input: input.into(),
        delay_ms: 0,
        target: EventTarget::Slot(bri_events::Slot::SelfBrick),
        output: output.into(),
        params,
    }
}
/// Where a player stands to touch the item on a brick planted at `brick`.
fn over(brick: [f32; 3]) -> Vec3 {
    Vec3::new(brick[0], brick[1] + 0.25, brick[2])
}

/// A timer brick keeps v20's behaviour: the item fades back in after its
/// respawn time. A reset brick's item stays gone until the mini-game resets,
/// then is there to take again.
#[test]
fn a_reset_item_stays_gone_until_the_minigame_resets() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(10.0, 0.05, 10.0));
    g.send(
        host,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings::default(),
        }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;
    let at = [4.0, 0.1, 0.0];
    let brick = g.spawner(host, at, ItemRestore::Reset, vec![]);
    let timer_at = [-4.0, 0.1, 0.0];
    let timer = g.spawner(host, timer_at, ItemRestore::Timer, vec![]);
    // Members spawn (and respawn on reset) right over the reset brick.
    g.s.set_spawn_points(vec![over(at)]).unwrap();
    g.steps(2);
    let taker = g.join("Taker", Vec3::new(10.0, 0.05, -10.0));
    g.send(taker, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
    // Outside the game, v20 lets anyone take a builder's item.
    let other = g.join("Other", over(timer_at));
    g.steps(3);
    assert_eq!(g.wands(taker), 1, "the reset brick's item was taken");
    assert_eq!(g.wands(other), 1, "the timer brick's item was taken");
    assert_eq!(g.available_at(brick), NEVER);
    assert!(g.available_at(timer) < NEVER);
    // The 1 s timer brings the timer brick's item back (v20 gives the taker
    // a second one); the reset brick's item is still gone much later.
    g.steps(700);
    assert_eq!(g.wands(other), 2);
    assert_eq!(g.wands(taker), 1);
    assert_eq!(g.available_at(brick), NEVER);
    // The view a late joiner is sent shows the item as gone too.
    g.join("Late", Vec3::new(10.0, 0.05, -10.0));
    g.steps(2);
    assert!(
        g.s.weapon_view()
            .static_items
            .iter()
            .any(|i| i.brick == brick && i.available_at == NEVER)
    );
    // `MiniGameSO::Reset` brings every item of the game's builders back.
    g.send(host, Command::MiniGame(MiniGameRequest::Reset))
        .unwrap();
    assert!(g.available_at(brick) < NEVER, "the reset brought the item back");
    // The reset respawned both members with fresh tools over the brick:
    // one of them takes the item, once, and it is gone again.
    g.steps(3);
    assert_eq!(g.wands(taker) + g.wands(host), 1);
    assert_eq!(g.available_at(brick), NEVER);
}

/// `onItemPickup` fires on the brick once per item really taken: when two
/// players touch it the same tick one gets it and the row runs once, and
/// touching a brick whose item is gone runs nothing. `hideItem` takes the
/// item away and `restoreItem` brings it back, timer or not.
#[test]
fn pickup_fires_once_per_item_taken_and_events_hide_and_restore_it() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(10.0, 0.05, 10.0));
    let at = [4.0, 0.1, 0.0];
    let brick = g.spawner(
        host,
        at,
        ItemRestore::Reset,
        vec![
            row("onItemPickup", "setColor", vec![EventValue::Color(1)]),
            row("onRelay", "setColor", vec![EventValue::Color(0)]),
            row("onActivate", "hideItem", vec![]),
            row("onToolBreak", "restoreItem", vec![]),
        ],
    );
    g.steps(2);
    let a = g.join("A", over(at));
    let b = g.join("B", over(at));
    g.steps(3);
    assert_eq!(
        g.wands(a) + g.wands(b),
        1,
        "two players touching one item get one item between them"
    );
    assert_eq!(g.color(brick), 1, "onItemPickup ran");
    assert_eq!(g.available_at(brick), NEVER);
    // Clear the mark; standing on the empty brick fires nothing.
    g.s.fire_brick_input(brick, "onRelay", Some(host));
    g.steps(5);
    assert_eq!(g.color(brick), 0);
    assert_eq!(g.wands(a) + g.wands(b), 1);
    // `restoreItem` brings it back: the other player takes it, once.
    g.s.fire_brick_input(brick, "onToolBreak", Some(host));
    g.steps(3);
    assert_eq!(g.wands(a) + g.wands(b), 2);
    assert_eq!(g.color(brick), 1);
    assert_eq!(g.available_at(brick), NEVER);
    // `hideItem` on a restored item takes it away before anyone gets it.
    g.s.fire_brick_input(brick, "onToolBreak", Some(host));
    g.s.fire_brick_input(brick, "onActivate", Some(host));
    g.s.fire_brick_input(brick, "onRelay", Some(host));
    g.steps(5);
    assert_eq!(g.available_at(brick), NEVER);
    assert_eq!(g.wands(a) + g.wands(b), 2);
    assert_eq!(g.color(brick), 0, "a hidden item is not picked up");
    // Deleting the brick takes its item with it, and the restore outputs
    // on a brick that spawns nothing are refused quietly.
    g.s.edit_brick(host, brick, Edit::Properties(WrenchProperties {
        raycast: true,
        colliding: true,
        visible: true,
        ..Default::default()
    }))
    .unwrap();
    g.steps(2);
    assert!(g.s.weapon_view().static_items.iter().all(|i| i.brick != brick));
    g.s.fire_brick_input(brick, "onToolBreak", Some(host));
    g.steps(2);
    assert!(g.s.weapon_view().static_items.is_empty());
}

/// A save written before restore modes existed reads as the timer mode,
/// and a reset brick's mode survives the save.
#[test]
fn restore_mode_defaults_to_timer_and_round_trips() {
    let old = serde_json::json!({
        "item": null, "position": 0, "direction": 2, "respawn_ms": 4000
    });
    let spawn: ItemSpawn = serde_json::from_value(old).unwrap();
    assert_eq!(spawn.restore, ItemRestore::Timer);
    assert_eq!(spawn.available_again(10), Some(10 + 480));
    let reset = ItemSpawn {
        restore: ItemRestore::Reset,
        ..spawn.clone()
    };
    assert_eq!(reset.available_again(10), None);
    let text = serde_json::to_string(&reset).unwrap();
    assert!(text.contains("\"restore\":\"reset\""));
    assert_eq!(serde_json::from_str::<ItemSpawn>(&text).unwrap(), reset);
    // The timer mode is left out of saves, as before.
    assert!(!serde_json::to_string(&spawn).unwrap().contains("restore"));
}
