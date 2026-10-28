//! The shipped reaction policy, with synthetic profile metadata. No downloaded
//! recordings, network access or generated content are needed by CI.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog, Dynamic, PlayerKey,
    ops::{Op, SoundAt},
    script::{Budget, Call, PlayerView, Runtime, Snapshot},
    state::Namespace,
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

struct Policy {
    runtime: Runtime,
    state: Namespace,
    snapshot: Snapshot,
}
impl Policy {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("npc_popculture_rules");
        std::fs::create_dir(&dir).unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/npc_reactions");
        let mut manifest: Value =
            serde_json::from_slice(&std::fs::read(source.join("rules-package.json")).unwrap())
                .unwrap();
        manifest.as_object_mut().unwrap().remove("dependencies");
        manifest["provides"]
            .as_array_mut()
            .unwrap()
            .retain(|p| p["kind"] != "weapons");
        std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
        for file in ["behaviour.json", "reactions.rhai"] {
            std::fs::copy(source.join(file), dir.join(file)).unwrap();
        }
        std::fs::write(
            dir.join("catalog.rhai"),
            r#"
fn clips(event) { [#{id:"fixture",profile:"npc_popculture:sound/fixture",
    tier:"micro",iconic:true,seconds:1.5}] }
"#,
        )
        .unwrap();
        let set = PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "npc_popculture_rules".into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: "npc_popculture_rules".into(),
                role: None,
            }],
        };
        let catalog = Catalog::load(root.path(), &set, true).unwrap();
        let runtime = Runtime::compile(&catalog).unwrap();
        assert!(
            runtime.unknown_calls().is_empty(),
            "{:?}",
            runtime.unknown_calls()
        );
        let behaviour: Value =
            serde_json::from_slice(&std::fs::read(source.join("behaviour.json")).unwrap()).unwrap();
        let mut state = Namespace::default();
        for (key, entry) in behaviour["state"]["global"].as_object().unwrap() {
            state.global.insert(key.clone(), entry["default"].clone());
        }
        let bots = (1..=40)
            .map(|id| PlayerView {
                id,
                key: PlayerKey::session(id),
                bot: true,
                alive: true,
                health: 100.0,
                max_health: 100.0,
                grounded: true,
                minigame: Some(1),
                position: [id as f32 * 30.0, 2.0, 7.0],
                ..Default::default()
            })
            .collect();
        for id in 1..=40 {
            state
                .players
                .entry(PlayerKey::session(id))
                .or_default()
                .insert("voice".into(), json!({}));
        }
        Self {
            runtime,
            state,
            snapshot: Snapshot {
                tick: 1,
                bots,
                ..Default::default()
            },
        }
    }
    fn call(&mut self, function: &str, args: Vec<Dynamic>) -> Vec<Op> {
        let out = self
            .runtime
            .call(
                "npc_popculture_rules",
                Call {
                    function,
                    args,
                    budget: Budget::Tick,
                    snapshot: Arc::new(self.snapshot.clone()),
                    caller: None,
                    aim: None,
                    entity: None,
                    state: self.state.clone(),
                    entity_vars: Default::default(),
                    world: None,
                },
            )
            .unwrap_or_else(|e| panic!("{function}: {e:?}"));
        self.state = out.state;
        out.ops
    }
    fn cue(&mut self, bot: i64) {
        self.call(
            "cue",
            vec![bot.into(), "death".into(), 5_i64.into(), 100_i64.into()],
        );
    }
}

#[test]
fn npc_reactions_are_positional_and_an_iconic_clip_cannot_echo_across_bots() {
    let mut p = Policy::new();
    p.cue(1);
    let first = p.call("mix", vec![]);
    assert!(matches!(&first[..], [Op::Sound(s)]
        if s.at == SoundAt::Position([30.0, 3.0, 7.0]) && s.radius == 60.0 && s.volume == 1.0));
    p.snapshot.tick = 1440;
    p.cue(2);
    assert!(
        p.call("mix", vec![]).is_empty(),
        "global iconic cooldown spans bot IDs"
    );
    p.snapshot.tick = 15000;
    p.cue(2);
    assert_eq!(p.call("mix", vec![]).len(), 1);
}

#[test]
fn npc_reactions_bound_pending_work_and_do_not_treat_game_guards_as_stuck() {
    let mut p = Policy::new();
    for id in 1..=40 {
        p.cue(id);
    }
    assert_eq!(p.state.global["pending"].as_array().unwrap().len(), 16);
    p.state.global.insert("pending".into(), json!([]));
    for tick in (30..7200).step_by(30) {
        p.snapshot.tick = tick;
        assert!(p.call("on_tick", vec![]).is_empty());
    }
    assert_eq!(p.state.global["played"], 0);
}

#[test]
fn npc_reactions_sample_real_bot_health_and_keep_humans_silent() {
    let mut p = Policy::new();
    p.call("on_tick", vec![]);
    p.snapshot.tick = 31;
    p.snapshot.bots[0].health = 60.0;
    p.state.global.insert("rng".into(), json!(2));
    p.call("on_tick", vec![]);
    assert_eq!(
        p.state.global["played"], 1,
        "bot health observation must use bots(), not players()"
    );
    assert_eq!(p.state.global["by_event"]["heavy_damage"], 1);
    let human = PlayerView {
        id: 50,
        key: PlayerKey::session(50),
        alive: true,
        ..Default::default()
    };
    p.snapshot.players.push(human);
    p.cue(50);
    assert!(p.state.global["pending"].as_array().unwrap().is_empty());
}

#[test]
fn npc_reactions_confirmed_objective_outcome_displaces_a_full_combat_queue() {
    let mut p = Policy::new();
    for id in 1..=16 {
        p.cue(id);
    }
    p.call("on_bot_objective_completed", vec![40_i64.into()]);
    let pending = p.state.global["pending"].as_array().unwrap();
    assert_eq!(pending.len(), 16);
    assert!(
        pending
            .iter()
            .any(|q| q["bot"] == 40 && q["event"] == "objective_completed")
    );
    assert_eq!(p.call("mix", vec![]).len(), 1);
    assert_eq!(p.state.global["by_event"]["objective_completed"], 1);
}

#[test]
fn npc_reactions_round_result_uses_canonical_winners_not_individual_objectives() {
    let mut p = Policy::new();
    p.call("on_tick", vec![]);
    p.state.global.insert("rng".into(), json!(2));
    let event = rhai::serde::to_dynamic(json!({
        "kind": "round_end", "game": 1, "players": [1], "teams": []
    }))
    .unwrap();
    p.call("on_minigame", vec![event]);
    let pending = p.state.global["pending"].as_array().unwrap();
    assert!(
        pending
            .iter()
            .any(|q| q["bot"] == 1 && q["event"] == "round_win")
    );
    assert!(
        pending
            .iter()
            .any(|q| q["bot"] != 1 && q["event"] == "round_loss")
    );
    assert!(
        !p.state.global["eligible"]
            .as_object()
            .unwrap()
            .contains_key("objective_completed")
    );
    let before = p.state.global["eligible"].clone();
    let draw = rhai::serde::to_dynamic(json!({
        "kind":"round_end", "game":1, "players":[], "teams":[]
    }))
    .unwrap();
    p.call("on_minigame", vec![draw]);
    assert_eq!(p.state.global["eligible"], before, "a draw is not a loss");
}

#[test]
fn npc_reactions_critical_outcome_waits_for_its_own_voice_even_after_fast_travel() {
    let mut p = Policy::new();
    p.state.global.insert(
        "active".into(),
        json!([
            {"bot":40,"expires":250,"x":0.0,"y":0.0,"z":0.0}
        ]),
    );
    p.call("on_bot_objective_completed", vec![40_i64.into()]);
    assert!(p.call("mix", vec![]).is_empty());
    assert_eq!(p.state.global["pending"].as_array().unwrap().len(), 1);
    p.snapshot.tick = 251;
    assert_eq!(p.call("mix", vec![]).len(), 1);
}

#[test]
fn npc_reactions_do_not_celebrate_a_credited_teammate_kill() {
    let mut p = Policy::new();
    p.snapshot.bots[0].team = Some(1);
    p.snapshot.bots[1].team = Some(1);
    p.call("on_tick", vec![]);
    p.call("on_death", vec![1_i64.into(), 2_i64.into()]);
    assert!(
        !p.state.global["eligible"]
            .as_object()
            .unwrap()
            .contains_key("kill")
    );
    assert_eq!(p.state.players[&PlayerKey::session(2)]["voice"]["kills"], 0);
}
