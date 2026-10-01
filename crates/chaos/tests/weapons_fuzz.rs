//! The weapons runtime against a world that answers every query legally but
//! unkindly: hits at distance zero (a shot from inside a brick), normals
//! facing along the shot, explosions centred on their targets, brick events
//! that bounce or redirect by zero or by huge amounts. And Add-On weapons
//! imported from damaged scripts. Every event, projectile and dropped item
//! must stay finite, and no call may panic.
use bri_addon_import::{Options, import};
use bri_chaos::{bots::Rng, fixture, scan::ensure_finite};
use bri_weapons::{
    ActorId, ContactResponse, Filter, Frame, Hit, Nearby, Pack, ProjectileContact, Query, TargetId,
    WeaponsWorld,
};
use glam::{Quat, Vec3};
use proptest::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Answers within the adapter contract (unit normals, fractions in 0..=1,
/// finite positions), choosing the awkward answers often.
struct Unkind(Rng);
impl Unkind {
    fn normal(&mut self, along: Vec3) -> Vec3 {
        let along = along.normalize_or(Vec3::Y);
        match self.0.below(5) {
            0 => along,
            1 => -along,
            2 => Vec3::Y,
            _ => Vec3::new(
                self.0.range(-1.0, 1.0),
                self.0.range(-1.0, 1.0),
                self.0.range(-1.0, 1.0),
            )
            .normalize_or(Vec3::NEG_Y),
        }
    }
    fn target(&mut self) -> TargetId {
        match self.0.below(4) {
            0 => TargetId::Actor(ActorId(1 + self.0.below(3) as u64)),
            1 => TargetId::Vehicle(self.0.below(3) as u64),
            2 => TargetId::Brick(1 + self.0.below(3) as u64),
            _ => TargetId::Map(0),
        }
    }
}
impl Query for Unkind {
    fn on_contact(&mut self, _: &ProjectileContact) -> ContactResponse {
        match self.0.below(12) {
            0 => ContactResponse::Delete,
            1 => ContactResponse::Explode,
            2 => ContactResponse::Bounce([0.0, -1.0, 1e9, 1.0][self.0.below(4)]),
            3 => ContactResponse::Redirect {
                vector: [
                    Vec3::ZERO,
                    Vec3::splat(1e30),
                    Vec3::new(0.0, 1e-30, 0.0),
                    Vec3::X,
                ][self.0.below(4)],
                normalized: self.0.chance(0.5),
            },
            _ => ContactResponse::Continue,
        }
    }
    fn sweep(&mut self, start: Vec3, end: Vec3, _: Filter) -> Option<Hit> {
        if self.0.chance(0.5) {
            return None;
        }
        let fraction = match self.0.below(4) {
            0 => 0.0,
            1 => 1.0,
            _ => self.0.unit(),
        };
        Some(Hit {
            target: self.target(),
            position: start.lerp(end, fraction),
            normal: self.normal(end - start),
            fraction,
            color: self.0.chance(0.5).then_some([0.5, 0.2, 1.0]),
        })
    }
    fn sweep_box(
        &mut self,
        start: Vec3,
        end: Vec3,
        _: Vec3,
        _: Quat,
        filter: Filter,
    ) -> Option<Hit> {
        self.sweep(start, end, filter)
    }
    fn radius(&mut self, center: Vec3, radius: f32, limit: usize) -> Vec<Nearby> {
        (0..self.0.below(5).min(limit))
            .map(|_| {
                let distance = if self.0.chance(0.4) {
                    0.0
                } else {
                    self.0.unit() * radius
                };
                Nearby {
                    target: self.target(),
                    center: if self.0.chance(0.4) {
                        center
                    } else {
                        center + Vec3::X * distance
                    },
                    distance,
                }
            })
            .collect()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        true
    }
}

fn frame(rng: &mut Rng) -> Frame {
    let v = |rng: &mut Rng, s: f32| Vec3::new(rng.range(-s, s), rng.range(-s, s), rng.range(-s, s));
    let position = v(rng, 50.0);
    let mut direction = v(rng, 1.0);
    if direction.length_squared() <= 0.11 {
        direction = Vec3::NEG_Z;
    }
    Frame {
        body_yaw: rng.range(-std::f32::consts::PI, std::f32::consts::PI),
        position,
        eye: position + Vec3::Y * 2.0,
        muzzle: [position + v(rng, 1.0), position + v(rng, 1.0)],
        direction,
        velocity: if rng.chance(0.1) {
            v(rng, 5000.0)
        } else {
            v(rng, 20.0)
        },
        scale: [0.01, 1.0, 100.0, 0.2][rng.below(4)],
        grounded: rng.chance(0.5),
        horse: rng.chance(0.1),
        first_person: rng.chance(0.5),
        can_jet: rng.chance(0.5),
        ..Frame::default()
    }
}

/// Everything the runtime shows the host: it must be finite at every tick.
fn check(world: &WeaponsWorld, events: &[bri_weapons::Event]) -> Result<(), TestCaseError> {
    let fail = |e: anyhow::Error| TestCaseError::fail(format!("{e:#}"));
    ensure_finite("events", events).map_err(fail)?;
    ensure_finite("projectiles", &world.projectiles().collect::<Vec<_>>()).map_err(fail)?;
    ensure_finite("drops", &world.drops().collect::<Vec<_>>()).map_err(fail)?;
    Ok(())
}

/// Three players with every item, shooting, switching, dropping and picking
/// up for `ticks`.
fn play(pack: Pack, items: &[String], seed: u64, ticks: u32) -> Result<(), TestCaseError> {
    let mut world = WeaponsWorld::new(pack).map_err(|e| TestCaseError::reject(format!("{e:#}")))?;
    let mut rng = Rng::new(seed);
    let mut query = Unkind(Rng::new(seed ^ 0xabcd));
    let actors = [ActorId(1), ActorId(2), ActorId(3)];
    for id in actors {
        world.add_actor(id, 5).unwrap();
        for item in items.iter().take(5) {
            let _ = world.give(id, item);
        }
    }
    for _ in 0..ticks {
        for id in actors {
            let _ = world.set_frame(id, frame(&mut rng));
            match rng.below(40) {
                0..=2 => {
                    let _ = world.equip(id, rng.chance(0.9).then(|| rng.below(5)));
                }
                3..=12 => {
                    let _ = world.trigger(id, rng.chance(0.6));
                }
                13 => {
                    let _ = world.drop_item(id, rng.below(5));
                }
                14 => {
                    let drops: Vec<_> = world.drops().map(|d| d.id).collect();
                    if !drops.is_empty() {
                        let _ = world.pickup(id, drops[rng.below(drops.len())]);
                    }
                }
                15 => {
                    let _ = world.set_ammo(id, rng.chance(0.5));
                }
                16 => {
                    world.remove_actor(id);
                    let _ = world.add_actor(id, 5);
                    let _ = world.give(id, &items[rng.below(items.len())]);
                }
                _ => {}
            }
        }
        let events = world.step(&mut query);
        check(&world, &events)?;
    }
    Ok(())
}

fn synthetic() -> &'static (Pack, Vec<String>) {
    static PACK: OnceLock<(Pack, Vec<String>)> = OnceLock::new();
    PACK.get_or_init(|| fixture::synthetic_weapons().unwrap())
}

proptest! {
    #![proptest_config(bri_chaos::proptest_config(64, 0x9a4))]

    #[test]
    fn weapons_stay_finite_in_an_unkind_world(seed in any::<u64>()) {
        let (pack, items) = synthetic();
        play(pack.clone(), items, seed, 400)?;
    }
}

fn blaster() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../addon-import/tests/fixtures/Weapon_Synthetic_Blaster")
}

fn scratch() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("bri-chaos-addon-{}-{n}", std::process::id()))
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[derive(Debug, Clone)]
enum Damage {
    /// Replace the nth number in the file.
    Number(usize, &'static str),
    DropLine(usize),
    RepeatLine(usize),
    Cut(usize),
    Insert(usize, &'static str),
}

fn damage() -> impl Strategy<Value = (usize, Damage)> {
    let numbers = prop_oneof![
        Just("nan"),
        Just("inf"),
        Just("-inf"),
        Just("1e39"),
        Just("-1e39"),
        Just("0"),
        Just("-0"),
        Just("-1"),
        Just("99999999999"),
        Just("1e-45"),
        Just("0 0 0"),
        Just("\"\""),
        Just("x"),
        Just("0.000001"),
        Just("-100000"),
    ];
    let inserts = prop_oneof![
        Just("{"),
        Just("}"),
        Just(";"),
        Just("\""),
        Just("//"),
        Just("/*"),
        Just("%"),
        Just("datablock ProjectileData(x){};"),
        Just("\u{0}"),
        Just("\u{202e}"),
        Just("exec(\"../../../x.cs\");"),
        Just("new ScriptObject(){};"),
    ];
    (
        0usize..4,
        prop_oneof![
            6 => (any::<usize>(), numbers).prop_map(|(n, v)| Damage::Number(n, v)),
            1 => any::<usize>().prop_map(Damage::DropLine),
            1 => any::<usize>().prop_map(Damage::RepeatLine),
            1 => any::<usize>().prop_map(Damage::Cut),
            1 => (any::<usize>(), inserts).prop_map(|(at, s)| Damage::Insert(at, s)),
        ],
    )
}

fn apply(text: &str, damage: &Damage) -> String {
    match damage {
        Damage::Number(n, v) => {
            let spans: Vec<(usize, usize)> = {
                let bytes = text.as_bytes();
                let mut spans = Vec::new();
                let mut i = 0;
                while i < bytes.len() {
                    if bytes[i].is_ascii_digit() {
                        let start = i;
                        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                            i += 1;
                        }
                        spans.push((start, i));
                    } else {
                        i += 1;
                    }
                }
                spans
            };
            if spans.is_empty() {
                return text.to_string();
            }
            let (a, b) = spans[n % spans.len()];
            format!("{}{v}{}", &text[..a], &text[b..])
        }
        Damage::DropLine(n) | Damage::RepeatLine(n) => {
            let mut lines: Vec<&str> = text.lines().collect();
            if lines.is_empty() {
                return text.to_string();
            }
            let i = n % lines.len();
            if matches!(damage, Damage::DropLine(_)) {
                lines.remove(i);
            } else {
                lines.insert(i, lines[i]);
            }
            lines.join("\n")
        }
        Damage::Cut(n) => {
            let mut at = n % (text.len() + 1);
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            text[..at].to_string()
        }
        Damage::Insert(n, s) => {
            let mut at = n % (text.len() + 1);
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            format!("{}{s}{}", &text[..at], &text[at..])
        }
    }
}

const FILES: [&str; 4] = ["blaster.cs", "server.cs", "bricks/pad.cs", "bricks/pad.blb"];

proptest! {
    #![proptest_config(bri_chaos::proptest_config(96, 0x9a4))]

    #[test]
    fn damaged_add_ons_import_or_refuse_and_their_weapons_stay_finite(
        damages in proptest::collection::vec(damage(), 1..5),
        seed in any::<u64>(),
    ) {
        let root = Scratch(scratch());
        let source = root.0.join("Weapon_Synthetic_Blaster");
        copy_dir(&blaster(), &source);
        for (file, d) in &damages {
            let path = source.join(FILES[*file]);
            let text = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, apply(&text, d)).unwrap();
        }
        let out = root.0.join("package");
        let imported = import(&Options {
            input: source,
            out: out.clone(),
            ..Default::default()
        });
        if imported.is_ok()
            && let Ok(bytes) = std::fs::read(out.join("assets/weapons.json"))
            && let Ok(pack) = Pack::from_json(&bytes)
        {
            let items: Vec<String> = pack_items(&bytes);
            if !items.is_empty() {
                play(pack, &items, seed, 200)?;
            }
        }
    }
}

fn pack_items(bytes: &[u8]) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    value["items"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn the_undamaged_blaster_imports_and_fires() {
    let root = Scratch(scratch());
    let out = root.0.join("package");
    import(&Options {
        input: blaster(),
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let bytes = std::fs::read(out.join("assets/weapons.json")).unwrap();
    let items = pack_items(&bytes);
    assert!(!items.is_empty());
    play(Pack::from_json(&bytes).unwrap(), &items, 7, 200).unwrap();
}
