//! Bot hardening scenarios: crowds and places that made bots do something a
//! player would not, each found by a probe, fixed, and kept here.: fixed scenarios bots play on invented content, scored
//! on what looks bad on camera (`gauntlet::Report`): stuck, idle or
//! circling bots, flip-flopping, void and own-side deaths, shots away from
//! the target or into an ally, and whether the scenario's goal moved at all.
//! Thresholds sit just past today's measured play, so they pass now and
//! fail when play gets worse; tighten them as the brain improves (each
//! threshold's comment gives the measured value). Every run is the same
//! run: bots draw from seeded generators. `BRI_GAUNTLET_ROUNDS=n` plays
//! every scenario n times as long, for soak runs, against the same
//! thresholds: problems that pile up over a long game fail there first.
//! `BRI_GAUNTLET_TRACE=<scenario>` prints every bot's state twice a second.
mod gauntlet;

use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::bot_kind::BotKind;
use bri_world::{Brick, OwnerId};
use gauntlet::*;
use glam::Vec3;
use std::collections::BTreeMap;

const FAR_A: Vec3 = Vec3::new(92.0, 0.05, 92.0);
const FAR_B: Vec3 = Vec3::new(92.0, 0.05, 86.0);
const GUN: &str = bri_weapons::testing::GUN_ITEM;

fn loadout(items: &[&str]) -> [Option<String>; 5] {
    let mut out: [Option<String>; 5] = Default::default();
    for (slot, item) in items.iter().enumerate() {
        out[slot] = Some(item.to_string());
    }
    out
}

/// How a battle is laid out.
struct Spec {
    name: &'static str,
    /// Spawn bricks of red (side 0) and blue (side 1).
    red: Vec<Vec3>,
    blue: Vec<Vec3>,
    kinds: [&'static str; 2],
    extra_kinds: Vec<BotKind>,
    vehicle_kinds: Vec<&'static str>,
    /// Arena bricks (red's), and blue's own.
    bricks: Vec<Brick>,
    blue_bricks: Vec<Brick>,
    settings: Settings,
    /// Arena brick and item definitions for a package.
    brick_defs: Vec<(&'static str, &'static str)>,
    item_defs: Vec<(&'static str, &'static str)>,
}
impl Spec {
    fn new(name: &'static str, red: Vec<Vec3>, blue: Vec<Vec3>, items: &[&str]) -> Self {
        Self {
            name,
            red,
            blue,
            kinds: [fixture::BOT, fixture::BOT],
            extra_kinds: Vec::new(),
            vehicle_kinds: Vec::new(),
            bricks: Vec::new(),
            blue_bricks: Vec::new(),
            settings: Settings {
                loadout: loadout(items),
                use_all_players_bricks: true,
                ..Default::default()
            },
            brick_defs: Vec::new(),
            item_defs: Vec::new(),
        }
    }
}

/// Spawn points in a line across z at `x`, `count` of them 4 apart.
fn line(x: f32, y: f32, z: f32, count: usize) -> Vec<Vec3> {
    (0..count)
        .map(|i| Vec3::new(x, y, z + (i as f32 - (count as f32 - 1.0) / 2.0) * 4.0))
        .collect()
}

struct Battle {
    name: &'static str,
    arena: Arena,
    sides: BTreeMap<OwnerId, u32>,
}

/// Two builders' bots in one mini-game; `package` runs before anyone
/// joins.
fn battle(spec: Spec, package: impl FnOnce(&mut Arena)) -> Battle {
    let mut arena = Arena::custom(&spec.brick_defs, &spec.item_defs, &spec.vehicle_kinds);
    arena.kinds(spec.extra_kinds, &spec.vehicle_kinds);
    package(&mut arena);
    let red = arena.join("Red builder", FAR_A);
    let blue = arena.join("Blue builder", FAR_B);
    let mut spawns = Vec::new();
    let mut red_bricks = spec.bricks;
    for at in &spec.red {
        red_bricks.push(spawner(spec.kinds[0], *at));
        spawns.push((*at, 0));
    }
    let mut blue_bricks = spec.blue_bricks;
    for at in &spec.blue {
        spawns.push((*at, 1));
        blue_bricks.push(spawner(spec.kinds[1], *at));
    }
    arena.load(red, red_bricks);
    arena.load(blue, blue_bricks);
    let game = arena.minigame(red, spec.settings);
    arena.join_game(blue, game);
    arena.step(TICKS_PER_SECOND);
    let sides = arena.sides_by_spawn(&spawns);
    assert_eq!(
        sides.len(),
        spec.red.len() + spec.blue.len(),
        "every bot spawned"
    );
    Battle {
        name: spec.name,
        arena,
        sides,
    }
}

impl Battle {
    /// Play `seconds` (times the soak rounds), scoring every tick;
    /// `each` adds scenario progress.
    fn play(
        &mut self,
        floor: f32,
        seconds: usize,
        mut each: impl FnMut(&bri_sim::session::Session, &mut Report),
    ) -> Report {
        let mut scorer = Scorer::new(self.name, floor);
        for _ in 0..seconds * TICKS_PER_SECOND * rounds() {
            timed_step(&mut self.arena, &mut scorer.report);
            let s = &self.arena.s;
            let vitals = s.vitals();
            let sides = &self.sides;
            scorer.sample(s, sides, |bot| {
                sides
                    .iter()
                    .any(|(o, side)| Some(side) != sides.get(&bot) && vitals[o].alive)
            });
            each(s, &mut scorer.report);
        }
        finish(&scorer.report);
        scorer.report
    }
}

fn stairs(at: Vec3, steps: usize, rise: f32, width: usize) -> Vec<Brick> {
    let mut out = Vec::new();
    for i in 0..steps {
        let top = rise * (i as f32 + 1.0);
        let mut y = 0.0;
        while y < top - 0.01 {
            let (def, h) = if top - y >= 0.6 - 0.01 {
                (fixture::BRICK, 0.6)
            } else {
                (fixture::PLATE, 0.2)
            };
            for w in 0..width {
                if def == fixture::BRICK {
                    // 2 x 1 units: lay it along z.
                    let mut b = brick(
                        def,
                        Vec3::new(
                            at.x + i as f32 + 0.5,
                            y + h / 2.0,
                            at.z + w as f32 * 2.0 + 1.0,
                        ),
                    );
                    b.quarter_turns = 1;
                    out.push(b);
                } else {
                    for dz in 0..4 {
                        for dx in 0..2 {
                            out.push(brick(
                                def,
                                Vec3::new(
                                    at.x + i as f32 + 0.25 + dx as f32 * 0.5,
                                    y + h / 2.0,
                                    at.z + w as f32 * 2.0 + 0.25 + dz as f32 * 0.5,
                                ),
                            ));
                        }
                    }
                }
            }
            y += h;
        }
    }
    out
}

fn once(bricks: Vec<Brick>) -> Vec<Brick> {
    let mut seen = std::collections::BTreeSet::new();
    bricks
        .into_iter()
        .filter(|b| {
            seen.insert((
                (b.position[0] * 4.0).round() as i32,
                (b.position[1] * 4.0).round() as i32,
                (b.position[2] * 4.0).round() as i32,
            ))
        })
        .collect()
}

/// Sixteen a side in a block of walls, a deck with stairs: the first half minute
/// of a crowd that has to find its way across. One path-search budget shared
/// by thirty-two searches left most of them waiting, their bots standing at
/// their spawns with no route yet (nearly half of the first thirty seconds
/// stuck); a budget grown with the crowd leaves a few percent.
#[test]
fn thirty_two_bots_set_off_across_a_walled_map_without_waiting_to_find_a_way() {
    let spread = |x: f32| -> Vec<Vec3> {
        line(x, 0.0, 52.0, 16)
            .into_iter()
            .enumerate()
            .map(|(i, v)| Vec3::new(v.x + (i % 2) as f32 * 2.0, v.y, v.z))
            .collect()
    };
    let mut spec = Spec::new("walled_map_32", spread(-92.0), spread(-24.0), &[GUN]);
    let mut bricks = Vec::new();
    for (x, z0, z1) in [
        (-76.0, 20.0, 50.0),
        (-66.0, 56.0, 86.0),
        (-52.0, 20.0, 46.0),
        (-44.0, 58.0, 86.0),
    ] {
        for course in fixture::barrier_courses() {
            bricks.extend(wall(
                Vec3::new(x, course - 1.5, z0),
                Vec3::new(x, course - 1.5, z1),
            ));
        }
    }
    bricks.extend(floor(Vec3::new(-64.0, 0.0, 30.0), [1, 1], 3.0));
    bricks.extend(stairs(Vec3::new(-69.0, 0.0, 33.0), 5, 0.6, 3));
    spec.bricks = once(bricks);
    let mut b = battle(spec, |a| {
        a.s.set_server_settings(bri_admin::ServerSettings {
            max_bots: 32,
            ..a.s.server_settings().clone()
        })
        .unwrap();
    });
    assert_eq!(b.sides.len(), 32);
    let r = b.play(0.0, 30, |_, _| {});
    eprintln!(
        "walled_map_32: stuck {:.3}, circling {:.3}, kills {}",
        r.share(r.stuck),
        r.share(r.circling),
        r.kills
    );
    assert!(r.share(r.stuck) < 0.12, "stuck {:.3}", r.share(r.stuck));
    assert!(r.kills > 0, "the sides met");
}
