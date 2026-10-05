//! Unfamiliar package-owned carriage through actual pickup and zone hooks.
//! No brain target, pickup/zone injection, or post-setup success writes.
use bri_chaos::fixture;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_world::{Brick, ContentRef, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn catalog(
    namespace: &str,
    query_write: bool,
    oversized: bool,
) -> Arc<bri_package_runtime::Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-carry-return-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join(namespace);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), json!({"schema_version":1,"id":namespace,"version":"1.0.0","api":1,
        "name":"Unfamiliar parcel policy","license":"CC0-1.0","capabilities":["minigame","player","world.edit"],
        "provides":[{"kind":"behaviour","id":format!("{namespace}:behaviour/main"),"file":"behaviour.json"},
        {"kind":"script","id":format!("{namespace}:script/main"),"file":"main.rhai"}]}).to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), json!({"schema_version":1,"script":"main.rhai","bot_objectives":true,
        "on_pickup":true,"on_spawn":true,"zones":[{"bricks":[format!("{namespace}:brick/desk")],"above":0.4,"period_ms":50}],"tick_interval":30,
        "state":{"global":{"epoch":{"default":0,"visible":"everyone","persist":false},"spawned":{"default":0,"visible":"everyone","persist":false}},
            "player":{"burden":{"default":null,"visible":"everyone","persist":false},"done":{"default":0,"visible":"everyone","persist":false},"visits":{"default":0,"visible":"everyone","persist":false}}}}).to_string()).unwrap();
    let query = if query_write {
        "try { set(\"epoch\",get(\"epoch\")); } catch(e) {}"
    } else if oversized {
        r#"
        if get("epoch") == 0 { return []; }
        let src=bricks("NS:brick/depot")[0];
        let dest=bricks("NS:brick/desk")[0];
        let out=[];
        for n in 0..9 {
            out.push(#{kind:"carry_return",id:`oversized:${n}`,source:#{kind:"brick",brick:src.id},item:item(),
                epoch:#{scope:"global",key:"epoch",path:[]},destinations:[dest.id],
                carriage:#{key:"burden",worn:#{slot:2,image:image()}},completion:#{scope:"player",key:"done",path:[]}});
        }
        return out;
        "#
    } else {
        ""
    };
    let script = r#"
fn item() { "NS:weapon/parcel" }
fn image() { "NS:image/parcel" }
fn on_spawn(p) {
    if !player(p).bot { return; }
    set("spawned",get("spawned")+1); add_player(p,"visits",1);
    if get_player(p,"burden") != () {
        let source=brick(get_player(p,"burden"));
        set_player(p,"burden",()); mount_image(p,(),2);
        if source != () {
            set_brick_item(source.id,item()); set("epoch",get("epoch")+1);
        }
    }
}
fn on_tick() {
    if get("epoch") != 0 { return; }
    for b in bricks("NS:brick/depot") {
        if b.game != () { set_brick_item(b.id,item()); set("epoch",1); }
    }
}
fn on_pickup(p, it, info) {
    let me=player(p);
    if it != item() { return; }
    if !me.bot || me.minigame == () || info.spawner == () || get_player(p,"burden") != () { return false; }
    let b=brick(info.spawner);
    if b.game != me.minigame { return false; }
    set_player(p,"burden",b.id);
    mount_image(p,image(),2);
    set_brick_item(b.id,());
    false
}
fn on_zone(p,b,event) {
    let me=player(p);
    if event != "enter" || !me.bot || me.minigame == () || get_player(p,"burden") == () { return; }
    if brick(b).game != me.minigame { return; }
    add_player(p,"done",1);
    add_score(p,7);
    set_player(p,"burden",());
    mount_image(p,(),2);
    // Real successful policy may atomically replace its item identity.
    let source=bricks("NS:brick/depot")[0];
    set_brick_item(source.id,item()); set("epoch",get("epoch")+1);
    end_round(me.minigame,#{players:[p]});
}
fn bot_objectives(p) {
    QUERY
    let me=player(p);
    if !me.bot || me.minigame == () || get("epoch") == 0 { return []; }
    let src=bricks("NS:brick/depot")[0];
    let dest=bricks("NS:brick/desk")[0];
    if src.game != me.minigame || dest.game != me.minigame { return []; }
    [#{kind:"carry_return",id:"ordinary-parcel",source:#{kind:"brick",brick:src.id},item:item(),
      epoch:#{scope:"global",key:"epoch",path:[]},destinations:[dest.id],
      carriage:#{key:"burden",worn:#{slot:2,image:image()}},completion:#{scope:"player",key:"done",path:[]}}]
}
"#.replace("QUERY",query).replace("NS",namespace);
    std::fs::write(dir.join("main.rhai"), script).unwrap();
    Arc::new(
        bri_package_runtime::Catalog::load(
            &root.0,
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: namespace.into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: namespace.into(),
                    role: None,
                }],
            },
            true,
        )
        .unwrap(),
    )
}
struct Game {
    s: Session,
    human: u64,
    bot: u64,
    seq: u64,
    namespace: String,
    source: u64,
}
impl Game {
    fn new(namespace: &str, offset: f32, query_write: bool, in_game: bool) -> Self {
        Self::with_query(namespace, offset, query_write, in_game, false)
    }
    fn with_query(
        namespace: &str,
        offset: f32,
        query_write: bool,
        in_game: bool,
        oversized: bool,
    ) -> Self {
        let variant = offset != 0.0;
        let (spawn_id, source_id, destination_id) =
            if variant { (301, 203, 17) } else { (1, 2, 3) };
        let mut simulation = fixture::synthetic_simulation(&[]).unwrap();
        for kind in ["depot", "desk"] {
            simulation.definitions.entries.insert(
                format!("{namespace}:brick/{kind}"),
                simulation.definitions.entries[fixture::PLATE].clone(),
            );
        }
        let mut s = Session::new(simulation);
        let mut pack = bri_weapons::testing::pack();
        let mut item = pack.items[bri_weapons::testing::GUN_ITEM].clone();
        let mut image = pack.images[bri_weapons::testing::GUN_IMAGE].clone();
        item.id = format!("{namespace}:weapon/parcel");
        item.image = format!("{namespace}:image/parcel");
        image.id = item.image.clone();
        pack.images.insert(image.id.clone(), image);
        pack.items.insert(item.id.clone(), item);
        s.set_weapon_pack(pack).unwrap();
        s.set_item_bounds(Default::default()).unwrap();
        let (vehicles, _) = fixture::synthetic_vehicles().unwrap();
        let mut kinds = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots;
        if variant {
            kinds[0].id = format!("{namespace}:bot/collector");
            kinds[0].name = "Collection attendant".into();
            kinds[0].first_names = vec!["Aster".into()];
        }
        let bot_kind = kinds[0].id.clone();
        s.set_vehicle_pack(vehicles, kinds).unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [bot_kind.clone()].into(),
            vehicle_bricks: [fixture::PLATE.into()].into(),
            ..Default::default()
        })
        .unwrap();
        s.install_packages(catalog(namespace, query_write, oversized), None)
            .unwrap();
        let at = Vec3::new(-40.0, 0.05, -40.0);
        s.set_spawn_points(vec![at]).unwrap();
        let human = s
            .join(
                if variant { "Dispatch owner" } else { "Author" }.into(),
                at,
                true,
            )
            .unwrap();
        let mut world = World::new(
            if variant {
                "Night dispatch exchange"
            } else {
                "Parcel composition"
            }
            .into(),
            "chaos/map".into(),
            vec![[1.0; 4]; 8],
        );
        let mut spawn = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            [offset + 0.25, 0.1, 20.25],
            human,
        );
        spawn.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(bot_kind),
            recolor: false,
            team: None,
        }));
        if variant {
            spawn.name = Some("Attendant station".into());
        }
        world.bricks.insert(spawn_id, spawn);
        world.bricks.insert(
            source_id,
            Brick::new(
                ContentRef::Resolved(format!("{namespace}:brick/depot")),
                [offset + 0.25, 0.1, 26.25],
                human,
            ),
        );
        world.bricks.insert(
            destination_id,
            Brick::new(
                ContentRef::Resolved(format!("{namespace}:brick/desk")),
                [offset + 8.25, 0.1, 20.25],
                human,
            ),
        );
        if variant {
            world.bricks.get_mut(&source_id).unwrap().name = Some("Outgoing parcel".into());
            world.bricks.get_mut(&destination_id).unwrap().name = Some("Receiving counter".into());
            let mut decor = Brick::new(
                ContentRef::Resolved(fixture::PLATE.into()),
                [offset - 12.75, 0.1, 34.25],
                human,
            );
            decor.name = Some("Unused registry plaque".into());
            decor.color = 3;
            world.bricks.insert(2, decor);
        }
        world.next_brick_id = if variant { 302 } else { 4 };
        s.command(
            human,
            100,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        )
        .unwrap();
        let mut seq = 1 << 40;
        for _ in 0..20 {
            seq += 1;
            s.movement(human, seq, MoveInput::default()).unwrap();
            s.step().unwrap();
        }
        // Ordinary build loading assigns live IDs in saved-brick order. Bind
        // pickup evidence to that exact live spawner, not its serialized ID.
        let source_kind = ContentRef::Resolved(format!("{namespace}:brick/depot"));
        let source_id = *s
            .simulation()
            .state()
            .bricks
            .iter()
            .find(|(_, brick)| brick.definition == source_kind)
            .expect("authored depot must exist after ordinary build loading")
            .0;
        let bot = *s.names().keys().find(|id| s.is_bot(**id)).unwrap();
        if in_game {
            s.command(
                human,
                101,
                Command::MiniGame(MiniGameRequest::Create {
                    color: 0,
                    settings: bri_minigames::Settings {
                        loadout: [None, None, None, None, None],
                        ..Default::default()
                    },
                }),
            )
            .unwrap();
        }
        if in_game {
            for _ in 0..20 {
                if s.minigame_views()
                    .iter()
                    .any(|game| game.members.contains(&bot))
                {
                    break;
                }
                seq += 1;
                s.movement(human, seq, MoveInput::default()).unwrap();
                s.step().unwrap();
            }
            assert!(
                s.minigame_views()
                    .iter()
                    .any(|game| game.members.contains(&bot)),
                "ordinary bot synchronization must admit the real member"
            );
            let state = s.package_state();
            let native = &state.packages[namespace].players[&bot];
            assert_eq!(
                native["burden"],
                json!(null),
                "real membership initializes declared carriage"
            );
            assert_eq!(
                native["done"],
                json!(0),
                "real membership initializes actual counter state"
            );
        }
        Self {
            s,
            human,
            bot,
            seq,
            namespace: namespace.into(),
            source: source_id,
        }
    }
    fn step(&mut self) {
        self.seq += 1;
        self.s
            .movement(self.human, self.seq, MoveInput::default())
            .unwrap();
        self.s.step().unwrap();
    }
    fn counter(&self, key: &str) -> i64 {
        self.s.package_state().packages[&self.namespace]
            .players
            .get(&self.bot)
            .and_then(|p| p.get(key))
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
    }
}
#[test]
fn unfamiliar_package_composes_real_pickup_and_return_with_canonical_winner() {
    for (namespace, offset) in [("parcel-bureau", 0.0), ("lunar-mail", 22.0)] {
        let mut g = Game::new(namespace, offset, false, true);
        let mut carried = false;
        let mut route = false;
        let mut plans = Vec::new();
        for _ in 0..120 * 35 {
            g.step();
            carried |= g.counter("burden") == g.source as i64;
            if let Some(detail) =
                g.s.bot_thoughts()
                    .into_iter()
                    .find(|b| b.bot == g.bot)
                    .and_then(|b| b.objective_detail)
            {
                route |=
                    detail.provider == "declared package pickup/zone" && detail.route.len() == 2;
                let identity = (detail.provider, detail.action, detail.route);
                if plans.len() < 16 && plans.last() != Some(&identity) {
                    plans.push(identity);
                }
            }
            if g.counter("done") > 0 {
                break;
            }
        }
        assert!(
            carried,
            "real pickup missing: {:?}, state={:?}, diagnostics={:?}, players={:?}",
            g.s.bot_thoughts(),
            g.s.package_state(),
            g.s.package_diagnostics(),
            g.s.snapshot().players
        );
        assert!(
            route,
            "shared two-action plan missing: {plans:?}; state={:?}; diagnostics={:?}",
            g.s.package_state(),
            g.s.package_diagnostics()
        );
        assert_eq!(
            g.counter("done"),
            1,
            "return missing: {:?}",
            g.s.bot_thoughts()
        );
        assert_eq!(
            g.s.minigame_views()[0]
                .members
                .iter()
                .filter(|id| **id == g.bot)
                .count(),
            1
        );
        assert!(
            g.s.round_results().any(|r| r.owners.contains(&g.bot)),
            "canonical winner missing"
        );
        assert!(
            g.s.package_diagnostics().is_empty(),
            "{:?}",
            g.s.package_diagnostics()
        );
    }
}
#[test]
fn a_query_that_catches_its_write_failure_cannot_offer_actions() {
    let mut g = Game::new("dishonest-mail", 0.0, true, true);
    for _ in 0..120 * 8 {
        g.step();
    }
    assert_eq!(g.counter("done"), 0);
    assert!(g.s.bot_thoughts().iter().all(|b| {
        b.objective_detail
            .as_ref()
            .is_none_or(|d| d.provider != "package")
    }));
    assert_eq!(
        g.s.package_state().packages[&g.namespace].global["epoch"],
        json!(1)
    );
}
#[test]
fn outside_minigame_anchored_controller_keeps_creature_hook_exclusions() {
    let mut g = Game::new("wild-mail", 0.0, false, false);
    for _ in 0..120 * 8 {
        g.step();
    }
    assert_eq!(
        g.s.package_state().packages[&g.namespace].global["spawned"],
        json!(0)
    );
    assert_eq!(g.counter("burden"), 0);
    assert_eq!(g.counter("done"), 0);
    assert!(
        g.s.bot_thoughts()
            .iter()
            .all(|b| b.objective_detail.is_none())
    );
}

#[test]
fn native_minigame_respawn_preserves_declared_player_state() {
    let mut g = Game::new("remembered-mail", 0.0, false, true);
    for _ in 0..5 {
        g.step();
    }
    assert_eq!(g.counter("visits"), 1);
    g.s.command(g.human, 102, Command::MiniGame(MiniGameRequest::RespawnAll))
        .unwrap();
    for _ in 0..5 {
        g.step();
    }
    assert_eq!(
        g.counter("visits"),
        2,
        "defaults must not overwrite an initialized package key"
    );
    assert_eq!(g.counter("done"), 0);
}

#[test]
fn ordinary_respawn_replaces_the_carried_source_without_completing_the_old_journey() {
    let mut g = Game::new("renewed-mail", 0.0, false, true);
    for _ in 0..120 * 20 {
        g.step();
        if g.counter("burden") == 2 {
            break;
        }
    }
    assert_eq!(
        g.counter("burden"),
        2,
        "must interrupt genuine native carriage"
    );
    assert_eq!(g.counter("done"), 0, "return must not precede interruption");
    let old_epoch = g.s.package_state().packages[&g.namespace].global["epoch"]
        .as_i64()
        .unwrap();
    let old_life = g.s.vitals()[&g.bot].spawn_tick;
    g.s.command(g.human, 102, Command::MiniGame(MiniGameRequest::RespawnAll))
        .unwrap();
    for _ in 0..5 {
        g.step();
    }
    assert!(
        g.s.vitals()[&g.bot].spawn_tick > old_life,
        "real actor life replaced"
    );
    assert_eq!(
        g.counter("burden"),
        0,
        "owner policy actually clears carriage"
    );
    let new_epoch = g.s.package_state().packages[&g.namespace].global["epoch"]
        .as_i64()
        .unwrap();
    assert!(
        new_epoch > old_epoch,
        "owner policy actually replaces its source incarnation"
    );
    assert_eq!(g.counter("done"), 0, "replacement is not successful return");
    assert_eq!(
        g.s.round_results().count(),
        0,
        "no fabricated completion after replacement"
    );
    let mut carried_again = false;
    let mut composed_again = false;
    for _ in 0..120 * 35 {
        g.step();
        carried_again |= g.counter("burden") == 2;
        composed_again |= g.s.bot_thoughts().iter().any(|t| {
            t.bot == g.bot
                && t.objective_detail.as_ref().is_some_and(|d| {
                    d.provider == "declared package pickup/zone" && d.route.len() == 2
                })
        });
        if g.counter("done") > 0 {
            break;
        }
    }
    assert!(
        carried_again && composed_again,
        "fresh incarnation must be picked up and planned again: {:?}",
        g.s.bot_thoughts()
    );
    assert_eq!(g.counter("done"), 1);
    assert_eq!(g.s.vitals()[&g.bot].score, 7);
    assert!(
        g.s.round_results().any(|r| r.owners.contains(&g.bot)),
        "real successful new journey winner"
    );
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
}

#[test]
fn changing_the_actual_carried_source_invalidates_the_saved_stamp_and_replans() {
    let mut g = Game::new("repainted-mail", 0.0, false, true);
    for _ in 0..120 * 20 {
        g.step();
        if g.counter("burden") == 2 {
            break;
        }
    }
    assert_eq!(g.counter("burden"), 2, "must edit during real carriage");
    assert_eq!(g.counter("done"), 0);
    let before =
        g.s.bot_thoughts()
            .iter()
            .find(|t| t.bot == g.bot)
            .unwrap()
            .objective_searches;
    g.s.edit_brick(g.human, 2, bri_world::authority::Edit::Color(1))
        .unwrap();
    assert_eq!(
        g.s.simulation().state().bricks[&2].color,
        1,
        "native author edit applied"
    );
    for _ in 0..5 {
        g.step();
    }
    assert_eq!(g.counter("done"), 0, "source edit is not package return");
    assert_eq!(g.s.round_results().count(), 0);
    let mut repaired = false;
    for _ in 0..120 * 35 {
        g.step();
        repaired |=
            g.s.bot_thoughts()
                .iter()
                .any(|t| t.bot == g.bot && t.objective_searches > before);
        if g.counter("done") > 0 {
            break;
        }
    }
    assert!(
        repaired,
        "changed source stamp must trigger a real model repair"
    );
    assert_eq!(
        g.counter("done"),
        1,
        "owner policy remains eligible after repaint"
    );
    assert_eq!(g.s.vitals()[&g.bot].score, 7);
    assert!(g.s.round_results().any(|r| r.owners.contains(&g.bot)));
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
}

#[test]
fn too_many_otherwise_valid_package_offers_are_bounded_before_grounding() {
    let mut g = Game::with_query("overfull-mail", 0.0, false, true, true);
    for _ in 0..240 {
        g.step();
        if g.s
            .package_diagnostics()
            .iter()
            .any(|d| d.code == "objective.query" && d.message.contains("count exceeds 8"))
        {
            break;
        }
    }
    let diagnostics = g.s.package_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "objective.query" && d.message.contains("count exceeds 8")),
        "missing truthful provider bound: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .filter(|d| d.code == "objective.query")
            .count()
            <= 1,
        "repeated provider errors are de-duplicated"
    );
    assert!(
        g.s.bot_thoughts()
            .iter()
            .filter(|t| t.bot == g.bot)
            .all(|t| t
                .objective_detail
                .as_ref()
                .is_none_or(|d| d.provider != "declared package pickup/zone")),
        "oversized provider must not partially admit a plan"
    );
    assert_eq!(g.counter("done"), 0);
    assert_eq!(g.s.round_results().count(), 0);
}
