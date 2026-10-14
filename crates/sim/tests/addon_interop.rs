//! Add-Ons building on each other (docs/modding/rules.md, "Add-Ons that
//! build on each other"): `shop` depends on `economy` and calls the
//! functions it exports (`call_addon`); `economy` sends events (`emit`)
//! that `shop` listens for. Each Add-On's part of a call runs on its own
//! state and capabilities, and the call is one transaction: a caller that
//! fails keeps nothing of either.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    session::{Command, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

const ECONOMY: &str = r#"
fn api_balance(from, p) { get_player(p, "money") }
fn api_charge(from, p, amount) {
    let money = get_player(p, "money");
    if amount > money { throw `${p} cannot afford ${amount}`; }
    set_player(p, "money", money - amount);
    set("charged_by", from);
    tell(p, `Paid ${amount}`);
    emit("paid", #{ player: p, amount: amount, by: from });
    money - amount
}
fn api_spin(from) { loop { } }
fn api_back(from) { call_addon("shop", "noop") }
fn api_views(from, who) { who.name }
fn cmd_reset(p) { set_player(p, "money", 100); }
"#;

const SHOP: &str = r#"
fn cmd_buy(p, price) {
    let left = call_addon("economy", "charge", p, price);
    bump("sold", 1);
    set("left", left);
}
fn cmd_buy_then_fail(p) {
    call_addon("economy", "charge", p, 10);
    bump("sold", 1);
    throw "the till jammed";
}
fn cmd_try_buy(p, price) {
    try {
        call_addon("economy", "charge", p, price);
        bump("sold", 1);
    } catch (e) {
        set("refused", `${e}`);
    }
    bump("tries", 1);
}
fn cmd_twice(p) {
    call_addon("economy", "charge", p, 10);
    call_addon("economy", "charge", p, 15);
}
fn cmd_balance(p) { set("seen", call_addon("economy", "balance", p)); }
fn cmd_spin(p) {
    try { call_addon("economy", "spin"); } catch (e) { set("refused", `${e}`); }
    bump("tries", 1);
}
fn cmd_back(p) {
    try { call_addon("economy", "back"); } catch (e) { set("refused", `${e}`); }
}
fn cmd_view(p) { set("seen", call_addon("economy", "views", player(p))); }
fn cmd_missing(p) {
    try { call_addon("economy", "steal", p); } catch (e) { set("refused", `${e}`); }
}
fn cmd_list(p) { set("seen", exports("economy")); }
fn api_noop(from) { () }
fn on_addon_event(from, name, data) {
    bump("heard", 1);
    set("last", `${from}:${name}:${data.amount}:${data.by}`);
}
fn bump(key, n) { set(key, get(key) + n); }
"#;

const OUTSIDER: &str = r#"
fn cmd_steal(p) {
    try { call_addon("economy", "charge", p, 50); } catch (e) { set("refused", `${e}`); }
}
"#;

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn package(root: &Root, id: &str, manifest: Value, behaviour: Value, script: &str) {
    let dir = root.0.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    let mut manifest = manifest;
    let fields = manifest.as_object_mut().unwrap();
    for (k, v) in [
        ("schema_version", json!(1)),
        ("id", json!(id)),
        ("version", json!("1.0.0")),
        ("api", json!(1)),
        ("name", json!(id)),
        ("license", json!("CC0-1.0")),
        (
            "provides",
            json!([
                { "kind": "behaviour", "id": format!("{id}:behaviour/main"), "file": "behaviour.json" },
                { "kind": "script", "id": format!("{id}:script/main"), "file": "main.rhai" }
            ]),
        ),
    ] {
        fields.entry(k).or_insert(v);
    }
    let mut behaviour = behaviour;
    let fields = behaviour.as_object_mut().unwrap();
    fields.insert("schema_version".into(), json!(1));
    fields.insert("script".into(), json!("main.rhai"));
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(dir.join("main.rhai"), script).unwrap();
}

fn temp_root() -> Root {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    Root(std::env::temp_dir().join(format!(
        "bri-interop-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )))
}

fn load(root: &Root, ids: &[&str]) -> Result<Catalog, Vec<bri_package::diag::Diagnostic>> {
    let set = PackageSet {
        schema_version: 1,
        packages: ids
            .iter()
            .map(|id| PackageEntry {
                id: (*id).into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: (*id).into(),
                role: None,
            })
            .collect(),
    };
    Catalog::load(&root.0, &set, true)
}

fn catalog() -> Arc<Catalog> {
    let root = temp_root();
    let counter = json!({ "default": 0, "visible": "everyone" });
    let text = json!({ "default": "", "visible": "everyone" });
    package(
        &root,
        "economy",
        json!({ "capabilities": ["chat"], "optional_dependencies": { "shop": "*" } }),
        json!({
            "commands": [{ "name": "reset" }],
            "state": {
                "player": { "money": { "default": 100, "visible": "everyone" } },
                "global": { "charged_by": text }
            },
            "exports": [
                { "name": "balance", "args": 1 },
                { "name": "charge", "args": 2 },
                { "name": "spin" },
                { "name": "back" },
                { "name": "views", "args": 1 }
            ],
            "events": ["paid"]
        }),
        ECONOMY,
    );
    package(
        &root,
        "shop",
        json!({ "dependencies": { "economy": "^1.0.0" } }),
        json!({
            "commands": [
                { "name": "buy", "args": ["int"] },
                { "name": "buy_then_fail" },
                { "name": "try_buy", "args": ["int"] },
                { "name": "twice" },
                { "name": "balance" },
                { "name": "spin" },
                { "name": "back" },
                { "name": "view" },
                { "name": "missing" },
                { "name": "list" }
            ],
            "state": { "global": {
                "sold": counter, "left": counter, "tries": counter, "heard": counter,
                "refused": text, "last": text, "seen": counter
            } },
            "exports": [{ "name": "noop" }],
            "listens": ["economy:paid"]
        }),
        SHOP,
    );
    package(
        &root,
        "outsider",
        json!({}),
        json!({
            "commands": [{ "name": "steal" }],
            "state": { "global": { "refused": text } }
        }),
        OUTSIDER,
    );
    Arc::new(load(&root, &["economy", "shop", "outsider"]).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    seq: u64,
    p: OwnerId,
}
impl Game {
    fn new() -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new("Interop".into(), "interop".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![
                    ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                        .translation(Vector::new(0.0, -0.5, 0.0))
                        .user_data(u128::MAX),
                ],
            )
            .unwrap(),
        );
        s.install_packages(catalog(), None).unwrap();
        let p = s.join("P".into(), Vec3::new(0.0, 0.05, 0.0), true).unwrap();
        let mut g = Self { s, seq: 0, p };
        g.steps(2);
        g
    }
    fn send(&mut self, package: &str, command: &str, args: Vec<PackageArg>) -> anyhow::Result<()> {
        self.seq += 1;
        self.s
            .command(
                self.p,
                self.seq,
                Command::Package(PackageCommand {
                    package: package.into(),
                    command: command.into(),
                    args,
                }),
            )
            .map(drop)
    }
    fn shop(&mut self, command: &str, args: Vec<PackageArg>) -> anyhow::Result<()> {
        self.send("shop", command, args)
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
    }
    fn global(&self, package: &str, key: &str) -> Value {
        self.s
            .package_state()
            .packages
            .get(package)
            .and_then(|ns| ns.global.get(key).cloned())
            .unwrap_or(Value::Null)
    }
    fn money(&self) -> i64 {
        self.s.package_state().packages["economy"].players[&self.p]["money"]
            .as_i64()
            .unwrap()
    }
}

#[test]
fn an_exported_function_runs_on_its_own_state_and_answers_at_once() {
    let mut g = Game::new();
    g.shop("buy", vec![PackageArg::Int(30)]).unwrap();
    assert_eq!(g.money(), 70);
    assert_eq!(g.global("shop", "left"), json!(70));
    assert_eq!(g.global("shop", "sold"), json!(1));
    // The callee knows who called it.
    assert_eq!(g.global("economy", "charged_by"), json!("shop"));
    g.shop("balance", vec![]).unwrap();
    assert_eq!(g.global("shop", "seen"), json!(70));
    // A second call in the same transaction sees what the first one did.
    g.shop("twice", vec![]).unwrap();
    assert_eq!(g.money(), 45);
}

#[test]
fn a_caller_that_fails_keeps_nothing_of_the_call_it_made() {
    let mut g = Game::new();
    assert!(g.shop("buy_then_fail", vec![]).is_err());
    assert_eq!(g.money(), 100, "the charge went through for a failed sale");
    assert_eq!(g.global("shop", "sold"), json!(0));
    assert_eq!(g.global("economy", "charged_by"), json!(""));
}

#[test]
fn a_refused_call_is_an_error_the_caller_can_catch_and_go_on() {
    let mut g = Game::new();
    g.shop("try_buy", vec![PackageArg::Int(500)]).unwrap();
    assert_eq!(g.money(), 100);
    assert_eq!(g.global("shop", "sold"), json!(0));
    assert_eq!(g.global("shop", "tries"), json!(1));
    let refused = g.global("shop", "refused").to_string();
    assert!(
        refused.contains("economy:charge failed") && refused.contains("cannot afford"),
        "{refused}"
    );
}

#[test]
fn events_reach_the_listeners_at_the_next_tick() {
    let mut g = Game::new();
    g.shop("buy", vec![PackageArg::Int(20)]).unwrap();
    assert_eq!(g.global("shop", "heard"), json!(0), "heard before the tick");
    g.steps(1);
    assert_eq!(g.global("shop", "heard"), json!(1));
    assert_eq!(g.global("shop", "last"), json!("economy:paid:20:shop"));
    // An event of a call that was not kept was never sent.
    let _ = g.shop("buy_then_fail", vec![]);
    g.steps(2);
    assert_eq!(g.global("shop", "heard"), json!(1));
}

#[test]
fn the_callees_operations_use_the_callees_capabilities() {
    let mut g = Game::new();
    // `shop` has no `chat` capability; `economy`'s `tell` still goes out.
    g.shop("buy", vec![PackageArg::Int(5)]).unwrap();
    let problems: Vec<String> =
        g.s.package_diagnostics()
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
    assert!(
        !problems.iter().any(|p| p.contains("op.capability")),
        "{problems:?}"
    );
}

#[test]
fn only_dependents_call_exports_and_only_what_is_exported() {
    let mut g = Game::new();
    g.send("outsider", "steal", vec![]).unwrap();
    assert_eq!(g.money(), 100);
    let refused = g.global("outsider", "refused").to_string();
    assert!(refused.contains("does not list it"), "{refused}");
    g.shop("missing", vec![]).unwrap();
    let refused = g.global("shop", "refused").to_string();
    assert!(refused.contains("exports no `steal`"), "{refused}");
    g.shop("list", vec![]).unwrap();
    assert_eq!(g.global("shop", "seen")["charge"], json!(2));
}

#[test]
fn a_chain_never_comes_back_into_an_add_on_already_running() {
    let mut g = Game::new();
    g.shop("back", vec![]).unwrap();
    let refused = g.global("shop", "refused").to_string();
    assert!(refused.contains("already running"), "{refused}");
}

#[test]
fn the_callees_work_counts_against_the_callers_budget() {
    let mut g = Game::new();
    // The endless export stops at the caller's budget; catching it does
    // not buy more, so the caller's own call ends there too.
    assert!(g.shop("spin", vec![]).is_err());
    assert_eq!(g.global("shop", "tries"), json!(0));
}

#[test]
fn views_cross_as_plain_values() {
    let mut g = Game::new();
    // `seen` is a counter, so writing a name to it would still be allowed;
    // the point is that the call succeeds and reads the name.
    g.shop("view", vec![]).unwrap();
    assert_eq!(g.global("shop", "seen"), json!("P"));
}

#[test]
fn listening_to_an_add_on_not_depended_on_is_refused_at_load() {
    let root = temp_root();
    package(
        &root,
        "economy",
        json!({}),
        json!({ "events": ["paid"] }),
        "",
    );
    package(
        &root,
        "eavesdropper",
        json!({}),
        json!({ "listens": ["economy:paid"] }),
        "fn on_addon_event(from, name, data) { }",
    );
    let catalog = load(&root, &["economy", "eavesdropper"]).unwrap();
    let problems = bri_package_runtime::script::Runtime::compile(&catalog)
        .err()
        .expect("refused");
    assert!(
        problems
            .iter()
            .any(|d| d.code == "behaviour.listens" && d.message.contains("does not depend")),
        "{problems:#?}"
    );
}

#[test]
fn an_export_without_its_function_is_refused_at_load() {
    let root = temp_root();
    package(
        &root,
        "economy",
        json!({}),
        json!({ "exports": [{ "name": "charge", "args": 2 }] }),
        "fn api_charge(p, amount) { }",
    );
    let catalog = load(&root, &["economy"]).unwrap();
    let problems = bri_package_runtime::script::Runtime::compile(&catalog)
        .err()
        .expect("refused");
    assert!(
        problems
            .iter()
            .any(|d| d.message.contains("api_charge") && d.message.contains("3 parameter")),
        "{problems:#?}"
    );
}
