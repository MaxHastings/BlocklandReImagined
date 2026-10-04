//! Held-out composition: package carriage unlocks ordinary guarded brick policy.
//! The package never awards score or a round. No solution controls are injected.
use bri_chaos::fixture;
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target, Value};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_world::{Brick, ContentRef, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy)]
struct Variant(bool);
impl Variant {
    fn ns(self) -> &'static str {
        if self.0 {
            "linen-exchange"
        } else {
            "violet-depository"
        }
    }
    fn at(self, x: f32, y: f32, z: f32) -> [f32; 3] {
        if self.0 {
            [35.25 + z, y, -35.25 - x]
        } else {
            [-24.75 + x, y, 25.25 + z]
        }
    }
    fn name(self, name: &str) -> String {
        format!("{}_{name}", self.ns())
    }
}
struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn policy(v: Variant) -> Arc<bri_package_runtime::Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-heldout-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let ns = v.ns();
    let dir = root.0.join(ns);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), json!({"schema_version":1,"id":ns,"version":"1.0.0","api":1,"name":"Invented exchange policy","license":"CC0-1.0","capabilities":["minigame","player","world.edit"],"provides":[{"kind":"behaviour","id":format!("{ns}:behaviour/main"),"file":"behaviour.json"},{"kind":"script","id":format!("{ns}:script/main"),"file":"main.rhai"}]}).to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), json!({"schema_version":1,"script":"main.rhai","bot_objectives":true,"on_pickup":true,"zones":[{"bricks":[format!("{ns}:brick/desk")],"above":0.4,"period_ms":50}],"tick_interval":1,"state":{"global":{"epoch":{"default":0,"visible":"everyone","persist":false}},"player":{"burden":{"default":null,"visible":"everyone","persist":false},"done":{"default":0,"visible":"everyone","persist":false}}}}).to_string()).unwrap();
    let script = r#"
fn item() { "NS:weapon/parcel" }
fn image() { "NS:image/parcel" }
fn on_tick() {
    if get("epoch") != 0 { return; }
    for b in bricks("NS:brick/depot") {
        if b.game != () { set_brick_item(b.id,item()); set("epoch",1); }
    }
}
fn on_pickup(p,it,info) {
    let me=player(p);
    if it != item() { return; }
    if !me.bot || me.minigame == () || info.spawner == () || get_player(p,"burden") != () { return false; }
    if brick(info.spawner).game != me.minigame { return false; }
    set_player(p,"burden",info.spawner); mount_image(p,image(),2);
    set_brick_item(info.spawner,()); false
}
fn on_zone(p,b,event) {
    let me=player(p);
    if event != "enter" || !me.bot || me.minigame == () || get_player(p,"burden") == () { return; }
    if brick(b).game != me.minigame { return; }
    add_player(p,"done",1); set_player(p,"burden",()); mount_image(p,(),2);
    for panel in bricks("NS:brick/switch") {
        if panel.game == me.minigame { set_brick_color(panel.id,1); }
    }
    set("epoch",get("epoch")+1);
}
fn bot_objectives(p) {
    let me=player(p);
    if !me.bot || me.minigame == () || get("epoch") == 0 || get_player(p,"done") > 0 { return []; }
    let src=bricks("NS:brick/depot")[0]; let dest=bricks("NS:brick/desk")[0];
    if src.game != me.minigame || dest.game != me.minigame { return []; }
    [#{kind:"carry_return",id:"exchange-receipt",source:#{kind:"brick",brick:src.id},item:item(),epoch:#{scope:"global",key:"epoch",path:[]},destinations:[dest.id],carriage:#{key:"burden",worn:#{slot:2,image:image()}},completion:#{scope:"player",key:"done",path:[]}}]
}
"#.replace("NS",ns);
    std::fs::write(dir.join("main.rhai"), script).unwrap();
    Arc::new(
        bri_package_runtime::Catalog::load(
            &root.0,
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: ns.into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: ns.into(),
                    role: None,
                }],
            },
            true,
        )
        .unwrap(),
    )
}
fn guard(subject: Subject, property: Property, key: &str, value: Datum) -> Condition {
    Condition {
        subject,
        property,
        key: key.into(),
        compare: Compare::Equal,
        value,
    }
}
fn row(
    output: &str,
    target: Slot,
    params: Vec<Value>,
    conditions: Vec<Condition>,
    delay_ms: u32,
) -> Row {
    Row {
        enabled: true,
        input: "onActivate".into(),
        output: output.into(),
        target: Target::Slot(target),
        params,
        conditions,
        delay_ms,
        preserved: None,
    }
}
struct Game {
    s: Session,
    author: u64,
    bot: u64,
    v: Variant,
    seq: u64,
    start: Vec3,
}
impl Game {
    fn new(v: Variant) -> Self {
        let ns = v.ns();
        let mut simulation = fixture::synthetic_simulation(&[]).unwrap();
        for role in ["depot", "desk", "switch"] {
            simulation.definitions.entries.insert(
                format!("{ns}:brick/{role}"),
                simulation.definitions.entries[fixture::PLATE].clone(),
            );
        }
        let mut s = Session::new(simulation);
        let mut weapons = bri_weapons::testing::pack();
        let mut item = weapons.items[bri_weapons::testing::GUN_ITEM].clone();
        let mut image = weapons.images[bri_weapons::testing::GUN_IMAGE].clone();
        item.id = format!("{ns}:weapon/parcel");
        item.image = format!("{ns}:image/parcel");
        image.id = item.image.clone();
        weapons.items.insert(item.id.clone(), item);
        weapons.images.insert(image.id.clone(), image);
        s.set_weapon_pack(weapons).unwrap();
        s.set_item_bounds(Default::default()).unwrap();
        let (vehicles, _) = fixture::synthetic_vehicles().unwrap();
        let mut kinds = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots;
        kinds[0].id = format!("{ns}:bot/controller");
        let bot_kind = kinds[0].id.clone();
        s.set_vehicle_pack(vehicles, kinds).unwrap();
        s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
            .unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [bot_kind.clone()].into(),
            vehicle_bricks: [fixture::PLATE.into()].into(),
            ..Default::default()
        })
        .unwrap();
        s.install_packages(policy(v), None).unwrap();
        let at = Vec3::new(-60., 0.05, -60.);
        s.set_spawn_points(vec![at]).unwrap();
        let author = s.join(v.name("author"), at, true).unwrap();
        let brick = |kind: &str, x: f32, z: f32, name: &str| {
            let mut b = Brick::new(ContentRef::Resolved(kind.into()), v.at(x, 0.1, z), author);
            b.name = Some(v.name(name));
            b
        };
        let mut spawn = brick(fixture::PLATE, 0., 0., "actor");
        spawn.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(bot_kind),
            recolor: false,
        }));
        let mut bricks = vec![
            spawn,
            brick(&format!("{ns}:brick/depot"), 0., 6., "parcel"),
            brick(&format!("{ns}:brick/desk"), 8., 0., "desk"),
        ];
        for (role, x, z) in [("left", -4., 12.), ("right", 6., 16.)] {
            let key = v.name(role);
            let guards = vec![guard(
                Subject::SelfBrick,
                Property::Color,
                "",
                Datum::Number(1),
            )];
            let mut b = brick(&format!("{ns}:brick/switch"), x, z, role);
            b.events = vec![
                row(
                    "setVariable",
                    Slot::SelfBrick,
                    vec![Value::Int(1), Value::Text(key), Value::Int(1)],
                    guards.clone(),
                    150,
                ),
                row(
                    "setColorFX",
                    Slot::SelfBrick,
                    vec![Value::Int(1)],
                    guards,
                    300,
                ),
            ];
            bricks.push(b);
        }
        let guards = ["left", "right"]
            .map(|k| {
                guard(
                    Subject::Player,
                    Property::Variable,
                    &v.name(k),
                    Datum::Number(1),
                )
            })
            .to_vec();
        let mut finish = brick(fixture::PLATE, 0., 3., "finish");
        finish.events = vec![
            row(
                "addPlayerScore",
                Slot::Player,
                vec![Value::Int(97)],
                guards.clone(),
                450,
            ),
            row("winRound", Slot::Player, vec![], guards.clone(), 450),
            row(
                "setColorFX",
                Slot::SelfBrick,
                vec![Value::Int(1)],
                guards,
                450,
            ),
        ];
        bricks.push(finish);
        let locked = vec![guard(
            Subject::Player,
            Property::Variable,
            &v.name("impossible"),
            Datum::Number(99),
        )];
        let mut decoy = brick(fixture::PLATE, 1., 2., "decoy");
        decoy.events = vec![
            row(
                "addPlayerScore",
                Slot::Player,
                vec![Value::Int(901)],
                locked.clone(),
                0,
            ),
            row("winRound", Slot::Player, vec![], locked.clone(), 0),
            row(
                "setColorFX",
                Slot::SelfBrick,
                vec![Value::Int(1)],
                locked,
                0,
            ),
        ];
        bricks.push(decoy);
        for i in 0..if v.0 { 9 } else { 3 } {
            let mut b = brick(fixture::PLATE, -12., i as f32 + 2., &format!("decor{i}"));
            b.colliding = false;
            b.raycast = false;
            bricks.push(b);
        }
        if v.0 {
            bricks.reverse();
        }
        let mut world = World::new(v.name("world"), "chaos/map".into(), vec![[1.; 4]; 8]);
        for (i, b) in bricks.into_iter().enumerate() {
            world.bricks.insert(i as u64 + 1, b);
        }
        world.next_brick_id = world.bricks.len() as u64 + 1;
        s.command(
            author,
            100,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        )
        .unwrap();
        let mut seq = 1 << 40;
        for _ in 0..40 {
            seq += 1;
            s.movement(author, seq, MoveInput::default()).unwrap();
            s.step().unwrap();
        }
        s.command(
            author,
            101,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: bri_minigames::Settings {
                    loadout: [None, None, None, None, None],
                    brick_damage: false,
                    ..Default::default()
                },
            }),
        )
        .unwrap();
        let bot = *s.names().keys().find(|o| s.is_bot(**o)).unwrap();
        let start = s
            .snapshot()
            .players
            .iter()
            .find(|p| p.owner == bot)
            .unwrap()
            .feet
            .into();
        Self {
            s,
            author,
            bot,
            v,
            seq,
            start,
        }
    }
    fn step(&mut self) {
        self.seq += 1;
        self.s
            .movement(self.author, self.seq, MoveInput::default())
            .unwrap();
        self.s.step().unwrap();
    }
    fn counter(&self, key: &str) -> i64 {
        self.s.package_state().packages[self.v.ns()]
            .players
            .get(&self.bot)
            .and_then(|p| p.get(key))
            .and_then(|n| n.as_i64())
            .unwrap_or(0)
    }
    fn brick(&self, role: &str) -> &Brick {
        self.s
            .simulation()
            .state()
            .bricks
            .values()
            .find(|b| b.name.as_deref() == Some(self.v.name(role).as_str()))
            .unwrap()
    }
}

#[test]
fn held_out_exchange_unlocks_delayed_switch_policy_and_actual_winner_in_two_worlds() {
    for v in [Variant(false), Variant(true)] {
        let mut g = Game::new(v);
        let mut carried = false;
        let mut moved = false;
        let mut returned_without_win = false;
        let mut providers = BTreeSet::new();
        let mut reasons = BTreeSet::new();
        for _ in 0..120 * 70 {
            g.step();
            carried |= g.counter("burden") > 0;
            moved |=
                g.s.snapshot()
                    .players
                    .iter()
                    .find(|p| p.owner == g.bot)
                    .is_some_and(|p| Vec3::from(p.feet).distance(g.start) > 4.);
            if let Some(t) = g.s.bot_thoughts().into_iter().find(|b| b.bot == g.bot) {
                if let Some(d) = t.objective_detail {
                    providers.insert(d.provider);
                }
                if let Some(reason) = t.objective_diagnostic {
                    reasons.insert(reason);
                }
            }
            if g.counter("done") == 1 && g.s.round_results().count() == 0 {
                returned_without_win = true;
                assert_eq!(
                    g.s.vitals()[&g.bot].score,
                    0,
                    "return policy must not manufacture the final brick award"
                );
            }
            assert_eq!(
                g.brick("decoy").color_effect,
                0,
                "nearer locked source never executes"
            );
            assert_eq!(
                g.s.vitals()[&g.author].score,
                0,
                "author never receives NPC credit"
            );
            if g.s.round_results().next_back().is_some() {
                break;
            }
        }
        let result = g.s.round_results().next_back();
        assert!(
            result.is_some(),
            "no real combined outcome: providers={providers:?}; reasons={reasons:?}; thought={:?}; package={:?}; diagnostics={:?}",
            g.s.bot_thoughts(),
            g.s.package_state(),
            g.s.package_diagnostics()
        );
        let result = result.unwrap();
        assert_eq!(result.owners, vec![g.bot]);
        assert_eq!(result.players.len(), 1);
        assert!(result.teams.is_empty());
        assert!(
            carried && moved && returned_without_win,
            "real intermediate carriage, travel and return without premature round win required"
        );
        assert_eq!(g.counter("done"), 1);
        assert_eq!(g.s.vitals()[&g.bot].score, 97);
        assert!(
            providers.contains("declared package pickup/zone")
                && providers.contains("native brick input"),
            "both ordinary providers must participate: {providers:?}"
        );
        for role in ["left", "right", "finish"] {
            assert_eq!(
                g.brick(role).color_effect,
                1,
                "missing canonical {role} witness"
            );
        }
        assert!(
            g.s.package_diagnostics().is_empty(),
            "{:?}",
            g.s.package_diagnostics()
        );
        for _ in 0..120 {
            g.step();
        }
        assert_eq!(g.s.vitals()[&g.bot].score, 97, "no duplicate delayed award");
        assert_eq!(g.s.round_results().count(), 1);
    }
}
