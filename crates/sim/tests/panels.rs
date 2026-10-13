//! Panels (`show_panel`, `on_panel`, docs/modding/rules.md): a shop
//! window an Add-On's rules show a player, built from the game's widgets,
//! and the purchase that comes back from it. The host passes on only what
//! the open panel could send.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{Catalog, panel::PanelEvent};
use bri_sim::{
    definitions::Definitions,
    session::{Command, Notice, PackageArg, PackageCommand, PanelCommand, Session},
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

const SHOP: &str = r#"
fn cmd_shop(p) {
    show_panel(p, #{
        id: "shop", title: "Gun Shop",
        items: [
            #{ kind: "text", text: "Pick a gun." },
            #{ kind: "list", id: "stock", rows: ["Pistol $5", "Rifle $20"], selected: 0 },
            #{ kind: "field", id: "note", label: "Engraving", max: 12 },
            #{ kind: "buttons", buttons: [#{ id: "buy", label: "Buy" }] }
        ]
    });
}
fn cmd_shut(p) { close_panel(p, "shop"); }
fn on_panel(p, e) {
    let log = get("log");
    log.push(`${e.panel} ${e.control} ${e.lists.stock} ${e.fields.note}`);
    set("log", log);
}
"#;

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn catalog() -> Arc<Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Root(std::env::temp_dir().join(format!(
        "bri-panels-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("shop");
    std::fs::create_dir_all(&dir).unwrap();
    let write = |file: &str, value: String| std::fs::write(dir.join(file), value).unwrap();
    write(
        "package.json",
        json!({
            "schema_version": 1, "id": "shop", "version": "1.0.0", "api": 1,
            "name": "shop", "license": "CC0-1.0", "capabilities": ["chat"],
            "provides": [
                { "kind": "behaviour", "id": "shop:behaviour/main", "file": "behaviour.json" },
                { "kind": "script", "id": "shop:script/main", "file": "main.rhai" }
            ]
        })
        .to_string(),
    );
    write(
        "behaviour.json",
        json!({
            "schema_version": 1, "script": "main.rhai", "on_panel": true,
            "commands": [{ "name": "shop" }, { "name": "shut" }],
            "state": { "global": { "log": { "default": [], "visible": "everyone" } } }
        })
        .to_string(),
    );
    write("main.rhai", SHOP.into());
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "shop".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "shop".into(),
            role: None,
        }],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    p: OwnerId,
    seq: u64,
}
impl Game {
    fn new() -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new("Shop".into(), "shop".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![
                    ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                ],
            )
            .unwrap(),
        );
        s.install_packages(catalog(), None).unwrap();
        let p = s
            .join("P".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        s.step().unwrap();
        Self { s, p, seq: 0 }
    }
    fn send(&mut self, command: Command) -> anyhow::Result<()> {
        self.seq += 1;
        self.s.command(self.p, self.seq, command).map(|_| ())
    }
    fn shop(&mut self, command: &str) {
        self.send(Command::Package(PackageCommand {
            package: "shop".into(),
            command: command.into(),
            args: Vec::<PackageArg>::new(),
        }))
        .unwrap();
        self.s.step().unwrap();
    }
    fn event(&mut self, control: &str, row: Option<u16>, note: &str) -> anyhow::Result<()> {
        self.send(Command::Panel(PanelCommand {
            package: "shop".into(),
            event: PanelEvent {
                panel: "shop".into(),
                control: control.into(),
                fields: [("note".into(), note.into())].into(),
                lists: [("stock".into(), row)].into(),
            },
        }))
    }
    fn log(&self) -> Vec<Value> {
        self.s.package_state().packages["shop"].global["log"]
            .as_array()
            .unwrap()
            .clone()
    }
    fn panels_sent(&mut self) -> Vec<Option<String>> {
        self.s
            .take_private_notices()
            .into_iter()
            .filter_map(|(_, n)| match n {
                Notice::Panel(p) => Some(p.map(|p| p.panel.title)),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn a_shop_panel_opens_and_its_purchase_reaches_the_rules() {
    let mut g = Game::new();
    // No panel open: nothing to press.
    assert!(g.event("buy", Some(1), "").is_err());
    g.shop("shop");
    assert_eq!(g.panels_sent(), [Some("Gun Shop".to_owned())]);
    assert_eq!(g.s.open_panel(g.p).unwrap().panel.id, "shop");
    g.event("buy", Some(1), "Max").unwrap();
    g.event("stock", None, "").unwrap();
    assert_eq!(g.log(), [json!("shop buy 1 Max"), json!("shop stock  ")]);
    // What the panel could not send is refused, and the rules hear nothing.
    for (control, row, note) in [
        ("sell", Some(0), ""),
        ("buy", Some(2), ""),
        ("buy", Some(0), "far too long a note"),
        ("note", Some(0), ""),
    ] {
        assert!(g.event(control, row, note).is_err(), "{control}");
    }
    assert_eq!(g.log().len(), 2);
    // Closing it is heard once, and then it is gone.
    g.event("close", Some(0), "").unwrap();
    assert!(g.s.open_panel(g.p).is_none());
    assert!(g.event("buy", Some(0), "").is_err());
    assert_eq!(g.log().len(), 3);
}

#[test]
fn the_rules_close_their_own_panel() {
    let mut g = Game::new();
    g.shop("shop");
    g.shop("shut");
    assert_eq!(g.panels_sent(), [Some("Gun Shop".to_owned()), None]);
    assert!(g.s.open_panel(g.p).is_none());
}
