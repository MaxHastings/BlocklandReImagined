//! The roleplay samples (`packages/samples/sample-rp-*`), a port of the
//! classic roleplay mods onto the Add-On building blocks: the bank is a
//! core other Add-Ons build on (`call_addon`, `emit`), its money is kept
//! in host data under each player's lasting id, and the backpack and
//! chest is a panel whose rows are dragged between lists, shared live
//! between the players who have it open.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog,
    panel::{PanelDrop, PanelEvent},
};
use bri_sim::{
    definitions::Definitions,
    session::{Command, PackageArg, PackageCommand, PanelCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const INVENTORY: &str = "sample-rp-inventory";

fn add_ons() -> Arc<Catalog> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples");
    let packages = ["sample-rp-bank", INVENTORY]
        .into_iter()
        .map(|id| PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: id.into(),
            role: None,
        })
        .collect();
    let set = PackageSet {
        schema_version: 1,
        packages,
    };
    Arc::new(Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
}
impl Game {
    fn new() -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new("Town".into(), "town".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![
                    ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                ],
            )
            .unwrap(),
        );
        s.install_packages(add_ons(), None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str, x: f32) -> OwnerId {
        let p = self
            .s
            .join(name.into(), Vec3::new(x, 0.05, 0.0), false)
            .unwrap();
        self.s.step().unwrap();
        p
    }
    fn send(&mut self, p: OwnerId, command: Command) -> anyhow::Result<()> {
        let seq = self.seq.entry(p).or_default();
        *seq += 1;
        let result = self.s.command(p, *seq, command).map(|_| ());
        self.s.step().unwrap();
        let problems = self.s.take_package_problems();
        assert!(problems.is_empty(), "{problems:#?}");
        result
    }
    fn chest(&mut self, p: OwnerId) {
        self.send(
            p,
            Command::Package(PackageCommand {
                package: INVENTORY.into(),
                command: "chest".into(),
                args: Vec::<PackageArg>::new(),
            }),
        )
        .unwrap();
    }
    fn panel(
        &mut self,
        p: OwnerId,
        control: &str,
        shop: Option<u16>,
        drop: Option<PanelDrop>,
    ) -> anyhow::Result<()> {
        self.send(
            p,
            Command::Panel(PanelCommand {
                package: INVENTORY.into(),
                event: PanelEvent {
                    panel: "chest".into(),
                    control: control.into(),
                    lists: [("shop".into(), shop)].into(),
                    drop,
                    ..PanelEvent::default()
                },
            }),
        )
    }
    fn buy(&mut self, p: OwnerId, row: u16) {
        self.panel(p, "buy", Some(row), None).unwrap();
    }
    fn drag(&mut self, p: OwnerId, from: &str, row: u16, to: &str) -> anyhow::Result<()> {
        let drop = PanelDrop {
            from: from.into(),
            row,
            at: None,
        };
        self.panel(p, to, Some(0), Some(drop))
    }
    /// The rows of one list in the player's open window.
    fn rows(&self, p: OwnerId, list: &str) -> Vec<String> {
        let shown = self.s.open_panel(p).expect("the window is open");
        shown
            .panel
            .items
            .iter()
            .find(|i| i.id == list)
            .unwrap()
            .rows
            .clone()
    }
    /// The window's line of text: money and the last note.
    fn note(&self, p: OwnerId) -> String {
        self.s
            .open_panel(p)
            .expect("the window is open")
            .panel
            .items[0]
            .text
            .clone()
    }
}

#[test]
fn the_shop_sells_through_the_bank_and_refuses_what_a_player_cannot_afford() {
    let mut g = Game::new();
    let max = g.join("Max", 0.0);
    g.chest(max);
    assert!(g.note(max).starts_with("You have $50."), "{}", g.note(max));
    assert!(g.rows(max, "pack").is_empty());
    // Rope $5, then two Pickaxes at $15 each.
    g.buy(max, 1);
    g.buy(max, 3);
    g.buy(max, 3);
    assert_eq!(g.rows(max, "pack"), ["Rope", "Pickaxe", "Pickaxe"]);
    assert!(g.note(max).starts_with("You have $15."), "{}", g.note(max));
    // A third Pickaxe is $15 of the $15 left; a fourth is refused whole.
    g.buy(max, 3);
    g.buy(max, 3);
    assert!(g.note(max).contains("cannot afford"), "{}", g.note(max));
    assert!(g.note(max).starts_with("You have $0."), "{}", g.note(max));
    assert_eq!(g.rows(max, "pack").len(), 4);
}

#[test]
fn items_dragged_into_the_chest_reach_everyone_who_has_it_open() {
    let mut g = Game::new();
    let max = g.join("Max", 0.0);
    let sam = g.join("Sam", 3.0);
    g.chest(max);
    g.buy(max, 0);
    g.buy(max, 2);
    g.chest(sam);
    assert_eq!(g.rows(max, "pack"), ["Bread", "Lamp"]);
    assert!(g.rows(sam, "pack").is_empty(), "each player has their own");

    // Max drags the Lamp into the chest; Sam's open window shows it.
    g.drag(max, "pack", 1, "chest").unwrap();
    assert_eq!(g.rows(max, "pack"), ["Bread"]);
    assert_eq!(g.rows(max, "chest"), ["Lamp"]);
    assert_eq!(g.rows(sam, "chest"), ["Lamp"]);

    // Sam takes it; Max's window loses it, so his own drag of it is now
    // a row his window no longer has.
    g.drag(sam, "chest", 0, "pack").unwrap();
    assert_eq!(g.rows(sam, "pack"), ["Lamp"]);
    assert!(g.rows(max, "chest").is_empty());
    assert!(g.drag(max, "chest", 0, "pack").is_err());

    // The shop list does not drag, and nothing drops on it.
    assert!(g.drag(max, "shop", 0, "pack").is_err());
    assert!(g.drag(max, "pack", 0, "shop").is_err());

    // Reordering within a list is a drop on the same list.
    g.buy(max, 1);
    assert_eq!(g.rows(max, "pack"), ["Bread", "Rope"]);
    g.drag(max, "pack", 0, "pack").unwrap();
    assert_eq!(g.rows(max, "pack"), ["Rope", "Bread"]);

    // Closing stops the updates.
    g.panel(sam, "close", Some(0), None).unwrap();
    g.drag(max, "pack", 0, "chest").unwrap();
    assert!(g.s.open_panel(sam).is_none());
}
