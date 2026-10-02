//! Where a shot lands, played through the authoritative session with a
//! made-up package: a round in the head or the body reaches `on_damage` with
//! `info.region` and its point, its damage type by name without
//! `$DamageType::` and the projectile that struck, so a rule can make
//! headshots count (Torque's `getDamageLocation`) per gun.
use bri_minigames::Settings;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{ActionAim, Command, MiniGameRequest, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

const GUN: &str = "kit:weapon/gun";

/// A head hit does triple damage; every hit is noted.
const SCRIPT: &str = r#"
fn on_damage(victim, attacker, amount, info) {
    set("hit", `${info.kind}|${info.type}|${info.region}|${info.y > 0.0}|${info.projectile}`);
    if info.region == "head" { amount * 3.0 } else { () }
}
"#;

fn weapons() -> bri_weapons::Pack {
    let pack = json!({
        "schema_version": 3,
        "id": "kit",
        "items": { GUN: { "ui_name": "Kit Gun", "image": "kit:image/gun" } },
        "images": {
            "kit:image/gun": {
                "projectile": "kit:projectile/round",
                "states": [
                    { "name": "Activate", "ticks": 6, "timeout": 1 },
                    { "name": "Ready", "down": 2 },
                    { "name": "Fire", "ticks": 12, "script": "onFire", "timeout": 3 },
                    { "name": "Hold", "up": 1 }
                ]
            }
        },
        "projectiles": {
            "kit:projectile/round": {
                "speed": 200.0, "lifetime_ticks": 240, "fade_ticks": 240,
                "damage": 20.0, "damage_type": "$DamageType::KitShot"
            }
        },
        "damage_types": {
            "kitshot": {
                "name": "KitShot", "suicide_message": "%1 shot themselves",
                "murder_message": "%2 shot %1", "vehicle_scale": 1.0, "direct": true
            }
        }
    });
    bri_weapons::Pack::from_json(&serde_json::to_vec(&pack).unwrap()).unwrap()
}

struct Root(std::path::PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn catalog() -> Arc<Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Root(std::env::temp_dir().join(format!(
        "bri-hit-regions-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("kit");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "kit", "version": "1.0.0", "api": 1,
        "name": "kit", "license": "CC0-1.0",
        "capabilities": ["damage"],
        "provides": [
            { "kind": "behaviour", "id": "kit:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "kit:script/main", "file": "main.rhai" }
        ]
    });
    let behaviour = json!({
        "schema_version": 1,
        "script": "main.rhai",
        "on_damage": true,
        "state": { "global": { "hit": { "default": "", "visible": "everyone" } } }
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(dir.join("main.rhai"), SCRIPT).unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "kit".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "kit".into(),
            role: None,
        }],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn new() -> Self {
        let ground = ColliderBuilder::cuboid(100.0, 0.5, 100.0)
            .translation(Vector::new(0.0, -0.5, 0.0))
            .user_data(u128::MAX);
        let mut s = Session::new(
            Simulation::new(
                World::new("Range".into(), "range".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![ground],
            )
            .unwrap(),
        );
        s.set_weapon_pack(weapons()).unwrap();
        s.install_packages(catalog(), None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.s.join(name.into(), at, false).unwrap();
        self.looks.insert(owner, MoveInput::default());
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        let look = self.looks[&owner];
        let aim = Some(ActionAim {
            yaw: look.yaw,
            pitch: look.pitch,
        });
        self.s.command_with_aim(owner, *n, command, aim).unwrap();
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            for (owner, input) in &self.looks {
                let m = self.moves.entry(*owner).or_default();
                *m += 1;
                let _ = self.s.movement(*owner, *m, *input);
            }
            self.s.step().unwrap();
        }
    }
    fn feet(&self, owner: OwnerId) -> Vec3 {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .map(|(p, _)| Vec3::from(p.feet))
            .unwrap()
    }
    fn hit(&self) -> Value {
        self.s
            .package_state()
            .packages
            .get("kit")
            .and_then(|ns| ns.global.get("hit").cloned())
            .unwrap_or(Value::Null)
    }
    /// A shoots at `height` above B's feet, from 6 units away.
    fn shoot_at(&mut self, a: OwnerId, b: OwnerId, height: f32) {
        // The default blockhead's eyes are 2.16 up.
        let eye = self.feet(a).y + 2.156;
        let pitch = ((self.feet(b).y + height - eye) / (self.feet(a).z - self.feet(b).z)).atan();
        self.looks.get_mut(&a).unwrap().pitch = pitch;
        self.steps(4);
        self.cmd(a, Command::WeaponTrigger { down: true });
        self.steps(2);
        self.cmd(a, Command::WeaponTrigger { down: false });
        self.steps(60);
    }
}

#[test]
fn a_round_tells_on_damage_where_it_struck() {
    let mut g = Game::new();
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    // A minigame handing out the gun, everyone where they stand.
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[0] = Some(GUN.into());
    g.cmd(
        a,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout,
                ..Settings::default()
            },
        }),
    );
    let game = g.s.minigame_views()[0].id;
    g.s.set_spawn_points(vec![g.feet(b)]).unwrap();
    g.cmd(b, Command::MiniGame(MiniGameRequest::Join { game }));
    g.steps(330);
    let slot = g.s.tool_inventories()[&a]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(GUN))
        .unwrap();
    g.cmd(a, Command::EquipTool { slot: Some(slot) });
    g.steps(20);
    // The chest: the round's own damage.
    g.shoot_at(a, b, 1.7);
    assert_eq!(
        g.hit(),
        json!("weapon|KitShot|torso|true|kit:projectile/round")
    );
    assert!((g.s.vitals()[&b].health - 80.0).abs() < 0.5);
    // The head: tripled by the rule.
    g.shoot_at(a, b, 2.45);
    assert_eq!(
        g.hit(),
        json!("weapon|KitShot|head|true|kit:projectile/round")
    );
    assert!((g.s.vitals()[&b].health - 20.0).abs() < 0.5);
}
